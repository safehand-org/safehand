#!/usr/bin/env bash
# Builds the browser WebAssembly package the app imports as @splitcard/wasm
# (crates/splitcard-wasm/pkg). Run once after cloning and after changing the
# Rust crates. Uses llvm-ar so secp256k1-sys's wasm32 archive gets a symbol
# index the linker can read, as in CI.
set -euo pipefail
cd "$(dirname "$0")/.."
host=$(rustc -vV | sed -n 's/^host: //p')
export AR_wasm32_unknown_unknown="$(rustc --print sysroot)/lib/rustlib/$host/bin/llvm-ar"
wasm-pack build crates/splitcard-wasm --target web
