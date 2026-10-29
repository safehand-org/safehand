import Link from "next/link";
import type { HomeAttention } from "./types";

const actionClass =
  "inline-flex h-10 shrink-0 items-center rounded-pill bg-sheet px-5 text-[13px] font-medium text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sheet/50";

export function AttentionCard({ attention }: { attention: HomeAttention }) {
  return (
    <article className="mt-1 rounded-[1.35rem] bg-ink px-[1.05rem] pt-3.5 pb-3.5 text-sheet">
      <div className="flex items-center justify-between text-[10px] font-medium tracking-[0.14em] text-muted uppercase">
        <p>Needs your attention</p>
        <p>{attention.timeAgo}</p>
      </div>
      <h2 className="mt-2.5 text-[18px] font-semibold tracking-[-0.02em]">
        {attention.title}
      </h2>
      <p className="mt-1.5 text-[13px] leading-5 text-foreground">
        {attention.body}
      </p>
      <div className="mt-4 flex items-center justify-between gap-3">
        <p className="text-[10px] font-medium tracking-[0.14em] text-muted uppercase">
          {attention.teammatesLabel}
        </p>
        {attention.href ? (
          <Link href={attention.href} prefetch={false} className={actionClass}>
            {attention.actionLabel}
          </Link>
        ) : (
          <button type="button" className={actionClass}>
            {attention.actionLabel}
          </button>
        )}
      </div>
    </article>
  );
}
