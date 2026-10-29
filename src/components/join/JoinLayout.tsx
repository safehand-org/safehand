"use client";

import type { ReactNode } from "react";
import Link from "next/link";
import { BackTriangleIcon } from "@/components/create/icons";

export function JoinLayout({
  title,
  subtitle,
  figure,
  showLogo = false,
  stepLabel,
  stepPill,
  progress,
  onBack,
  sheetNote,
  note,
  terms,
  actions,
  actionsAlign = "center",
  figureSize = "lg",
  figureClassName,
  children,
}: {
  title: ReactNode;
  subtitle: ReactNode;
  figure?: ReactNode;
  showLogo?: boolean;
  stepLabel?: string;
  stepPill?: string;
  progress?: number;
  onBack?: () => void;
  sheetNote?: ReactNode;
  note?: ReactNode;
  terms?: ReactNode;
  actions?: ReactNode;
  actionsAlign?: "center" | "end";
  figureSize?: "sm" | "lg";
  figureClassName?: string;
  children?: ReactNode;
}) {
  const headingSpace = figure
    ? figureSize === "sm"
      ? "relative mt-6 min-h-[4.5rem]"
      : "relative mt-6 min-h-[7.75rem]"
    : "relative mt-7";
  const titleWidth = figure
    ? figureSize === "sm"
      ? "max-w-[16.5rem]"
      : "max-w-[15.75rem]"
    : "max-w-[17.5rem]";
  const subtitleWidth = figure
    ? figureSize === "sm"
      ? "max-w-[19.5rem]"
      : "max-w-[15rem]"
    : "max-w-[20.5rem]";

  return (
    <div className="flex min-h-full flex-col">
      <section className="relative overflow-hidden rounded-b-sheet bg-sheet text-ink">
        {progress != null ? (
          <div className="h-1 w-full">
            <div className="h-full bg-ink" style={{ width: `${progress}%` }} />
          </div>
        ) : null}

        <div className={showLogo ? "px-5 pt-5 pb-6" : "px-5 pt-3.5 pb-6"}>
          <header className="flex items-center justify-between">
            {showLogo ? (
              // eslint-disable-next-line @next/next/no-img-element
              <img src="/logo.svg" alt="Safehand" width={36} height={24} className="h-6 w-auto" />
            ) : (
              <button
                type="button"
                onClick={onBack}
                className="flex items-center gap-1.5 text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/30"
              >
                <BackTriangleIcon />
                {stepLabel ? (
                  <span className="text-[10px] font-semibold tracking-[0.14em] uppercase">
                    {stepLabel}
                  </span>
                ) : null}
              </button>
            )}
            {stepPill ? (
              <span className="rounded-full border border-[#ebebeb] bg-white px-3 py-1.5 text-[10px] font-medium tracking-[0.12em] text-[#6f6f6f] uppercase">
                {stepPill}
              </span>
            ) : null}
          </header>

          <div className={headingSpace}>
            {figure ? (
              <div
                className={
                  figureClassName ??
                  (figureSize === "sm"
                    ? "pointer-events-none absolute top-3 right-0"
                    : "pointer-events-none absolute top-1 right-0")
                }
              >
                {figure}
              </div>
            ) : null}
            <div className={titleWidth}>
              <h1 className="text-[2.15rem] leading-[0.95] font-semibold tracking-[-0.03em]">
                {title}
              </h1>
            </div>
            <p className={`mt-3 text-[15px] leading-5 text-sheet-muted ${subtitleWidth}`}>
              {subtitle}
            </p>
          </div>

          {children ? <div className="mt-7">{children}</div> : null}
          {sheetNote ? (
            <p className="mt-5 px-4 text-center text-[12.5px] leading-5 text-sheet-muted">
              {sheetNote}
            </p>
          ) : null}
        </div>
      </section>

      <div className="flex flex-1 flex-col px-5 pt-4 pb-8">
        {note ? <DashedNote>{note}</DashedNote> : null}
        {terms ? <DashedNote>{terms}</DashedNote> : null}
        {actions ? (
          <div
            className={
              actionsAlign === "end"
                ? "mt-auto flex items-center justify-end pt-10"
                : "mt-auto flex items-center justify-center gap-3 pt-10"
            }
          >
            {actions}
          </div>
        ) : null}
      </div>
    </div>
  );
}

export function DashedNote({ children }: { children: ReactNode }) {
  return (
    <p className="rounded-[1.35rem] border border-dashed border-hairline px-5 py-3 text-center text-[12.5px] leading-[1.4] text-muted">
      {children}
    </p>
  );
}

export function OutlineButton({
  children,
  onClick,
  href,
}: {
  children: ReactNode;
  onClick?: () => void;
  href?: string;
}) {
  const className =
    "inline-flex h-12 items-center justify-center rounded-full border border-hairline bg-transparent px-8 text-[16px] font-medium text-white transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/40";

  if (href) {
    return (
      <Link href={href} prefetch={false} className={className}>
        {children}
      </Link>
    );
  }

  return (
    <button type="button" onClick={onClick} className={className}>
      {children}
    </button>
  );
}

export function FieldLabel({ children }: { children: ReactNode }) {
  return (
    <span className="text-[10px] font-medium tracking-[0.16em] text-[#8a8a8a] uppercase">
      {children}
    </span>
  );
}

export const TERMS_FOOTER = (
  <>
    By clicking continue, you agree to the{" "}
    <span className="font-medium text-[#d7d0c6]">
      Terms
      <br />
      and Conditions of SafeHand.
    </span>
  </>
);
