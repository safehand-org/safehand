// The committed browser package, initialised from its bytes for tests.

import { readFileSync } from "node:fs";
import * as wasm from "@splitcard/wasm";

wasm.initSync({
  module: readFileSync(new URL("../../crates/splitcard-wasm/pkg/splitcard_wasm_bg.wasm", import.meta.url)),
});

export default wasm;
