// Turns a team, its active card and its payout into the locked-card screen
// (BACKEND_PRD.md, "Card status — /card/[id]"). Sentences are built here.

import type {
  LockedAgreement,
  LockedCardPhase,
  LockedMember,
  LockedSplitCardDetail,
  LockedTimelineEvent,
} from "@/components/cards/locked/types";
import {
  WAITING_SUBTITLE,
  headerVariantFor,
  signedSubtitle,
} from "@/components/cards/locked/types";
import { PROFILE_COLORS } from "@/components/join/types";
import { formatSats } from "./account";
import type { CardJson, CardView, PayoutView, TeamView } from "./api";

const MONTHS = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];

/** `12 AUG, 2026` and `14:03 GMT +1`, in the viewer's time zone. */
export function stamp(unixSeconds: number): { date: string; time: string } {
  const at = new Date(unixSeconds * 1000);
  const date = `${at.getDate()} ${MONTHS[at.getMonth()]}, ${at.getFullYear()}`;
  const offsetMinutes = -at.getTimezoneOffset();
  const hours = Math.abs(offsetMinutes) / 60;
  const zone =
    offsetMinutes === 0
      ? "GMT"
      : `GMT ${offsetMinutes > 0 ? "+" : "-"}${Number.isInteger(hours) ? hours : hours.toFixed(1)}`;
  const pad = (value: number) => String(value).padStart(2, "0");
  return { date, time: `${pad(at.getHours())}:${pad(at.getMinutes())} ${zone}` };
}

/** "Bob", "Bob and Chidi", "Bob, Chidi and Dayo". */
function listNames(names: string[]): string {
  if (names.length <= 1) return names.join("");
  return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/** Who signed the card and when, in signing order. */
function signatures(card: CardView): { pubkey: string; at: number }[] {
  const signed = card.signatures.flatMap((json) => {
    try {
      const event = JSON.parse(json) as { pubkey?: unknown; created_at?: unknown };
      if (typeof event.pubkey !== "string" || typeof event.created_at !== "number") return [];
      return [{ pubkey: event.pubkey, at: event.created_at }];
    } catch {
      // An unreadable stored event is left out of the timeline.
      return [];
    }
  });
  return signed.sort((left, right) => left.at - right.at);
}

function timeline(
  title: string,
  card: CardView,
  parsed: CardJson,
  payout: PayoutView | null,
  phase: LockedCardPhase,
): LockedTimelineEvent[] {
  const nameOf = (pubkey: string) =>
    parsed.members.find((member) => member.pubkey === pubkey)?.name ?? "A teammate";
  const events: LockedTimelineEvent[] = [
    { label: `${nameOf(card.created_by)} created split card '${title}'`, date: stamp(card.created_at).date },
  ];
  const others = signatures(card).filter((entry) => entry.pubkey !== card.created_by);
  if (others.length > 0) {
    events.push({
      label: `${listNames(others.map((entry) => nameOf(entry.pubkey)))} signed card`,
      date: stamp(others[others.length - 1].at).date,
    });
  }
  if (card.activated_at !== null) {
    events.push({ label: "Card locked", date: stamp(card.activated_at).date });
  }
  if (phase === "payouts-in-progress" && payout) {
    const paidAt = payout.items.map((item) => item.paid_at).filter((at): at is number => at !== null);
    const allPaid = payout.items.length > 0 && payout.paid_count === payout.items.length;
    events.push(
      allPaid
        ? { label: "Final split out", date: stamp(Math.max(...paidAt)).date }
        : { label: "Final split out", pending: true },
    );
  } else {
    events.push({ label: "Prize paid out", pending: true });
  }
  return events;
}

function agreements(viewer: LockedMember | undefined, destination: string): LockedAgreement[] {
  const rules: LockedAgreement[] = [
    // Client copy with the PRD's default; there is no leave policy on the server yet.
    {
      title: "If you leave before you submit, you keep 15% of the prize.",
      body: "The rest is split equally among the people who stay.",
    },
    {
      title: "Changes need everyone to agree.",
      body: "Once the card is locked, no one can change it alone.",
    },
  ];
  if (!viewer) return rules;
  return [
    {
      title: `Your share is ${viewer.sharePercent}%`,
      body: `Paid to ${destination} when the prize arrives.`,
    },
    ...rules,
  ];
}

export function lockedDetailFrom({
  team,
  card,
  payout,
  viewer,
  host,
}: {
  team: TeamView;
  card: CardView;
  payout: PayoutView | null;
  viewer: string | null;
  host: string;
}): LockedSplitCardDetail {
  const parsed = JSON.parse(card.card) as CardJson;
  const phase: LockedCardPhase = payout?.phase ?? "waiting-for-prize";
  const total = parsed.members.reduce((sum, member) => sum + member.weight, 0);
  const items = new Map(payout?.items.map((item) => [item.pubkey, item]) ?? []);

  const members: LockedMember[] = parsed.members.map((member, index) => {
    const item = items.get(member.pubkey);
    return {
      id: member.pubkey,
      name: member.name,
      shortName: member.name,
      sharePercent: Number(((member.weight / total) * 100).toFixed(1)),
      color: PROFILE_COLORS[index % PROFILE_COLORS.length],
      signed: card.signing.signed.includes(member.pubkey),
      ...(phase === "payouts-in-progress" && item
        ? { payoutStatus: item.status === "paid" ? ("paid" as const) : ("pending" as const) }
        : {}),
      ...(item ? { payoutSats: Number(item.amount_sats) } : {}),
    };
  });
  const viewerMember = members.find((member) => member.id === viewer);
  const destination =
    parsed.members.find((member) => member.pubkey === viewer)?.payout.lightning.address ?? "";
  const agreed = payout && phase !== "waiting-for-prize" ? payout : null;

  return {
    id: team.slug,
    slug: team.slug,
    title: team.name,
    phase,
    payoutPhase: phase,
    headerVariant: headerVariantFor(phase),
    subtitle: phase === "payouts-in-progress" ? signedSubtitle(team.name) : WAITING_SUBTITLE,
    cardLink: `${host}/card/${team.slug}`,
    lockedOn: stamp(card.activated_at ?? card.created_at),
    members,
    agreements: agreements(viewerMember, destination),
    timeline: timeline(team.name, card, parsed, payout, phase),
    ...(agreed
      ? { amount: Number(agreed.prize_sats), currency: "sats" as const }
      : {}),
    ...(phase === "payouts-in-progress" && payout
      ? {
          prize: {
            amountLabel: formatSats(payout.prize_sats),
            unit: "sats" as const,
            badge: "PAYOUT" as const,
            ...stamp(payout.agreed_at ?? payout.items[0]?.paid_at ?? card.created_at),
          },
          payoutsSummary: { paid: payout.paid_count, total: payout.items.length },
        }
      : {}),
    shareDestination: destination,
  };
}
