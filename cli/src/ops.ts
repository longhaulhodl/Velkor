/**
 * Operational commands — start, stop, restart, status, logs, update.
 *
 * These wrap Docker Compose so users never need to remember docker commands.
 */

import { execSync, spawn as nodeSpawn } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import ora from "ora";

import {
  banner,
  section,
  success,
  failure,
  bullet,
  blank,
  brand,
  dim,
  ok,
  err,
  info,
  bright,
  accent,
  successBox,
} from "./ui.js";

// ---------------------------------------------------------------------------
// velkor start
// ---------------------------------------------------------------------------

export async function runStart(root: string) {
  banner();
  section("Starting Services");

  if (!existsSync(resolve(root, ".env"))) {
    failure("No .env file found. Run " + info("velkor setup") + " first.");
    return;
  }

  const spinner = ora({ text: "Starting containers...", color: "magenta" }).start();

  try {
    execSync("docker compose up -d", { cwd: root, stdio: "pipe", timeout: 180_000 });
    spinner.succeed(ok("All containers started"));
  } catch (e) {
    spinner.fail(err("Failed to start containers"));
    const msg = e instanceof Error ? e.message : String(e);
    const lines = msg.split("\n").filter((l) => l.trim()).slice(-5);
    for (const line of lines) {
      console.log(`  ${err("│")} ${line.trim()}`);
    }
    blank();
    bullet("Run " + info("docker compose up") + " for full output.");
    return;
  }

  blank();
  await waitForHealth(root);
  showEndpoints();
}

// ---------------------------------------------------------------------------
// velkor stop
// ---------------------------------------------------------------------------

export async function runStop(root: string) {
  banner();
  section("Stopping Services");

  const spinner = ora({ text: "Stopping containers...", color: "magenta" }).start();

  try {
    execSync("docker compose down", { cwd: root, stdio: "pipe", timeout: 60_000 });
    spinner.succeed(ok("All containers stopped"));
  } catch {
    spinner.fail(err("Failed to stop containers"));
    bullet("Run " + info("docker compose down") + " manually.");
  }
  blank();
}

// ---------------------------------------------------------------------------
// velkor restart
// ---------------------------------------------------------------------------

export async function runRestart(root: string) {
  banner();
  section("Restarting Services");

  // Stop
  const stopSpinner = ora({ text: "Stopping containers...", color: "magenta" }).start();
  try {
    execSync("docker compose down", { cwd: root, stdio: "pipe", timeout: 60_000 });
    stopSpinner.succeed(ok("Containers stopped"));
  } catch {
    stopSpinner.warn(brand("Some containers may not have stopped cleanly"));
  }

  // Rebuild
  const buildOk = await runWithProgress(
    "Rebuilding services",
    "docker compose build --progress quiet 2>&1",
    root,
    600_000,
  );

  if (!buildOk) return;

  // Start
  const startSpinner = ora({ text: "Starting containers...", color: "magenta" }).start();
  try {
    execSync("docker compose up -d", { cwd: root, stdio: "pipe", timeout: 180_000 });
    startSpinner.succeed(ok("All containers started"));
  } catch {
    startSpinner.fail(err("Failed to start containers"));
    bullet("Run " + info("docker compose up") + " for full output.");
    return;
  }

  blank();
  await waitForHealth(root);
  showEndpoints();
}

// ---------------------------------------------------------------------------
// velkor status
// ---------------------------------------------------------------------------

export async function runStatus(root: string) {
  banner();
  section("Service Status");

  try {
    const output = execSync("docker compose ps --format json", {
      cwd: root,
      stdio: "pipe",
      timeout: 10_000,
    }).toString();

    // docker compose ps --format json outputs one JSON object per line
    const lines = output.trim().split("\n").filter((l) => l.trim());

    if (lines.length === 0) {
      failure("No Velkor containers are running.");
      blank();
      bullet("Run " + info("velkor start") + " to start services.");
      blank();
      return;
    }

    for (const line of lines) {
      try {
        const svc = JSON.parse(line);
        const name = svc.Service || svc.Name || "unknown";
        const state = (svc.State || svc.Status || "unknown").toLowerCase();
        const health = (svc.Health || "").toLowerCase();

        let icon: string;
        let label: string;

        if (state === "running" && (health === "healthy" || health === "")) {
          icon = ok("●");
          label = ok("running");
        } else if (state === "running" && health === "starting") {
          icon = brand("◐");
          label = brand("starting");
        } else if (state === "exited" || state === "dead") {
          icon = err("●");
          label = err(state);
        } else {
          icon = dim("○");
          label = dim(state);
        }

        const ports = svc.Ports || svc.Publishers || "";
        const portStr = typeof ports === "string" && ports
          ? dim(` (${ports})`)
          : "";

        console.log(`  ${icon} ${bright(name.padEnd(16))} ${label}${portStr}`);
      } catch {
        // Not valid JSON, print raw
        console.log(`  ${dim("○")} ${line.trim()}`);
      }
    }
  } catch {
    // Fallback: run docker compose ps normally
    try {
      const output = execSync("docker compose ps", {
        cwd: root,
        stdio: "pipe",
        timeout: 10_000,
      }).toString();
      console.log(output);
    } catch {
      failure("Could not get container status.");
      bullet("Is Docker running?");
    }
  }
  blank();
}

// ---------------------------------------------------------------------------
// velkor logs [service]
// ---------------------------------------------------------------------------

export async function runLogs(root: string, service?: string, lines = "50") {
  const args = ["compose", "logs", "-f", "--tail", lines];
  if (service) args.push(service);

  const proc = nodeSpawn("docker", args, {
    cwd: root,
    stdio: "inherit",
  });

  // Let the user Ctrl+C to exit
  process.on("SIGINT", () => {
    proc.kill("SIGINT");
    process.exit(0);
  });

  proc.on("close", (code) => {
    process.exit(code ?? 0);
  });
}

// ---------------------------------------------------------------------------
// velkor update
// ---------------------------------------------------------------------------

export async function runUpdate(root: string) {
  banner();
  section("Updating Velkor");

  // Step 1: Git pull
  const pullSpinner = ora({ text: "Pulling latest changes...", color: "magenta" }).start();
  try {
    const output = execSync("git pull", { cwd: root, stdio: "pipe", timeout: 30_000 }).toString();
    if (output.includes("Already up to date")) {
      pullSpinner.succeed(ok("Already up to date"));
    } else {
      pullSpinner.succeed(ok("Updated to latest version"));
    }
  } catch (e) {
    pullSpinner.fail(err("Failed to pull updates"));
    const msg = e instanceof Error ? e.message : String(e);
    console.log(`  ${err("│")} ${msg.split("\n")[0]}`);
    blank();
    bullet("Check your git remote and network connection.");
    return;
  }

  // Step 2: Rebuild CLI
  const cliSpinner = ora({ text: "Rebuilding CLI...", color: "magenta" }).start();
  try {
    execSync("npm install && npx tsc && chmod +x dist/index.js", {
      cwd: resolve(root, "cli"),
      stdio: "pipe",
      timeout: 60_000,
    });
    cliSpinner.succeed(ok("CLI rebuilt"));
  } catch {
    cliSpinner.warn(brand("CLI rebuild skipped") + dim(" — no changes or build error"));
  }

  // Step 3: Rebuild Docker images
  const buildOk = await runWithProgress(
    "Rebuilding containers",
    "docker compose build --progress quiet 2>&1",
    root,
    600_000,
  );

  if (!buildOk) return;

  // Step 4: Restart
  const restartSpinner = ora({ text: "Restarting services...", color: "magenta" }).start();
  try {
    execSync("docker compose up -d", { cwd: root, stdio: "pipe", timeout: 180_000 });
    restartSpinner.succeed(ok("Services restarted"));
  } catch {
    restartSpinner.fail(err("Failed to restart"));
    bullet("Run " + info("velkor restart") + " to retry.");
    return;
  }

  blank();
  await waitForHealth(root);

  successBox([
    accent("  Update complete!  "),
    "",
    dim("Run ") + info("velkor status") + dim(" to check services"),
    dim("Run ") + info("velkor logs") + dim(" to watch logs"),
  ]);
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

async function waitForHealth(root: string) {
  const spinner = ora({ text: "Checking service health...", color: "magenta" }).start();

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
        execSync(svc.check, { cwd: root, stdio: "pipe", timeout: 3000 });
        up = true;
        break;
      } catch {
        spinner.text = `Waiting for ${svc.name}...`;
        await sleep(2000);
      }
    }
    if (!up) {
      allHealthy = false;
      spinner.warn(brand(svc.name) + dim(" — not responding yet"));
      break;
    }
  }

  if (allHealthy) {
    spinner.succeed(ok("All services healthy"));
  }
}

function showEndpoints() {
  blank();
  successBox([
    accent("  Velkor is running!  "),
    "",
    `${dim("Web UI:")}        ${info("http://localhost:8080")}`,
    `${dim("API Gateway:")}   ${info("http://localhost:3000")}`,
    `${dim("Core API:")}      ${info("http://localhost:3001")}`,
    `${dim("MinIO Console:")} ${info("http://localhost:9001")}`,
    "",
    dim("Run ") + info("velkor logs") + dim(" to watch logs"),
    dim("Run ") + info("velkor stop") + dim(" to shut down"),
  ]);
}

async function runWithProgress(
  label: string,
  command: string,
  cwd: string,
  timeoutMs: number,
): Promise<boolean> {
  const spinner = ora({ text: label + "...", color: "magenta" }).start();
  const startTime = Date.now();

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

function showBuildError(output: string) {
  const lines = output.split("\n");

  const rustError = lines.find((l) => l.includes("error[E") || l.includes("error: aborting"));
  if (rustError) {
    section("Rust Compilation Error");
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
    bullet("Fix the error above, then run " + info("velkor restart") + " to retry.");
    return;
  }

  const msrvError = lines.find((l) => l.includes("requires rustc"));
  if (msrvError) {
    section("Rust Version Error");
    const versionLines = lines.filter((l) => l.includes("requires rustc")).slice(0, 5);
    for (const line of versionLines) {
      console.log(`  ${err("│")} ${line.trim()}`);
    }
    blank();
    bullet("The Dockerfile Rust version needs to be bumped.");
    return;
  }

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

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}
