export type LockedCardPhase =
  | "waiting-for-prize"
  | "prize-set-waiting"
  | "prize-in-card"
  | "payouts-in-progress";

/** One header component switches its title from this variant. */
export type LockedHeaderVariant = "status" | "settings" | "details" | "setting";

export type PayoutCurrency = "NGN" | "sats";

export type LockedMember = {
  id: string;
  name: string;
  shortName: string;
  sharePercent: number;
  color: string;
  signed: boolean;
  payoutStatus?: "paid" | "pending";
  /** The member's share of the prize in sats, from the payout API. */
  payoutSats?: number;
};

export type LockedAgreement = {
  title: string;
  body: string;
};

export type LockedTimelineEvent = {
  label: string;
  date?: string;
  pending?: boolean;
};

export type LockedPrize = {
  amountLabel: string;
  unit: PayoutCurrency | "units";
  badge: "RECEIVED" | "ROUND" | "IN CARD" | "PAYOUT";
  date: string;
  time: string;
};

export type LockedSplitCardDetail = {
  id: string;
  slug: string;
  title: string;
  phase: LockedCardPhase;
  /** Mirrors phase so payout routing can read it without guessing from the slug. */
  payoutPhase: LockedCardPhase;
  headerVariant: LockedHeaderVariant;
  subtitle: string;
  cardLink: string;
  lockedOn: { date: string; time: string };
  members: LockedMember[];
  agreements: LockedAgreement[];
  timeline: LockedTimelineEvent[];
  prize?: LockedPrize;
  /** The agreed prize in sats, once there is one. */
  amount?: number;
  currency?: PayoutCurrency;
  payoutsSummary?: { paid: number; total: number };
  /** The viewer's Lightning Address on the card, or "" if they are not on it. */
  shareDestination: string;
};

export const WAITING_SUBTITLE =
  "Everyone has signed, so this card is locked. We're waiting for the prize.";

export function signedSubtitle(title: string) {
  return `All teammates for '${title}' have signed the split card.`;
}

export const WAITING_PAYOUT_NOTE =
  "No payouts yet. Your share will be sent when the prize arrives.";

export const PRIZE_SET_NOTE =
  "The prize is set. Your share will be paid once the prize arrives.";

export const SENT_PAYOUT_NOTE = "Payouts for this split card has been sent";

export function headerVariantFor(phase: LockedCardPhase): LockedHeaderVariant {
  switch (phase) {
    case "prize-set-waiting":
      return "settings";
    case "prize-in-card":
      return "details";
    case "payouts-in-progress":
      return "setting";
    default:
      return "status";
  }
}

export function headerLabel(variant: LockedHeaderVariant) {
  switch (variant) {
    case "settings":
      return "Split card settings";
    case "details":
      return "Split card details";
    case "setting":
      return "Split card setting";
    default:
      return "Split card status";
  }
}

export function payoutNote(phase: LockedCardPhase) {
  if (phase === "waiting-for-prize") return WAITING_PAYOUT_NOTE;
  if (phase === "prize-set-waiting") return PRIZE_SET_NOTE;
  return SENT_PAYOUT_NOTE;
}
