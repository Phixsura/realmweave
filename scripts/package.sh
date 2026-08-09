#!/usr/bin/env bash
# Build a distributable Realmweave client bundle for the current platform.
#
# Usage:
#   scripts/package.sh              # plain build
#   scripts/package.sh --steam     # with Steamworks integration
#
# Output: dist/realmweave-<platform>/

set -euo pipefail
cd "$(dirname "$0")/.."

FEATURES=""
SUFFIX=""
if [[ "${1:-}" == "--steam" ]]; then
    FEATURES="--features steam"
    SUFFIX="-steam"
fi

case "$(uname -s)" in
    Darwin) PLATFORM="macos" ;;
    Linux) PLATFORM="linux" ;;
    MINGW*|MSYS*|CYGWIN*) PLATFORM="windows" ;;
    *) echo "unsupported platform"; exit 1 ;;
esac

echo "==> building release client ($PLATFORM$SUFFIX)"
# shellcheck disable=SC2086
cargo build --release -p realmweave-client $FEATURES

OUT="dist/realmweave-$PLATFORM$SUFFIX"
rm -rf "$OUT"
mkdir -p "$OUT"

BIN=target/release/realmweave-client
[[ "$PLATFORM" == "windows" ]] && BIN="$BIN.exe"
cp "$BIN" "$OUT/"
cp -r boards "$OUT/boards"
cp README.md LICENSE "$OUT/"

if [[ -n "$SUFFIX" ]]; then
    # Development app id (Spacewar) until Realmweave has its own.
    echo "480" > "$OUT/steam_appid.txt"
    echo "NOTE: replace steam_appid.txt with the real app id before upload."
fi

echo "==> packaged $OUT"
ls -la "$OUT"
