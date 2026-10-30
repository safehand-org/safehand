"use client";

import { usePendingCards } from "@/lib/pendingCards";

/** How many cards wait for the viewer's signature, on the Cards tab. */
export function CardsBadge() {
  const count = usePendingCards().length;
  if (count === 0) return null;
  return (
    <span
      aria-label={`${count} card${count === 1 ? "" : "s"} to sign`}
      className="absolute -top-1 -right-1 flex min-w-4 items-center justify-center rounded-full bg-[#E23B3B] px-1 text-[10px] leading-4 font-semibold text-white"
    >
      {count}
    </span>
  );
}
