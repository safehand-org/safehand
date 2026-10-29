// Turns `GET /me` into the data the home and cards screens render. The
// sentences live here, not on the server (BACKEND_PRD.md, "Display strings
// the adapter owns").

import type { SplitCard } from "@/components/cards/types";
import type { HomeActivityItem, HomeAttention, HomeData } from "@/components/home/types";
import type { Me, NeedsSignature, TeamSummary } from "./api";

/** How many rows the home screen shows; the cards tab has the full list. */
const HOME_ACTIVITY_ROWS = 5;

/** `250000` sats as `250,000`. Sats arrive as decimal strings. */
export function formatSats(sats: string): string {
  return BigInt(sats).toLocaleString("en-US");
}

/** A share in basis points as a whole percent, as home and cards print it. */
function sharePercent(basisPoints: number): string {
  return `${Math.round(basisPoints / 100)}%`;
}

function timeAgo(unixSeconds: number, now = Date.now()): string {
  const seconds = Math.max(0, Math.floor(now / 1000) - unixSeconds);
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

/** Where to review and sign a card: the team's invite link. */
export function inviteHref(team: { slug: string; join_code: string }): string {
  return `/invite/card/${encodeURIComponent(team.slug)}?join=${encodeURIComponent(team.join_code)}`;
}

function cardDetail(team: TeamSummary): string {
  if (team.list_status === "draft") {
    if (team.signed_count === null || team.member_count === null) return "No card yet";
    return `${team.signed_count} of ${team.member_count} signed`;
  }
  if (team.list_status === "paid-out") {
    return team.you_received_sats
      ? `${formatSats(team.you_received_sats)} sats paid to you`
      : "Payouts in progress";
  }
  if (team.phase === "prize-set-waiting") return "The prize is set. Waiting for it to arrive";
  return "Everyone has signed. Waiting for the prize";
}

export function splitCardsFrom(me: Me): SplitCard[] {
  return me.teams.map((team) => {
    const linked = team.list_status !== "draft";
    const received = team.you_received_sats;
    return {
      id: team.slug,
      title: team.name,
      detail: cardDetail(team),
      status: team.list_status,
      ...(received ? { amount: Number(received), currency: "sats" as const } : {}),
      ...(team.phase ? { phase: team.phase } : {}),
      ...(linked ? { href: `/card/${encodeURIComponent(team.slug)}` } : {}),
    };
  });
}

const ACTIVITY_LABEL = { draft: "Draft", locked: "Locked", "paid-out": "Paid out" } as const;

function activityFrom(me: Me): HomeActivityItem[] {
  return splitCardsFrom(me)
    .slice(0, HOME_ACTIVITY_ROWS)
    .map((card) => ({
      id: card.id,
      title: card.title,
      detail: card.detail,
      status: ACTIVITY_LABEL[card.status as keyof typeof ACTIVITY_LABEL] ?? "Draft",
      ...(card.href ? { href: card.href } : {}),
    }));
}

export type SignatureRequest = {
  title: string;
  body: string;
  href: string;
};

/** The "needs your signature" strip on the cards tab. */
export function signatureRequestFrom(ask: NeedsSignature | null): SignatureRequest | null {
  if (!ask) return null;
  return {
    title: ask.name,
    body: `${ask.inviter_name} invited you. Your share is ${sharePercent(ask.share_basis_points)}.`,
    href: inviteHref(ask),
  };
}

function attentionFrom(ask: NeedsSignature | null): HomeAttention | null {
  if (!ask) return null;
  return {
    title: ask.name,
    body: `${ask.inviter_name} invited you to a split. Your share is ${sharePercent(ask.share_basis_points)}.`,
    timeAgo: timeAgo(ask.invited_at),
    teammatesLabel: `${ask.member_count} teammates`,
    actionLabel: "Review And Sign",
    href: inviteHref(ask),
  };
}

export function homeDataFrom(me: Me): HomeData {
  const hasTeams = me.teams.length > 0;
  const waiting = me.teams.filter((team) => team.you_must_sign).length;
  let subtitle = "Your split cards are up to date.";
  if (waiting > 0) {
    subtitle = `You have ${waiting} card${waiting === 1 ? "" : "s"} waiting for your signature.`;
  } else if (!hasTeams) {
    subtitle = "No split cards yet. Let's make your first one in two minutes.";
  }
  return {
    greeting: {
      title: hasTeams ? "Welcome Back," : "Hello,",
      name: `${me.profile?.name ?? ""}.`,
      subtitle,
    },
    attention: attentionFrom(me.needs_signature),
    inviteMessage: hasTeams
      ? "Join in with the link or scan the qr code."
      : "Got an invite? Open the link or scan the code to join your team.",
    activity: activityFrom(me),
  };
}

export type CardsSummary = {
  receivedSats: string;
  paidTeamCount: number;
  /** The server has no cancelled teams yet. */
  cancelledCount: number;
};

export function cardsSummaryFrom(me: Me): CardsSummary {
  return {
    receivedSats: formatSats(me.summary.received_sats),
    paidTeamCount: me.summary.paid_team_count,
    cancelledCount: 0,
  };
}
