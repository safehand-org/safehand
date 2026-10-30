// Creating a version 2 card and signing it, as the last wizard step runs it:
// save the creator's profile, create the team, build the card in
// splitcard-core, submit it, then sign it with the creator's Lightning
// Address. Every step records its result in `CreateProgress`, so a retry
// resumes at the step that failed and never creates a second team.
//
// The steps' effects are passed in (`CreateDeps`), so this file has no
// browser or network code and runs under `node --test`.

import type { Wasm } from "./wasm";

type NewCardInput = Parameters<Wasm["new_card"]>[0];
type UnsignedSignatureEvent = ReturnType<Wasm["signature_template"]>;

export type CreateStepId = "profile" | "team" | "card" | "submit" | "sign";

export const CREATE_STEPS: { id: CreateStepId; label: string }[] = [
  { id: "profile", label: "Saving your Lightning address" },
  { id: "team", label: "Creating your team" },
  { id: "card", label: "Building the card" },
  { id: "submit", label: "Saving the card" },
  { id: "sign", label: "Signing with your Nostr key" },
];

export type CreatedTeam = { id: string; slug: string };

export type CreateProgress = {
  done: CreateStepId[];
  team: CreatedTeam | null;
  /** The canonical card JSON, exactly as built. */
  cardJson: string | null;
  cardHash: string | null;
};

export const NO_PROGRESS: CreateProgress = { done: [], team: null, cardJson: null, cardHash: null };

export type CreateInput = {
  /** The creator's profile name. */
  name: string;
  teamName: string;
  event: string;
  lightningAddress: string;
  /** Members in card order, the creator first, pubkeys in hex. */
  members: NewCardInput["members"];
  /** Whole percent a leaver keeps; undefined for "nothing". */
  leaveKeepPercent?: number;
};

export type SignDeps = {
  pubkey(): Promise<string>;
  template(hash: string, pubkey: string, createdAt: number, address: string): UnsignedSignatureEvent;
  sign(unsigned: UnsignedSignatureEvent): Promise<string>;
  post(hash: string, eventJson: string): Promise<unknown>;
  /** Unix seconds. */
  now(): number;
};

export type CreateDeps = {
  saveProfile(name: string, lightningAddress: string): Promise<unknown>;
  createTeam(team: { slug: string; name: string; event_label: string }): Promise<CreatedTeam>;
  newCard(input: NewCardInput): string;
  validateCard(cardJson: string): void;
  cardHash(cardJson: string): string;
  submitCard(teamId: string, cardJson: string): Promise<{ hash: string }>;
  signer: SignDeps;
  /** A random id for the card, and random slug suffixes. */
  randomId(): string;
};

/** A failed step: what failed, why, and the progress to retry from. */
export class CreateStepError extends Error {
  readonly step: CreateStepId;
  readonly reason: unknown;
  readonly progress: CreateProgress;

  constructor(step: CreateStepId, cause: unknown, progress: CreateProgress) {
    super(cause instanceof Error ? cause.message : String(cause));
    this.step = step;
    this.reason = cause;
    this.progress = progress;
  }
}

/** The `code` of an API or WASM error, if it has one. */
export function errorCode(error: unknown): string | null {
  if (typeof error === "object" && error !== null && "code" in error) {
    const code = (error as { code: unknown }).code;
    return typeof code === "string" ? code : null;
  }
  return null;
}

const MAX_SLUG = 32;

/** A team slug from its name: 3-32 characters of a-z, 0-9 and dashes. */
export function teamSlug(teamName: string): string {
  const base = teamName
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, MAX_SLUG)
    .replace(/-+$/, "");
  return base.length >= 3 ? base : `team-${base || "split"}`;
}

/** The slug with a random suffix, for when the plain one is taken. */
export function suffixedSlug(slug: string, suffix: string): string {
  const tail = `-${suffix.toLowerCase().replace(/[^a-z0-9]/g, "").slice(0, 6)}`;
  return `${slug.slice(0, MAX_SLUG - tail.length).replace(/-+$/, "")}${tail}`;
}

const SLUG_ATTEMPTS = 5;

/** Signs a card for the signed-in member, with their payout address. */
export async function signCard(deps: SignDeps, hash: string, lightningAddress: string) {
  const pubkey = await deps.pubkey();
  const unsigned = deps.template(hash, pubkey, deps.now(), lightningAddress);
  const signed = await deps.sign(unsigned);
  await deps.post(hash, signed);
}

/**
 * Runs the steps not yet in `from.done`, calling `onStep` as each starts.
 * Resolves to the finished progress, or throws a `CreateStepError` holding
 * the progress to pass back in to retry.
 */
export async function createAndSign(
  input: CreateInput,
  from: CreateProgress,
  deps: CreateDeps,
  onStep: (step: CreateStepId) => void = () => {},
): Promise<CreateProgress> {
  let progress = { ...from, done: [...from.done] };
  const run = async (step: CreateStepId, action: () => Promise<Partial<CreateProgress>>) => {
    if (progress.done.includes(step)) return;
    onStep(step);
    try {
      const result = await action();
      progress = { ...progress, ...result, done: [...progress.done, step] };
    } catch (error) {
      throw new CreateStepError(step, error, progress);
    }
  };

  await run("profile", async () => {
    await deps.saveProfile(input.name, input.lightningAddress);
    return {};
  });

  await run("team", async () => {
    const slug = teamSlug(input.teamName);
    for (let attempt = 0; ; attempt += 1) {
      const candidate = attempt === 0 ? slug : suffixedSlug(slug, deps.randomId());
      try {
        const team = await deps.createTeam({
          slug: candidate,
          name: input.teamName,
          event_label: input.event,
        });
        return { team: { id: team.id, slug: team.slug } };
      } catch (error) {
        if (errorCode(error) !== "team_slug_taken" || attempt + 1 >= SLUG_ATTEMPTS) throw error;
      }
    }
  });

  await run("card", async () => {
    const cardJson = deps.newCard({
      card_id: deps.randomId(),
      event: input.event,
      team: input.teamName,
      members: input.members,
      ...(input.leaveKeepPercent === undefined
        ? {}
        : { leave_keep_percent: input.leaveKeepPercent }),
    });
    deps.validateCard(cardJson);
    return { cardJson, cardHash: deps.cardHash(cardJson) };
  });

  await run("submit", async () => {
    const { team, cardJson, cardHash } = progress;
    if (!team || !cardJson || !cardHash) throw new Error("The card was not built.");
    const stored = await deps.submitCard(team.id, cardJson);
    if (stored.hash !== cardHash) {
      throw Object.assign(new Error("The server saved a different card. Nothing was signed."), {
        code: "card_hash_mismatch",
      });
    }
    return {};
  });

  await run("sign", async () => {
    if (!progress.cardHash) throw new Error("The card was not built.");
    await signCard(deps.signer, progress.cardHash, input.lightningAddress);
    return {};
  });

  return progress;
}
