"use client";

import { useState } from "react";
import { DashedNote } from "@/components/join/JoinLayout";
import { cardsSummaryFrom, signatureRequestFrom, splitCardsFrom } from "@/lib/account";
import { useMe } from "@/lib/useMe";
import { CardList } from "./CardList";
import { CardsHeader } from "./CardsHeader";
import { SignatureCard } from "./SignatureCard";
import { SummaryCard } from "./SummaryCard";
import { cardFilters, cardsForFilter, type CardFilterId } from "./types";

export function CardsView() {
  const account = useMe();
  const [filter, setFilter] = useState<CardFilterId>("all");
  const me = account.status === "ready" ? account.me : null;
  const cards = me ? cardsForFilter(filter, splitCardsFrom(me)) : null;
  const signature = me ? signatureRequestFrom(me.needs_signature) : null;

  return (
    <>
      <section className="rounded-b-sheet bg-sheet px-5 pt-5 pb-5 text-ink">
        <CardsHeader />
        {me ? <SummaryCard summary={cardsSummaryFrom(me)} /> : null}
        {signature ? <SignatureCard request={signature} /> : null}
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
        {cards ? <CardList cards={cards} /> : null}
        {account.status === "error" ? (
          <div className="mt-6">
            <DashedNote>
              {account.message}{" "}
              <button type="button" onClick={account.retry} className="text-[#34b6ee]">
                Try again
              </button>
            </DashedNote>
          </div>
        ) : null}
      </div>
    </>
  );
}
