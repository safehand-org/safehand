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
  const tags = [
    ["u", url],
    ["method", method],
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
  /** Sign the request. Fails with `auth_missing` when there is no key. */
  signed?: boolean;
};

export async function api<T>(method: string, path: string, options: Options = {}): Promise<T> {
  const url = `${window.location.origin}/api${path}`;
  const body =
    options.json === undefined ? null : new TextEncoder().encode(JSON.stringify(options.json));
  const headers: Record<string, string> = {};
  if (body) headers["Content-Type"] = "application/json";
  if (options.signed) {
    const identity = await currentIdentity();
    if (!identity) throw new ApiError(401, "auth_missing", "Create a login first.");
    headers.Authorization = await authorization(identity, method, url, body);
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

export type Me = {
  pubkey: string;
  profile: Profile | null;
  teams: unknown[];
  needs_signature: unknown;
  summary: { received_sats: number; paid_team_count: number; cancelled_count: number };
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
