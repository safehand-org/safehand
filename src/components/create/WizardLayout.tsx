import type { ReactNode } from "react";
import Link from "next/link";
import { BellIcon, HamburgerIcon } from "@/components/home/icons";
import { BackTriangleIcon } from "./icons";

const STEP_PILLS: Record<number, string> = {
  1: "The Basics",
  2: "Add Teammates",
  3: "Shares",
  4: "If Someone Leaves",
  5: "Review",
};

export function WizardLayout({
  step,
  title,
  subtitle,
  note,
  extraNote,
  actions,
  sheetFooter,
  onBack,
  backHref,
  children,
}: {
  step: 1 | 2 | 3 | 4 | 5 | 6;
  title: string;
  subtitle: string;
  note?: string;
  extraNote?: string;
  actions: ReactNode;
  sheetFooter?: ReactNode;
  onBack: () => void;
  backHref?: string;
  children: ReactNode;
}) {
  const isInvite = step === 6;

  return (
    <div className="flex min-h-full flex-col">
      <section className="overflow-hidden rounded-b-sheet bg-sheet text-ink">
        {isInvite ? null : (
          <div className="h-[3px] w-full">
            <div
              className="h-full bg-ink"
              style={{ width: `${(step / 5) * 100}%` }}
            />
          </div>
        )}

        <div className={isInvite ? "px-5 pt-8 pb-6" : "px-5 pt-4 pb-6"}>
          {isInvite ? <InviteHeader onBack={onBack} /> : (
            <header className="flex items-center justify-between">
              {backHref ? (
                <Link
                  href={backHref}
                  prefetch={false}
                  className="flex items-center gap-1.5 text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/30"
                >
                  <BackTriangleIcon />
                  <span className="text-[10px] font-semibold tracking-[0.14em] uppercase">
                    Step {step} of 5
                  </span>
                </Link>
              ) : (
                <button
                  type="button"
                  onClick={onBack}
                  className="flex items-center gap-1.5 text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/30"
                >
                  <BackTriangleIcon />
                  <span className="text-[10px] font-semibold tracking-[0.14em] uppercase">
                    Step {step} of 5
                  </span>
                </button>
              )}
              <span className="rounded-full border border-[#ebebeb] bg-white px-3 py-1.5 text-[10px] font-medium tracking-[0.12em] text-[#6f6f6f] uppercase">
                {STEP_PILLS[step]}
              </span>
            </header>
          )}

          <div className="mt-7 max-w-[15.75rem]">
            <h1 className="text-[2.15rem] leading-[0.95] font-semibold tracking-[-0.03em]">
              {title}
            </h1>
          </div>
          <p className="mt-3 max-w-[20.5rem] text-[15px] leading-5 text-sheet-muted">
            {subtitle}
          </p>

          <div className="mt-7">{children}</div>
        </div>
        {sheetFooter}
      </section>

      <div className="flex flex-1 flex-col px-5 pt-4 pb-8">
        {note ? <DashedNote>{note}</DashedNote> : null}
        {extraNote ? (
          <div className="mt-3">
            <DashedNote>{extraNote}</DashedNote>
          </div>
        ) : null}
        <div className="mt-auto flex items-center justify-center gap-3 pt-10">
          {actions}
        </div>
      </div>
    </div>
  );
}

function InviteHeader({ onBack }: { onBack: () => void }) {
  return (
    <header className="flex items-center justify-between">
      <button
        type="button"
        onClick={onBack}
        className="flex items-center gap-1 text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/30"
      >
        <BackTriangleIcon />
        <span className="text-[15px] font-semibold tracking-[0.02em] uppercase">
          Invite
        </span>
      </button>
      <div className="flex items-center gap-3.5">
        <Link
          href="/notifications"
          prefetch={false}
          aria-label="Notifications"
          className="flex items-center justify-center text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/30"
        >
          <BellIcon />
        </Link>
        <button
          type="button"
          aria-label="Open menu"
          className="flex items-center justify-center text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/30"
        >
          <HamburgerIcon />
        </button>
      </div>
    </header>
  );
}

function DashedNote({ children }: { children: ReactNode }) {
  return (
    <p className="rounded-[1.35rem] border border-dashed border-hairline px-4 py-3 text-[12.5px] leading-[1.4] text-muted">
      {children}
    </p>
  );
}

export function PrimaryButton({
  children,
  disabled,
  onClick,
  href,
}: {
  children: ReactNode;
  disabled?: boolean;
  onClick?: () => void;
  href?: string;
}) {
  const className =
    "inline-flex h-12 items-center justify-center rounded-full bg-white px-10 text-[16px] font-medium text-ink transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/70 enabled:active:bg-neutral-200 disabled:cursor-not-allowed";

  if (href) {
    return (
      <Link href={href} prefetch={false} className={className}>
        {children}
      </Link>
    );
  }

  return (
    <button type="button" disabled={disabled} onClick={onClick} className={className}>
      {children}
    </button>
  );
}

export function SecondaryButton({
  children,
  onClick,
  href,
  disabled,
  className,
}: {
  children: ReactNode;
  onClick?: () => void;
  href?: string;
  disabled?: boolean;
  className?: string;
}) {
  const classes =
    `inline-flex h-12 items-center justify-center gap-2 rounded-full bg-[#1a1816] px-7 text-[16px] font-medium text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/30 disabled:cursor-not-allowed disabled:opacity-45 ${className ?? ""}`.trim();

  if (href) {
    return (
      <Link href={href} prefetch={false} className={classes}>
        {children}
      </Link>
    );
  }

  return (
    <button type="button" disabled={disabled} onClick={onClick} className={classes}>
      {children}
    </button>
  );
}

export function Field({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <label className="block">
      <span className="text-[10px] font-medium tracking-[0.16em] text-[#8a8a8a] uppercase">
        {label}
      </span>
      <input
        type="text"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="mt-2.5 h-12 w-full rounded-full border border-[#e7e7e7] bg-white px-5 text-[15px] text-ink outline-none focus-visible:ring-2 focus-visible:ring-ink/15"
      />
    </label>
  );
}
