/* tslint:disable */
/* eslint-disable */
/**
 * A card as a JSON string. Only Rust parses it: never `JSON.parse` a card and
 * re-serialize it, because weights above 2^53 - 1 would silently change.
 */
export type CardJson = string;

/** A non-negative integer amount of sats as a decimal string, e.g. `"100001"`. */
export type SatsString = string;

/** One member's payout, in card member order. */
export interface Payout {
    /** Member Nostr pubkey, 64 lowercase hex characters. */
    pubkey: string;
    /** Amount in sats as a decimal string. */
    amount_sats: SatsString;
}

/** A NIP-01 unsigned event, ready for `window.nostr.signEvent`. */
export interface UnsignedSignatureEvent {
    pubkey: string;
    created_at: number;
    kind: number;
    tags: string[][];
    content: string;
}

/**
 * A NIP-01 event to sign with `sign_event`. `pubkey` is optional; when
 * present it must be the signing key's.
 */
export interface UnsignedEvent {
    pubkey?: string;
    created_at: number;
    kind: number;
    tags: string[][];
    content: string;
}

/** A signed NIP-01 event, as a JSON string. */
export type SignedEventJson = string;

/** A JSON array of signed NIP-01 events, as a string. */
export type SignedEventsJson = string;

/** An event that did not count toward a card's status. */
export interface RejectedEvent {
    event_id: string;
    code: SignatureErrorCode;
    message: string;
}

/**
 * A verified signer. `lightning_address` is the payout address from the
 * event's `lud16` tag: always set for a version 2 card, `null` for version 1.
 */
export interface Signer {
    pubkey: string;
    lightning_address: string | null;
}

/** A card's signing status, in card member order. */
export type CardStatus =
| { status: "draft"; signed: Signer[]; missing: string[]; rejected: RejectedEvent[] }
| { status: "active"; signed: Signer[]; rejected: RejectedEvent[] };

/** Every code a Split Card WASM function can throw. */
export type ErrorCode = "invalid_card_json" | "invalid_event_json" | "invalid_amount" | "invalid_created_at" | "output_serialization" | "invalid_card_input" | "invalid_secret_key" | "invalid_public_key" | "secret_key_not_public" | "invalid_aux_rand" | "invalid_unsigned_event" | "event_pubkey_mismatch" | "unsupported_version" | "empty_field" | "invalid_member_count" | "invalid_pubkey" | "duplicate_pubkey" | "empty_member_name" | "zero_weight" | "weight_too_large" | "invalid_lightning_address" | "invalid_member_label" | "invalid_supersedes" | "invalid_peer_review_max_shift_points" | "invalid_leave_keep_percent" | "no_weights" | "split_zero_weight" | "prize_too_small" | "share_rounds_to_zero" | "hash_serialize" | "invalid_card_hash" | "invalid_template_pubkey" | "invalid_template_lightning_address" | "tag_construction" | "invalid_event_id" | "invalid_signature" | "wrong_kind" | "missing_d_tag" | "multiple_d_tags" | "wrong_d_tag_hash" | "missing_splitcard_tag" | "multiple_splitcard_tags" | "wrong_splitcard_tag_hash" | "missing_lud16_tag" | "multiple_lud16_tags" | "invalid_lud16" | "unexpected_lud16_tag" | "non_member" | "invalid_old_card" | "invalid_new_card" | "wrong_supersedes_hash" | "missing_new_member_signatures" | "missing_old_member_signatures";

/** Codes for card validation failures. */
export type ValidationErrorCode =
| "unsupported_version"
| "empty_field"
| "invalid_member_count"
| "invalid_pubkey"
| "duplicate_pubkey"
| "empty_member_name"
| "zero_weight"
| "weight_too_large"
| "invalid_lightning_address"
| "invalid_member_label"
| "invalid_supersedes"
| "invalid_peer_review_max_shift_points"
| "invalid_leave_keep_percent";

/** Codes for signature event failures, as listed in `CardStatus.rejected`. */
export type SignatureErrorCode =
| "invalid_event_id"
| "invalid_signature"
| "wrong_kind"
| "missing_d_tag"
| "multiple_d_tags"
| "wrong_d_tag_hash"
| "missing_splitcard_tag"
| "multiple_splitcard_tags"
| "wrong_splitcard_tag_hash"
| "missing_lud16_tag"
| "multiple_lud16_tags"
| "invalid_lud16"
| "unexpected_lud16_tag"
| "non_member"
| "hash_serialize";

/** `details` for each error code. Any u64 is a decimal string. */
export interface ErrorDetails {
    invalid_card_json: { line: number; column: number };
    invalid_event_json: { line: number; column: number };
    invalid_amount: { value: string };
    invalid_created_at: null;
    output_serialization: null;
    invalid_card_input: null;
    invalid_secret_key: null;
    invalid_public_key: null;
    secret_key_not_public: null;
    invalid_aux_rand: { length: number };
    invalid_unsigned_event: null;
    event_pubkey_mismatch: { expected: string; actual: string };
    unsupported_version: { version: number };
    empty_field: { field: string };
    invalid_member_count: { count: number };
    invalid_pubkey: { index: number };
    duplicate_pubkey: { index: number };
    empty_member_name: { index: number };
    zero_weight: { index: number };
    weight_too_large: { index: number; weight: string };
    invalid_lightning_address: { index: number };
    invalid_member_label: { index: number };
    invalid_supersedes: null;
    invalid_peer_review_max_shift_points: null;
    invalid_leave_keep_percent: { keep_percent: number };
    no_weights: null;
    split_zero_weight: { index: number };
    prize_too_small: { prize_sats: SatsString; member_count: number };
    share_rounds_to_zero: { index: number; pubkey: string };
    hash_serialize: null;
    invalid_card_hash: null;
    invalid_template_pubkey: null;
    invalid_template_lightning_address: null;
    tag_construction: null;
    invalid_event_id: null;
    invalid_signature: null;
    wrong_kind: null;
    missing_d_tag: null;
    multiple_d_tags: null;
    wrong_d_tag_hash: null;
    missing_splitcard_tag: null;
    multiple_splitcard_tags: null;
    wrong_splitcard_tag_hash: null;
    missing_lud16_tag: null;
    multiple_lud16_tags: null;
    invalid_lud16: null;
    unexpected_lud16_tag: null;
    non_member: null;
    invalid_old_card: { cause: SplitCardError<ValidationErrorCode> };
    invalid_new_card: { cause: SplitCardError<ValidationErrorCode> };
    wrong_supersedes_hash: null;
    missing_new_member_signatures: { missing: string[] };
    missing_old_member_signatures: { missing: string[] };
}

/**
 * The object every function throws. Narrow on `code` to type `details`:
 *
 * ```ts
 * try { compute_payouts(card, "1e3"); } catch (e) {
 *   const error = e as SplitCardError;
 *   if (error.code === "invalid_amount") console.log(error.details.value);
 * }
 * ```
 */
export type SplitCardError<C extends ErrorCode = ErrorCode> = {
    [K in C]: { code: K; message: string; details: ErrorDetails[K] };
}[C];

/** A member for `new_card`: a hex pubkey, a whole-number weight and an optional label. */
export interface NewCardMember {
    pubkey: string;
    weight: number;
    label?: string;
}

/** The fields of a new version 2 card, for `new_card`. */
export interface NewCardInput {
    card_id: string;
    event: string;
    team: string;
    members: NewCardMember[];
    /** Whole percent a leaver keeps, 1 to 99; leave out for "nothing". */
    leave_keep_percent?: number;
    supersedes?: string;
}



/**
 * Returns the lowercase hex SHA-256 hash of a card's canonical JSON.
 *
 * # Errors
 *
 * Throws `invalid_card_json` if the card does not parse.
 */
export function card_hash(card_json: string): string;

/**
 * Computes a card's signing status from a JSON array of signed events.
 *
 * Invalid events are listed in `rejected` in input order and do not count.
 *
 * # Errors
 *
 * Throws `invalid_card_json`, `invalid_event_json` or a validation error.
 */
export function card_status(card_json: string, events_json: string): CardStatus;

/**
 * Checks that a new card validly replaces an old one, given a JSON array of
 * signed events over the new card.
 *
 * # Errors
 *
 * Throws `invalid_card_json`, `invalid_event_json` or the first failed
 * replacement rule.
 */
export function check_supersede(old_card_json: string, new_card_json: string, events_json: string): void;

/**
 * Validates a card and splits `prize_sats` (a decimal string) between its members.
 *
 * Returns `[{ pubkey, amount_sats }]` in card member order, with each
 * `amount_sats` a decimal string.
 *
 * # Errors
 *
 * Throws `invalid_card_json`, `invalid_amount`, a validation error or a split error.
 */
export function compute_payouts(card_json: string, prize_sats: string): Payout[];

/**
 * Validates a card and returns its members' shares in basis points, in
 * member order, summing to 10,000.
 *
 * Takes the card rather than raw weights so large weights never pass through
 * JavaScript numbers.
 *
 * # Errors
 *
 * Throws `invalid_card_json`, a validation error or a split error.
 */
export function display_basis_points(card_json: string): number[];

/**
 * Builds a version 2 card from its fields, validates it and returns its
 * canonical JSON, the exact string to hash, submit and sign.
 *
 * # Errors
 *
 * Throws `invalid_card_input` for fields of the wrong shape, or the first
 * validation error.
 */
export function new_card(input: NewCardInput): CardJson;

/**
 * Returns the `npub` form of a public key given as 64 hex characters.
 *
 * # Errors
 *
 * Throws `invalid_public_key`.
 */
export function npub_encode(pubkey: string): string;

/**
 * Reads a teammate's public key, given as an `npub` or 64 hex characters,
 * and returns it as lowercase hex.
 *
 * # Errors
 *
 * Throws `secret_key_not_public` for an `nsec`, so the page can warn that
 * it was a secret, or `invalid_public_key`.
 */
export function parse_public_key(input: string): string;

/**
 * Checks a secret key, given as 64 hex characters or an `nsec`, and
 * returns it as lowercase hex.
 *
 * # Errors
 *
 * Throws `invalid_secret_key`.
 */
export function parse_secret_key(secret: string): string;

/**
 * Returns the public key of a secret key, as 64 lowercase hex characters.
 *
 * # Errors
 *
 * Throws `invalid_secret_key`.
 */
export function public_key(secret: string): string;

/**
 * Returns the `nsec` backup form of a secret key.
 *
 * # Errors
 *
 * Throws `invalid_secret_key`.
 */
export function secret_key_nsec(secret: string): string;

/**
 * Signs a NIP-01 event and returns it as JSON.
 *
 * `unsigned_json` is `{ pubkey?, created_at, kind, tags, content }`, such as
 * the output of `signature_template` or a NIP-98 event. `aux_rand` is 32
 * bytes from `crypto.getRandomValues`.
 *
 * # Errors
 *
 * Throws `invalid_secret_key`, `invalid_aux_rand`, `invalid_unsigned_event`
 * or `event_pubkey_mismatch`.
 */
export function sign_event(secret: string, unsigned_json: string, aux_rand: Uint8Array): SignedEventJson;

/**
 * Builds the NIP-01 unsigned event a member signs for a card hash, ready for
 * `window.nostr.signEvent`.
 *
 * `created_at` is Unix seconds as a whole JS number from 0 to 4294967295.
 * Pass `lightning_address`, the signer's payout address, for a version 2
 * card; leave it out for version 1. It becomes the event's `lud16` tag.
 *
 * # Errors
 *
 * Throws `invalid_created_at`, `invalid_card_hash`, `invalid_template_pubkey`
 * or `invalid_template_lightning_address`.
 */
export function signature_template(card_hash_hex: string, pubkey: string, created_at: number, lightning_address?: string | null): UnsignedSignatureEvent;

/**
 * Validates a card, returning nothing if it is valid.
 *
 * # Errors
 *
 * Throws `invalid_card_json` or the first validation error.
 */
export function validate_card(card_json: string): void;

/**
 * Verifies a signed event JSON over a card and returns the signer: their
 * pubkey, and for a version 2 card the payout address they signed.
 *
 * # Errors
 *
 * Throws `invalid_card_json`, `invalid_event_json` or the first failed
 * signature check.
 */
export function verify_signature(card_json: string, event_json: string): Signer;

/**
 * Returns the Split Card WASM package version.
 */
export function version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly card_hash: (a: number, b: number) => [number, number, number, number];
    readonly card_status: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly check_supersede: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
    readonly compute_payouts: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly display_basis_points: (a: number, b: number) => [number, number, number];
    readonly new_card: (a: any) => [number, number, number, number];
    readonly npub_encode: (a: number, b: number) => [number, number, number, number];
    readonly parse_public_key: (a: number, b: number) => [number, number, number, number];
    readonly parse_secret_key: (a: number, b: number) => [number, number, number, number];
    readonly public_key: (a: number, b: number) => [number, number, number, number];
    readonly secret_key_nsec: (a: number, b: number) => [number, number, number, number];
    readonly sign_event: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly signature_template: (a: number, b: number, c: number, d: number, e: any, f: number, g: number) => [number, number, number];
    readonly validate_card: (a: number, b: number) => [number, number];
    readonly verify_signature: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly version: () => [number, number];
    readonly rustsecp256k1_v0_10_0_context_create: (a: number) => number;
    readonly rustsecp256k1_v0_10_0_context_destroy: (a: number) => void;
    readonly rustsecp256k1_v0_10_0_default_error_callback_fn: (a: number, b: number) => void;
    readonly rustsecp256k1_v0_10_0_default_illegal_callback_fn: (a: number, b: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
