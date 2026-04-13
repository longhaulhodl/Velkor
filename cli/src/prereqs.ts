/**
 * Prerequisite checks — Docker, ports, disk space.
 */

import { execSync } from "node:child_process";
import { createConnection } from "node:net";
import ora from "ora";
import { brand, ok, err, dim, success, failure } from "./ui.js";

export interface PrereqResult {
  docker: boolean;
  compose: boolean;
  ports: { port: number; available: boolean }[];
}

export async function checkPrerequisites(): Promise<PrereqResult> {
  const result: PrereqResult = {
    docker: false,
    compose: false,
    ports: [],
  };

  // Docker
  const dockerSpinner = ora({
    text: "Checking Docker...",
    color: "magenta",
  }).start();

  try {
    execSync("docker info", { stdio: "pipe" });
    result.docker = true;
    dockerSpinner.succeed(ok("Docker") + dim(" — running"));
  } catch {
    result.docker = false;
    dockerSpinner.fail(err("Docker") + dim(" — not found or not running"));
  }

  // Docker Compose
  const composeSpinner = ora({
    text: "Checking Docker Compose...",
    color: "magenta",
  }).start();

  try {
    const version = execSync("docker compose version --short", {
      stdio: "pipe",
    })
      .toString()
      .trim();
    result.compose = true;
    composeSpinner.succeed(
      ok("Docker Compose") + dim(` — v${version}`)
    );
  } catch {
    result.compose = false;
    composeSpinner.fail(
      err("Docker Compose") + dim(" — not found")
    );
  }

  // Required ports
  const portsToCheck = [
    { port: 5432, name: "PostgreSQL" },
    { port: 6379, name: "Redis" },
    { port: 9000, name: "MinIO" },
    { port: 3001, name: "Core API" },
    { port: 3000, name: "API Gateway" },
    { port: 8080, name: "Web UI" },
  ];

  const portSpinner = ora({
    text: "Checking port availability...",
    color: "magenta",
  }).start();

  const portResults: { port: number; available: boolean }[] = [];
  const busy: string[] = [];

  for (const { port, name } of portsToCheck) {
    const available = await isPortAvailable(port);
    portResults.push({ port, available });
    if (!available) {
      busy.push(`${port} (${name})`);
    }
  }

  result.ports = portResults;

  if (busy.length === 0) {
    portSpinner.succeed(
      ok("Ports") + dim(" — all required ports available")
    );
  } else {
    portSpinner.warn(
      brand("Ports") +
        dim(` — in use: ${busy.join(", ")}`)
    );
  }

  return result;
}

function isPortAvailable(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const conn = createConnection({ port, host: "127.0.0.1" });
    conn.once("connect", () => {
      conn.destroy();
      resolve(false); // something is listening
    });
    conn.once("error", () => {
      resolve(true); // nothing there — port is free
    });
    conn.setTimeout(500, () => {
      conn.destroy();
      resolve(true);
    });
  });
}

export function prereqsPassed(result: PrereqResult): boolean {
  return result.docker && result.compose;
}
