# =============================================================================
# Velkor — One-line installer for Windows (PowerShell)
#
# Usage:
#   irm https://raw.githubusercontent.com/maxheld/velkor/main/scripts/install.ps1 | iex
#
# What it does:
#   1. Checks prerequisites (git, node, docker)
#   2. Clones the repo (or pulls if already cloned)
#   3. Installs CLI dependencies
#   4. Launches the interactive setup wizard
# =============================================================================

$ErrorActionPreference = "Stop"

# ---------------------------------------------------------------------------
# Colors
# ---------------------------------------------------------------------------

function Write-Violet { param($Text) Write-Host $Text -ForegroundColor Magenta }
function Write-Ok     { param($Text) Write-Host "  ✔ $Text" -ForegroundColor Green }
function Write-Warn   { param($Text) Write-Host "  ! $Text" -ForegroundColor Yellow }
function Write-Err    { param($Text) Write-Host "  ✖ $Text" -ForegroundColor Red }
function Write-Info   { param($Text) Write-Host "  ▸ $Text" -ForegroundColor Magenta }
function Write-Dim    { param($Text) Write-Host "    $Text" -ForegroundColor DarkGray }

# ---------------------------------------------------------------------------
# Banner
# ---------------------------------------------------------------------------

function Show-Banner {
    Write-Host ""
    Write-Violet " ██╗   ██╗███████╗██╗     ██╗  ██╗ ██████╗ ██████╗ "
    Write-Violet " ██║   ██║██╔════╝██║     ██║ ██╔╝██╔═══██╗██╔══██╗"
    Write-Violet " ██║   ██║█████╗  ██║     █████╔╝ ██║   ██║██████╔╝"
    Write-Violet " ╚██╗ ██╔╝██╔══╝  ██║     ██╔═██╗ ██║   ██║██╔══██╗"
    Write-Violet "  ╚████╔╝ ███████╗███████╗██║  ██╗╚██████╔╝██║  ██║"
    Write-Violet "   ╚═══╝  ╚══════╝╚══════╝╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝"
    Write-Host ""
    Write-Dim "Self-hosted multi-agent orchestration platform"
    Write-Dim "Installer v0.1.0"
    Write-Host ""
}

# ---------------------------------------------------------------------------
# Prerequisite checks
# ---------------------------------------------------------------------------

function Test-Command {
    param($Name)
    return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

function Assert-Prerequisites {
    Write-Info "Checking prerequisites..."
    Write-Host ""

    # Git
    if (-not (Test-Command "git")) {
        Write-Err "git is required. Install it from https://git-scm.com/"
        exit 1
    }
    $gitVer = (git --version) -replace "git version ", ""
    Write-Ok "git ($gitVer)"

    # Node.js
    if (-not (Test-Command "node")) {
        Write-Err "Node.js is required (v18+). Install it from https://nodejs.org/"
        exit 1
    }
    $nodeVer = node --version
    $nodeMajor = [int]($nodeVer -replace "v(\d+)\..*", '$1')
    if ($nodeMajor -lt 18) {
        Write-Err "Node.js v18+ required, found $nodeVer. Update at https://nodejs.org/"
        exit 1
    }
    Write-Ok "node ($nodeVer)"

    # npm
    if (-not (Test-Command "npm")) {
        Write-Err "npm is required (ships with Node.js)"
        exit 1
    }
    $npmVer = npm --version
    Write-Ok "npm (v$npmVer)"

    # Docker
    if (-not (Test-Command "docker")) {
        Write-Err "Docker is required. Install Docker Desktop from https://docs.docker.com/get-docker/"
        exit 1
    }
    try {
        $null = docker info 2>&1
        $dockerVer = (docker --version) -replace "Docker version ([^,]+),.*", '$1'
        Write-Ok "docker ($dockerVer)"
    }
    catch {
        Write-Err "Docker is installed but not running. Start Docker Desktop."
        exit 1
    }

    # Docker Compose
    try {
        $composeVer = docker compose version --short 2>&1
        Write-Ok "docker compose ($composeVer)"
    }
    catch {
        Write-Err "Docker Compose is required. It ships with Docker Desktop."
        exit 1
    }

    Write-Host ""
    Write-Ok "All prerequisites met"
    Write-Host ""
}

# ---------------------------------------------------------------------------
# Clone or update
# ---------------------------------------------------------------------------

$RepoUrl = "https://github.com/maxheld/velkor.git"
$InstallDir = if ($env:VELKOR_DIR) { $env:VELKOR_DIR } else { Join-Path $HOME "velkor" }

function Install-Repo {
    if (Test-Path (Join-Path $InstallDir ".git")) {
        Write-Info "Existing installation found at $InstallDir"
        Write-Info "Pulling latest changes..."
        git -C $InstallDir pull --ff-only --quiet 2>$null
        Write-Ok "Repository updated"
    }
    elseif (Test-Path $InstallDir) {
        Write-Warn "$InstallDir exists but isn't a git repo"
        Write-Info "Using existing directory"
    }
    else {
        Write-Info "Cloning Velkor to $InstallDir..."
        git clone --depth 1 $RepoUrl $InstallDir 2>$null
        Write-Ok "Repository cloned"
    }
    Write-Host ""
}

# ---------------------------------------------------------------------------
# Install CLI
# ---------------------------------------------------------------------------

function Install-Cli {
    Write-Info "Installing CLI dependencies..."

    Push-Location (Join-Path $InstallDir "cli")
    npm install --silent 2>$null
    Write-Ok "CLI dependencies installed"

    npx tsc 2>$null
    Write-Ok "CLI built"
    Pop-Location

    Write-Host ""
}

# ---------------------------------------------------------------------------
# Launch setup
# ---------------------------------------------------------------------------

function Start-Setup {
    Push-Location $InstallDir

    Write-Violet "┌────────────────────────────────────────┐"
    Write-Host "  │ " -NoNewline -ForegroundColor Magenta
    Write-Host "Launching setup wizard...              " -NoNewline -ForegroundColor White
    Write-Host "│" -ForegroundColor Magenta
    Write-Violet "└────────────────────────────────────────┘"
    Write-Host ""

    node cli/dist/index.js setup

    Pop-Location
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

Show-Banner
Assert-Prerequisites
Install-Repo
Install-Cli
Start-Setup
