/**
 * Branded terminal UI primitives for the Velkor CLI.
 *
 * Color scheme: violet primary, amber accent, dim gray for secondary text.
 */

import chalk from "chalk";

// ---------------------------------------------------------------------------
// Brand colors
// ---------------------------------------------------------------------------

/** Primary brand — titles, highlights, key information */
export const brand = chalk.hex("#A855F7"); // violet-500
/** Accent — success indicators, calls-to-action */
export const accent = chalk.hex("#F59E0B"); // amber-500
/** Muted text — secondary info, hints */
export const dim = chalk.gray;
/** Success — checkmarks, completion */
export const ok = chalk.hex("#22C55E"); // green-500
/** Error — failures, warnings */
export const err = chalk.hex("#EF4444"); // red-500
/** Info — URLs, values the user entered */
export const info = chalk.hex("#38BDF8"); // sky-400
/** Bold white — section content emphasis */
export const bright = chalk.white.bold;

// ---------------------------------------------------------------------------
// ASCII Art Banner
// ---------------------------------------------------------------------------

const LOGO_RAW = `
 ██╗   ██╗███████╗██╗     ██╗  ██╗ ██████╗ ██████╗
 ██║   ██║██╔════╝██║     ██║ ██╔╝██╔═══██╗██╔══██╗
 ██║   ██║█████╗  ██║     █████╔╝ ██║   ██║██████╔╝
 ╚██╗ ██╔╝██╔══╝  ██║     ██╔═██╗ ██║   ██║██╔══██╗
  ╚████╔╝ ███████╗███████╗██║  ██╗╚██████╔╝██║  ██║
   ╚═══╝  ╚══════╝╚══════╝╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝`;

export function banner() {
  console.log(brand(LOGO_RAW));
  console.log();
  console.log(
    dim("  Self-hosted multi-agent orchestration platform")
  );
  console.log(dim("  v0.1.0"));
  console.log();
}

// ---------------------------------------------------------------------------
// Box-drawn section headers
// ---------------------------------------------------------------------------

export function section(title: string) {
  const line = "─".repeat(title.length + 2);
  console.log();
  console.log(brand(`┌${line}┐`));
  console.log(brand(`│ ${bright(title)} ${brand("│")}`));
  console.log(brand(`└${line}┘`));
  console.log();
}

// ---------------------------------------------------------------------------
// Status lines
// ---------------------------------------------------------------------------

export function success(msg: string) {
  console.log(`  ${ok("✔")} ${msg}`);
}

export function failure(msg: string) {
  console.log(`  ${err("✖")} ${msg}`);
}

export function skip(msg: string) {
  console.log(`  ${dim("○")} ${dim(msg)}`);
}

export function bullet(msg: string) {
  console.log(`  ${brand("▸")} ${msg}`);
}

export function blank() {
  console.log();
}

// ---------------------------------------------------------------------------
// Final success box
// ---------------------------------------------------------------------------

export function successBox(lines: string[]) {
  const maxLen = Math.max(...lines.map((l) => stripAnsi(l).length));
  const pad = (s: string) => s + " ".repeat(maxLen - stripAnsi(s).length);

  console.log();
  console.log(ok(`  ╔${"═".repeat(maxLen + 2)}╗`));
  for (const line of lines) {
    console.log(ok(`  ║ `) + pad(line) + ok(` ║`));
  }
  console.log(ok(`  ╚${"═".repeat(maxLen + 2)}╝`));
  console.log();
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

// eslint-disable-next-line no-control-regex
const ANSI_RE = /\x1B\[[0-9;]*m/g;
function stripAnsi(s: string): string {
  return s.replace(ANSI_RE, "");
}

export function keyValue(key: string, value: string) {
  console.log(`  ${dim(key + ":")} ${info(value)}`);
}
