import assert from "node:assert/strict";
import { test } from "node:test";
import { errorMessage } from "@/lib/errors";

// Every code the v2 card flow adds gets its own plain-English message.
const NEW_CODES = [
  "secret_key_rejected",
  "secret_key_not_public",
  "invalid_public_key",
  "lightning_address_unreachable",
  "profile_required",
  "submitter_not_member",
  "card_declined",
  "card_not_draft",
  "not_a_card_member",
  "already_signed",
  "missing_lud16_tag",
  "multiple_lud16_tags",
  "invalid_lud16",
  "unexpected_lud16_tag",
  "invalid_member_label",
  "invalid_leave_keep_percent",
  "invalid_template_lightning_address",
  "invalid_card_input",
  "card_hash_mismatch",
  "extension_missing",
  "signing_refused",
];

test("each new error code has its own message", () => {
  for (const code of NEW_CODES) {
    const server = `server text for ${code}`;
    const message = errorMessage(Object.assign(new Error(server), { code }));
    assert.notEqual(message, server, code);
    assert.ok(message.length > 10, code);
  }
});

test("unknown codes fall back to the error's message", () => {
  assert.equal(errorMessage(Object.assign(new Error("Plain."), { code: "nope" })), "Plain.");
  assert.equal(errorMessage("x"), "Something went wrong. Try again.");
});
