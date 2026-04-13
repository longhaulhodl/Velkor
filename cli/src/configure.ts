/**
 * `velkor configure` — reconfigure individual sections without redoing full setup.
 *
 * Usage:
 *   velkor configure              — interactive section picker
 *   velkor configure --section llm
 *   velkor configure --section embeddings
 *   velkor configure --section web-search
 */

import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { resolve } from "node:path";
import { select, search, input, password } from "@inquirer/prompts";
import ora from "ora";
import { parse, stringify } from "yaml";

import {
  banner,
  section,
  success,
  failure,
  skip,
  bullet,
  blank,
  brand,
  dim,
  ok,
  info,
  bright,
} from "./ui.js";

type SectionName = "llm" | "embeddings" | "web-search";

const SECTIONS: { name: string; value: SectionName; description: string }[] = [
  {
    name: "LLM Provider",
    value: "llm",
    description: "Change the language model provider and API key",
  },
  {
    name: "Embeddings",
    value: "embeddings",
    description: "Change the embedding provider for vector search",
  },
  {
    name: "Web Search",
    value: "web-search",
    description: "Change the web search provider",
  },
];

export async function runConfigure(
  projectRoot: string,
  sectionArg?: string
) {
  banner();

  const envPath = resolve(projectRoot, ".env");
  const configPath = resolve(projectRoot, "config.docker.yaml");

  if (!existsSync(envPath) || !existsSync(configPath)) {
    failure(
      "No existing configuration found. Run " +
        info("velkor setup") +
        " first."
    );
    process.exit(1);
  }

  let envLines = readFileSync(envPath, "utf-8").split("\n");
  const configRaw = readFileSync(configPath, "utf-8");
  const config = parse(configRaw) as Record<string, unknown>;

  // Pick section
  let sectionName: SectionName;

  if (sectionArg && SECTIONS.some((s) => s.value === sectionArg)) {
    sectionName = sectionArg as SectionName;
  } else {
    sectionName = await select({
      message: brand("Which section do you want to reconfigure?"),
      choices: SECTIONS.map((s) => ({
        name: `${bright(s.name)} ${dim("— " + s.description)}`,
        value: s.value,
      })),
    });
  }

  let result: { envLines: string[]; config: Record<string, unknown> };

  switch (sectionName) {
    case "llm":
      result = await configureLlm(envLines, config);
      break;
    case "embeddings":
      result = await configureEmbeddings(envLines, config);
      break;
    case "web-search":
      result = await configureWebSearch(envLines, config);
      break;
  }

  envLines = result.envLines;
  Object.assign(config, result.config);

  // Write back
  writeFileSync(envPath, envLines.join("\n"), "utf-8");
  writeFileSync(configPath, stringify(config, { lineWidth: 120 }), "utf-8");

  blank();
  success(`Configuration updated.`);
  bullet(
    "Run " +
      info("velkor restart") +
      " to apply changes."
  );
  blank();
}

// ---------------------------------------------------------------------------
// Section: LLM
// ---------------------------------------------------------------------------

async function configureLlm(
  envLines: string[],
  config: Record<string, unknown>
) {
  section("LLM Provider");

  const provider = await select({
    message: brand("Which LLM provider?"),
    choices: [
      {
        name: `${bright("OpenRouter")} ${dim("— 200+ models")}`,
        value: "openrouter",
      },
      {
        name: `${bright("Anthropic")} ${dim("— Claude direct")}`,
        value: "anthropic",
      },
      {
        name: `${bright("OpenAI")} ${dim("— GPT-4o, o3")}`,
        value: "openai",
      },
      {
        name: `${bright("Ollama")} ${dim("— local, no key")}`,
        value: "ollama",
      },
      {
        name: `${bright("Custom endpoint")}`,
        value: "custom",
      },
    ],
  });

  // Clear only the LLM provider key being replaced — preserve keys used by
  // other subsystems (e.g. OPENAI_API_KEY used for embeddings).
  const llmKeyMap: Record<string, string> = {
    openrouter: "OPENROUTER_API_KEY",
    anthropic: "ANTHROPIC_API_KEY",
    openai: "OPENAI_API_KEY",
    custom: "CUSTOM_LLM_API_KEY",
  };
  // Remove the key for the newly selected provider (we'll re-add it below)
  const newKeyName = llmKeyMap[provider];
  if (newKeyName) {
    envLines = envLines.filter((l) => !l.startsWith(`${newKeyName}=`));
  }
  // Also remove keys for providers we're no longer using as LLM providers,
  // BUT only if they aren't needed for embeddings
  const mem = config.memory as Record<string, unknown> | undefined;
  const embeddingModel = String(mem?.embedding_model ?? "");
  for (const [prov, keyName] of Object.entries(llmKeyMap)) {
    if (prov === provider) continue; // already handled above
    if (prov === "openai" && embeddingModel.startsWith("openai/")) continue;
    if (prov === "ollama") continue; // no key to remove
    // Don't remove keys that may still be needed
    // Only remove if this provider was the *previous* LLM provider
    const existingProviders = (config.providers ?? {}) as Record<string, Record<string, string>>;
    if (existingProviders[prov] && !embeddingModel.startsWith(`${prov}/`)) {
      envLines = envLines.filter((l) => !l.startsWith(`${keyName}=`));
    }
  }

  const providers: Record<string, Record<string, string>> = {};

  if (provider === "ollama") {
    const baseUrl = await input({
      message: brand("Ollama URL:"),
      default: "http://localhost:11434",
    });
    const model = await input({
      message: brand("Default model:"),
      default: "llama3.1:8b",
    });
    providers.ollama = {
      base_url: baseUrl,
      default_model: model,
    };
    success(`Ollama at ${info(baseUrl)}`);
  } else if (provider === "custom") {
    const baseUrl = await input({
      message: brand("Base URL:"),
    });
    const apiKey = await password({
      message: brand("API key:"),
      mask: "•",
    });
    const model = await input({
      message: brand("Default model:"),
    });
    envLines.push(`CUSTOM_LLM_API_KEY=${apiKey}`);
    providers.custom = {
      api_key: "${CUSTOM_LLM_API_KEY}",
      base_url: baseUrl,
      default_model: model,
    };
    success(`Custom endpoint at ${info(baseUrl)}`);
  } else {
    const keyNames: Record<string, string> = {
      openrouter: "OPENROUTER_API_KEY",
      anthropic: "ANTHROPIC_API_KEY",
      openai: "OPENAI_API_KEY",
    };
    const envVar = keyNames[provider];
    const apiKey = await password({
      message: brand(`${provider} API key:`),
      mask: "•",
    });
    envLines.push(`${envVar}=${apiKey}`);

    // Validate key
    const valid = await validateApiKey(provider, apiKey);
    if (valid) {
      success(`API key ${dim("verified ✓")}`);
    } else {
      failure(`Could not verify key — check that it's correct`);
    }

    // Model selection
    const model = await selectModel(provider, apiKey);
    success(`Model: ${info(model)}`);

    providers[provider] = {
      api_key: `\${${envVar}}`,
      default_model: model,
    };
  }

  // Merge — keep existing embedding provider in providers if present
  const existing = (config.providers ?? {}) as Record<
    string,
    Record<string, string>
  >;
  // Preserve ollama/openai if they were embedding-only
  if (existing.openai && !providers.openai) {
    // check if it was used for embeddings
    const mem = config.memory as Record<string, unknown> | undefined;
    if (
      mem?.embedding_model &&
      String(mem.embedding_model).startsWith("openai/")
    ) {
      providers.openai = existing.openai;
    }
  }
  if (existing.ollama && !providers.ollama) {
    const mem = config.memory as Record<string, unknown> | undefined;
    if (
      mem?.embedding_model &&
      String(mem.embedding_model).startsWith("ollama/")
    ) {
      providers.ollama = existing.ollama;
    }
  }

  config.providers = providers;

  // Update fallback chain with the new default model
  const defaultModel = Object.values(providers).find(
    (p) => p.default_model
  )?.default_model;
  if (defaultModel) {
    const routing = (config.routing ?? {}) as Record<string, unknown>;
    routing.fallback_chain = [defaultModel];
    config.routing = routing;
  }

  return { envLines, config };
}

// ---------------------------------------------------------------------------
// Section: Embeddings
// ---------------------------------------------------------------------------

async function configureEmbeddings(
  envLines: string[],
  config: Record<string, unknown>
) {
  section("Embedding Provider");

  const provider = await select({
    message: brand("Embedding provider?"),
    choices: [
      {
        name: `${bright("OpenAI")} ${dim("— text-embedding-3-small")}`,
        value: "openai",
      },
      {
        name: `${bright("Ollama")} ${dim("— nomic-embed-text, local")}`,
        value: "ollama",
      },
      {
        name: `${dim("Disable")} ${dim("— FTS-only")}`,
        value: "skip",
      },
    ],
  });

  const memory = (config.memory ?? {}) as Record<string, unknown>;
  const providers = (config.providers ?? {}) as Record<
    string,
    Record<string, string>
  >;

  if (provider === "openai") {
    // Check if we already have an openai provider entry
    if (!providers.openai) {
      const apiKey = await password({
        message: brand("OpenAI API key:"),
        mask: "•",
      });
      envLines.push(`OPENAI_API_KEY=${apiKey}`);
      providers.openai = {
        api_key: "${OPENAI_API_KEY}",
        default_model: "gpt-4o",
      };
    }
    memory.embedding_model = "openai/text-embedding-3-small";
    memory.embedding_dimensions = 1536;
    success("OpenAI embeddings " + dim("configured"));
  } else if (provider === "ollama") {
    if (!providers.ollama) {
      const baseUrl = await input({
        message: brand("Ollama URL:"),
        default: "http://localhost:11434",
      });
      providers.ollama = {
        base_url: baseUrl,
        default_model: "llama3.1:8b",
      };
    }
    memory.embedding_model = "ollama/nomic-embed-text";
    memory.embedding_dimensions = 768;
    success("Ollama embeddings " + dim("(nomic-embed-text)"));
  } else {
    memory.embedding_model = "none";
    memory.embedding_dimensions = 0;
    skip("Embeddings disabled — FTS-only search");
  }

  config.memory = memory;
  config.providers = providers;

  return { envLines, config };
}

// ---------------------------------------------------------------------------
// Section: Web Search
// ---------------------------------------------------------------------------

async function configureWebSearch(
  envLines: string[],
  config: Record<string, unknown>
) {
  section("Web Search");

  const provider = await select({
    message: brand("Search provider?"),
    choices: [
      {
        name: `${bright("Perplexity")} ${dim("— AI-synthesized answers")}`,
        value: "perplexity",
      },
      {
        name: `${bright("Tavily")} ${dim("— purpose-built search API")}`,
        value: "tavily",
      },
      {
        name: `${bright("Brave Search")} ${dim("— privacy-focused")}`,
        value: "brave",
      },
      {
        name: `${bright("DuckDuckGo")} ${dim("— free, no key")}`,
        value: "duckduckgo",
      },
      {
        name: `${dim("Disable")} ${dim("— no web search")}`,
        value: "none",
      },
    ],
  });

  // Clean old search keys
  envLines = envLines.filter(
    (l) =>
      !l.startsWith("TAVILY_API_KEY=") &&
      !l.startsWith("BRAVE_API_KEY=") &&
      !l.startsWith("SERPER_API_KEY=") &&
      !l.startsWith("PERPLEXITY_API_KEY=")
  );

  const tools = (config.tools ?? {}) as Record<string, unknown>;
  const webSearch: Record<string, unknown> = {
    provider,
    max_results: 5,
  };

  if (provider === "perplexity") {
    const apiKey = await password({
      message: brand("Perplexity API key (pplx-... or sk-or-...):"),
      mask: "•",
    });
    envLines.push(`PERPLEXITY_API_KEY=${apiKey}`);

    const searchModel = await select({
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
    webSearch.perplexity = { api_key: "${PERPLEXITY_API_KEY:-}", model: searchModel };
    success(`Perplexity ${info(searchModel)} ${dim("configured")}`);
  } else if (provider === "tavily") {
    const apiKey = await password({
      message: brand("Tavily API key:"),
      mask: "•",
    });
    envLines.push(`TAVILY_API_KEY=${apiKey}`);
    webSearch.tavily_api_key = "${TAVILY_API_KEY:-}";
    success("Tavily " + dim("configured"));
  } else if (provider === "brave") {
    const apiKey = await password({
      message: brand("Brave Search API key:"),
      mask: "•",
    });
    envLines.push(`BRAVE_API_KEY=${apiKey}`);
    webSearch.brave_api_key = "${BRAVE_API_KEY:-}";
    success("Brave Search " + dim("configured"));
  } else if (provider === "duckduckgo") {
    success("DuckDuckGo " + dim("— no key needed"));
  } else {
    skip("Web search disabled");
  }

  tools.web_search = webSearch;
  config.tools = tools;

  return { envLines, config };
}

// ---------------------------------------------------------------------------
// API key validation
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
          const arch = m.architecture as Record<string, string> | undefined;
          const modality = arch?.modality ?? (m.type as string) ?? "";
          if (modality && !modality.includes("text")) return false;
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
// Model selection — fetch live models, search to filter
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
        : models.slice(0, 20);

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
