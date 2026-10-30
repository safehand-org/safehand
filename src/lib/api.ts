// The Split Card API, called through this site's `/api` proxy.
//
// Requests are signed with NIP-98: the event covers the exact URL, method
// and body bytes, so the body is encoded once and those same bytes are sent.
// The URL is this page's origin plus `/api`, which is the server's
// PUBLIC_BASE_URL in every deployment.

import { currentSigner, storedSignerKind, type Signer } from "./signer";
import { randomBytes } from "./wasm";

/** An error response from the server: `{ code, message, details }`. */
export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
    readonly details: unknown = null,
  ) {
    super(message);
  }
}

const NIP98_KIND = 27235;

function hex(bytes: ArrayBuffer): string {
  return Array.from(new Uint8Array(bytes), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function base64(text: string): string {
  let binary = "";
  for (const byte of new TextEncoder().encode(text)) binary += String.fromCharCode(byte);
  return btoa(binary);
}

async function authorization(
  signer: Signer,
  method: string,
  url: string,
  body: Uint8Array<ArrayBuffer> | null,
): Promise<string> {
  // The event id must be unique (the server refuses replays), and two
  // requests to the same URL in the same second would otherwise share one.
  const tags = [
    ["u", url],
    ["method", method],
    ["nonce", hex(randomBytes().buffer as ArrayBuffer)],
  ];
  if (body && body.length > 0) {
    tags.push(["payload", hex(await crypto.subtle.digest("SHA-256", body))]);
  }
  const event = await signer.signEvent({
    pubkey: await signer.getPublicKey(),
    created_at: Math.floor(Date.now() / 1000),
    kind: NIP98_KIND,
    tags,
    content: "",
  });
  return `Nostr ${base64(event)}`;
}

type Options = {
  /** JSON body, serialized once and sent as signed. */
  json?: unknown;
  /**
   * A body sent byte for byte, such as a card or a signed event, which
   * travel as exact strings and are never re-serialized.
   */
  raw?: string;
  /**
   * Sign the request: `true` fails with `auth_missing` when there is no
   * key, `"if-available"` sends it unsigned instead.
   */
  signed?: boolean | "if-available";
};

export async function api<T>(method: string, path: string, options: Options = {}): Promise<T> {
  const url = `${window.location.origin}/api${path}`;
  const text =
    options.raw ?? (options.json === undefined ? undefined : JSON.stringify(options.json));
  const body = text === undefined ? null : new TextEncoder().encode(text);
  const headers: Record<string, string> = {};
  if (body) headers["Content-Type"] = "application/json";
  if (options.signed) {
    const signer = await currentSigner();
    if (signer) {
      try {
        headers.Authorization = await authorization(signer, method, url, body);
      } catch (error) {
        throw new ApiError(
          401,
          "signing_refused",
          error instanceof Error ? error.message : "Your request wasn't signed.",
        );
      }
    } else if (options.signed === true) {
      throw storedSignerKind() === "nip07"
        ? new ApiError(
            401,
            "extension_missing",
            "Your Nostr extension isn't available. Unlock it, or sign in again.",
          )
        : new ApiError(401, "auth_missing", "Create a login first.");
    }
  }

  let response: Response;
  try {
    response = await fetch(url, { method, headers, body, cache: "no-store" });
  } catch {
    throw new ApiError(0, "network_error", "Couldn't reach Safehand. Check your connection.");
  }
  const reply = await response.text();
  let data: unknown = null;
  try {
    data = reply ? JSON.parse(reply) : null;
  } catch {
    // Not JSON; handled below.
  }
  if (!response.ok) {
    const error = (data ?? {}) as { code?: string; message?: string; details?: unknown };
    throw new ApiError(
      response.status,
      error.code ?? "http_error",
      error.message ?? `Request failed (${response.status}).`,
      error.details ?? null,
    );
  }
  return data as T;
}

// Responses.

export type Profile = {
  pubkey: string;
  name: string;
  /** Absent until set, and for profiles that are not yours or a teammate's. */
  lightning_address?: string;
  bio?: string;
  updated_at?: number;
};

/** One of the viewer's teams, from `GET /me`. Sats are decimal strings. */
export type TeamSummary = {
  team_id: string;
  slug: string;
  name: string;
  event_label: string;
  list_status: "draft" | "locked" | "paid-out";
  phase: "waiting-for-prize" | "prize-set-waiting" | "payouts-in-progress" | null;
  you_received_sats: string | null;
  card_hash: string | null;
  signed_count: number | null;
  member_count: number | null;
  participant_count: number;
  you_must_sign: boolean;
  joined_at: number;
  updated_at: number;
};

/** The newest card waiting for the viewer's signature. */
export type NeedsSignature = {
  team_id: string;
  slug: string;
  name: string;
  card_hash: string;
  inviter_name: string;
  /** The viewer's share; 10,000 is the whole prize. */
  share_basis_points: number;
  member_count: number;
  invited_at: number;
  join_code: string;
};

export type Me = {
  pubkey: string;
  profile: Profile | null;
  teams: TeamSummary[];
  needs_signature: NeedsSignature | null;
  pending_cards: PendingCard[];
  summary: { received_sats: string; paid_team_count: number };
};

/** Someone who joined a team. Addresses are shown only to teammates. */
export type Participant = {
  pubkey: string;
  name: string;
  lightning_address?: string | null;
  joined_at: number;
};

export type TeamView = {
  id: string;
  slug: string;
  name: string;
  event_label: string;
  /** Only for participants. */
  join_code?: string | null;
  active_card_hash: string | null;
  /** The newest draft that would replace the active card (or be the first). */
  draft_card_hash: string | null;
  created_at: number;
  participants: Participant[];
};

export type CardView = {
  hash: string;
  team_id: string;
  status: "draft" | "active" | "superseded" | "stale" | "declined";
  /** The card as a JSON string; parse it. */
  card: string;
  created_by: string;
  created_at: number;
  activated_at: number | null;
  signing: { state: "draft" | "active"; signed: string[]; missing: string[] };
  /** Stored signature events, as JSON strings. */
  signatures: string[];
  members: CardMember[];
  declined: CardDecline[];
};

export type PayoutItem = {
  index: number;
  pubkey: string;
  name: string;
  lightning_address: string;
  amount_sats: string;
  status: "pending" | "paid";
  paid_at: number | null;
};

export type PayoutView = {
  team_id: string;
  card_hash: string;
  prize_sats: string;
  phase: "waiting-for-prize" | "prize-set-waiting" | "payouts-in-progress";
  agreed_at: number | null;
  paid_count: number;
  items: PayoutItem[];
};

// Calls.

export type ProfileUpdate = {
  name: string;
  /** Omit to keep the stored address. */
  lightning_address?: string;
  /** Omit to keep the stored bio; an empty string clears it. */
  bio?: string;
};

export function putProfile(update: ProfileUpdate): Promise<Profile> {
  return api<Profile>("PUT", "/me/profile", { json: update, signed: true });
}

export function getMe(): Promise<Me> {
  return api<Me>("GET", "/me", { signed: true });
}

/** The team's prize and payouts; participants only. */
export function getPayout(teamId: string): Promise<PayoutView> {
  return api<PayoutView>("GET", `/teams/${encodeURIComponent(teamId)}/payout`, { signed: true });
}

// Version 2 cards: members by npub, signing in the app.

/** A draft that lists the viewer and waits for their signature. */
export type PendingCard = {
  team_id: string;
  slug: string;
  name: string;
  card_hash: string;
  creator_name: string;
  /** The viewer's share; 10,000 is the whole prize. */
  share_basis_points: number;
  created_at: number;
};

/** One member of a card, as the server describes it. */
export type CardMember = {
  pubkey: string;
  npub: string;
  /** The card's label, else their profile name, else null. */
  name: string | null;
  /** A whole number, as a string. */
  weight: string;
  share_basis_points: number;
  signed: boolean;
  declined: boolean;
  /** Where their share is paid; null until a version 2 member signs. */
  lightning_address: string | null;
};

export type CardDecline = { pubkey: string; name: string | null; declined_at: number };

/** The parts of a team the create flow needs. */
export type CreatedTeam = { id: string; slug: string };

export function getProfile(pubkey: string): Promise<Profile> {
  return api<Profile>("GET", `/profiles/${encodeURIComponent(pubkey)}`, { signed: "if-available" });
}

export function createTeam(team: {
  slug: string;
  name: string;
  event_label: string;
}): Promise<TeamView> {
  return api<TeamView>("POST", "/teams", { json: team, signed: true });
}

export function getTeamBySlug(slug: string): Promise<TeamView> {
  return api<TeamView>("GET", `/teams/by-slug/${encodeURIComponent(slug)}`, {
    signed: "if-available",
  });
}

/** Submits a card exactly as built: the string is the body. */
export function submitCard(teamId: string, cardJson: string): Promise<CardView> {
  return api<CardView>("POST", `/teams/${encodeURIComponent(teamId)}/cards`, {
    raw: cardJson,
    signed: true,
  });
}

export function getCard(hash: string): Promise<CardView> {
  return api<CardView>("GET", `/cards/${encodeURIComponent(hash)}`, { signed: "if-available" });
}

/** Posts a signed card event exactly as signed. */
export function postSignature(hash: string, eventJson: string): Promise<CardView> {
  return api<CardView>("POST", `/cards/${encodeURIComponent(hash)}/signatures`, {
    raw: eventJson,
  });
}

export function declineCard(hash: string): Promise<CardView> {
  return api<CardView>("POST", `/cards/${encodeURIComponent(hash)}/decline`, { signed: true });
}
