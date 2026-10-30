// Smoke test of the real JS boundary, against the browser package.
//
//   wasm-pack build crates/splitcard-wasm --target web
//   node crates/splitcard-wasm/tests/smoke.mjs
//
// This loads the same `--target web` pkg/ the frontend ships, initialising it
// synchronously from the .wasm bytes instead of fetching them, so no separate
// `--target nodejs` build is needed. Exits non-zero on any mismatch.

import { readFileSync } from "node:fs";
import {
  initSync,
  compute_payouts,
  display_basis_points,
  signature_template,
  card_hash,
  npub_encode,
} from "../pkg/splitcard_wasm.js";

const wasm = readFileSync(new URL("../pkg/splitcard_wasm_bg.wasm", import.meta.url));
initSync({ module: wasm });

let failures = 0;
function check(label, actual, expected) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a === e) {
    console.log(`ok   ${label}`);
  } else {
    failures += 1;
    console.error(`FAIL ${label}\n  expected ${e}\n  actual   ${a}`);
  }
}

// A card is passed as a JSON string that only Rust parses. It is written out
// by hand here, never built with JSON.stringify, to model that rule.
const pk = (n) => n.toString(16).padStart(64, "0");
const card = `{"version":1,"card_id":"btcpp-lagos-2026/team-42","event":"BTC++ Lagos 2026","team":"Team 42","members":[{"pubkey":"${pk(1)}","name":"Ada","weight":36,"payout":{"lightning":{"address":"ada@example.com"}}},{"pubkey":"${pk(2)}","name":"Bola","weight":35,"payout":{"lightning":{"address":"bola@example.org"}}},{"pubkey":"${pk(3)}","name":"Chidi","weight":29,"payout":{"lightning":{"address":"chidi@example.net"}}}],"rules":{"peer_review":{"max_shift_points":10}},"supersedes":"${"a".repeat(64)}"}`;

check("compute_payouts 100,001 sats at 36/35/29", compute_payouts(card, "100001"), [
  { pubkey: pk(1), amount_sats: "36001" },
  { pubkey: pk(2), amount_sats: "35000" },
  { pubkey: pk(3), amount_sats: "29000" },
]);
check("display_basis_points", display_basis_points(card), [3600, 3500, 2900]);

// A version 2 card: members by pubkey, payout addresses go in signatures.
const v2 = `{"version":2,"card_id":"lagos/llamas","event":"Lagos","team":"Llamas","members":[{"pubkey":"${pk(1)}","weight":1,"label":"Ada"},{"pubkey":"${pk(2)}","weight":1},{"pubkey":"${pk(3)}","weight":1}],"rules":{"peer_review":null,"leave":{"kind":"keep_percent","keep_percent":15}},"supersedes":null}`;
check("version 2 display_basis_points, equal split", display_basis_points(v2), [3334, 3333, 3333]);
// The secp256k1 generator's x coordinate, a valid x-only public key.
const g = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
check(
  "npub_encode generator",
  npub_encode(g),
  "npub10xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqpkge6d",
);
try {
  npub_encode("zz");
  failures += 1;
  console.error("FAIL npub_encode(\"zz\") did not throw");
} catch (error) {
  check("npub_encode invalid hex", error.code, "invalid_public_key");
}
const template = signature_template(card_hash(v2), g, 1700000000, "ada@example.com");
check("version 2 template carries the lud16 tag", template.tags[2], ["lud16", "ada@example.com"]);
check("version 1 template has no lud16 tag", signature_template(card_hash(card), g, 1700000000).tags.length, 2);

try {
  compute_payouts(card, "1e3");
  failures += 1;
  console.error("FAIL compute_payouts(\"1e3\") did not throw");
} catch (error) {
  check("thrown error is a plain { code, message, details } object", Object.keys(error).sort(), [
    "code",
    "details",
    "message",
  ]);
  check("thrown error code", error.code, "invalid_amount");
  check("thrown error details", error.details, { value: "1e3" });
  check("thrown error message is a string", typeof error.message, "string");
}

if (failures > 0) {
  console.error(`${failures} smoke check(s) failed`);
  process.exit(1);
}
console.log("smoke test passed");
