import assert from "node:assert/strict";
import { test } from "node:test";
import {
  createAndSign,
  CreateStepError,
  NO_PROGRESS,
  suffixedSlug,
  teamSlug,
  type CreateDeps,
  type CreateInput,
} from "@/lib/createCard";
import wasm from "./wasm";

const SECRET = "0000000000000000000000000000000000000000000000000000000000000003";
const CREATOR = wasm.public_key(SECRET);
const OTHER = "c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5";

const INPUT: CreateInput = {
  name: "Ada",
  teamName: "Llamas",
  event: "Nostr Hack",
  lightningAddress: "ada@example.com",
  members: [
    { pubkey: CREATOR, weight: 1, label: "Ada" },
    { pubkey: OTHER, weight: 1, label: "Bob" },
  ],
  leaveKeepPercent: 15,
};

/** Real splitcard-wasm and a fake server that records every call. */
function fakes(failures: Partial<Record<string, unknown[]>> = {}) {
  const calls: string[] = [];
  const posted: { hash: string; event: string }[] = [];
  const fail = (name: string) => {
    const queue = failures[name];
    if (queue && queue.length > 0) throw queue.shift();
  };
  let ids = 0;
  const deps: CreateDeps = {
    saveProfile: async (name, address) => {
      calls.push(`profile ${name} ${address}`);
      fail("profile");
    },
    createTeam: async (team) => {
      calls.push(`team ${team.slug}`);
      fail("team");
      return { id: "7", slug: team.slug };
    },
    newCard: (input) => wasm.new_card(input),
    validateCard: (json) => wasm.validate_card(json),
    cardHash: (json) => wasm.card_hash(json),
    submitCard: async (teamId, json) => {
      calls.push(`submit ${teamId}`);
      fail("submit");
      return { hash: (failures.wrongHash ? "0".repeat(64) : wasm.card_hash(json)) };
    },
    signer: {
      pubkey: async () => CREATOR,
      template: (hash, pubkey, createdAt, address) =>
        wasm.signature_template(hash, pubkey, createdAt, address),
      sign: async (unsigned) => {
        fail("sign");
        return wasm.sign_event(SECRET, JSON.stringify(unsigned), new Uint8Array(32).fill(7));
      },
      post: async (hash, event) => {
        calls.push("signature");
        fail("post");
        posted.push({ hash, event });
      },
      now: () => 1_800_000_000,
    },
    randomId: () => `id${(ids += 1)}`,
  };
  return { deps, calls, posted };
}

test("runs every step in order and signs a valid v2 card with the payout address", async () => {
  const { deps, calls, posted } = fakes();
  const done = await createAndSign(INPUT, NO_PROGRESS, deps);
  assert.deepEqual(calls, ["profile Ada ada@example.com", "team llamas", "submit 7", "signature"]);
  assert.deepEqual(done.done, ["profile", "team", "card", "submit", "sign"]);
  assert.ok(done.cardJson && done.cardHash);
  assert.equal(wasm.card_hash(done.cardJson), done.cardHash);
  assert.match(done.cardJson, /"leave":\{"keep_percent":15,"kind":"keep_percent"\}/);
  const signer = wasm.verify_signature(done.cardJson, posted[0].event);
  assert.deepEqual(signer, { pubkey: CREATOR, lightning_address: "ada@example.com" });
});

test("a retry resumes at the failed step and never creates a second team", async () => {
  const refused = Object.assign(new Error("cancelled"), { code: "signing_refused" });
  const { deps, calls } = fakes({ sign: [refused] });
  let progress = NO_PROGRESS;
  await assert.rejects(createAndSign(INPUT, progress, deps), (error: unknown) => {
    assert.ok(error instanceof CreateStepError);
    assert.equal(error.step, "sign");
    progress = error.progress;
    return true;
  });
  assert.deepEqual(progress.done, ["profile", "team", "card", "submit"]);
  const finished = await createAndSign(INPUT, progress, deps);
  assert.equal(calls.filter((call) => call.startsWith("team")).length, 1);
  assert.equal(calls.filter((call) => call.startsWith("submit")).length, 1);
  assert.equal(finished.cardHash, progress.cardHash, "the same card is signed");
});

test("a taken slug is retried with a suffix", async () => {
  const taken = Object.assign(new Error("taken"), { code: "team_slug_taken" });
  const { deps, calls } = fakes({ team: [taken] });
  const done = await createAndSign(INPUT, NO_PROGRESS, deps);
  assert.deepEqual(
    calls.filter((call) => call.startsWith("team")),
    ["team llamas", "team llamas-id1"],
  );
  assert.equal(done.team?.slug, "llamas-id1");
});

test("a server that stores a different card stops before signing", async () => {
  const { deps, calls } = fakes({ wrongHash: [true] });
  await assert.rejects(createAndSign(INPUT, NO_PROGRESS, deps), (error: unknown) => {
    assert.ok(error instanceof CreateStepError);
    assert.equal(error.step, "submit");
    assert.equal((error.reason as { code?: string }).code, "card_hash_mismatch");
    return true;
  });
  assert.ok(!calls.includes("signature"));
});

test("slugs fit the server's rules", () => {
  assert.equal(teamSlug("Team 42!"), "team-42");
  assert.equal(teamSlug("Ω"), "team-split");
  assert.equal(teamSlug("ab"), "team-ab");
  assert.equal(teamSlug("x".repeat(40)).length, 32);
  assert.equal(suffixedSlug("x".repeat(32), "ABC123").length, 32);
  assert.match(suffixedSlug("x".repeat(32), "ABC123"), /-abc123$/);
});
