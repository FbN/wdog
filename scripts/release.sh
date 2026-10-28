#!/bin/sh
# Static musl release binaries: dist/wdog (http only) and dist/wdog-https.
# Needs: rustup target add x86_64-unknown-linux-musl. ring (https) needs a C compiler:
# musl-gcc if installed, else the system gcc works.
# --test also runs the whole test suite compiled for musl (e2e against the static binary).
set -eu
cd "$(dirname "$0")/.."
T=x86_64-unknown-linux-musl
if ! command -v x86_64-linux-musl-gcc >/dev/null 2>&1 && ! command -v musl-gcc >/dev/null 2>&1; then
  export CC_x86_64_unknown_linux_musl="${CC_x86_64_unknown_linux_musl:-gcc}"
fi
mkdir -p dist
for f in "" https; do
  if [ -n "$f" ]; then feat="--features $f"; out=dist/wdog-$f; else feat=""; out=dist/wdog; fi
  # shellcheck disable=SC2086
  cargo build --release --target $T $feat
  cp target/$T/release/wdog "$out"
  if ldd "$out" 2>&1 | grep -q '=>'; then
    echo "error: $out is dynamically linked" >&2
    exit 1
  fi
  if [ "${1:-}" = "--test" ]; then
    # shellcheck disable=SC2086
    cargo test --release --target $T $feat
  fi
  ls -l "$out"
done
