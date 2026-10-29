import Link from "next/link";
import { BellIcon, HamburgerIcon } from "@/components/home/icons";

export function CardsHeader({
  label = "Cards",
  backHref = "/",
  compact = false,
}: {
  label?: string;
  backHref?: string;
  compact?: boolean;
}) {
  return (
    <header className="flex items-center justify-between">
      <Link
        href={backHref}
        prefetch={false}
        className="flex items-center gap-1 text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/30"
      >
        <BackTriangleIcon />
        <span
          className={
            compact
              ? "text-[11px] font-semibold tracking-[0.12em] uppercase"
              : "text-[15px] font-semibold uppercase tracking-[0.02em]"
          }
        >
          {label}
        </span>
      </Link>
      <div className="flex items-center gap-2.5">
        <Link
          href="/notifications"
          prefetch={false}
          aria-label="Notifications"
          className="relative flex items-center justify-center text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/30"
        >
          <BellIcon />
          <span
            aria-hidden="true"
            className="absolute -top-0.5 -right-0.5 size-1.5 rounded-full bg-[var(--accent)]"
          />
        </Link>
        <button
          type="button"
          aria-label="Open menu"
          className="flex h-8 w-11 items-center justify-center rounded-[12px] bg-hamburger text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/30"
        >
          <HamburgerIcon />
        </button>
      </div>
    </header>
  );
}

function BackTriangleIcon() {
  return (
    <svg
      viewBox="0 0 10 14"
      aria-hidden="true"
      className="h-[13px] w-[9px]"
      fill="currentColor"
    >
      <path d="M10 0.6 0.4 7 10 13.4V0.6Z" />
    </svg>
  );
}
