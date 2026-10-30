#!/usr/bin/env bash
# Builds the browser WebAssembly package the app imports as @splitcard/wasm
# (crates/splitcard-wasm/pkg). The package is committed, so hosts that only
# run `npm run build` (no Rust) can build the app: run this after changing
# splitcard-core or splitcard-wasm and commit the result. CI fails when the
# committed package is stale (scripts/wasm-source-hash.sh).
set -euo pipefail
cd "$(dirname "$0")/.."
host=$(rustc -vV | sed -n 's/^host: //p')
sysroot=$(rustc --print sysroot)
# llvm-ar gives secp256k1-sys's wasm32 archive a symbol index the linker can
# read, as in CI.
export AR_wasm32_unknown_unknown="$sysroot/lib/rustlib/$host/bin/llvm-ar"
# Panic messages embed source paths; keep this machine's out of the package.
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="\
--remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo \
--remap-path-prefix=$sysroot=/rust \
--remap-path-prefix=$PWD=/splitcard"
wasm-pack build crates/splitcard-wasm --target web
# wasm-pack ignores the whole package by default; it is committed here.
rm -f crates/splitcard-wasm/pkg/.gitignore
scripts/wasm-source-hash.sh > crates/splitcard-wasm/pkg/SOURCE_HASH
