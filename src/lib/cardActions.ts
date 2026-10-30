// The real effects behind `@/lib/createCard`: the API, splitcard-wasm and
// the browser's signer.

import { createTeam, postSignature, putProfile, submitCard } from "./api";
import { signCard, type CreateDeps, type SignDeps } from "./createCard";
import { currentSigner } from "./signer";
import { loadWasm, type Wasm } from "./wasm";

function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function signer() {
  const found = await currentSigner();
  if (!found) {
    throw Object.assign(new Error("Sign in first."), { code: "auth_missing" });
  }
  return found;
}

export function signDeps(wasm: Wasm): SignDeps {
  return {
    pubkey: async () => (await signer()).getPublicKey(),
    template: (hash, pubkey, createdAt, address) =>
      wasm.signature_template(hash, pubkey, createdAt, address),
    sign: async (unsigned) => {
      const found = await signer();
      try {
        return await found.signEvent(unsigned);
      } catch (error) {
        throw Object.assign(
          new Error(error instanceof Error ? error.message : "Signing was cancelled."),
          { code: "signing_refused" },
        );
      }
    },
    post: postSignature,
    now: () => Math.floor(Date.now() / 1000),
  };
}

export async function createDeps(): Promise<CreateDeps> {
  const wasm = await loadWasm();
  return {
    saveProfile: (name, lightningAddress) =>
      putProfile({ name, lightning_address: lightningAddress }),
    createTeam,
    newCard: (input) => wasm.new_card(input),
    validateCard: (cardJson) => wasm.validate_card(cardJson),
    cardHash: (cardJson) => wasm.card_hash(cardJson),
    submitCard,
    signer: signDeps(wasm),
    randomId: () => hex(crypto.getRandomValues(new Uint8Array(8))),
  };
}

/**
 * Signs a card someone else created: saves the member's payout address on
 * their profile when it changed, then signs with it.
 */
export async function signAsMember(
  hash: string,
  profile: { name: string; lightning_address?: string | null },
  lightningAddress: string,
) {
  const address = lightningAddress.trim();
  if (profile.lightning_address !== address) {
    await putProfile({ name: profile.name, lightning_address: address });
  }
  await signCard(signDeps(await loadWasm()), hash, address);
}
