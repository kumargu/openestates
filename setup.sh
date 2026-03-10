#!/bin/bash
# OpenEstates — new machine setup (macOS)
# Usage: ./setup.sh

set -e

echo "=== OpenEstates Setup ==="

# --- Homebrew ---
if ! command -v brew &>/dev/null; then
  echo "Installing Homebrew..."
  /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
else
  echo "Homebrew: already installed"
fi

# --- Git ---
if ! command -v git &>/dev/null; then
  echo "Installing git..."
  brew install git
else
  echo "Git: $(git --version)"
fi

# --- Rust ---
if ! command -v rustup &>/dev/null; then
  echo "Installing Rust via rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  source "$HOME/.cargo/env"
else
  echo "Rust: $(rustc --version)"
  rustup update stable
fi

# --- Node.js ---
if ! command -v node &>/dev/null; then
  echo "Installing Node.js via Homebrew..."
  brew install node
else
  echo "Node: $(node --version)"
fi

# --- Python 3 ---
if ! command -v python3 &>/dev/null; then
  echo "Installing Python 3..."
  brew install python
else
  echo "Python: $(python3 --version)"
fi

# --- Project dependencies ---
echo ""
echo "=== Installing project dependencies ==="

# Frontend (npm)
echo "Installing frontend dependencies..."
(cd frontend && npm install)

# Backend (cargo) — just check it builds
echo "Building backend (first build may take a minute)..."
(cd backend && cargo build)

# Python pipeline
echo "Installing Python dependencies..."
python3 -m pip install --user -r requirements.txt

# Playwright browsers (needed for pipeline crawling)
echo "Installing Playwright browsers..."
python3 -m playwright install firefox

# --- Environment file ---
if [ ! -f .env ]; then
  echo ""
  echo "Creating .env file..."
  cat > .env <<'EOF'
ANTHROPIC_API_KEY=
OPENAI_API_KEY=
GOOGLE_AI_API_KEY=
EOF
  echo "!! Fill in your API keys in .env"
else
  echo ".env file already exists"
fi

echo ""
echo "=== Setup complete ==="
echo ""
echo "To start developing:"
echo "  ./dev.sh          # runs backend (:4000) + frontend (:5173)"
echo ""
echo "Required API keys in .env:"
echo "  ANTHROPIC_API_KEY   — Claude (pipeline enrichment)"
echo "  OPENAI_API_KEY      — embeddings"
echo "  GOOGLE_AI_API_KEY   — Gemini (live discovery + semantic search)"
