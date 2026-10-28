#!/bin/sh
# fmt, clippy and tests; uses cargo-nextest when installed.
set -eu
cd "$(dirname "$0")/.."
cargo fmt --check
cargo clippy --all-targets -- -D warnings
if cargo nextest --version >/dev/null 2>&1; then
  cargo nextest run
else
  cargo test
fi
