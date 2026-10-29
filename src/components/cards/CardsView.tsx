"use client";

import { useState } from "react";
import { CardList } from "./CardList";
import { CardsHeader } from "./CardsHeader";
import { SignatureCard } from "./SignatureCard";
import { SummaryCard } from "./SummaryCard";
import {
  cardFilters,
  cardsForFilter,
  cardsSample,
  type CardFilterId,
} from "./types";

export function CardsView() {
  const [filter, setFilter] = useState<CardFilterId>("all");
  const cards = cardsForFilter(filter, cardsSample);

  return (
    <>
      <section className="rounded-b-sheet bg-sheet px-5 pt-5 pb-5 text-ink">
        <CardsHeader />
        <SummaryCard />
        <SignatureCard />
      </section>

      <div className="px-5 pt-5 pb-6">
        <div className="flex gap-2 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
          {cardFilters.map((item) => {
            const selected = item.id === filter;
            return (
              <button
                key={item.id}
                type="button"
                aria-pressed={selected}
                onClick={() => setFilter(item.id)}
                className={
                  selected
                    ? "shrink-0 rounded-full bg-[#1a1816] px-2.5 py-2 text-[13px] font-medium text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/40"
                    : "shrink-0 rounded-full bg-[#2a2826] px-2.5 py-2 text-[13px] font-medium text-[#8d959b] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/30"
                }
              >
                {item.label}
              </button>
            );
          })}
        </div>
        <CardList cards={cards} />
      </div>
    </>
  );
}
