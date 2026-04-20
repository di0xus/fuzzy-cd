#!/usr/bin/env bash
# hop install.sh — curl-based installation / upgrade
# Usage: curl -fsSL https://raw.githubusercontent.com/di0xus/hop/main/install.sh | bash
set -e

REPO="di0xus/hop"
INSTALL_DIR="${HOP_INSTALL_DIR:-$HOME/.local/bin}"
BINARY="$INSTALL_DIR/hop"
RELEASE_URL="https://github.com/${REPO}/releases/latest/download"
API_URL="https://api.github.com/repos/${REPO}/releases/latest"

detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "macos" ;;
        *)       echo "hop: unsupported OS: $(uname -s)" >&2; exit 1 ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64)        echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        *)             echo "hop: unsupported architecture: $(uname -m)" >&2; exit 1 ;;
    esac
}

say() { echo "hop: $1"; }

latest_version() {
    # Fetch tag_name from GitHub API, strip leading 'v'
    curl -fsSL "$API_URL" | grep '"tag_name"' | cut -d '"' -f4 | sed 's/^v//'
}

local_version() {
    "$BINARY" --version 2>/dev/null | cut -d' ' -f2
}

need_update() {
    local latest="$1"
    local local="$2"
    # Simple semver comparison: always update if different major.minor
    # (this handles 0.6.0 -> 0.6.1 gracefully)
    [ "$latest" != "$local" ]
}

# Map to target triple used in release assets
case "$(detect_os)-$(detect_arch)" in
    linux-x86_64)   target="x86_64-unknown-linux-gnu" ;;
    linux-aarch64)  target="aarch64-unknown-linux-gnu" ;;
    macos-x86_64)   target="x86_64-apple-darwin" ;;
    macos-aarch64) target="aarch64-apple-darwin" ;;
esac

binary_name="hop-${target}"
url="${RELEASE_URL}/${binary_name}"

if [ -x "$BINARY" ]; then
    local_ver=$(local_version)
    latest_ver=$(latest_version)

    if [ -z "$latest_ver" ]; then
        say "warning: could not determine latest version — checking GitHub directly"
    elif [ "$local_ver" = "$latest_ver" ]; then
        say "already on latest version ($latest_ver)"
        exit 0
    else
        say "installed: $local_ver  →  latest: $latest_ver"
    fi

    # Backup old binary
    backup="${BINARY}.bak"
    say "backing up current binary to ${backup}"
    cp "$BINARY" "$backup"
else
    mkdir -p "$INSTALL_DIR"
fi

say "installing to ${INSTALL_DIR}..."
say "downloading ${url}..."
curl -fsSL "$url" -o "$BINARY"
chmod +x "$BINARY"

# Verify
if ! [ -x "$BINARY" ]; then
    echo "hop: download failed — binary not found at ${BINARY}" >&2
    exit 1
fi

say "installed ${BINARY}"

# Check if install dir is in PATH
case ":$PATH:" in
    *":${INSTALL_DIR}:"*) ;;
    *) say "NOTE: add ${INSTALL_DIR} to your PATH if it's not there:" >&2
       say "  export PATH=\"${INSTALL_DIR}:\$PATH\"" >&2 ;;
esac

say "run 'hop --version' to confirm"
