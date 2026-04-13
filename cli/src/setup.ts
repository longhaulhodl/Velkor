/**
 * Interactive setup wizard — the heart of `velkor setup`.
 */

import { execSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  select,
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
    success(`${keyLabel} ${dim("configured")}`);
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

  if (searchProvider === "perplexity") {
    // Can use OpenRouter key or Perplexity direct key
    if (llmProvider === "openrouter" && llmApiKey) {
      const reuse = await confirm({
        message: brand("Use your OpenRouter key for Perplexity search?"),
        default: true,
      });
      if (reuse) {
        searchApiKey = llmApiKey;
        success(`Perplexity via OpenRouter ${dim("(perplexity/sonar-pro)")}`);
      } else {
        searchApiKey = await password({
          message: brand("Perplexity API key (pplx-...):"),
          mask: "•",
        });
        success(`Perplexity direct ${dim("configured")}`);
      }
    } else {
      searchApiKey = await password({
        message: brand(
          "Perplexity API key (pplx-...) or OpenRouter key (sk-or-...):"
        ),
        mask: "•",
      });
      success(`Perplexity ${dim("configured")}`);
    }
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
  const buildSpinner = ora({
    text: "Building and starting services (this may take a few minutes on first run)...",
    color: "magenta",
  }).start();

  try {
    execSync("docker compose up --build -d", {
      cwd: projectRoot,
      stdio: "pipe",
      timeout: 600_000, // 10 min
    });
    buildSpinner.succeed(ok("All services started"));
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e);
    buildSpinner.fail(err("Failed to start services"));
    failure(msg);
    bullet("Try running " + info("docker compose up --build") + " manually to see full output.");
    return;
  }

  // Wait for API health
  const healthSpinner = ora({
    text: "Waiting for API gateway...",
    color: "magenta",
  }).start();

  let healthy = false;
  for (let i = 0; i < 30; i++) {
    try {
      execSync("curl -sf http://localhost:3000/health", {
        stdio: "pipe",
        timeout: 3000,
      });
      healthy = true;
      break;
    } catch {
      await sleep(2000);
    }
  }

  if (healthy) {
    healthSpinner.succeed(ok("API gateway") + dim(" — healthy"));
  } else {
    healthSpinner.warn(
      brand("API gateway") +
        dim(" — not responding yet, may still be starting")
    );
  }

  // Create admin user if we have credentials
  if (answers?.adminEmail && answers?.adminPassword && healthy) {
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
            display_name: "Admin",
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
  lines.push(
    dim("Run ") +
      info("docker compose logs -f") +
      dim(" to watch logs")
  );
  lines.push(
    dim("Run ") +
      info("velkor configure") +
      dim(" to change settings")
  );

  successBox(lines);
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}
