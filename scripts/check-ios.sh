#!/usr/bin/env bash
# Checks the iOS side of the workspace (macOS with Xcode only):
#   1. `cargo check` of every crate for the iOS simulator and device targets.
#   2. A generic iOS Simulator build of every crate's Swift package (`Package.swift`).
# Usage: scripts/check-ios.sh [--swift-only | --rust-only] [crate-name...]
set -euo pipefail
cd "$(dirname "$0")/.."

mode="all"
case "${1:-}" in
  --swift-only) mode="swift"; shift ;;
  --rust-only) mode="rust"; shift ;;
esac

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"

if [[ "$mode" != "swift" ]]; then
  for triple in aarch64-apple-ios-sim aarch64-apple-ios; do
    rustup target add "$triple" >/dev/null 2>&1 || true
    echo "==> cargo check --target $triple"
    cargo check --workspace --target "$triple"
  done
fi

[[ "$mode" == "rust" ]] && exit 0

if [[ $# -gt 0 ]]; then
  packages=("$@")
else
  packages=()
  for manifest in packages/*/Package.swift; do
    packages+=("$(basename "$(dirname "$manifest")")")
  done
fi

failed=()
for crate in "${packages[@]}"; do
  dir="packages/$crate"
  [[ -f "$dir/Package.swift" ]] || { echo "skip $crate (no Package.swift)"; continue; }
  echo "==> xcodebuild $crate"
  log="$CARGO_TARGET_DIR/xcode-logs/$crate.log"
  mkdir -p "$(dirname "$log")"
  if (cd "$dir" && xcodebuild -scheme "$crate" \
      -destination 'generic/platform=iOS Simulator' \
      -skipPackagePluginValidation -skipMacroValidation \
      -derivedDataPath "$CARGO_TARGET_DIR/xcode/$crate" build) >"$log" 2>&1; then
    # Warnings from the crate's own sources fail the check like clippy warnings do.
    if grep -E "/packages/$crate/ios/.*: warning:" "$log" | sort -u | grep .; then
      failed+=("$crate (warnings)")
    fi
  else
    grep -E "error:" "$log" | sort -u | head -20 || true
    failed+=("$crate")
  fi
done

if [[ ${#failed[@]} -gt 0 ]]; then
  echo "failed: ${failed[*]} (logs in $CARGO_TARGET_DIR/xcode-logs)" >&2
  exit 1
fi
echo "ok: ${#packages[@]} Swift packages"
