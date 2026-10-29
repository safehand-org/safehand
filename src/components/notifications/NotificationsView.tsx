"use client";

import Image from "next/image";
import Link from "next/link";
import { useState } from "react";
import { NotificationsHeader } from "./NotificationsHeader";
import {
  notificationFilters,
  notificationsForFilter,
  notificationsSample,
  type AppNotification,
  type NotificationFilterId,
} from "./types";

export function NotificationsView() {
  const [filter, setFilter] = useState<NotificationFilterId>("all");
  const items = notificationsForFilter(filter, notificationsSample);
  const featured = items.find((item) => item.featured);
  const earlier = items.filter((item) => !item.featured);

  return (
    <>
      <section className="rounded-b-sheet bg-sheet px-5 pt-5 pb-5 text-ink">
        <NotificationsHeader />
        <div className="mt-7 flex items-start justify-between gap-2">
          <div className="min-w-0 pt-1">
            <h1 className="text-[2.15rem] leading-[0.95] font-semibold tracking-[-0.03em]">
              Notifications
            </h1>
            <p className="mt-3 max-w-[16rem] text-[15px] leading-5 text-sheet-muted">
              Updates about your cards and payouts.
            </p>
          </div>
          <Image
            src="/dark-stickman/running-dark.svg"
            alt=""
            width={119}
            height={130}
            className="pointer-events-none -mt-1 h-[5.35rem] w-auto shrink-0"
          />
        </div>
        {featured ? <FeaturedNotification item={featured} /> : null}
      </section>

      <div className="px-5 pt-5 pb-6">
        <div className="flex items-center gap-2">
          <div className="flex min-w-0 flex-1 gap-2 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
            {notificationFilters.map((item) => {
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
          <button
            type="button"
            className="shrink-0 text-[10px] font-semibold tracking-[0.12em] text-[#3b82f6] uppercase focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/30"
          >
            Mark all as read
          </button>
        </div>
        <EarlierList items={earlier} />
      </div>
    </>
  );
}

function FeaturedNotification({ item }: { item: AppNotification }) {
  return (
    <section className="mt-8">
      <p className="text-[11px] font-medium tracking-[0.16em] text-sheet-muted uppercase">
        Today
      </p>
      <article className="mt-3 rounded-[1.35rem] bg-[var(--summary)] px-4 py-4">
        <div className="flex items-start justify-between gap-3">
          <h2 className="text-[16px] font-medium tracking-[-0.015em] text-ink">
            {item.title}
          </h2>
          <p className="shrink-0 pt-0.5 text-[10px] font-medium tracking-[0.12em] text-[var(--summary-muted)] uppercase">
            {item.time}
          </p>
        </div>
        <p className="mt-1.5 text-[13px] leading-5 text-sheet-muted">
          {item.body}
        </p>
        {item.actionLabel ? (
          <Link
            href="/cards"
            prefetch={false}
            className="mt-4 inline-flex h-10 items-center rounded-full bg-ink px-5 text-[13px] font-medium text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/40"
          >
            {item.actionLabel}
          </Link>
        ) : null}
      </article>
    </section>
  );
}

function EarlierList({ items }: { items: AppNotification[] }) {
  if (items.length === 0) {
    return (
      <p className="px-2 py-10 text-center text-[13px] text-muted">
        No notifications in this filter.
      </p>
    );
  }

  return (
    <section className="mt-6">
      <h2 className="text-[11px] font-medium tracking-[0.16em] text-muted uppercase">
        Earlier
      </h2>
      <ul className="mt-3 flex flex-col gap-2.5">
        {items.map((item) => (
          <li
            key={item.id}
            className="flex items-start gap-3 rounded-[1.6rem] bg-[var(--raised)] px-4 py-4"
          >
            <div className="min-w-0 flex-1">
              <p className="text-[16px] font-medium tracking-[-0.015em] text-white">
                {item.title}
              </p>
              <p className="mt-1 text-[12.5px] leading-[1.35] text-[#8b939a]">
                {item.body}
              </p>
            </div>
            <p className="shrink-0 pt-0.5 text-[10px] font-medium tracking-[0.12em] text-[#8b939a] uppercase">
              {item.time}
            </p>
          </li>
        ))}
      </ul>
    </section>
  );
}
