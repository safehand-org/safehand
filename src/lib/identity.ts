// The member's Nostr key, kept only in this browser, for those who do not
// use a NIP-07 extension (see `@/lib/signer`).
//
// The secret key is never sent to the server: requests are signed with it
// locally (NIP-98, see `@/lib/api`). Losing it loses the account, which is
// why signup shows its `nsec` backup before anything else.

import { chooseSigner } from "./signer";
import { loadWasm, randomBytes } from "./wasm";

const KEY_STORAGE = "safehand.key";

export type Identity = { secret: string; pubkey: string };

function readStored(): string | null {
  try {
    return window.localStorage.getItem(KEY_STORAGE);
  } catch {
    return null;
  }
}

function store(secret: string) {
  try {
    window.localStorage.setItem(KEY_STORAGE, secret);
  } catch {
    throw new Error("This browser is blocking storage, so your login can't be saved here.");
  }
}

/** The stored identity, or null if this browser has none. */
export async function currentIdentity(): Promise<Identity | null> {
  const stored = readStored();
  if (!stored) return null;
  const wasm = await loadWasm();
  try {
    const secret = wasm.parse_secret_key(stored);
    return { secret, pubkey: wasm.public_key(secret) };
  } catch {
    return null;
  }
}

/** Whether a key is stored, without loading the WebAssembly package. */
export function hasStoredKey(): boolean {
  return readStored() !== null;
}

/** Creates and stores a new key. */
export async function createIdentity(): Promise<Identity> {
  const wasm = await loadWasm();
  // A random 32 bytes is a valid key except with negligible probability.
  for (;;) {
    const hex = Array.from(randomBytes(), (byte) => byte.toString(16).padStart(2, "0")).join("");
    try {
      const secret = wasm.parse_secret_key(hex);
      store(secret);
      chooseSigner("local");
      return { secret, pubkey: wasm.public_key(secret) };
    } catch {
      continue;
    }
  }
}

/**
 * Stores a key the member pasted back in (`nsec1…` or 64 hex characters).
 * Returns null if it is not a valid key.
 */
export async function restoreIdentity(input: string): Promise<Identity | null> {
  const wasm = await loadWasm();
  let secret: string;
  try {
    secret = wasm.parse_secret_key(input.trim());
  } catch {
    return null;
  }
  store(secret);
  chooseSigner("local");
  return { secret, pubkey: wasm.public_key(secret) };
}

/** The `nsec` backup of an identity. */
export async function backupKey(identity: Identity): Promise<string> {
  const wasm = await loadWasm();
  return wasm.secret_key_nsec(identity.secret);
}

