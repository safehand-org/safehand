import Link from "next/link";
import type { HomeActivityItem } from "./types";

export function ActivityList({ items }: { items: HomeActivityItem[] }) {
  return (
    <>
      <p className="mt-6 text-[11px] font-medium tracking-[0.16em] text-muted uppercase">
        Activity
      </p>
      {items.length === 0 ? (
        <ActivityEmpty />
      ) : (
        <ul className="mt-3 flex flex-col gap-2.5">
          {items.map((item) => (
            <ActivityRow key={item.id} item={item} />
          ))}
        </ul>
      )}
    </>
  );
}

export function ActivityRow({ item }: { item: HomeActivityItem }) {
  const className =
    "flex items-center gap-3 rounded-[1.35rem] bg-chip px-4 py-3.5";
  const body = (
    <>
      <div className="min-w-0 flex-1">
        <p className="text-[16px] font-medium tracking-[-0.015em] text-white">
          {item.title}
        </p>
        <p className="mt-1 text-[12.5px] leading-[1.35] text-[#8b939a]">
          {item.detail}
        </p>
      </div>
      <ActivityPill status={item.status} />
    </>
  );

  if (!item.href) {
    return <li className={className}>{body}</li>;
  }

  return (
    <li>
      <Link
        href={item.href}
        prefetch={false}
        className={`${className} focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/30`}
      >
        {body}
      </Link>
    </li>
  );
}

function ActivityPill({ status }: { status: string }) {
  const key = status.toLowerCase();
  const tone =
    key === "draft"
      ? "bg-white text-[#2c2c2c]"
      : key === "locked"
        ? "bg-[#1a1d1f] text-[#6b7378]"
        : key === "paid out" || key === "paid-out"
          ? "bg-[#34b6ee] text-white"
          : "text-foreground ring-1 ring-hairline/80";

  return (
    <span
      className={`inline-flex h-[22px] shrink-0 items-center self-center rounded-full px-2.5 text-[10px] leading-none font-semibold tracking-[0.08em] uppercase ${tone}`}
    >
      {status}
    </span>
  );
}

function ActivityEmpty() {
  return (
    <section className="mt-3 rounded-panel border border-dashed border-hairline px-4 pt-7 pb-6">
      <div className="text-center">
        <h2 className="text-[15px] font-medium text-sheet">
          No activity record yet.
        </h2>
        <p className="mt-1.5 text-[12.5px] leading-5 text-muted">
          Create a split card or join a split to get started.
        </p>
      </div>

      <div className="mt-5 grid grid-cols-2 gap-3">
        <Link
          href="/create"
          prefetch={false}
          className="flex h-12 items-center justify-center rounded-pill bg-sheet px-3 text-[13px] font-medium text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sheet/50"
        >
          Create Split Card
        </Link>
        <Link
          href="/scan"
          prefetch={false}
          className="flex h-12 items-center justify-center rounded-pill border border-sheet/40 px-3 text-[13px] font-medium text-sheet focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sheet/40"
        >
          Join With A Link
        </Link>
      </div>
    </section>
  );
}
