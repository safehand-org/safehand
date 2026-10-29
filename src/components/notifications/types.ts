export type NotificationKind = "card" | "payout";

export type AppNotification = {
  id: string;
  title: string;
  body: string;
  time: string;
  kind: NotificationKind;
  featured?: boolean;
  actionLabel?: string;
};

export const notificationFilters = [
  { id: "all", label: "All" },
  { id: "card", label: "Cards" },
  { id: "payout", label: "Paid Out" },
] as const;

export type NotificationFilterId = (typeof notificationFilters)[number]["id"];

// TODO: notifications should include prize-detected and payout events from the API.
export const notificationsSample: AppNotification[] = [
  {
    id: "bikini-invite",
    title: "Goodness invited you to Bikini Bottom",
    body: "Your share is 45%. Read the card before you sign.",
    time: "2H AGO",
    kind: "card",
    featured: true,
    actionLabel: "Review And Sign",
  },
  {
    id: "lagos-locked",
    title: "Lagos Build is locked",
    body: "Everyone has signed. Waiting for the prize.",
    time: "2 OCT",
    kind: "card",
  },
  {
    id: "nairobi-payout",
    title: "You received 250,000 sats",
    body: "Nairobi Sprint",
    time: "30 SEP",
    kind: "payout",
  },
];

export function notificationsForFilter(
  filter: NotificationFilterId,
  items: AppNotification[],
) {
  if (filter === "all") return items;
  return items.filter((item) => item.kind === filter);
}
