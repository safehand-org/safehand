#!/usr/bin/env bash
# Prints a hash of everything the browser WebAssembly package is built from,
# so CI can tell whether the committed crates/splitcard-wasm/pkg is stale.
#   scripts/wasm-source-hash.sh          print the hash
#   scripts/wasm-source-hash.sh --check  compare it with pkg/SOURCE_HASH
# The .wasm bytes themselves differ between machines (the bundled secp256k1
# C code depends on the clang version), so the inputs are compared instead.
set -euo pipefail
cd "$(dirname "$0")/.."

inputs() {
  printf '%s\n' rust-toolchain.toml Cargo.toml Cargo.lock scripts/build-wasm.sh \
    crates/splitcard-core/Cargo.toml crates/splitcard-wasm/Cargo.toml
  find crates/splitcard-core/src crates/splitcard-wasm/src -type f
}

hash=$(inputs | LC_ALL=C sort | while read -r file; do
  printf '%s %s\n' "$(shasum -a 256 <"$file" | cut -d' ' -f1)" "$file"
done | shasum -a 256 | cut -d' ' -f1)

if [[ ${1:-} == --check ]]; then
  stored=$(cat crates/splitcard-wasm/pkg/SOURCE_HASH 2>/dev/null || true)
  if [[ $stored != "$hash" ]]; then
    echo "crates/splitcard-wasm/pkg is out of date with the Rust source." >&2
    echo "Run npm run build:wasm and commit crates/splitcard-wasm/pkg." >&2
    exit 1
  fi
  echo "crates/splitcard-wasm/pkg matches the Rust source"
else
  echo "$hash"
fi
