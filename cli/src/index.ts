#!/usr/bin/env node

/**
 * Velkor CLI — setup wizard and configuration management.
 *
 * Usage:
 *   velkor setup                       Full interactive setup wizard
 *   velkor configure                   Reconfigure a section interactively
 *   velkor configure --section llm     Reconfigure a specific section
 */

import { Command } from "commander";
import { existsSync } from "node:fs";
import { resolve } from "node:path";

// Resolve project root — find the directory containing docker-compose.yml
function findProjectRoot(): string {
  const cwd = process.cwd();
  if (existsSync(resolve(cwd, "docker-compose.yml"))) {
    return cwd;
  }
  const parent = resolve(cwd, "..");
  if (existsSync(resolve(parent, "docker-compose.yml"))) {
    return parent;
  }
  return cwd;
}

const program = new Command();

program
  .name("velkor")
  .description("Velkor — self-hosted multi-agent orchestration platform")
  .version("0.1.0");

program
  .command("setup")
  .description("Interactive setup wizard — configure and launch Velkor")
  .action(async () => {
    const { runSetup } = await import("./setup.js");
    await runSetup(findProjectRoot());
  });

program
  .command("configure")
  .description("Reconfigure an individual section")
  .option(
    "-s, --section <section>",
    "Section to reconfigure (llm, embeddings, web-search)"
  )
  .action(async (opts) => {
    const { runConfigure } = await import("./configure.js");
    await runConfigure(findProjectRoot(), opts.section);
  });

program.parse();
