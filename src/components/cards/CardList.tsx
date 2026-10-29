import Link from "next/link";
import type { SplitCard } from "./types";

const pillClass = {
  draft: "bg-white text-[#2c2c2c]",
  locked: "bg-[#1a1d1f] text-[#8e969c]",
  "paid-out": "bg-[var(--accent)] text-white",
  cancelled: "bg-[#2a2926] text-[#9aa1a6]",
} as const;

const pillLabel = {
  draft: "Draft",
  locked: "Locked",
  "paid-out": "Paid out",
  cancelled: "Cancelled",
} as const;

export function CardList({ cards }: { cards: SplitCard[] }) {
  if (cards.length === 0) {
    return (
      <p className="px-2 py-10 text-center text-[13px] text-muted">
        No cancelled cards.
      </p>
    );
  }

  return (
    <ul className="mt-3.5 flex flex-col gap-2.5">
      {cards.map((card) => (
        <CardRow key={card.id} card={card} />
      ))}
    </ul>
  );
}

function CardRow({ card }: { card: SplitCard }) {
  const className =
    "flex items-center gap-3 rounded-[1.6rem] bg-[var(--raised)] px-4 py-4";
  const body = (
    <>
      <div className="min-w-0 flex-1">
        <p className="text-[16px] font-medium tracking-[-0.015em] text-white">
          {card.title}
        </p>
        <p className="mt-1 text-[12.5px] leading-[1.35] text-[#8b939a]">
          {card.detail}
        </p>
      </div>
      <span
        className={`shrink-0 rounded-full px-3 py-1.5 text-[10px] font-semibold tracking-[0.08em] uppercase ${pillClass[card.status]}`}
      >
        {pillLabel[card.status]}
      </span>
    </>
  );

  const href =
    card.href ??
    (card.status === "locked" || card.status === "paid-out"
      ? `/card/${card.id}`
      : undefined);

  if (!href) {
    return <li className={className}>{body}</li>;
  }

  return (
    <li>
      <Link
        href={href}
        prefetch={false}
        className={`${className} focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/30`}
      >
        {body}
      </Link>
    </li>
  );
}
