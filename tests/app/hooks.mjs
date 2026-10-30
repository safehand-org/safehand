// Resolves the app's import aliases for `node --test`, as tsconfig.json
// does: `@/…` is src/, `@splitcard/wasm` is the committed browser package,
// and extensionless relative imports are TypeScript files.

import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";

const root = new URL("../../", import.meta.url);

function typescriptFile(url) {
  for (const extension of [".ts", ".tsx", "/index.ts"]) {
    const candidate = new URL(`${url.href}${extension}`);
    if (existsSync(fileURLToPath(candidate))) return candidate.href;
  }
  return null;
}

export async function resolve(specifier, context, nextResolve) {
  if (specifier === "@splitcard/wasm") {
    return {
      url: new URL("crates/splitcard-wasm/pkg/splitcard_wasm.js", root).href,
      shortCircuit: true,
    };
  }
  if (specifier.startsWith("@/")) {
    const found = typescriptFile(new URL(`src/${specifier.slice(2)}`, root));
    if (found) return { url: found, shortCircuit: true };
  }
  if ((specifier.startsWith("./") || specifier.startsWith("../")) && context.parentURL) {
    const base = new URL(specifier, context.parentURL);
    if (!existsSync(fileURLToPath(base))) {
      const found = typescriptFile(base);
      if (found) return { url: found, shortCircuit: true };
    }
  }
  return nextResolve(specifier, context);
}
