export type HistoryStatus = "paid-out" | "cancelled";

export type HistoryCard = {
  id: string;
  title: string;
  detail: string;
  amount: string | null;
  date: string | null;
  month: string;
  status: HistoryStatus;
};

export const historyFilters = [
  { id: "all", label: "All" },
  { id: "paid-out", label: "Paid Out" },
  { id: "cancelled", label: "Cancelled" },
] as const;

export type HistoryFilterId = (typeof historyFilters)[number]["id"];

// TODO: history should list payout events from the payouts API, not this sample.
export const historySample: HistoryCard[] = [
  {
    id: "nairobi-sprint",
    title: "Nairobi Sprint",
    detail: "3 people",
    amount: "250,000 SATS",
    date: "30 SEP",
    month: "September",
    status: "paid-out",
  },
  {
    id: "abuja-hack",
    title: "Abuja Hack",
    detail: "2 teammates",
    amount: "40,000 SATS",
    date: "21 SEP",
    month: "September",
    status: "paid-out",
  },
  {
    id: "accra-weekend",
    title: "Accra Weekend",
    detail: "A teammate left",
    amount: null,
    date: null,
    month: "August",
    status: "cancelled",
  },
];

export function historyForFilter(filter: HistoryFilterId, cards: HistoryCard[]) {
  if (filter === "all") return cards;
  return cards.filter((card) => card.status === filter);
}

export function groupHistoryByMonth(cards: HistoryCard[]) {
  const groups: { month: string; cards: HistoryCard[] }[] = [];

  for (const card of cards) {
    const existing = groups.find((group) => group.month === card.month);
    if (existing) {
      existing.cards.push(card);
    } else {
      groups.push({ month: card.month, cards: [card] });
    }
  }

  return groups;
}
