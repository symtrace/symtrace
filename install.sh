#!/usr/bin/env bash
set -euo pipefail

# Symtrace Standalone Universal Shell Installer (Linux / macOS)
# Repository: https://github.com/JashT14/symtrace

REPO="JashT14/symtrace"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# ANSI Colors & Styles
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m' # No Color

animate_banner() {
  clear 2>/dev/null || true
  echo -e "${CYAN}${BOLD}"
  echo "  ____                  _                   "
  echo " / ___| _7 _   _ _ __ ___ | |_ _ __ __ _  ___ ___ "
  echo " \___ \| | | | | '_ \` _ \| __| '__/ _\` |/ __/ _ \\"
  echo "  ___) | |_| | | | | | | | |_| | | (_| | (_|  __/"
  echo " |____/ \__, | |_| |_| |_|\__|_|  \__,_|\___\___|"
  echo "        |___/                                    "
  echo -e "${MAGENTA}${BOLD}   Deterministic AST Semantic Diff Engine v0.4.5${NC}"
  echo -e "${DIM}───────────────────────────────────────────────────${NC}"
  echo ""
}

spinner_step() {
  local msg="$1"
  local spinstr='⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'
  local delay=0.08
  for (( i=0; i<12; i++ )); do
    local char="${spinstr:$((i%8)):1}"
    echo -ne "\r${CYAN}[ ${char} ]${NC} ${msg}"
    sleep "$delay"
  done
  echo -e "\r${GREEN}[ ✓ ]${NC} ${msg}"
}

animate_banner

# ── Detect OS & Architecture ──────────────────────────────────────────
spinner_step "Analyzing system target architecture..."

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$OS" in
  linux)
    PLATFORM="unknown-linux-gnu"
    ;;
  darwin)
    PLATFORM="apple-darwin"
    ;;
  *)
    echo -e "\n${YELLOW}Error: Unsupported operating system: $OS${NC}" >&2
    exit 1
    ;;
esac

case "$ARCH" in
  x86_64|amd64)
    ARCH_TARGET="x86_64"
    ;;
  aarch64|arm64)
    ARCH_TARGET="aarch64"
    ;;
  *)
    echo -e "\n${YELLOW}Error: Unsupported architecture: $ARCH${NC}" >&2
    exit 1
    ;;
esac

TARGET="${ARCH_TARGET}-${PLATFORM}"
TARBALL="symtrace-${TARGET}.tar.gz"
DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/${TARBALL}"

echo -e "      ${DIM}Platform: ${OS} (${ARCH}) ──► ${TARGET}${NC}\n"

# ── Download Release Asset ─────────────────────────────────────────────
spinner_step "Downloading binary payload from GitHub Releases..."

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

if command -v curl >/dev/null 2>&1; then
  curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/$TARBALL"
elif command -v wget >/dev/null 2>&1; then
  wget -q "$DOWNLOAD_URL" -O "$TMP_DIR/$TARBALL"
else
  echo -e "\n${YELLOW}Error: Neither curl nor wget is available.${NC}" >&2
  exit 1
fi

# ── Extract Binary & Initialize Engine ───────────────────────────────
spinner_step "Unpacking Tree-sitter grammars & AST engine..."
tar -xzf "$TMP_DIR/$TARBALL" -C "$TMP_DIR"

spinner_step "Binding 4-hash BLAKE3 identity tracker & LRU caches..."
mkdir -p "$INSTALL_DIR"
mv "$TMP_DIR/symtrace" "$INSTALL_DIR/symtrace"
chmod +x "$INSTALL_DIR/symtrace"

sleep 0.1

echo -e "\n${GREEN}${BOLD}✨ Installation Successful!${NC}\n"
echo -e "${DIM}Binary Location :${NC} ${BOLD}${INSTALL_DIR}/symtrace${NC}"

# ── Verify Installation ───────────────────────────────────────────────
if command -v symtrace >/dev/null 2>&1; then
  echo -e "${DIM}Version Check   :${NC} ${GREEN}$(symtrace --version)${NC}"
else
  echo -e "\n${YELLOW}Note:${NC} ${INSTALL_DIR} is not currently in your PATH."
  echo -e "Add it by appending this to your shell config (${DIM}~/.bashrc${NC} / ${DIM}~/.zshrc${NC}):"
  echo -e "  ${CYAN}export PATH=\"${INSTALL_DIR}:\$PATH\"${NC}"
fi

echo -e "\n${MAGENTA}${BOLD}┌─► Symtrace is ready for semantic diffing! ⚡${NC}"
echo -e "${DIM}└─► Try running: symtrace . HEAD~1 HEAD${NC}\n"
