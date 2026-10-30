"use client";

import { useEffect, useState, type ReactNode } from "react";
import { ApiError, getPayout, type CardView, type PayoutView, type TeamView } from "@/lib/api";
import { lockedDetailFrom } from "@/lib/cardDetail";
import { currentPubkey } from "@/lib/signer";
import { LockedCardStatusView } from "./LockedCardStatusView";
import type { LockedSplitCardDetail } from "./types";

/**
 * The locked card for whoever is viewing: their own share and, for
 * teammates, the payout (a signed read). Anyone with the link sees the card.
 */
export function LockedCardScreen({
  team,
  card,
  fallback,
}: {
  team: TeamView;
  card: CardView;
  fallback: ReactNode;
}) {
  const [detail, setDetail] = useState<LockedSplitCardDetail | null>(null);

  useEffect(() => {
    let current = true;
    void (async () => {
      const viewer = await currentPubkey().catch(() => null);
      let payout: PayoutView | null = null;
      if (viewer && team.participants.some((participant) => participant.pubkey === viewer)) {
        try {
          payout = await getPayout(team.id);
        } catch (error) {
          // No prize yet is `payout_not_found`: the card is waiting for it.
          // Any other failure keeps the placeholder rather than guess a phase.
          if (!(error instanceof ApiError && error.code === "payout_not_found")) return;
        }
      }
      if (current) {
        setDetail(lockedDetailFrom({ team, card, payout, viewer, host: window.location.host }));
      }
    })();
    return () => {
      current = false;
    };
  }, [team, card]);

  return detail ? <LockedCardStatusView card={detail} /> : fallback;
}
