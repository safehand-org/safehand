import Image from "next/image";
import Link from "next/link";
import { BellIcon, HamburgerIcon } from "./icons";

export function HomeHeader() {
  return (
    <header className="flex items-center justify-between">
      <Image
        src="/logo.svg"
        alt="Safehand"
        width={36}
        height={24}
        preload
        className="h-6 w-auto"
      />
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
