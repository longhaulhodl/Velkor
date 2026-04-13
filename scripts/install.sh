#!/usr/bin/env bash
# =============================================================================
# Velkor — One-line installer
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/longhaulhodl/Velkor/main/scripts/install.sh | bash
#
# Custom install directory:
#   export VELKOR_DIR=~/my-velkor && curl -fsSL https://raw.githubusercontent.com/longhaulhodl/Velkor/main/scripts/install.sh | bash
#
# What it does:
#   1. Checks prerequisites (git, node, docker)
#   2. Clones the repo (or pulls if already cloned)
#   3. Installs CLI dependencies
#   4. Launches the interactive setup wizard
# =============================================================================

set -euo pipefail

# ---------------------------------------------------------------------------
# Colors (matching the Velkor brand: violet primary, amber accent)
# ---------------------------------------------------------------------------

VIOLET=$'\033[38;5;135m'
AMBER=$'\033[38;5;214m'
GREEN=$'\033[38;5;77m'
RED=$'\033[38;5;203m'
DIM=$'\033[2m'
BOLD=$'\033[1m'
RESET=$'\033[0m'

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

info()    { printf "${VIOLET}▸${RESET} %s\n" "$1"; }
ok()      { printf "${GREEN}✔${RESET} %s\n" "$1"; }
warn()    { printf "${AMBER}!${RESET} %s\n" "$1"; }
fail()    { printf "${RED}✖${RESET} %s\n" "$1"; exit 1; }
dim()     { printf "${DIM}  %s${RESET}\n" "$1"; }
banner_line() { printf "${VIOLET}%s${RESET}\n" "$1"; }

# ---------------------------------------------------------------------------
# Banner
# ---------------------------------------------------------------------------

print_banner() {
  echo ""
  banner_line " ██╗   ██╗███████╗██╗     ██╗  ██╗ ██████╗ ██████╗ "
  banner_line " ██║   ██║██╔════╝██║     ██║ ██╔╝██╔═══██╗██╔══██╗"
  banner_line " ██║   ██║█████╗  ██║     █████╔╝ ██║   ██║██████╔╝"
  banner_line " ╚██╗ ██╔╝██╔══╝  ██║     ██╔═██╗ ██║   ██║██╔══██╗"
  banner_line "  ╚████╔╝ ███████╗███████╗██║  ██╗╚██████╔╝██║  ██║"
  banner_line "   ╚═══╝  ╚══════╝╚══════╝╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝"
  echo ""
  dim "Self-hosted multi-agent orchestration platform"
  dim "Installer v0.1.0"
  echo ""
}

# ---------------------------------------------------------------------------
# Prerequisite checks
# ---------------------------------------------------------------------------

check_command() {
  if command -v "$1" &>/dev/null; then
    ok "$1 found $(dim_version "$@")"
    return 0
  else
    return 1
  fi
}

dim_version() {
  local ver=""
  case "$1" in
    node)  ver="$(node --version 2>/dev/null || true)" ;;
    git)   ver="$(git --version 2>/dev/null | awk '{print $3}' || true)" ;;
    docker) ver="$(docker --version 2>/dev/null | awk '{print $3}' | tr -d ',' || true)" ;;
  esac
  [ -n "$ver" ] && printf "${DIM}(%s)${RESET}" "$ver"
}

check_prerequisites() {
  info "Checking prerequisites..."
  echo ""

  # Git
  if ! check_command git; then
    fail "git is required. Install it from https://git-scm.com/"
  fi

  # Node.js
  if ! check_command node; then
    fail "Node.js is required (v18+). Install it from https://nodejs.org/"
  fi

  # Check Node version >= 18
  NODE_MAJOR=$(node -e "console.log(process.versions.node.split('.')[0])")
  if [ "$NODE_MAJOR" -lt 18 ]; then
    fail "Node.js v18+ required, found v$(node --version). Update at https://nodejs.org/"
  fi

  # npm
  if ! check_command npm; then
    fail "npm is required (ships with Node.js)"
  fi

  # Docker
  if ! check_command docker; then
    fail "Docker is required. Install it from https://docs.docker.com/get-docker/"
  fi

  # Docker Compose
  if docker compose version &>/dev/null; then
    local compose_ver
    compose_ver=$(docker compose version --short 2>/dev/null || echo "unknown")
    ok "docker compose found ${DIM}(${compose_ver})${RESET}"
  else
    fail "Docker Compose is required. It ships with Docker Desktop, or install the plugin: https://docs.docker.com/compose/install/"
  fi

  # Docker running?
  if ! docker info &>/dev/null; then
    fail "Docker is installed but not running. Start Docker Desktop or the Docker daemon."
  fi

  echo ""
  ok "All prerequisites met"
  echo ""
}

# ---------------------------------------------------------------------------
# Clone or update
# ---------------------------------------------------------------------------

REPO_URL="https://github.com/longhaulhodl/Velkor.git"
INSTALL_DIR="${VELKOR_DIR:-$HOME/velkor}"

clone_repo() {
  if [ -d "$INSTALL_DIR/.git" ]; then
    info "Existing installation found at ${AMBER}${INSTALL_DIR}${RESET}"
    info "Pulling latest changes..."
    git -C "$INSTALL_DIR" pull --ff-only --quiet 2>/dev/null || true
    ok "Repository updated"
  elif [ -d "$INSTALL_DIR" ]; then
    # Directory exists but isn't a git repo — don't clobber it
    warn "${INSTALL_DIR} exists but isn't a git repo"
    info "Using existing directory"
  else
    info "Cloning Velkor to ${AMBER}${INSTALL_DIR}${RESET}..."
    git clone --depth 1 "$REPO_URL" "$INSTALL_DIR" 2>/dev/null
    ok "Repository cloned"
  fi
  echo ""
}

# ---------------------------------------------------------------------------
# Install CLI dependencies
# ---------------------------------------------------------------------------

install_cli() {
  info "Installing CLI dependencies..."

  cd "$INSTALL_DIR/cli"
  npm install --silent 2>/dev/null
  ok "CLI dependencies installed"

  # Build TypeScript
  npx tsc 2>/dev/null
  ok "CLI built"

  # Register 'velkor' as a global command
  npm link --silent 2>/dev/null
  ok "velkor command registered globally"
  echo ""
}

# ---------------------------------------------------------------------------
# Launch setup wizard
# ---------------------------------------------------------------------------

launch_setup() {
  cd "$INSTALL_DIR"

  printf "${VIOLET}┌────────────────────────────────────────┐${RESET}\n"
  printf "${VIOLET}│${RESET} ${BOLD}Launching setup wizard...${RESET}              ${VIOLET}│${RESET}\n"
  printf "${VIOLET}└────────────────────────────────────────┘${RESET}\n"
  echo ""

  node cli/dist/index.js setup
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
  print_banner
  check_prerequisites
  clone_repo
  install_cli
  launch_setup
}

main "$@"
