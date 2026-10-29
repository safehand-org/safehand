export type CardStatus = "draft" | "locked" | "paid-out" | "cancelled";

export type SplitCard = {
  id: string;
  title: string;
  detail: string;
  status: CardStatus;
  /** Sats paid to the viewer, from `GET /me`. */
  amount?: number;
  currency?: "sats" | "NGN";
  /** Payout-flow phase opened from this row. */
  phase?:
    | "waiting-for-prize"
    | "prize-set-waiting"
    | "prize-in-card"
    | "payouts-in-progress";
  href?: string;
};

export const cardFilters = [
  { id: "all", label: "All" },
  { id: "locked", label: "Locked" },
  { id: "paid-out", label: "Paid Out" },
  { id: "cancelled", label: "Cancelled" },
  { id: "drafts", label: "Drafts" },
] as const;

export type CardFilterId = (typeof cardFilters)[number]["id"];

export function cardsForFilter(filter: CardFilterId, cards: SplitCard[]) {
  if (filter === "all") return cards;
  if (filter === "drafts") return cards.filter((card) => card.status === "draft");
  return cards.filter((card) => card.status === filter);
}
