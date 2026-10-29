"use client";

import Image from "next/image";
import { useState } from "react";
import { HistoryHeader } from "./HistoryHeader";
import { HistoryList } from "./HistoryList";
import {
  historyFilters,
  historyForFilter,
  historySample,
  type HistoryFilterId,
} from "./types";

export function HistoryView() {
  const [filter, setFilter] = useState<HistoryFilterId>("all");
  const cards = historyForFilter(filter, historySample);

  return (
    <>
      <section className="rounded-b-sheet bg-sheet px-5 pt-5 pb-6 text-ink">
        <HistoryHeader />
        <div className="mt-7 flex items-end justify-between gap-3">
          <div className="min-w-0">
            <h1 className="text-[2.15rem] leading-[0.95] font-semibold tracking-[-0.03em]">
              Past
              <br />
              Cards
            </h1>
            <p className="mt-3 max-w-[16.5rem] text-[15px] leading-5 text-sheet-muted">
              Cards that have been paid out or cancelled.
            </p>
          </div>
          <Image
            src="/dark-stickman/worried-dark.svg"
            alt=""
            width={74}
            height={165}
            className="pointer-events-none h-[8.15rem] w-auto shrink-0"
          />
        </div>
      </section>

      <div className="px-5 pt-5 pb-6">
        <div className="flex gap-2 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
          {historyFilters.map((item) => {
            const selected = item.id === filter;
            return (
              <button
                key={item.id}
                type="button"
                aria-pressed={selected}
                onClick={() => setFilter(item.id)}
                className={
                  selected
                    ? "shrink-0 rounded-full bg-[#1a1816] px-3.5 py-2 text-[13px] font-medium text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/40"
                    : "shrink-0 rounded-full bg-[#2a2826] px-3.5 py-2 text-[13px] font-medium text-[#8d959b] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/30"
                }
              >
                {item.label}
              </button>
            );
          })}
        </div>
        <HistoryList cards={cards} />
      </div>
    </>
  );
}
