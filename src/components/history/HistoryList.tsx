import type { HistoryCard } from "./types";
import { groupHistoryByMonth } from "./types";

const statusClass = {
  "paid-out": "text-[#3b82f6]",
  cancelled: "text-[#8b939a]",
} as const;

const statusLabel = {
  "paid-out": "Paid out",
  cancelled: "Cancelled",
} as const;

export function HistoryList({ cards }: { cards: HistoryCard[] }) {
  if (cards.length === 0) {
    return (
      <p className="px-2 py-10 text-center text-[13px] text-muted">
        No cards in this filter.
      </p>
    );
  }

  const groups = groupHistoryByMonth(cards);

  return (
    <div className="mt-6 flex flex-col gap-6">
      {groups.map((group) => (
        <section key={group.month}>
          <h2 className="text-[11px] font-medium tracking-[0.16em] text-muted uppercase">
            {group.month}
          </h2>
          <ul className="mt-3 flex flex-col gap-2.5">
            {group.cards.map((card) => (
              <HistoryRow key={card.id} card={card} />
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}

function HistoryRow({ card }: { card: HistoryCard }) {
  return (
    <li className="rounded-[1.6rem] bg-[var(--raised)] px-4 py-4">
      <p className="text-[16px] font-medium tracking-[-0.015em] text-white">
        {card.title}
      </p>
      <p className="mt-1 text-[12.5px] leading-[1.35] text-[#8b939a]">
        {card.detail}
      </p>
      {card.amount || card.date ? (
        <div className="mt-3 flex items-center text-[11px] font-medium tracking-[0.08em] uppercase">
          <span className="shrink-0 text-[#8b939a]">{card.amount}</span>
          <span aria-hidden="true" className="mx-3 h-3 w-px bg-[#3a3834]" />
          <span className="min-w-0 flex-1 text-[#8b939a]">{card.date}</span>
          <span aria-hidden="true" className="mx-3 h-3 w-px bg-[#3a3834]" />
          <span className={`shrink-0 ${statusClass[card.status]}`}>
            {statusLabel[card.status]}
          </span>
        </div>
      ) : (
        <p
          className={`mt-3 text-right text-[11px] font-medium tracking-[0.08em] uppercase ${statusClass[card.status]}`}
        >
          {statusLabel[card.status]}
        </p>
      )}
    </li>
  );
}
