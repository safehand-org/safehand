"use client";

import Image from "next/image";
import Link from "next/link";
import { useState } from "react";
import { CardsHeader } from "@/components/cards/CardsHeader";
import {
  headerLabel,
  payoutNote,
  type LockedCardPhase,
  type LockedMember,
  type LockedSplitCardDetail,
} from "./types";

export function LockedCardStatusView({ card }: { card: LockedSplitCardDetail }) {
  const copy = sectionCopy(card.phase);
  const runningHero = !card.prize;

  return (
    <div className="absolute inset-0 flex flex-col">
      <div className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto overscroll-contain [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
        <section className="rounded-b-sheet bg-sheet px-5 pt-5 pb-5 text-ink">
          <CardsHeader
            label={headerLabel(card.headerVariant)}
            backHref="/cards"
            compact
          />

          <div className="relative mt-5">
            <h1 className="text-[2.35rem] leading-[0.95] font-semibold tracking-[-0.035em]">
              {card.title}
            </h1>
            <p className="mt-3 max-w-[21rem] text-[14.5px] leading-[1.35] text-sheet-muted">
              {card.subtitle}
            </p>
            {runningHero ? (
              <div className="mt-2 flex justify-center">
                <Image
                  src="/dark-stickman/running-dark.svg"
                  alt=""
                  width={119}
                  height={130}
                  className="pointer-events-none h-[148px] w-auto"
                />
              </div>
            ) : null}
          </div>

          {card.prize ? <PrizeCard card={card} /> : null}
          {card.payoutsSummary ? (
            <>
              <PayoutProgress
                paid={card.payoutsSummary.paid}
                total={card.payoutsSummary.total}
              />
              <PayoutsSummary
                paid={card.payoutsSummary.paid}
                total={card.payoutsSummary.total}
              />
            </>
          ) : null}
          <CardLinkPanel card={card} />
        </section>

        <div className="px-5 pt-4 pb-6">
          <p className="rounded-[1.35rem] border border-dashed border-[#3c444a] px-5 py-3.5 text-center text-[13px] leading-[1.4] text-[#9aa1a6]">
            {payoutNote(card.phase)}
          </p>
          <WhoGetsWhat card={card} label={copy.shares} />
          {copy.showAgreed ? <AgreedShare card={card} label={copy.agreed} /> : null}
          {copy.showAgreements ? (
            <Agreements card={card} label={copy.agreements} />
          ) : null}
          {copy.showTimeline ? <Timeline card={card} /> : null}
        </div>
      </div>
      <StatusFooter card={card} />
    </div>
  );
}

function sectionCopy(phase: LockedCardPhase) {
  if (phase === "prize-set-waiting") {
    return {
      shares: "Who's left to sign",
      agreed: "Power share",
      agreements: "What everyone agreed to",
      showAgreed: true,
      showAgreements: true,
      showTimeline: true,
    };
  }
  if (phase === "prize-in-card") {
    return {
      shares: "Split details",
      agreed: "",
      agreements: "",
      showAgreed: false,
      showAgreements: false,
      showTimeline: false,
    };
  }
  if (phase === "payouts-in-progress") {
    return {
      shares: "Who gets what",
      agreed: "Agreed shares",
      agreements: "What do you agree to",
      showAgreed: true,
      showAgreements: true,
      showTimeline: true,
    };
  }
  return {
    shares: "Who gets what",
    agreed: "Agreed share",
    agreements: "What everyone agreed to",
    showAgreed: true,
    showAgreements: true,
    showTimeline: true,
  };
}

function PrizeCard({ card }: { card: LockedSplitCardDetail }) {
  const prize = card.prize;
  if (!prize) return null;

  return (
    <div className="relative mt-4 overflow-hidden rounded-[1.2rem] bg-[#eef3f4] px-4 pt-3.5 pb-3.5">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="text-[10px] font-semibold tracking-[0.16em] text-[#43c7ff] uppercase">
            {prize.badge}
          </p>
          <p className="mt-2 flex items-baseline gap-2">
            <span className="text-[2.45rem] leading-none font-bold tracking-[-0.045em]">
              {prize.amountLabel}
            </span>
            <span className="text-[15px] font-medium text-[#8e989f]">{prize.unit}</span>
          </p>
        </div>
        <Image
          src="/dark-stickman/standing-dark.svg"
          alt=""
          width={73}
          height={177}
          className="pointer-events-none h-[72px] w-auto shrink-0"
        />
      </div>
      <div className="mt-2 flex items-center justify-between gap-3 text-[11px] font-medium tracking-[0.06em] text-[#8e989f] uppercase">
        <span>{prize.date}</span>
        <span>{prize.time}</span>
      </div>
    </div>
  );
}

function PayoutProgress({ paid, total }: { paid: number; total: number }) {
  const width = total === 0 ? 0 : (paid / total) * 100;
  return (
    <div className="mt-3 h-[3px] overflow-hidden rounded-full bg-[#d5dce0]">
      <div className="h-full bg-ink" style={{ width: `${width}%` }} />
    </div>
  );
}

function PayoutsSummary({ paid, total }: { paid: number; total: number }) {
  return (
    <div className="mt-3 flex items-center justify-between">
      <span className="rounded-full bg-[#f3f4f5] px-3.5 py-1.5 text-[10px] font-semibold tracking-[0.14em] text-[#7d868c] uppercase">
        Payouts
      </span>
      <span className="flex items-center gap-1.5 text-[11px] font-semibold tracking-[0.1em] text-ink uppercase">
        {paid} of {total} paid
        <svg viewBox="0 0 10 10" aria-hidden="true" className="size-2.5 fill-current">
          <path d="M2 1.2 8.2 5 2 8.8V1.2Z" />
        </svg>
      </span>
    </div>
  );
}

function CardLinkPanel({ card }: { card: LockedSplitCardDetail }) {
  return (
    <div className="mt-4 rounded-[1.2rem] bg-[#0c0e10] px-4 py-3.5 text-white">
      <p className="text-[12px] text-[#8b939a]">Card Link:</p>
      <p className="mt-1 text-[16px] font-semibold tracking-[-0.015em]">
        {card.cardLink}
      </p>
      <div className="mt-3 grid grid-cols-[1fr_auto_1fr] items-center gap-3 text-[10px] font-medium tracking-[0.08em] text-[#8d949b] uppercase">
        <span>Locked on {card.lockedOn.date}</span>
        <span aria-hidden="true" className="h-3.5 w-px bg-white/20" />
        <span className="text-right">{card.lockedOn.time}</span>
      </div>
    </div>
  );
}

function WhoGetsWhat({
  card,
  label,
}: {
  card: LockedSplitCardDetail;
  label: string;
}) {
  const [left, center, right] = card.members;
  if (!left || !center || !right) return null;

  return (
    <section className="mt-6">
      <SectionLabel>{label}</SectionLabel>
      <div className="mt-3 rounded-[1.35rem] bg-[#141618] px-4 py-4">
        <ShareLabel member={center} align="center" emphasize />
        <div className="mt-3 flex h-2.5 overflow-hidden rounded-full">
          {card.members.map((member) => (
            <div
              key={member.id}
              className="h-full"
              style={{ flexGrow: member.sharePercent, flexBasis: 0, background: member.color }}
            />
          ))}
        </div>
        <div className="mt-3 flex items-start justify-between gap-3">
          <ShareLabel member={left} />
          <ShareLabel member={right} align="end" short />
        </div>
      </div>
    </section>
  );
}

function formatShare(value: number) {
  return `${value}%`;
}

function ShareLabel({
  member,
  align = "start",
  emphasize = false,
  short = false,
}: {
  member: LockedMember;
  align?: "start" | "center" | "end";
  emphasize?: boolean;
  short?: boolean;
}) {
  const name = short ? member.shortName : member.name;
  const alignClass =
    align === "center"
      ? "items-center text-center"
      : align === "end"
        ? "items-end text-right"
        : "items-start";

  return (
    <div className={`flex min-w-0 flex-col ${alignClass}`}>
      <p
        className={emphasize ? "text-[14px] font-medium" : "text-[13px] font-medium"}
        style={{ color: member.color }}
      >
        {name}
      </p>
      <p className="text-[11px] text-[#8b939a]">{formatShare(member.sharePercent)}</p>
    </div>
  );
}

function AgreedShare({
  card,
  label,
}: {
  card: LockedSplitCardDetail;
  label: string;
}) {
  return (
    <section className="mt-6">
      <SectionLabel>{label}</SectionLabel>
      <ul className="mt-3 flex flex-col gap-2.5">
        {card.members.map((member) => {
          const chip = shareChip(card.phase, member);
          return (
            <li
              key={member.id}
              className="flex items-center gap-3 rounded-[1.15rem] bg-[#141618] px-4 py-3.5"
            >
              <span
                aria-hidden="true"
                className="size-2.5 shrink-0 rounded-[3px]"
                style={{ background: member.color }}
              />
              <p className="min-w-0 flex-1 truncate text-[15px] font-medium text-white">
                {member.name}{" "}
                <span className="font-medium text-[#d5dbe0]">{member.sharePercent}%</span>
              </p>
              <span
                className={
                  chip.tone === "paid"
                    ? "shrink-0 rounded-full bg-[#34b6ee] px-3 py-1 text-[10px] font-semibold tracking-[0.1em] text-white uppercase"
                    : "shrink-0 rounded-full bg-[#2a2e31] px-3 py-1 text-[10px] font-semibold tracking-[0.1em] text-[#9aa1a6] uppercase"
                }
              >
                {chip.label}
              </span>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

function shareChip(phase: LockedCardPhase, member: LockedMember) {
  if (phase !== "payouts-in-progress" || !member.payoutStatus) {
    return { label: "Signed", tone: "muted" as const };
  }
  if (member.payoutStatus === "paid") {
    return { label: "Paid", tone: "paid" as const };
  }
  return { label: "Pending", tone: "muted" as const };
}

function Agreements({
  card,
  label,
}: {
  card: LockedSplitCardDetail;
  label: string;
}) {
  if (card.agreements.length === 0) return null;

  return (
    <section className="mt-6">
      <SectionLabel>{label}</SectionLabel>
      <div className="mt-3 flex flex-col gap-2.5">
        {card.agreements.map((item) => (
          <article
            key={item.title}
            className="flex items-center gap-3 rounded-[1.25rem] bg-[#1a1d1f] px-4 py-4"
          >
            <div className="min-w-0 flex-1">
              <p className="text-[15px] font-semibold tracking-[-0.02em] text-white">
                {item.title}
              </p>
              <p className="mt-1 text-[12.5px] leading-5 text-[#8b939a]">{item.body}</p>
            </div>
            <CheckBadge />
          </article>
        ))}
      </div>
    </section>
  );
}

function CheckBadge() {
  return (
    <span className="flex size-6 shrink-0 items-center justify-center rounded-full bg-white text-ink">
      <svg
        viewBox="0 0 24 24"
        aria-hidden="true"
        className="size-3.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <path d="M5.5 12.4 9.4 16.2 18.5 7.6" />
      </svg>
    </span>
  );
}

function Timeline({ card }: { card: LockedSplitCardDetail }) {
  if (card.timeline.length === 0) return null;

  return (
    <section className="mt-6">
      <SectionLabel>What&apos;s happened so far</SectionLabel>
      <ol className="relative mt-4 ml-1 border-l border-[#3a4248]">
        {card.timeline.map((event) => (
          <li
            key={event.label}
            className="relative flex items-start justify-between gap-3 pb-4 pl-4 last:pb-0"
          >
            <span
              aria-hidden="true"
              className="absolute top-1.5 -left-[5px] size-2 rounded-[2px] bg-[#9aa1a6]"
            />
            <p className="min-w-0 flex-1 text-[13.5px] leading-5 text-[#e8e4dc]">
              {event.label}
            </p>
            <p
              className={
                event.pending
                  ? "shrink-0 pt-0.5 text-[11px] font-semibold tracking-[0.08em] text-[#43c7ff] uppercase"
                  : "shrink-0 pt-0.5 text-[11px] font-medium tracking-[0.04em] text-[#8b939a] uppercase"
              }
            >
              {event.pending ? "Pending" : event.date}
            </p>
          </li>
        ))}
      </ol>
    </section>
  );
}

function SectionLabel({ children }: { children: string }) {
  return (
    <h2 className="text-[11px] font-medium tracking-[0.16em] text-[#8b939a] uppercase">
      {children}
    </h2>
  );
}

function StatusFooter({ card }: { card: LockedSplitCardDetail }) {
  const [copied, setCopied] = useState(false);

  async function onShare() {
    // TODO: share the canonical card URL from the API, not the mock slug
    const url = card.cardLink.startsWith("http")
      ? card.cardLink
      : `https://${card.cardLink}`;
    try {
      if (typeof navigator.share === "function") {
        await navigator.share({ title: card.title, text: card.title, url });
        return;
      }
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") return;
    }
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      setCopied(false);
    }
  }

  return (
    <div className="grid shrink-0 grid-cols-2 gap-3 bg-background px-5 pt-2 pb-[max(1.25rem,env(safe-area-inset-bottom))]">
      <Link
        href="/"
        prefetch={false}
        className="flex h-[3.25rem] items-center justify-center rounded-full border border-white/75 text-[16px] font-medium text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/40"
      >
        Go Home
      </Link>
      <button
        type="button"
        onClick={onShare}
        className="flex h-[3.25rem] items-center justify-center rounded-full bg-white text-[16px] font-medium text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/70"
      >
        {copied ? "Copied" : "Share"}
      </button>
    </div>
  );
}
