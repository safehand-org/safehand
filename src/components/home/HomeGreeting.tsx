import Image from "next/image";
import { NpubShare } from "./NpubShare";
import type { HomeGreeting as HomeGreetingData } from "./types";

export function HomeGreeting({
  greeting,
  compact,
  npub,
}: {
  greeting: HomeGreetingData;
  compact: boolean;
  npub?: string | null;
}) {
  return (
    <div className={compact ? "relative mt-2" : "relative mt-3 min-h-[168px]"}>
      <div className={compact ? "min-w-0 max-w-[15.75rem]" : "min-w-0 max-w-[13.5rem] pt-8"}>
        <h1 className="text-[2.125rem] font-semibold leading-[1.12] tracking-[-0.03em]">
          {greeting.title}
          <br />
          <span className="block min-w-0 truncate">{greeting.name}</span>
        </h1>
      </div>
      {npub ? <NpubShare npub={npub} /> : null}
      <p
        className={
          compact
            ? "mt-3 max-w-[15.75rem] text-[13.5px] leading-5 text-sheet-muted"
            : "mt-3 max-w-[13.5rem] text-[13.5px] leading-5 text-sheet-muted"
        }
      >
        {greeting.subtitle}
      </p>

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
