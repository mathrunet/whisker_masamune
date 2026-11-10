#!/usr/bin/env bash
# Type-check (and build C sources of) the workspace for Android the same way `whisker build` does:
# NDK clang for CC/CXX/linker and llvm-ar for AR (see whisker-build `cargo_build_dylib`).
# Usage: scripts/check-android.sh [cargo check args...]
set -euo pipefail
cd "$(dirname "$0")/.."

API="${ANDROID_API:-24}"
TRIPLE="aarch64-linux-android"
NDK="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
if [[ -z "$NDK" ]]; then
  NDK="$(ls -d "$HOME/Library/Android/sdk/ndk/"* "$HOME/Android/Sdk/ndk/"* 2>/dev/null | sort -V | tail -1 || true)"
fi
[[ -d "$NDK" ]] || { echo "Android NDK not found. Set ANDROID_NDK_HOME." >&2; exit 1; }
case "$(uname -s)" in
  Darwin) HOST="darwin-x86_64" ;;
  Linux) HOST="linux-x86_64" ;;
  *) echo "unsupported host" >&2; exit 1 ;;
esac
BIN="$NDK/toolchains/llvm/prebuilt/$HOST/bin"

rustup target add "$TRIPLE" >/dev/null 2>&1 || true
export ANDROID_NDK_HOME="$NDK"
export CC_aarch64_linux_android="$BIN/${TRIPLE}${API}-clang"
export CXX_aarch64_linux_android="$BIN/${TRIPLE}${API}-clang++"
export AR_aarch64_linux_android="$BIN/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"
exec cargo check --workspace --target "$TRIPLE" "$@"
