#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mode="${1:-core}"
case "$mode" in
  core) features=wasm; destination=web/pkg ;;
  runtime) features=wasm-runtime; destination=web/pkg-runtime ;;
  *) echo 'usage: scripts/build-wasm.sh [core|runtime]' >&2; exit 2 ;;
esac
cargo build --locked --profile wasm --lib --target wasm32-unknown-unknown --features "$features"
wasm-bindgen --target web --out-dir "$destination" target/wasm32-unknown-unknown/wasm/smarter_diagrams.wasm
