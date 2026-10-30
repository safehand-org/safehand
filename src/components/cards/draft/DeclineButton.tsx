"use client";

import { useState } from "react";
import { declineCard, type CardView } from "@/lib/api";
import { errorMessage } from "@/lib/errors";

export const DECLINE_CONFIRM =
  "Decline this card? It can never be locked, and your team will need to start a new card.";

/** Declines a draft after asking to confirm. */
export function DeclineButton({
  hash,
  onDeclined,
  className = "inline-flex h-12 items-center justify-center rounded-full border border-hairline px-6 text-[16px] font-medium text-white",
}: {
  hash: string;
  onDeclined: (card: CardView) => void;
  className?: string;
}) {
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function decline() {
    setBusy(true);
    setError(null);
    try {
      onDeclined(await declineCard(hash));
      setConfirming(false);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setBusy(false);
    }
  }

  if (!confirming) {
    return (
      <button type="button" onClick={() => setConfirming(true)} className={className}>
        Decline
      </button>
    );
  }
  return (
    <div role="alertdialog" aria-label="Confirm decline" className="flex flex-col gap-2 text-[13px]">
      <p>{DECLINE_CONFIRM}</p>
      {error ? <p className="text-[#E23B3B]">{error}</p> : null}
      <div className="flex gap-2">
        <button
          type="button"
          disabled={busy}
          onClick={() => void decline()}
          className="rounded-full bg-[#C62828] px-4 py-2 font-medium text-white"
        >
          {busy ? "Declining…" : "Yes, decline"}
        </button>
        <button
          type="button"
          onClick={() => setConfirming(false)}
          className="rounded-full border border-hairline px-4 py-2"
        >
          Cancel
        </button>
      </div>
    </div>
  );
}
