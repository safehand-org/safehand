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
  /** TODO: replace with the payout API amount for this member */
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
  /** TODO: prize total comes from prize detection, not this mock */
  amount?: number;
  currency?: PayoutCurrency;
  payoutsSummary?: { paid: number; total: number };
  /** TODO: payout destination saved with the signature, not the demo address */
  shareDestination: string;
};

const PHASES: LockedCardPhase[] = [
  "waiting-for-prize",
  "prize-set-waiting",
  "prize-in-card",
  "payouts-in-progress",
];

const PHASE_QUERY: Record<string, LockedCardPhase> = {
  waiting: "waiting-for-prize",
  "waiting-for-prize": "waiting-for-prize",
  "prize-set": "prize-set-waiting",
  "prize-set-waiting": "prize-set-waiting",
  settings: "prize-set-waiting",
  paid: "prize-in-card",
  "prize-in-card": "prize-in-card",
  "payouts-sent": "prize-in-card",
  "prize-received": "prize-in-card",
  payout: "payouts-in-progress",
  "payouts-in-progress": "payouts-in-progress",
};

export const WAITING_SUBTITLE =
  "Everyone has signed, so this card is locked. We're waiting for the prize.";

export const SIGNED_SUBTITLE =
  "All teammates for 'Team 42' have signed the split card.";

export const WAITING_PAYOUT_NOTE =
  "No payouts yet. Your share will be sent when the prize arrives.";

export const PRIZE_SET_NOTE =
  "The prize is set. Your share will be paid once the prize arrives.";

export const SENT_PAYOUT_NOTE = "Payouts for this split card has been sent";

const SHARE_DESTINATION = "greedy@lion.com";
const TEAM_LOCK = { date: "22 JAN, 2025", time: "14:03 GMT +1" };
const LAGOS_LOCK = { date: "12 AUG, 2026", time: "14:03 GMT +1" };
const PRIZE_AMOUNT = 410_000;

const BLUE = "#43c7ff";
const ORANGE = "#f78125";
const PURPLE = "#8a38f5";

const EQUAL_MEMBERS: LockedMember[] = [
  {
    id: "flora",
    name: "Flora",
    shortName: "Flora",
    sharePercent: 33.3,
    color: BLUE,
    signed: true,
  },
  {
    id: "goodness",
    name: "Goodness",
    shortName: "Goodness",
    sharePercent: 33.3,
    color: ORANGE,
    signed: true,
  },
  {
    id: "spongebob",
    name: "Spongebob Squarepants",
    shortName: "Spongebob",
    sharePercent: 33.3,
    color: PURPLE,
    signed: true,
  },
];

function headerVariantFor(phase: LockedCardPhase): LockedHeaderVariant {
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

function agreements(shareLabel: string, destination: string): LockedAgreement[] {
  return [
    {
      title: `Your share is ${shareLabel}`,
      body: `Paid to ${destination} when the prize arrives.`,
    },
    {
      title: "If you leave before you submit, you keep 15% of the prize.",
      body: "The rest is split equally among the people who stay.",
    },
    {
      title: "Changes need everyone to agree.",
      body: "Once the card is locked, no one can change it alone.",
    },
  ];
}

function baseTimeline(
  createdBy: string,
  signedBy: string,
  title: string,
  closing: LockedTimelineEvent,
): LockedTimelineEvent[] {
  return [
    {
      label: `${createdBy} created split card '${title}'`,
      date: "22 JAN, 2025",
    },
    { label: signedBy, date: "22 JAN, 2025" },
    { label: "Card locked", date: "22 JAN, 2025" },
    closing,
  ];
}

function stamp(
  id: string,
  phase: LockedCardPhase,
  detail: Omit<
    LockedSplitCardDetail,
    "id" | "phase" | "payoutPhase" | "headerVariant" | "slug"
  >,
): LockedSplitCardDetail {
  return {
    id,
    slug: "team-42",
    phase,
    payoutPhase: phase,
    headerVariant: headerVariantFor(phase),
    ...detail,
  };
}

/** Lagos Build stays on the locked-waiting screen from the earlier comps. */
function lagosWaiting(id: string): LockedSplitCardDetail {
  const title = "Lagos Build";
  return {
    id,
    slug: "lagos-build",
    title,
    phase: "waiting-for-prize",
    payoutPhase: "waiting-for-prize",
    headerVariant: "status",
    subtitle: WAITING_SUBTITLE,
    cardLink: "yourapp/card/lagos-build",
    lockedOn: LAGOS_LOCK,
    members: EQUAL_MEMBERS.map((member) => ({ ...member })),
    agreements: agreements("33.3%", SHARE_DESTINATION),
    timeline: [
      { label: `Flora created split card '${title}'`, date: "5 AUG, 2026" },
      { label: "Goodness and Spongebob signed card", date: "5 AUG, 2026" },
      { label: "Card locked", date: "5 AUG, 2026" },
      { label: "Prize paid out", pending: true },
    ],
    shareDestination: SHARE_DESTINATION,
  };
}

function teamWaiting(id: string): LockedSplitCardDetail {
  const title = "Team 42";
  return stamp(id, "waiting-for-prize", {
    title,
    subtitle: SIGNED_SUBTITLE,
    cardLink: "yourapp/card/team-42",
    lockedOn: TEAM_LOCK,
    members: EQUAL_MEMBERS.map((member) => ({ ...member })),
    agreements: agreements("33.3%", SHARE_DESTINATION),
    timeline: baseTimeline("Flora", "Goodness and Spongebob signed card", title, {
      label: "Prize paid out",
      pending: true,
    }),
    shareDestination: SHARE_DESTINATION,
  });
}

function teamPrizeSet(id: string): LockedSplitCardDetail {
  const title = "Team 42";
  return stamp(id, "prize-set-waiting", {
    title,
    subtitle: WAITING_SUBTITLE,
    cardLink: "yourapp/card/team-42",
    lockedOn: TEAM_LOCK,
    members: EQUAL_MEMBERS.map((member) => ({ ...member })),
    agreements: agreements("33.3%", SHARE_DESTINATION),
    timeline: baseTimeline("Flora", "Goodness and Spongebob signed card", title, {
      label: "Prize paid out",
      pending: true,
    }),
    shareDestination: SHARE_DESTINATION,
    amount: PRIZE_AMOUNT,
    currency: "NGN",
  });
}

function teamPrizeInCard(id: string): LockedSplitCardDetail {
  const title = "Team 42";
  return stamp(id, "prize-in-card", {
    title,
    subtitle: SIGNED_SUBTITLE,
    cardLink: "yourapp/card/team-42",
    lockedOn: TEAM_LOCK,
    members: [
      {
        id: "favour",
        name: "Favour",
        shortName: "Favour",
        sharePercent: 30,
        color: BLUE,
        signed: true,
      },
      {
        id: "goodness",
        name: "Goodness",
        shortName: "Goodness",
        sharePercent: 40,
        color: ORANGE,
        signed: true,
      },
      {
        id: "solomonfaith",
        name: "Solomonfaith",
        shortName: "Solomonfaith",
        sharePercent: 30,
        color: PURPLE,
        signed: true,
      },
    ],
    agreements: [],
    timeline: [],
    prize: {
      amountLabel: "410,000",
      unit: "NGN",
      badge: "IN CARD",
      date: TEAM_LOCK.date,
      time: TEAM_LOCK.time,
    },
    amount: PRIZE_AMOUNT,
    currency: "NGN",
    shareDestination: SHARE_DESTINATION,
  });
}

function teamPayouts(id: string): LockedSplitCardDetail {
  const title = "Team 42";
  // TODO: payout execution — paid vs pending must come from the payout API
  return stamp(id, "payouts-in-progress", {
    title,
    subtitle: SIGNED_SUBTITLE,
    cardLink: "yourapp/card/team-42",
    lockedOn: TEAM_LOCK,
    members: [
      {
        id: "flare",
        name: "Flare",
        shortName: "Flare",
        sharePercent: 30,
        color: BLUE,
        signed: true,
        payoutStatus: "paid",
      },
      {
        id: "goodness",
        name: "Goodness",
        shortName: "Goodness",
        sharePercent: 40,
        color: ORANGE,
        signed: true,
        payoutStatus: "paid",
      },
      {
        id: "spongebob",
        name: "Spongebob",
        shortName: "Spongebob",
        sharePercent: 30,
        color: PURPLE,
        signed: true,
        payoutStatus: "pending",
      },
    ],
    agreements: agreements("30%", SHARE_DESTINATION),
    timeline: baseTimeline("Flare", "Goodness and Spongebob signed card", title, {
      label: "Final split out",
      pending: true,
    }),
    prize: {
      amountLabel: "410,000",
      unit: "NGN",
      badge: "PAYOUT",
      date: TEAM_LOCK.date,
      time: TEAM_LOCK.time,
    },
    amount: PRIZE_AMOUNT,
    currency: "NGN",
    payoutsSummary: { paid: 2, total: 3 },
    shareDestination: SHARE_DESTINATION,
  });
}

const TEAM_BY_PHASE: Record<LockedCardPhase, (id: string) => LockedSplitCardDetail> = {
  "waiting-for-prize": teamWaiting,
  "prize-set-waiting": teamPrizeSet,
  "prize-in-card": teamPrizeInCard,
  "payouts-in-progress": teamPayouts,
};

export function payoutNote(phase: LockedCardPhase) {
  if (phase === "waiting-for-prize") return WAITING_PAYOUT_NOTE;
  if (phase === "prize-set-waiting") return PRIZE_SET_NOTE;
  return SENT_PAYOUT_NOTE;
}

export function formatSats(value: number) {
  return value.toLocaleString("en-US", {
    minimumFractionDigits: 3,
    maximumFractionDigits: 3,
  });
}

export function isLockedCardPhase(value: string | undefined): value is LockedCardPhase {
  return PHASES.includes(value as LockedCardPhase);
}

export function resolvePayoutPhase(value: string | undefined): LockedCardPhase | undefined {
  if (!value) return undefined;
  return PHASE_QUERY[value];
}

/**
 * TODO: prize detection should move a locked card from waiting-for-prize
 * to prize-set-waiting when the prize lands.
 * TODO: payout execution should fill paid/pending and the in-card total.
 * TODO: history and notifications should read these payout events from the API
 * instead of the local samples in history/types and notifications/types.
 *
 * Cards → Team 42 opens payouts-in-progress.
 * ?phase=waiting | prize-set-waiting | prize-in-card | payouts-in-progress
 * also accepts waiting, prize-set, paid, payouts-sent, and payout.
 */
export const lockedCards: Record<string, LockedSplitCardDetail> = {
  "lagos-build": lagosWaiting("lagos-build"),
  "team-42": teamPayouts("team-42"),
  "team-42-waiting": teamWaiting("team-42-waiting"),
  "team-42-payout": teamPayouts("team-42-payout"),
  "team-42-paid": teamPrizeInCard("team-42-paid"),
  "team-42-received": teamPrizeInCard("team-42-received"),
  "team-42-payouts": teamPayouts("team-42-payouts"),
};

export function getLockedCard(id: string, phase?: string) {
  const card = lockedCards[id];
  if (!card) return null;
  const next = resolvePayoutPhase(phase);
  if (!next || next === card.phase) return card;
  if (id === "lagos-build") return card;
  return TEAM_BY_PHASE[next](id);
}
