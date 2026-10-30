// Checks a key typed into the "add teammate" field. Parsing is
// splitcard-wasm's `parse_public_key`, passed in so this runs under tests.

import { errorCode } from "./createCard";

export const SECRET_KEY_WARNING = "That's a secret key. Never share it.";

/** Longest name the card takes for a member (splitcard-core's limit). */
export const MAX_LABEL_CHARS = 40;

export type TeammateKeyCheck =
  | { ok: true; pubkey: string }
  /** `clear`: empty the field, because what was typed must not linger. */
  | { ok: false; message: string; clear: boolean };

export function checkTeammateKey(
  parsePublicKey: (input: string) => string,
  input: string,
  ownPubkey: string | null,
  existing: { pubkey: string; name: string }[],
): TeammateKeyCheck {
  let pubkey: string;
  try {
    pubkey = parsePublicKey(input);
  } catch (error) {
    if (errorCode(error) === "secret_key_not_public") {
      return { ok: false, message: SECRET_KEY_WARNING, clear: true };
    }
    return { ok: false, message: "That isn't a valid npub. It starts with npub1.", clear: false };
  }
  if (pubkey === ownPubkey) {
    return { ok: false, message: "That's your own npub. You're already on the card.", clear: false };
  }
  const already = existing.find((member) => member.pubkey === pubkey);
  if (already) {
    return { ok: false, message: `${already.name} is already on the card.`, clear: false };
  }
  return { ok: true, pubkey };
}

/** A name for the card: trimmed and cut to the card's limit. */
export function cardLabel(name: string): string {
  return Array.from(name.trim()).slice(0, MAX_LABEL_CHARS).join("");
}
