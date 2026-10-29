import Image from "next/image";
import type { ReactNode } from "react";
import { Stickman } from "./Stickman";

export function SignupHeader({
  subtitle,
  stickmanSrc,
  stickmanWidth,
  stickmanHeight,
  stickmanClassName,
  children,
}: {
  subtitle: string;
  stickmanSrc: string;
  stickmanWidth: number;
  stickmanHeight: number;
  stickmanClassName?: string;
  children?: ReactNode;
}) {
  return (
    <section className="relative overflow-hidden rounded-b-sheet bg-sheet px-5 pt-5 pb-6 text-ink">
      <Image
        src="/logo.svg"
        alt="Safehand"
        width={36}
        height={24}
        preload
        className="h-6 w-auto"
      />

      <div className="relative mt-8 min-h-[8.75rem]">
        <div className="max-w-[14.5rem]">
          <h1 className="text-[2.15rem] leading-[0.95] font-semibold tracking-[-0.03em]">
            Welcome To
            <br />
            Safehand
          </h1>
          <p className="mt-3 max-w-[19rem] text-[15px] leading-5 text-sheet-muted">
            {subtitle}
          </p>
        </div>

        <div className="pointer-events-none absolute top-[-0.35rem] right-0">
          <Stickman
            src={stickmanSrc}
            width={stickmanWidth}
            height={stickmanHeight}
            className={stickmanClassName}
          />
        </div>
      </div>

      {children}
    </section>
  );
}
