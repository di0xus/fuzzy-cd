#!/usr/bin/env bash
# hop install.sh — curl-based installation
# Usage: curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash
set -e

REPO="di0xus/hop"
INSTALL_DIR="${HOP_INSTALL_DIR:-$HOME/.local/bin}"
RELEASE_URL="https://github.com/${REPO}/releases/latest/download"

detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "macos" ;;
        *)       echo "hop: unsupported OS: $(uname -s)" >&2; exit 1 ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64)      echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        *)           echo "hop: unsupported architecture: $(uname -m)" >&2; exit 1 ;;
    esac
}

say() { echo "hop: $1"; }

say "installing to ${INSTALL_DIR}..."

# Create install dir
mkdir -p "$INSTALL_DIR"

# Detect platform
os=$(detect_os)
arch=$(detect_arch)

# Map to target triple used in release assets
case "${os}-${arch}" in
    linux-x86_64)   target="x86_64-unknown-linux-gnu" ;;
    linux-aarch64)  target="aarch64-unknown-linux-gnu" ;;
    macos-x86_64)  target="x86_64-apple-darwin" ;;
    macos-aarch64) target="aarch64-apple-darwin" ;;
esac

binary="hop-${target}"
url="${RELEASE_URL}/${binary}"

say "downloading ${url}..."
curl -fsSL "$url" -o "${INSTALL_DIR}/hop"

# Make executable
chmod +x "${INSTALL_DIR}/hop"

# Verify
if ! [ -x "${INSTALL_DIR}/hop" ]; then
    echo "hop: download failed — binary not found at ${INSTALL_DIR}/hop" >&2
    exit 1
fi

say "installed ${INSTALL_DIR}/hop"

# Check if install dir is in PATH
case ":$PATH:" in
    *":${INSTALL_DIR}:"*) ;;
    *) say "NOTE: add ${INSTALL_DIR} to your PATH if it's not there:" >&2
       say "  export PATH=\"${INSTALL_DIR}:\$PATH\"" >&2 ;;
esac
