/**
 * Interactive setup wizard — the heart of `velkor setup`.
 */

import { execSync, spawn as nodeSpawn } from "node:child_process";
import { writeFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  select,
  search,
  input,
  password,
  confirm,
} from "@inquirer/prompts";
import ora from "ora";

import {
  banner,
  section,
  success,
  failure,
  skip,
  bullet,
  blank,
  successBox,
  keyValue,
  brand,
  dim,
  ok,
  err,
  info,
  accent,
  bright,
} from "./ui.js";
import {
  checkPrerequisites,
  prereqsPassed,
} from "./prereqs.js";
import {
  generateEnv,
  generateConfig,
  envExists,
  type WizardAnswers,
} from "./generators.js";

export async function runSetup(projectRoot: string) {
  banner();

  // ─── Prerequisites ──────────────────────────────────────────────────
  section("Prerequisites");

  const prereqs = await checkPrerequisites();

  if (!prereqsPassed(prereqs)) {
    blank();
    failure(
      "Docker and Docker Compose are required. Install them and try again."
    );
    failure("https://docs.docker.com/get-docker/");
    process.exit(1);
  }

  // Warn if .env already exists
  if (envExists(projectRoot)) {
    blank();
    const overwrite = await confirm({
      message: `${accent(".env")} already exists. Overwrite it?`,
      default: false,
    });
    if (!overwrite) {
      bullet(dim("Keeping existing .env — skipping to service startup"));
      await offerDockerUp(projectRoot);
      return;
    }
  }

  // ─── LLM Provider ──────────────────────────────────────────────────
  section("LLM Provider");
  console.log(
    dim("  Choose how Velkor connects to language models.\n")
  );

  const llmProvider = await select({
    message: brand("Which LLM provider?"),
    choices: [
      {
        name: `${bright("OpenRouter")} ${dim("— access 200+ models with one key (recommended)")}`,
        value: "openrouter",
      },
      {
        name: `${bright("Anthropic")} ${dim("— Claude models direct")}`,
        value: "anthropic",
      },
      {
        name: `${bright("OpenAI")} ${dim("— GPT-4o, o3, etc.")}`,
        value: "openai",
      },
      {
        name: `${bright("Ollama")} ${dim("— local models, no API key")}`,
        value: "ollama",
      },
      {
        name: `${bright("Custom endpoint")} ${dim("— any OpenAI-compatible API")}`,
        value: "custom",
      },
    ],
  });

  let llmApiKey = "";
  let llmBaseUrl: string | undefined;
  let llmModel: string | undefined;

  if (llmProvider === "ollama") {
    llmBaseUrl = await input({
      message: brand("Ollama URL:"),
      default: "http://localhost:11434",
    });
    llmModel = await input({
      message: brand("Default model:"),
      default: "llama3.1:8b",
    });
    success(`Ollama at ${info(llmBaseUrl)}`);
  } else if (llmProvider === "custom") {
    llmBaseUrl = await input({
      message: brand("Base URL (e.g. https://api.together.xyz/v1):"),
    });
    llmApiKey = await password({
      message: brand("API key:"),
      mask: "•",
    });
    llmModel = await input({
      message: brand("Default model:"),
    });
    success(`Custom endpoint at ${info(llmBaseUrl)}`);
  } else {
    const keyLabel =
      llmProvider === "openrouter"
        ? "OpenRouter API key"
        : llmProvider === "anthropic"
          ? "Anthropic API key"
          : "OpenAI API key";

    llmApiKey = await password({
      message: brand(`${keyLabel}:`),
      mask: "•",
    });

    if (!llmApiKey.trim()) {
      failure("API key is required.");
      process.exit(1);
    }

    // Validate the API key
    const valid = await validateApiKey(llmProvider, llmApiKey);
    if (valid) {
      success(`${keyLabel} ${dim("verified ✓")}`);
    } else {
      failure(`Could not verify ${keyLabel} — check that it's correct`);
      const proceed = await confirm({
        message: brand("Continue anyway?"),
        default: false,
      });
      if (!proceed) process.exit(1);
    }

    // Model selection
    llmModel = await selectModel(llmProvider, llmApiKey);
    success(`Model: ${info(llmModel)}`);
  }

  // ─── Embedding Provider ─────────────────────────────────────────────
  section("Embedding Provider");
  console.log(
    dim(
      "  Embeddings power semantic search across memory and documents.\n"
    )
  );

  // Suggest a smart default based on LLM choice
  const embeddingDefault =
    llmProvider === "openai"
      ? "openai"
      : llmProvider === "ollama"
        ? "ollama"
        : llmProvider === "openrouter"
          ? "openai"
          : "skip";

  const embeddingProvider = await select({
    message: brand("Embedding provider?"),
    default: embeddingDefault,
    choices: [
      {
        name: `${bright("OpenAI")} ${dim("— text-embedding-3-small, best quality")}`,
        value: "openai",
      },
      {
        name: `${bright("Ollama")} ${dim("— nomic-embed-text, runs locally")}`,
        value: "ollama",
      },
      {
        name: `${dim("Skip")} ${dim("— FTS-only search (no vectors)")}`,
        value: "skip",
      },
    ],
  });

  let embeddingApiKey: string | undefined;

  if (embeddingProvider === "openai") {
    // If they already gave an OpenAI key for LLM, reuse it
    if (llmProvider === "openai") {
      embeddingApiKey = llmApiKey;
      success(`Reusing OpenAI API key`);
    } else {
      embeddingApiKey = await password({
        message: brand("OpenAI API key for embeddings:"),
        mask: "•",
      });
      success(`OpenAI embedding key ${dim("configured")}`);
    }
  } else if (embeddingProvider === "ollama") {
    success(`Ollama embeddings ${dim("(nomic-embed-text)")}`);
  } else {
    skip("Skipping embeddings — vector search disabled, FTS still works");
  }

  // ─── Web Search Provider ────────────────────────────────────────────
  section("Web Search");
  console.log(
    dim(
      "  Give agents the ability to search the web for current information.\n"
    )
  );

  const searchProvider = await select({
    message: brand("Search provider?"),
    choices: [
      {
        name: `${bright("Perplexity")} ${dim("— AI-synthesized answers with citations")}`,
        value: "perplexity",
      },
      {
        name: `${bright("Tavily")} ${dim("— purpose-built search API")}`,
        value: "tavily",
      },
      {
        name: `${bright("Brave Search")} ${dim("— privacy-focused web search")}`,
        value: "brave",
      },
      {
        name: `${bright("DuckDuckGo")} ${dim("— free, no API key needed")}`,
        value: "duckduckgo",
      },
      {
        name: `${dim("Skip")} ${dim("— no web search")}`,
        value: "skip",
      },
    ],
  });

  let searchApiKey: string | undefined;

  let searchModel: string | undefined;

  if (searchProvider === "perplexity") {
    // Can use OpenRouter key or Perplexity direct key
    if (llmProvider === "openrouter" && llmApiKey) {
      const reuse = await confirm({
        message: brand("Use your OpenRouter key for Perplexity search?"),
        default: true,
      });
      if (reuse) {
        searchApiKey = llmApiKey;
      } else {
        searchApiKey = await password({
          message: brand("Perplexity API key (pplx-...):"),
          mask: "•",
        });
      }
    } else {
      searchApiKey = await password({
        message: brand(
          "Perplexity API key (pplx-...) or OpenRouter key (sk-or-...):"
        ),
        mask: "•",
      });
    }

    // Perplexity model selection
    searchModel = await select({
      message: brand("Perplexity search model?"),
      choices: [
        {
          name: `${bright("sonar-pro")} ${dim("— thorough search with citations (recommended)")}`,
          value: "sonar-pro",
        },
        {
          name: `${bright("sonar")} ${dim("— fast, lightweight search")}`,
          value: "sonar",
        },
        {
          name: `${bright("sonar-reasoning-pro")} ${dim("— deep research with reasoning")}`,
          value: "sonar-reasoning-pro",
        },
        {
          name: `${bright("sonar-reasoning")} ${dim("— reasoning-powered search")}`,
          value: "sonar-reasoning",
        },
      ],
    });
    success(`Perplexity ${info(searchModel)} ${dim("configured")}`);
  } else if (
    searchProvider === "tavily" ||
    searchProvider === "brave"
  ) {
    const label =
      searchProvider === "tavily" ? "Tavily" : "Brave Search";
    searchApiKey = await password({
      message: brand(`${label} API key:`),
      mask: "•",
    });
    success(`${label} ${dim("configured")}`);
  } else if (searchProvider === "duckduckgo") {
    success("DuckDuckGo " + dim("— no key needed"));
  } else {
    skip("Web search disabled");
  }

  // ─── Admin User ─────────────────────────────────────────────────────
  section("Admin Account");
  console.log(
    dim("  Create the first admin user for the platform.\n")
  );

  const adminEmail = await input({
    message: brand("Admin email:"),
    validate: (v) =>
      v.includes("@") ? true : "Enter a valid email address",
  });

  const adminPassword = await password({
    message: brand("Admin password:"),
    mask: "•",
    validate: (v) =>
      v.length >= 8 ? true : "Password must be at least 8 characters",
  });

  success(`Admin account ${dim(adminEmail)}`);

  // ─── Generate Files ─────────────────────────────────────────────────
  section("Generating Configuration");

  const answers: WizardAnswers = {
    llmProvider,
    llmApiKey,
    llmBaseUrl,
    llmModel,
    embeddingProvider,
    embeddingApiKey,
    searchProvider,
    searchApiKey,
    searchModel,
    adminEmail,
    adminPassword,
  };

  const genSpinner = ora({ text: "Writing .env...", color: "magenta" }).start();
  const envPath = generateEnv(answers, projectRoot);
  genSpinner.succeed(ok(".env") + dim(` — ${envPath}`));

  const cfgSpinner = ora({
    text: "Writing config.docker.yaml...",
    color: "magenta",
  }).start();
  const configPath = generateConfig(answers, projectRoot);
  cfgSpinner.succeed(ok("config.docker.yaml") + dim(` — ${configPath}`));

  // Store admin credentials for the first-boot seed
  const seedPath = resolve(projectRoot, ".velkor-seed.json");
  writeFileSync(
    seedPath,
    JSON.stringify({
      email: adminEmail,
      password: adminPassword,
      display_name: "Admin",
    }),
    "utf-8"
  );
  success(`Admin seed ${dim("saved for first boot")}`);

  // ─── Docker Compose ─────────────────────────────────────────────────
  await offerDockerUp(projectRoot, answers);
}

// ---------------------------------------------------------------------------
// Offer to start services
// ---------------------------------------------------------------------------

async function offerDockerUp(
  projectRoot: string,
  answers?: WizardAnswers
) {
  section("Launch Services");

  const start = await confirm({
    message: brand("Start Velkor with docker compose up?"),
    default: true,
  });

  if (!start) {
    blank();
    bullet("Run " + info("docker compose up --build -d") + " when you're ready.");
    blank();
    return;
  }

  blank();

  // ── Step 1: Pull images ──────────────────────────────────────────────
  const pullSpinner = ora({
    text: "Pulling container images...",
    color: "magenta",
  }).start();

  try {
    execSync("docker compose pull --quiet 2>/dev/null", {
      cwd: projectRoot,
      stdio: "pipe",
      timeout: 300_000,
    });
    pullSpinner.succeed(ok("Images pulled"));
  } catch {
    pullSpinner.succeed(ok("Images") + dim(" — using cache / will pull during build"));
  }

  // ── Step 2: Build custom images ──────────────────────────────────────
  const buildOk = await runWithProgress(
    "Building Rust core, API gateway, and frontend",
    "docker compose build --progress quiet 2>&1",
    projectRoot,
    600_000, // 10 min — Rust compile is slow on first run
  );

  if (!buildOk) return;

  // ── Step 3: Start services ───────────────────────────────────────────
  const startSpinner = ora({
    text: "Starting services...",
    color: "magenta",
  }).start();

  try {
    execSync("docker compose up -d", {
      cwd: projectRoot,
      stdio: "pipe",
      timeout: 180_000,
    });
    startSpinner.succeed(ok("Containers started"));
  } catch (e: unknown) {
    startSpinner.fail(err("Failed to start containers"));
    showDockerError(e);
    return;
  }

  // ── Step 4: Wait for health ──────────────────────────────────────────
  const healthSpinner = ora({
    text: "Waiting for services to become healthy...",
    color: "magenta",
  }).start();

  const services = [
    { name: "PostgreSQL", check: "docker compose exec -T postgres pg_isready -U velkor" },
    { name: "Redis", check: "docker compose exec -T redis redis-cli ping" },
    { name: "Core API", check: "curl -sf http://localhost:3001/internal/health" },
    { name: "API Gateway", check: "curl -sf http://localhost:3000/health" },
  ];

  let allHealthy = true;
  for (const svc of services) {
    let up = false;
    for (let i = 0; i < 30; i++) {
      try {
        execSync(svc.check, { cwd: projectRoot, stdio: "pipe", timeout: 3000 });
        up = true;
        break;
      } catch {
        healthSpinner.text = `Waiting for ${svc.name}...`;
        await sleep(2000);
      }
    }
    if (!up) {
      allHealthy = false;
      healthSpinner.warn(brand(svc.name) + dim(" — not responding yet"));
      break;
    }
  }

  if (allHealthy) {
    healthSpinner.succeed(ok("All services healthy"));
  }

  // ── Step 5: Create admin user ────────────────────────────────────────
  if (answers?.adminEmail && answers?.adminPassword && allHealthy) {
    const adminSpinner = ora({
      text: "Creating admin user...",
      color: "magenta",
    }).start();

    try {
      execSync(
        `curl -sf -X POST http://localhost:3000/api/v1/auth/register ` +
          `-H "Content-Type: application/json" ` +
          `-d '${JSON.stringify({
            email: answers.adminEmail,
            password: answers.adminPassword,
            name: "Admin",
          })}'`,
        { stdio: "pipe", timeout: 10_000 }
      );
      adminSpinner.succeed(ok("Admin user created"));
    } catch {
      adminSpinner.warn(
        brand("Admin user") +
          dim(" — could not create (may already exist)")
      );
    }
  }

  // ─── Success Screen ─────────────────────────────────────────────────
  showSuccessScreen(answers);
}

// ---------------------------------------------------------------------------
// Run a long Docker command with a spinner + elapsed time, parse errors
// ---------------------------------------------------------------------------

async function runWithProgress(
  label: string,
  command: string,
  cwd: string,
  timeoutMs: number,
): Promise<boolean> {
  const spinner = ora({ text: label + "...", color: "magenta" }).start();
  const startTime = Date.now();

  // Update spinner with elapsed time
  const timer = setInterval(() => {
    const elapsed = Math.floor((Date.now() - startTime) / 1000);
    const min = Math.floor(elapsed / 60);
    const sec = elapsed % 60;
    const timeStr = min > 0 ? `${min}m ${sec}s` : `${sec}s`;
    spinner.text = `${label}... ${dim(`(${timeStr})`)}`;
  }, 1000);

  return new Promise((resolve) => {
    const proc = nodeSpawn("sh", ["-c", command], {
      cwd,
      stdio: ["ignore", "pipe", "pipe"],
    });

    let stdout = "";
    let stderr = "";

    proc.stdout?.on("data", (d: Buffer) => { stdout += d.toString(); });
    proc.stderr?.on("data", (d: Buffer) => { stderr += d.toString(); });

    const timeout = setTimeout(() => {
      proc.kill("SIGTERM");
      clearInterval(timer);
      spinner.fail(err("Build timed out"));
      failure(`Build exceeded ${Math.floor(timeoutMs / 60000)} minute limit`);
      resolve(false);
    }, timeoutMs);

    proc.on("close", (code) => {
      clearTimeout(timeout);
      clearInterval(timer);

      const elapsed = Math.floor((Date.now() - startTime) / 1000);
      const min = Math.floor(elapsed / 60);
      const sec = elapsed % 60;
      const timeStr = min > 0 ? `${min}m ${sec}s` : `${sec}s`;

      if (code === 0) {
        spinner.succeed(ok("Build complete") + dim(` (${timeStr})`));
        resolve(true);
      } else {
        spinner.fail(err("Build failed"));
        blank();
        showBuildError(stdout + stderr);
        resolve(false);
      }
    });
  });
}

// ---------------------------------------------------------------------------
// Smart error extraction from Docker build output
// ---------------------------------------------------------------------------

function showBuildError(output: string) {
  const lines = output.split("\n");

  // Try to find the most useful error information
  // Pattern 1: Rust compilation errors
  const rustError = lines.find((l) => l.includes("error[E") || l.includes("error: aborting"));
  if (rustError) {
    section("Rust Compilation Error");
    // Find all error lines and a few lines of context
    const errorLines = lines.filter(
      (l) =>
        l.includes("error") ||
        l.includes("requires rustc") ||
        l.includes("not found") ||
        l.includes("cannot find")
    ).slice(0, 10);
    for (const line of errorLines) {
      console.log(`  ${err("│")} ${line.trim()}`);
    }
    blank();
    bullet("Fix the error above, then run " + info("docker compose build") + " to retry.");
    return;
  }

  // Pattern 2: MSRV / rustc version mismatch
  const msrvError = lines.find((l) => l.includes("requires rustc"));
  if (msrvError) {
    section("Rust Version Error");
    const versionLines = lines.filter((l) => l.includes("requires rustc")).slice(0, 5);
    for (const line of versionLines) {
      console.log(`  ${err("│")} ${line.trim()}`);
    }
    blank();
    bullet("The Dockerfile Rust version needs to be bumped.");
    bullet("Update " + info("core/Dockerfile") + " to use a newer " + info("rust:<version>-bookworm") + " image.");
    return;
  }

  // Pattern 3: Dockerfile / build context errors
  const dockerError = lines.find(
    (l) => l.includes("failed to solve") || l.includes("COPY failed") || l.includes("not found in build context")
  );
  if (dockerError) {
    section("Docker Build Error");
    console.log(`  ${err("│")} ${dockerError.trim()}`);
    blank();
    bullet("Check your Dockerfile and ensure all referenced files exist.");
    return;
  }

  // Pattern 4: npm / node build errors
  const npmError = lines.find((l) => l.includes("npm error") || l.includes("ERR!"));
  if (npmError) {
    section("Node.js Build Error");
    const npmLines = lines
      .filter((l) => l.includes("npm error") || l.includes("ERR!") || l.includes("error TS"))
      .slice(0, 10);
    for (const line of npmLines) {
      console.log(`  ${err("│")} ${line.trim()}`);
    }
    blank();
    bullet("Check the TypeScript / npm error above.");
    return;
  }

  // Fallback: show the last meaningful lines
  section("Build Error");
  const meaningful = lines
    .filter((l) => l.trim().length > 0 && !l.startsWith("#") && !l.includes("Pulling") && !l.includes("Waiting"))
    .slice(-15);
  for (const line of meaningful) {
    console.log(`  ${err("│")} ${line.trim()}`);
  }
  blank();
  bullet("Run " + info("docker compose build 2>&1 | tail -50") + " for full output.");
}

function showDockerError(e: unknown) {
  const msg = e instanceof Error ? e.message : String(e);
  const lines = msg.split("\n").filter((l) => l.trim());
  // Show at most 10 lines
  for (const line of lines.slice(-10)) {
    console.log(`  ${err("│")} ${line.trim()}`);
  }
  blank();
  bullet("Run " + info("docker compose up") + " to see full output.");
}

function showSuccessScreen(answers?: WizardAnswers) {
  const lines = [
    accent("  Welcome to Velkor!  "),
    "",
    `${dim("Web UI:")}        ${info("http://localhost:8080")}`,
    `${dim("API Gateway:")}   ${info("http://localhost:3000")}`,
    `${dim("Core API:")}      ${info("http://localhost:3001")}`,
    `${dim("MinIO Console:")} ${info("http://localhost:9001")}`,
  ];

  if (answers?.adminEmail) {
    lines.push("");
    lines.push(`${dim("Admin email:")}   ${info(answers.adminEmail)}`);
    lines.push(`${dim("Admin pass:")}    ${dim("(as entered during setup)")}`);
  }

  lines.push("");
  lines.push(bright("Useful commands:"));
  lines.push(
    `  ${info("velkor start")}      ${dim("Start all services")}`
  );
  lines.push(
    `  ${info("velkor stop")}       ${dim("Stop all services")}`
  );
  lines.push(
    `  ${info("velkor restart")}    ${dim("Rebuild and restart")}`
  );
  lines.push(
    `  ${info("velkor status")}     ${dim("Check service health")}`
  );
  lines.push(
    `  ${info("velkor logs")}       ${dim("Tail logs (Ctrl+C to exit)")}`
  );
  lines.push(
    `  ${info("velkor update")}     ${dim("Pull latest and rebuild")}`
  );
  lines.push(
    `  ${info("velkor configure")}  ${dim("Change LLM, search, etc.")}`
  );

  successBox(lines);
}

// ---------------------------------------------------------------------------
// API key validation — lightweight check that the key is accepted
// ---------------------------------------------------------------------------

async function validateApiKey(
  provider: string,
  apiKey: string
): Promise<boolean> {
  try {
    let url: string;
    const headers: Record<string, string> = {
      Authorization: `Bearer ${apiKey}`,
    };

    switch (provider) {
      case "openrouter":
        url = "https://openrouter.ai/api/v1/auth/key";
        break;
      case "openai":
        url = "https://api.openai.com/v1/models?limit=1";
        break;
      case "anthropic":
        url = "https://api.anthropic.com/v1/models";
        delete headers.Authorization;
        headers["x-api-key"] = apiKey;
        headers["anthropic-version"] = "2023-06-01";
        break;
      default:
        return true;
    }

    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 8000);
    const res = await fetch(url, { headers, signal: controller.signal });
    clearTimeout(timeout);
    return res.ok;
  } catch {
    return false;
  }
}

// ---------------------------------------------------------------------------
// Fetch available models from provider API
// ---------------------------------------------------------------------------

interface ModelInfo {
  id: string;
  name: string;
  context?: number;
  pricing?: string;
}

async function fetchModels(
  provider: string,
  apiKey: string
): Promise<ModelInfo[]> {
  try {
    const headers: Record<string, string> = {};
    let url: string;

    switch (provider) {
      case "openrouter":
        url = "https://openrouter.ai/api/v1/models";
        headers.Authorization = `Bearer ${apiKey}`;
        break;
      case "openai":
        url = "https://api.openai.com/v1/models";
        headers.Authorization = `Bearer ${apiKey}`;
        break;
      case "anthropic":
        url = "https://api.anthropic.com/v1/models";
        headers["x-api-key"] = apiKey;
        headers["anthropic-version"] = "2023-06-01";
        break;
      default:
        return [];
    }

    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 15000);
    const res = await fetch(url, { headers, signal: controller.signal });
    clearTimeout(timeout);

    if (!res.ok) return [];
    const json = (await res.json()) as Record<string, unknown>;

    if (provider === "openrouter") {
      const data = (json.data ?? []) as Array<Record<string, unknown>>;
      return data
        .filter((m) => {
          // Filter to text/chat models — skip image gen, embedding, moderation
          const arch = m.architecture as Record<string, string> | undefined;
          const modality = arch?.modality ?? (m.type as string) ?? "";
          // Keep if modality includes text output or isn't specified
          if (modality && !modality.includes("text")) return false;
          // Skip models flagged as embedding or moderation
          const id = String(m.id);
          if (id.includes("embed") || id.includes("moderat")) return false;
          return true;
        })
        .map((m) => {
          const pricing = m.pricing as Record<string, string> | undefined;
          const prompt = pricing?.prompt ? parseFloat(pricing.prompt) : 0;
          const completion = pricing?.completion
            ? parseFloat(pricing.completion)
            : 0;
          // Format as $/M tokens
          const pricingStr =
            prompt > 0
              ? `$${(prompt * 1_000_000).toFixed(2)}/$${(completion * 1_000_000).toFixed(2)} per M tok`
              : "free";

          return {
            id: String(m.id),
            name: String(m.name ?? m.id),
            context: (m.context_length as number) ?? 0,
            pricing: pricingStr,
          };
        });
    }

    if (provider === "openai") {
      const data = (json.data ?? []) as Array<Record<string, unknown>>;
      return data
        .filter((m) => {
          const id = String(m.id);
          // Only show GPT/o-series chat models, skip embeddings, tts, whisper, dall-e
          return (
            id.startsWith("gpt-") ||
            id.startsWith("o1") ||
            id.startsWith("o3") ||
            id.startsWith("o4") ||
            id.startsWith("chatgpt-")
          );
        })
        .map((m) => ({
          id: String(m.id),
          name: String(m.id),
        }));
    }

    if (provider === "anthropic") {
      const data = (json.data ?? []) as Array<Record<string, unknown>>;
      return data.map((m) => ({
        id: String(m.id),
        name: String(m.display_name ?? m.id),
      }));
    }

    return [];
  } catch {
    return [];
  }
}

// ---------------------------------------------------------------------------
// Model selection — fetch live models from API, search to filter
// ---------------------------------------------------------------------------

async function selectModel(
  provider: string,
  apiKey: string
): Promise<string> {
  const spinner = ora({
    text: "Fetching available models...",
    color: "magenta",
  }).start();

  const models = await fetchModels(provider, apiKey);
  spinner.stop();

  if (models.length === 0) {
    // Fallback if API fetch fails — manual input
    bullet(dim("Could not fetch model list — enter model ID manually"));
    const hint =
      provider === "openrouter"
        ? "e.g. anthropic/claude-sonnet-4-20250514"
        : provider === "anthropic"
          ? "e.g. claude-sonnet-4-20250514"
          : "e.g. gpt-4o";
    return input({
      message: brand(`Model ID (${hint}):`),
      validate: (v: string) => (v.trim() ? true : "Model ID is required"),
    });
  }

  console.log(
    dim(`  ${models.length} models available — type to search\n`)
  );

  const selected = await search({
    message: brand("Default model?"),
    source: async (term) => {
      const q = (term ?? "").toLowerCase().trim();

      const filtered = q
        ? models.filter(
            (m) =>
              m.id.toLowerCase().includes(q) ||
              m.name.toLowerCase().includes(q)
          )
        : models.slice(0, 20); // Show first 20 when no search term

      return filtered.slice(0, 30).map((m) => {
        const ctx = m.context
          ? dim(` ${Math.floor(m.context / 1000)}k ctx`)
          : "";
        const price = m.pricing ? dim(` ${m.pricing}`) : "";
        return {
          name: `${bright(m.name)}${ctx}${price}`,
          value: m.id,
          description: m.id,
        };
      });
    },
  });

  return selected;
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}
