// Who signs for this browser: a Nostr key kept here, or a NIP-07 browser
// extension that holds the key itself. Either way the secret key never
// leaves the browser; Safehand's server only ever sees signed events.

import { currentIdentity, hasStoredKey, type Identity } from "./identity";
import { loadWasm, randomBytes } from "./wasm";

/** A NIP-01 event before signing. `pubkey` is optional. */
export type UnsignedEvent = {
  pubkey?: string;
  created_at: number;
  kind: number;
  tags: string[][];
  content: string;
};

export interface Signer {
  /** The signer's public key, as 64 lowercase hex characters. */
  getPublicKey(): Promise<string>;
  /** Signs an event and returns it as JSON. */
  signEvent(unsigned: UnsignedEvent): Promise<string>;
}

/** `window.nostr`, as NIP-07 extensions provide it. */
type Nip07 = {
  getPublicKey(): Promise<string>;
  signEvent(event: Omit<UnsignedEvent, "pubkey">): Promise<{ pubkey?: unknown }>;
};

export type SignerKind = "local" | "nip07";

const SIGNER_STORAGE = "safehand.signer";

/** The signer this browser chose, without loading anything. */
export function storedSignerKind(): SignerKind | null {
  let chosen: string | null = null;
  try {
    chosen = window.localStorage.getItem(SIGNER_STORAGE);
  } catch {
    // Storage blocked: fall back to whether a key is stored.
  }
  if (chosen === "nip07") return "nip07";
  return hasStoredKey() ? "local" : null;
}

/** Whether this browser can sign in, by either signer. */
export function hasSigner(): boolean {
  return storedSignerKind() !== null;
}

let cached: Promise<Signer | null> | null = null;

/** Records which signer to use from now on. */
export function chooseSigner(kind: SignerKind) {
  try {
    window.localStorage.setItem(SIGNER_STORAGE, kind);
  } catch {
    throw new Error("This browser is blocking storage, so your login can't be saved here.");
  }
  cached = null;
}

export function localSigner(identity: Identity): Signer {
  return {
    getPublicKey: async () => identity.pubkey,
    async signEvent(unsigned) {
      const wasm = await loadWasm();
      return wasm.sign_event(identity.secret, JSON.stringify(unsigned), randomBytes());
    },
  };
}

function extension(): Nip07 | null {
  return (window as { nostr?: Nip07 }).nostr ?? null;
}

/** Extensions inject `window.nostr` after the page starts; wait briefly. */
export async function waitForExtension(timeoutMs = 1500): Promise<Nip07 | null> {
  const started = Date.now();
  for (;;) {
    const found = extension();
    if (found || Date.now() - started >= timeoutMs) return found;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

export function nip07Signer(nostr: Nip07): Signer {
  let pubkey: Promise<string> | null = null;
  const getPublicKey = () => {
    pubkey ??= nostr.getPublicKey().then(async (key) => (await loadWasm()).parse_public_key(key));
    // Let a refused prompt be asked again.
    pubkey.catch(() => {
      pubkey = null;
    });
    return pubkey;
  };
  return {
    getPublicKey,
    async signEvent(unsigned) {
      const expected = await getPublicKey();
      // The extension fills in its own pubkey.
      const { created_at, kind, tags, content } = unsigned;
      const signed = await nostr.signEvent({ created_at, kind, tags, content });
      if (signed.pubkey !== expected) {
        throw new Error("Your Nostr extension signed with a different key. Sign in again.");
      }
      return JSON.stringify(signed);
    },
  };
}

async function loadSigner(): Promise<Signer | null> {
  const kind = storedSignerKind();
  if (kind === "nip07") {
    const nostr = await waitForExtension();
    return nostr ? nip07Signer(nostr) : null;
  }
  if (kind === "local") {
    const identity = await currentIdentity();
    return identity ? localSigner(identity) : null;
  }
  return null;
}

/** The chosen signer, or null if there is none or the extension is missing. */
export function currentSigner(): Promise<Signer | null> {
  if (!cached) {
    cached = loadSigner();
    cached.catch(() => {
      cached = null;
    });
  }
  return cached;
}

/** The signed-in public key, or null. */
export async function currentPubkey(): Promise<string | null> {
  const signer = await currentSigner();
  return signer ? signer.getPublicKey().catch(() => null) : null;
}

/**
 * The signed-in member's `npub`, encoded in WebAssembly from the signer's
 * public key. When `serverPubkey` is set (from `GET /me`) and it does not
 * match, returns null and logs: the wrong key is signed in.
 * Never returns an `nsec`.
 */
export async function currentNpub(serverPubkey?: string): Promise<string | null> {
  const pubkey = await currentPubkey();
  if (!pubkey) return null;
  if (serverPubkey !== undefined && pubkey !== serverPubkey) {
    console.error("Wrong key signed in: local public key does not match GET /me.");
    return null;
  }
  const wasm = await loadWasm();
  try {
    return wasm.npub_encode(pubkey);
  } catch (error) {
    console.error("Could not encode npub from the signed-in public key.", error);
    return null;
  }
}

/**
 * Signs in with a NIP-07 extension. Resolves to an error message, or null
 * once signed in.
 */
export async function connectExtension(): Promise<string | null> {
  const nostr = await waitForExtension();
  if (!nostr) {
    return "No Nostr extension found. Install one such as Alby or nos2x, then reload this page.";
  }
  try {
    await nip07Signer(nostr).getPublicKey();
  } catch {
    return "Your Nostr extension didn't share your public key. Try again.";
  }
  chooseSigner("nip07");
  return null;
}
