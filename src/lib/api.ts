// The Split Card API, called through this site's `/api` proxy.
//
// Requests are signed with NIP-98: the event covers the exact URL, method
// and body bytes, so the body is encoded once and those same bytes are sent.
// The URL is this page's origin plus `/api`, which is the server's
// PUBLIC_BASE_URL in every deployment.

import { currentIdentity, type Identity } from "./identity";
import { loadWasm, randomBytes } from "./wasm";

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
  identity: Identity,
  method: string,
  url: string,
  body: Uint8Array<ArrayBuffer> | null,
): Promise<string> {
  const wasm = await loadWasm();
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
  const unsigned = {
    pubkey: identity.pubkey,
    created_at: Math.floor(Date.now() / 1000),
    kind: NIP98_KIND,
    tags,
    content: "",
  };
  const event = wasm.sign_event(identity.secret, JSON.stringify(unsigned), randomBytes());
  return `Nostr ${base64(event)}`;
}

type Options = {
  /** JSON body, serialized once and sent as signed. */
  json?: unknown;
  /**
   * Sign the request: `true` fails with `auth_missing` when there is no
   * key, `"if-available"` sends it unsigned instead.
   */
  signed?: boolean | "if-available";
};

export async function api<T>(method: string, path: string, options: Options = {}): Promise<T> {
  const url = `${window.location.origin}/api${path}`;
  const body =
    options.json === undefined ? null : new TextEncoder().encode(JSON.stringify(options.json));
  const headers: Record<string, string> = {};
  if (body) headers["Content-Type"] = "application/json";
  if (options.signed) {
    const identity = await currentIdentity();
    if (identity) {
      headers.Authorization = await authorization(identity, method, url, body);
    } else if (options.signed === true) {
      throw new ApiError(401, "auth_missing", "Create a login first.");
    }
  }

  let response: Response;
  try {
    response = await fetch(url, { method, headers, body, cache: "no-store" });
  } catch {
    throw new ApiError(0, "network_error", "Couldn't reach Safehand. Check your connection.");
  }
  const text = await response.text();
  let data: unknown = null;
  try {
    data = text ? JSON.parse(text) : null;
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
  summary: { received_sats: string; paid_team_count: number };
};

/** Someone who joined a team. Addresses are shown only to teammates. */
export type Participant = {
  pubkey: string;
  name: string;
  lightning_address?: string;
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
  created_at: number;
  participants: Participant[];
};

export type CardView = {
  hash: string;
  team_id: string;
  status: "draft" | "active" | "superseded" | "stale";
  /** The card as a JSON string; parse it. */
  card: string;
  created_by: string;
  created_at: number;
  activated_at: number | null;
  signing: { state: "draft" | "active"; signed: string[]; missing: string[] };
  /** Stored signature events, as JSON strings. */
  signatures: string[];
};

/** A card's JSON, parsed. Weights fit in JS numbers in this product. */
export type CardJson = {
  card_id: string;
  event: string;
  team: string;
  members: {
    pubkey: string;
    name: string;
    weight: number;
    payout: { lightning: { address: string } };
  }[];
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
