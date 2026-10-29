// The Split Card WebAssembly package, loaded once in the browser.
//
// Build it with `npm run build:wasm`. It is only imported on demand, so pages
// that never sign or hash anything do not download it.

import type * as SplitCardWasm from "@splitcard/wasm";

export type Wasm = typeof SplitCardWasm;

let loading: Promise<Wasm> | null = null;

export function loadWasm(): Promise<Wasm> {
  if (!loading) {
    loading = import("@splitcard/wasm").then(async (wasm) => {
      await wasm.default();
      return wasm;
    });
    // Let a failed load (a network blip) be retried.
    loading.catch(() => {
      loading = null;
    });
  }
  return loading;
}

/** 32 bytes from the browser's cryptographic random source. */
export function randomBytes(): Uint8Array {
  return crypto.getRandomValues(new Uint8Array(32));
}
