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
export type ErrorCode = "invalid_card_json" | "invalid_event_json" | "invalid_amount" | "invalid_created_at" | "output_serialization" | "invalid_secret_key" | "invalid_aux_rand" | "invalid_unsigned_event" | "event_pubkey_mismatch" | "unsupported_version" | "empty_field" | "invalid_member_count" | "invalid_pubkey" | "duplicate_pubkey" | "empty_member_name" | "zero_weight" | "weight_too_large" | "invalid_lightning_address" | "invalid_member_label" | "invalid_supersedes" | "invalid_peer_review_max_shift_points" | "invalid_leave_keep_percent" | "no_weights" | "split_zero_weight" | "prize_too_small" | "share_rounds_to_zero" | "hash_serialize" | "invalid_card_hash" | "invalid_template_pubkey" | "invalid_template_lightning_address" | "tag_construction" | "invalid_event_id" | "invalid_signature" | "wrong_kind" | "missing_d_tag" | "multiple_d_tags" | "wrong_d_tag_hash" | "missing_splitcard_tag" | "multiple_splitcard_tags" | "wrong_splitcard_tag_hash" | "missing_lud16_tag" | "multiple_lud16_tags" | "invalid_lud16" | "unexpected_lud16_tag" | "non_member" | "invalid_old_card" | "invalid_new_card" | "wrong_supersedes_hash" | "missing_new_member_signatures" | "missing_old_member_signatures";

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
  invalid_secret_key: null;
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
