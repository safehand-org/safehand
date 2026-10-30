"use client";

import { usePathname, useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import { DeclineButton } from "@/components/cards/draft/DeclineButton";
import { refreshPending, usePendingCards } from "@/lib/pendingCards";
import { formatBasisPoints } from "@/lib/shares";

const POLL_MS = 30_000;
const NOT_NOW_STORAGE = "safehand.not-now";

function readNotNow(): string[] {
  try {
    return JSON.parse(window.sessionStorage.getItem(NOT_NOW_STORAGE) ?? "[]") as string[];
  } catch {
    return [];
  }
}

function storeNotNow(hashes: string[]) {
  try {
    window.sessionStorage.setItem(NOT_NOW_STORAGE, JSON.stringify(hashes));
  } catch {
    // Without storage, "Not now" lasts until the page reloads.
  }
}

/**
 * Asks the viewer to sign cards that list them. Checks `GET /me` on load,
 * when the tab regains focus and every 30 seconds.
 */
export function SignPrompt() {
  const router = useRouter();
  const pathname = usePathname();
  const pending = usePendingCards();
  const [notNow, setNotNow] = useState<string[]>([]);

  useEffect(() => {
    // Session storage is only readable in the browser.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setNotNow(readNotNow());
    const refresh = () => void refreshPending().catch(() => null);
    refresh();
    const timer = window.setInterval(refresh, POLL_MS);
    window.addEventListener("focus", refresh);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", refresh);
    };
  }, []);

  if (pathname.startsWith("/signup") || pathname.startsWith("/create")) return null;
  const card = pending.find(
    (item) =>
      !notNow.includes(item.card_hash) && pathname !== `/card/${encodeURIComponent(item.slug)}`,
  );
  if (!card) return null;

  return (
    <div
      role="dialog"
      aria-label="Sign a split card"
      className="absolute inset-x-3 bottom-3 z-50 rounded-[1.35rem] bg-sheet p-4 text-ink shadow-[0_8px_30px_rgba(0,0,0,0.35)]"
    >
      <p className="text-[16px] font-semibold tracking-[-0.02em]">
        {card.creator_name} added you to team {card.name}
      </p>
      <p className="mt-1 text-[13px] text-sheet-muted">
        Your share is {formatBasisPoints(card.share_basis_points)}. Read the card, then sign it.
      </p>
      <div className="mt-3 flex flex-wrap items-center gap-2">
        <button
          type="button"
          onClick={() => router.push(`/card/${encodeURIComponent(card.slug)}`)}
          className="rounded-full bg-ink px-5 py-2.5 text-[14px] font-medium text-white"
        >
          Sign
        </button>
        <button
          type="button"
          onClick={() => {
            const next = [...notNow, card.card_hash];
            setNotNow(next);
            storeNotNow(next);
          }}
          className="rounded-full border border-[#d8d8d8] px-5 py-2.5 text-[14px] font-medium"
        >
          Not now
        </button>
        <DeclineButton
          hash={card.card_hash}
          onDeclined={() => void refreshPending().catch(() => null)}
          className="rounded-full px-3 py-2.5 text-[14px] font-medium text-[#C62828]"
        />
      </div>
    </div>
  );
}
