import Link from "next/link";
import { CardsBadge } from "./CardsBadge";

const tabs = [
  { href: "/", label: "Home", icon: HomeIcon, plus: false },
  { href: "/cards", label: "Cards", icon: CardsIcon, plus: false },
  { href: "/create", label: "Create", icon: PlusIcon, plus: true },
  { href: "/scan", label: "Scan", icon: ScanIcon, plus: false },
  { href: "/history", label: "History", icon: HistoryIcon, plus: false },
] as const;

export function BottomNav({ activeHref = "/" }: { activeHref?: string }) {
  return (
    <nav
      aria-label="Primary"
      className="shrink-0 bg-background px-3 pb-[max(0.75rem,env(safe-area-inset-bottom))] pt-2"
    >
      <ul className="flex items-end justify-between">
        {tabs.map((tab) => {
          const isActive = tab.href === activeHref;

          if (tab.plus) {
            const Icon = tab.icon;
            return (
              <li key={tab.href}>
                <Link
                  href={tab.href}
                  prefetch={false}
                  aria-label={tab.label}
                  className="flex size-12 items-center justify-center rounded-full bg-sheet text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sheet/50"
                >
                  <Icon />
                </Link>
              </li>
            );
          }

          const Icon = tab.icon;
          return (
            <li key={tab.href}>
              <Link
                href={tab.href}
                prefetch={false}
                aria-current={isActive ? "page" : undefined}
                className={
                  isActive
                    ? "flex min-w-[4.5rem] flex-col items-center gap-0.5 rounded-chip bg-chip px-3 py-2 text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sheet/40"
                    : "flex min-w-[3.25rem] flex-col items-center gap-0.5 px-2 py-1.5 text-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sheet/40"
                }
              >
                <span className="relative">
                  <Icon filled={isActive} />
                  {tab.href === "/cards" ? <CardsBadge /> : null}
                </span>
                <span className="text-[11px] font-medium leading-none">
                  {tab.label}
                </span>
              </Link>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}

function HomeIcon({ filled = false }: { filled?: boolean }) {
  return (
    <svg
      viewBox="0 0 24 24"
      aria-hidden="true"
      className="size-[22px]"
      fill={filled ? "currentColor" : "none"}
      stroke="currentColor"
      strokeWidth={filled ? 0 : 1.7}
      strokeLinejoin="round"
    >
      <path d="M4.5 10.5 12 4.25l7.5 6.25V20a1.25 1.25 0 0 1-1.25 1.25h-4.5v-6.5h-4.5v6.5H5.75A1.25 1.25 0 0 1 4.5 20z" />
    </svg>
  );
}

function CardsIcon({ filled: _filled = false }: { filled?: boolean }) {
  return (
    <svg
      viewBox="0 0 24 24"
      aria-hidden="true"
      className="size-[22px]"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinejoin="round"
    >
      <rect x="3.5" y="7" width="13" height="10.5" rx="2" />
      <path d="M7.5 7V6.25A2.25 2.25 0 0 1 9.75 4h8.5A2.25 2.25 0 0 1 20.5 6.25v7.5A2.25 2.25 0 0 1 18.25 16H17" />
    </svg>
  );
}

function PlusIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      aria-hidden="true"
      className="size-6"
      fill="none"
      stroke="currentColor"
      strokeWidth={2.2}
      strokeLinecap="round"
    >
      <path d="M12 5.5v13M5.5 12h13" />
    </svg>
  );
}

function ScanIcon({ filled: _filled = false }: { filled?: boolean }) {
  return (
    <svg
      viewBox="0 0 24 24"
      aria-hidden="true"
      className="size-[22px]"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinejoin="round"
    >
      <path d="M7 4.75H5.75A1 1 0 0 0 4.75 5.75V7" />
      <path d="M17 4.75h1.25a1 1 0 0 1 1 1V7" />
      <path d="M7 19.25H5.75a1 1 0 0 1-1-1V17" />
      <path d="M17 19.25h1.25a1 1 0 0 0 1-1V17" />
      <rect x="7.25" y="7.25" width="4" height="4" rx="0.6" />
      <rect x="12.75" y="7.25" width="4" height="4" rx="0.6" />
      <rect x="7.25" y="12.75" width="4" height="4" rx="0.6" />
      <path d="M13.25 13.25h1.5v1.5h1.5v2.5h-3z" />
    </svg>
  );
}

function HistoryIcon({ filled: _filled = false }: { filled?: boolean }) {
  return (
    <svg
      viewBox="0 0 24 24"
      aria-hidden="true"
      className="size-[22px]"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M7.25 6.75 4.75 9.25 7.25 11.75" />
      <path d="M5.5 9.25h6.25a6 6 0 1 1-1.2 11.88" />
      <path d="M12.25 9.75v3.7l2.4 1.4" />
    </svg>
  );
}
