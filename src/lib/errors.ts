// Plain-English messages for the error codes the card screens can meet,
// from the server (`{ code, message, details }`) or from splitcard-wasm.

import { errorCode } from "./createCard";

const MESSAGES: Record<string, string> = {
  network_error: "Couldn't reach Safehand. Check your connection.",
  auth_missing: "Sign in first.",
  extension_missing: "Your Nostr extension isn't available. Unlock it, or sign in again.",
  signing_refused: "Signing was cancelled, so nothing was sent.",
  secret_key_rejected: "That looks like a secret key (nsec). It was not sent. Never share it.",
  secret_key_not_public: "That's a secret key. Never share it.",
  invalid_public_key: "That isn't a valid npub. It starts with npub1.",
  profile_required: "Set up your profile before signing.",
  lightning_address_required: "Add your Lightning address first.",
  invalid_lightning_address: "That isn't a Lightning address. It looks like name@wallet.com.",
  invalid_template_lightning_address:
    "That isn't a Lightning address. It looks like name@wallet.com.",
  lightning_address_unreachable:
    "We couldn't reach that Lightning address. Check it, or try one from another wallet.",
  team_slug_taken: "A team with that name already exists. Try again.",
  invalid_slug: "Choose a team name with some letters or numbers.",
  submitter_not_member: "You must be on the card you create.",
  card_hash_mismatch: "The server saved a different card. Nothing was signed.",
  card_not_found: "This card doesn't exist.",
  card_declined: "Someone declined this card, so it can't be signed. Start a new card.",
  card_stale: "This card was replaced, so it can't be signed. Start a new card.",
  card_superseded: "This card was replaced by a newer one.",
  card_not_draft: "This card is already locked.",
  not_a_card_member: "You're not on this card.",
  non_member: "You're not on this card.",
  already_signed: "You already signed this card, so you can't decline it.",
  missing_lud16_tag: "Your signature needs your Lightning address. Try signing again.",
  multiple_lud16_tags: "Your signature had more than one Lightning address. Try signing again.",
  invalid_lud16: "The Lightning address in your signature isn't valid.",
  unexpected_lud16_tag: "This older card keeps addresses in the card itself. Sign without one.",
  invalid_member_label: "Names on the card must be 40 characters or fewer.",
  invalid_leave_keep_percent: "What a leaver keeps must be a whole percent from 1 to 99.",
  duplicate_pubkey: "Someone is on the card twice.",
  invalid_member_count: "A card needs 2 to 20 people.",
  zero_weight: "Every share must be more than 0%.",
  empty_field: "Fill in every field.",
  invalid_card_input: "The card couldn't be built from these details.",
  team_not_found: "This team doesn't exist.",
};

/** A message to show for any thrown error. */
export function errorMessage(error: unknown): string {
  const code = errorCode(error);
  if (code && MESSAGES[code]) return MESSAGES[code];
  if (error instanceof Error && error.message) return error.message;
  return "Something went wrong. Try again.";
}
