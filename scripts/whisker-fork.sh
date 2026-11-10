#!/usr/bin/env bash
# Keeps every whisker crate on one revision of the mathrunet/whisker fork.
#
#   scripts/whisker-fork.sh check [--allow-path]
#       Fails unless every `whisker*` package in Cargo.lock comes from the fork's
#       `main` branch at a single revision. `whisker-router` may also come from
#       whisker_masamune (the patched router). `--allow-path` accepts local path
#       sources while testing against a working tree of the fork.
#
#   scripts/whisker-fork.sh cli
#       Installs whisker-cli from the fork at the revision pinned in Cargo.lock,
#       so the CLI always matches the libraries the app is built with.
#
# Run it from the directory that holds Cargo.lock. This script is maintained in
# whisker_masamune and copied into apps.
set -euo pipefail

FORK_SOURCE="git+https://github.com/mathrunet/whisker?branch=main#"
FORK_URL="https://github.com/mathrunet/whisker"
LOCK="${WHISKER_FORK_LOCK:-Cargo.lock}"

usage() {
  sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'
  exit 2
}

[[ -f "$LOCK" ]] || { echo "error: $LOCK not found (run from the workspace root)" >&2; exit 1; }

# Prints "name<TAB>version<TAB>source" for every whisker package in the lock.
whisker_packages() {
  python3 -I - "$LOCK" <<'PY'
import sys, tomllib
with open(sys.argv[1], "rb") as f:
    lock = tomllib.load(f)
for p in lock.get("package", []):
    name = p["name"]
    if name == "whisker" or name.startswith("whisker-"):
        print(f'{name}\t{p["version"]}\t{p.get("source", "")}')
PY
}

cmd_check() {
  local allow_path=0
  [[ "${1:-}" == "--allow-path" ]] && allow_path=1
  local revs="" bad=0 count=0
  while IFS=$'\t' read -r name version source; do
    count=$((count + 1))
    if [[ -z "$source" ]]; then
      if [[ $allow_path -eq 1 || "$name" == "whisker-router" ]]; then
        continue
      fi
      echo "path source: $name $version (use --allow-path while testing a local fork)" >&2
      bad=1
    elif [[ "$source" == "$FORK_SOURCE"* ]]; then
      revs+="${source#"$FORK_SOURCE"}"$'\n'
    elif [[ "$name" == "whisker-router" && "$source" == git+https://github.com/mathrunet/whisker_masamune* ]]; then
      continue
    else
      echo "not from the fork: $name $version ($source)" >&2
      echo "  add it to [patch.crates-io] with the fork source, or raise the version requirement" >&2
      bad=1
    fi
  done < <(whisker_packages)

  local unique
  unique=$(printf '%s' "$revs" | sort -u | sed '/^$/d')
  if [[ $(printf '%s\n' "$unique" | sed '/^$/d' | wc -l) -gt 1 ]]; then
    echo "whisker crates come from more than one fork revision:" >&2
    printf '  %s\n' $unique >&2
    bad=1
  fi
  if [[ $bad -ne 0 ]]; then
    exit 1
  fi
  echo "ok: $count whisker packages, fork revision ${unique:-<path>}"
}

cmd_cli() {
  local rev
  rev=$(whisker_packages | awk -F'\t' '$1 == "whisker" { print $3 }' | head -1)
  if [[ "$rev" != "$FORK_SOURCE"* ]]; then
    echo "error: whisker is not locked to the fork (source: ${rev:-path})." >&2
    echo "  for a local fork tree run: cargo install --path <fork>/crates/whisker-cli --locked" >&2
    exit 1
  fi
  rev="${rev#"$FORK_SOURCE"}"
  echo "installing whisker-cli from $FORK_URL @ $rev"
  cargo install --git "$FORK_URL" --rev "$rev" whisker-cli --locked
}

case "${1:-}" in
  check) shift; cmd_check "$@" ;;
  cli) cmd_cli ;;
  *) usage ;;
esac
