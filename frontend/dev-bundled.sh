#!/usr/bin/env bash
# Dev launcher that runs the app via a proper .app bundle so macOS TCC
# (Privacy & Security → Screen & System Audio Recording) actually
# tracks it. `cargo tauri dev` runs the bare binary, which macOS won't
# register in the privacy lists — that breaks screen recording.
#
# Flow:
#   1. start the Next.js dev server on :3118 (same as dev-gpu.sh)
#   2. cargo build the meetily binary in debug mode (no-op if up-to-date)
#   3. refresh the .app bundle's executable from target/debug/meetily
#      (the bundle is created on first run)
#   4. ad-hoc codesign the bundle so its TCC identity is stable
#   5. launch the bundle via `open` and stream its log
#   6. on exit, kill the Next dev server
#
# After the first run, add Meetily.app to:
#   System Settings → Privacy & Security → Screen & System Audio Recording
# Then relaunch this script. Permission persists across rebuilds because
# the bundle id (com.meetily.dev) is what macOS tracks.

set -euo pipefail

FRONTEND_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$FRONTEND_DIR/.." && pwd)"
BIN="$ROOT_DIR/target/debug/meetily"
APP="$ROOT_DIR/target/debug/Meetily.app"
LOG="${MEETILY_DEV_LOG:-/tmp/meetily-dev-bundled.log}"

echo "[dev-bundled] frontend dir: $FRONTEND_DIR"
echo "[dev-bundled] log:          $LOG"

# 1. Next.js dev server.
cd "$FRONTEND_DIR"
echo "[dev-bundled] starting Next.js on :3118..."
pnpm next dev -p 3118 >"${LOG}.next" 2>&1 &
NEXT_PID=$!
trap 'echo "[dev-bundled] shutting down..."; kill "$NEXT_PID" 2>/dev/null || true; exit 0' INT TERM EXIT

# 2. cargo build the dev binary.
echo "[dev-bundled] cargo build (debug)..."
cd "$ROOT_DIR/frontend/src-tauri"
cargo build 2>&1 | tail -5

# 2b. Ensure the llama-helper sidecar is available. Built once on first
# run; subsequent runs skip if the binary exists. Slow first time
# (llama-cpp-2 pulls in a non-trivial C++ build) but only once.
TARGET_TRIPLE=$(rustc -vV | grep "host:" | awk '{print $2}')
HELPER_BIN_RELEASE="$ROOT_DIR/target/release/llama-helper"
if [[ ! -x "$HELPER_BIN_RELEASE" ]]; then
  echo "[dev-bundled] building llama-helper (one-time, ~5 min)..."
  HELPER_FEATURES=""
  if [[ "$OSTYPE" == "darwin"* ]]; then
    HELPER_FEATURES="--features metal"
  fi
  (cd "$ROOT_DIR/llama-helper" && cargo build --release $HELPER_FEATURES) || {
    echo "[dev-bundled] llama-helper build failed — summary generation will not work."
  }
fi

# 3. Refresh / create the .app bundle.
echo "[dev-bundled] refreshing $APP ..."
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
if [[ ! -f "$APP/Contents/Info.plist" ]]; then
  cp "$ROOT_DIR/frontend/src-tauri/Info.plist" "$APP/Contents/Info.plist"
  /usr/libexec/PlistBuddy -c "Add :CFBundleName string Meetily" "$APP/Contents/Info.plist" 2>/dev/null || true
  /usr/libexec/PlistBuddy -c "Add :CFBundleDisplayName string Meetily" "$APP/Contents/Info.plist" 2>/dev/null || true
  /usr/libexec/PlistBuddy -c "Add :CFBundleIdentifier string com.meetily.dev" "$APP/Contents/Info.plist" 2>/dev/null || true
  /usr/libexec/PlistBuddy -c "Add :CFBundleExecutable string meetily" "$APP/Contents/Info.plist" 2>/dev/null || true
  /usr/libexec/PlistBuddy -c "Add :CFBundlePackageType string APPL" "$APP/Contents/Info.plist" 2>/dev/null || true
  /usr/libexec/PlistBuddy -c "Add :CFBundleVersion string 0.3.0-dev" "$APP/Contents/Info.plist" 2>/dev/null || true
  /usr/libexec/PlistBuddy -c "Add :CFBundleShortVersionString string 0.3.0-dev" "$APP/Contents/Info.plist" 2>/dev/null || true
  /usr/libexec/PlistBuddy -c "Add :LSMinimumSystemVersion string 13.0" "$APP/Contents/Info.plist" 2>/dev/null || true
fi
cp "$BIN" "$APP/Contents/MacOS/meetily"
# Copy the llama-helper sidecar next to meetily (the resolver's preferred
# lookup path). Use the target-triple-suffixed name to match the
# production lookup convention.
if [[ -x "$HELPER_BIN_RELEASE" ]]; then
  cp "$HELPER_BIN_RELEASE" "$APP/Contents/MacOS/llama-helper-$TARGET_TRIPLE"
  echo "[dev-bundled] llama-helper copied: llama-helper-$TARGET_TRIPLE"
fi

# 4. Ad-hoc sign so the TCC identity is stable.
codesign --force --deep --sign - "$APP" 2>&1 | tail -1 || true

# 5. Wait for Next to be ready, then launch the bundle.
echo "[dev-bundled] waiting for Next..."
for _ in $(seq 1 60); do
  if curl -sf http://localhost:3118 >/dev/null 2>&1; then
    echo "[dev-bundled] Next is up."
    break
  fi
  sleep 0.5
done

echo "[dev-bundled] launching $APP ..."
echo "[dev-bundled] === IMPORTANT: First run only ==="
echo "[dev-bundled] After the app starts, add it to:"
echo "[dev-bundled]   System Settings → Privacy & Security → Screen & System Audio Recording"
echo "[dev-bundled] (use the + button under the top list, navigate to the path above and pick Meetily.app)"
echo "[dev-bundled] Then quit + re-run this script."
echo

# 'open' returns immediately by default; use --wait-apps so the trap kills Next when the app quits.
exec open --wait-apps --stdout "$LOG" --stderr "$LOG" "$APP"
