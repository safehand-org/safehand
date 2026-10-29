export type CardStatus = "draft" | "locked" | "paid-out" | "cancelled";

export type SplitCard = {
  id: string;
  title: string;
  detail: string;
  status: CardStatus;
  /** TODO: payout amount from the payouts API */
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

export const cardsSample: SplitCard[] = [
  {
    id: "kano-hack-night",
    title: "Kano Hack Night",
    detail: "1 of 3 signed",
    status: "draft",
  },
  {
    id: "lagos-build",
    title: "Lagos Build",
    detail: "Everyone has signed. Waiting for the prize",
    status: "locked",
  },
  {
    id: "team-42",
    title: "Team 42",
    detail: "250,000 sats paid to you",
    status: "paid-out",
    amount: 250_000,
    currency: "sats",
    phase: "payouts-in-progress",
    href: "/card/team-42",
  },
];

export function cardsForFilter(filter: CardFilterId, cards: SplitCard[]) {
  if (filter === "all") return cards;
  if (filter === "drafts") return cards.filter((card) => card.status === "draft");
  return cards.filter((card) => card.status === filter);
}
