# Split Card

Split Card lets hackathon teams agree on prize splits before hacking, sign those agreements with Nostr keys, timestamp them with OpenTimestamps, and get paid over Lightning.

## Crates

- `splitcard-core`: schema, hashing, and split math with no I/O or async.
- `splitcard-wasm`: a `wasm-bindgen` wrapper that depends only on `splitcard-core`.
- `splitcard-ln`: Lightning support.
- `splitcard-server`: the Axum API, which may depend on `splitcard-core` and `splitcard-ln`.

Internal dependencies point inward: core has no internal dependencies; WASM depends only on core; the server depends on core and Lightning; nothing depends on the server.

## Local development

```sh
cargo build --workspace
cargo test --workspace
cargo run -p splitcard-server
```
