import Link from "next/link";
import { BellIcon, HamburgerIcon } from "@/components/home/icons";

export function ScanHeader() {
  return (
    <header className="flex items-center justify-between">
      <Link
        href="/"
        prefetch={false}
        className="flex items-center gap-1 text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/30"
      >
        <BackTriangleIcon />
        <span className="text-[15px] font-semibold uppercase tracking-[0.02em]">
          Scan to join
        </span>
      </Link>
      <div className="flex items-center gap-3.5">
        <Link
          href="/notifications"
          prefetch={false}
          aria-label="Notifications"
          className="relative flex items-center justify-center text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/30"
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
          className="flex items-center justify-center text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/30"
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
