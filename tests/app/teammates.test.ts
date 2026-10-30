import assert from "node:assert/strict";
import { test } from "node:test";
import { cardLabel, checkTeammateKey, SECRET_KEY_WARNING } from "@/lib/teammates";
import wasm from "./wasm";

const SECRET = "0000000000000000000000000000000000000000000000000000000000000003";
const own = wasm.public_key(SECRET);
const other = "c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5";

test("an npub or hex key is accepted as the teammate's hex key", () => {
  const npub = wasm.npub_encode(other);
  assert.ok(npub.startsWith("npub1"));
  assert.deepEqual(checkTeammateKey(wasm.parse_public_key, npub, own, []), {
    ok: true,
    pubkey: other,
  });
  assert.deepEqual(checkTeammateKey(wasm.parse_public_key, ` ${other} `, own, []), {
    ok: true,
    pubkey: other,
  });
});

test("an nsec is refused with a warning and the field is cleared", () => {
  const nsec = wasm.secret_key_nsec(SECRET);
  assert.deepEqual(checkTeammateKey(wasm.parse_public_key, nsec, own, []), {
    ok: false,
    message: SECRET_KEY_WARNING,
    clear: true,
  });
  assert.equal(SECRET_KEY_WARNING, "That's a secret key. Never share it.");
});

test("your own npub, duplicates and junk are refused", () => {
  const ownNpub = wasm.npub_encode(own);
  const self = checkTeammateKey(wasm.parse_public_key, ownNpub, own, []);
  assert.equal(self.ok, false);
  const twice = checkTeammateKey(wasm.parse_public_key, other, own, [{ pubkey: other, name: "Bob" }]);
  assert.deepEqual(twice, { ok: false, message: "Bob is already on the card.", clear: false });
  const junk = checkTeammateKey(wasm.parse_public_key, "npub1nope", own, []);
  assert.equal(junk.ok, false);
  assert.equal(!junk.ok && junk.clear, false);
});

test("card labels are trimmed and cut to 40 characters", () => {
  assert.equal(cardLabel("  Ada  "), "Ada");
  assert.equal(cardLabel("x".repeat(50)).length, 40);
  assert.equal(Array.from(cardLabel("🦙".repeat(45))).length, 40);
});
