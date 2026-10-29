import type { ReactNode } from "react";
import { BottomNav } from "./BottomNav";

export function PhoneShell({
  children,
  activeHref = "/",
  showNav = true,
}: {
  children: ReactNode;
  activeHref?: string;
  showNav?: boolean;
}) {
  return (
    <div className="flex h-svh justify-center overflow-hidden bg-background">
      <div className="flex h-full w-full max-w-[390px] flex-col overflow-hidden">
        <div className="relative min-h-0 flex-1 overflow-x-hidden overflow-y-auto overscroll-contain [overflow-anchor:none] [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
          {children}
        </div>
        {showNav ? <BottomNav activeHref={activeHref} /> : null}
      </div>
    </div>
  );
}
