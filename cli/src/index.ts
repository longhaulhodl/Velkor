#!/usr/bin/env node

/**
 * Velkor CLI — setup, operations, and configuration management.
 *
 * Usage:
 *   velkor setup                       Full interactive setup wizard
 *   velkor start                       Start all services
 *   velkor stop                        Stop all services
 *   velkor restart                     Restart all services (rebuild if needed)
 *   velkor status                      Show service status
 *   velkor logs [service]              Tail logs (optionally for one service)
 *   velkor update                      Pull latest version and rebuild
 *   velkor configure                   Reconfigure a section interactively
 *   velkor configure --section llm     Reconfigure a specific section
 */

import { Command } from "commander";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { execSync, spawn as nodeSpawn } from "node:child_process";

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

function ensureProject(root: string) {
  if (!existsSync(resolve(root, "docker-compose.yml"))) {
    console.error(
      "Could not find docker-compose.yml. Run this from your Velkor directory or run 'velkor setup' first."
    );
    process.exit(1);
  }
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
  .command("start")
  .description("Start all Velkor services")
  .action(async () => {
    const root = findProjectRoot();
    ensureProject(root);
    const { runStart } = await import("./ops.js");
    await runStart(root);
  });

program
  .command("stop")
  .description("Stop all Velkor services")
  .action(async () => {
    const root = findProjectRoot();
    ensureProject(root);
    const { runStop } = await import("./ops.js");
    await runStop(root);
  });

program
  .command("restart")
  .description("Rebuild and restart all services")
  .action(async () => {
    const root = findProjectRoot();
    ensureProject(root);
    const { runRestart } = await import("./ops.js");
    await runRestart(root);
  });

program
  .command("status")
  .description("Show status of all Velkor services")
  .action(async () => {
    const root = findProjectRoot();
    ensureProject(root);
    const { runStatus } = await import("./ops.js");
    await runStatus(root);
  });

program
  .command("logs [service]")
  .description("Tail service logs (optionally for a specific service)")
  .option("-n, --lines <n>", "Number of lines to show", "50")
  .action(async (service: string | undefined, opts) => {
    const root = findProjectRoot();
    ensureProject(root);
    const { runLogs } = await import("./ops.js");
    await runLogs(root, service, opts.lines);
  });

program
  .command("update")
  .description("Pull the latest version and rebuild")
  .action(async () => {
    const root = findProjectRoot();
    ensureProject(root);
    const { runUpdate } = await import("./ops.js");
    await runUpdate(root);
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
