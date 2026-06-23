#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  tools/verify_fuzz_parse_any.sh [max-total-time-seconds]

Runs the parse_any cargo-fuzz target against a temporary copy of the checked
seed corpus so libFuzzer can add minimized mutations without modifying
fuzz/corpus/parse_any.

Environment:
  RUSTUP_TOOLCHAIN       default: nightly
  FUZZ_RSS_LIMIT_MB     default: 512
  FUZZ_TIMEOUT_SECONDS  default: 5
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_DIR=$(cd "$SCRIPT_DIR/.." && pwd)
MAX_TOTAL_TIME=${1:-30}
TOOLCHAIN=${RUSTUP_TOOLCHAIN:-nightly}
RSS_LIMIT=${FUZZ_RSS_LIMIT_MB:-512}
TIMEOUT=${FUZZ_TIMEOUT_SECONDS:-5}
SEED_DIR="$REPO_DIR/fuzz/corpus/parse_any"

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo was not found in PATH" >&2
  exit 2
fi
if ! cargo fuzz --help >/dev/null 2>&1; then
  echo "error: cargo-fuzz is not installed; run: cargo install cargo-fuzz --locked" >&2
  exit 2
fi
if ! rustup toolchain list | grep -Eq "^${TOOLCHAIN}([ -]|$)"; then
  echo "error: Rust toolchain '$TOOLCHAIN' is not installed; run: rustup toolchain install $TOOLCHAIN --profile minimal" >&2
  exit 2
fi
if [[ ! -d "$SEED_DIR" ]]; then
  echo "error: fuzz seed corpus not found: $SEED_DIR" >&2
  exit 2
fi

seed_count=$(find "$SEED_DIR" -maxdepth 1 -type f ! -name '.gitkeep' | wc -l)
if [[ "$seed_count" -eq 0 ]]; then
  echo "error: fuzz seed corpus is empty: $SEED_DIR" >&2
  exit 2
fi

WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/tablegram_parse_any_fuzz.XXXXXX")
cleanup() {
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT INT TERM

cp -a "$SEED_DIR/." "$WORK_DIR/"

echo "parse_any fuzz smoke: seeds=$seed_count temp=$WORK_DIR max_total_time=$MAX_TOTAL_TIME rss=${RSS_LIMIT}MB timeout=${TIMEOUT}s"
(
  cd "$REPO_DIR"
  cargo "+$TOOLCHAIN" fuzz run parse_any "$WORK_DIR" -- \
    -max_total_time="$MAX_TOTAL_TIME" \
    -rss_limit_mb="$RSS_LIMIT" \
    -timeout="$TIMEOUT"
)
