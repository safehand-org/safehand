import Image from "next/image";
import type { HomeGreeting as HomeGreetingData } from "./types";

export function HomeGreeting({
  greeting,
  compact,
}: {
  greeting: HomeGreetingData;
  compact: boolean;
}) {
  return (
    <div className={compact ? "relative mt-2" : "relative mt-3 min-h-[168px]"}>
      <div className={compact ? "max-w-[15.75rem]" : "max-w-[13.5rem] pt-8"}>
        <h1 className="text-[2.125rem] font-semibold leading-[1.12] tracking-[-0.03em]">
          {greeting.title}
          <br />
          {greeting.name}
        </h1>
        <p className="mt-3 text-[13.5px] leading-5 text-sheet-muted">
          {greeting.subtitle}
        </p>
      </div>

      {compact ? null : (
        <Image
          src="/dark-stickman/handsup-dark.svg"
          alt=""
          width={108}
          height={173}
          className="pointer-events-none absolute top-0 right-0 h-[168px] w-auto"
        />
      )}
    </div>
  );
}
