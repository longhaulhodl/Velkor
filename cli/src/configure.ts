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
import { select, input, password } from "@inquirer/prompts";
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

  // Clear old provider keys
  envLines = envLines.filter(
    (l) =>
      !l.startsWith("OPENROUTER_API_KEY=") &&
      !l.startsWith("ANTHROPIC_API_KEY=") &&
      !l.startsWith("OPENAI_API_KEY=") &&
      !l.startsWith("CUSTOM_LLM_API_KEY=")
  );

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

    const defaults: Record<string, string> = {
      openrouter: "anthropic/claude-sonnet-4-20250514",
      anthropic: "claude-sonnet-4-20250514",
      openai: "gpt-4o",
    };
    providers[provider] = {
      api_key: `\${${envVar}}`,
      default_model: defaults[provider],
    };
    success(`${provider} ${dim("configured")}`);
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
    webSearch.perplexity = { api_key: "${PERPLEXITY_API_KEY:-}" };
    success("Perplexity " + dim("configured"));
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
