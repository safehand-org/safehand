import Image from "next/image";

export function SummaryCard() {
  return (
    <section className="mt-5 rounded-[1.35rem] bg-[var(--summary)] px-5 pt-4 pb-4">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 pt-1">
          <p className="text-[10px] font-medium tracking-[0.16em] text-[var(--summary-muted)] uppercase">
            Total received
          </p>
          <p className="mt-2.5 flex items-baseline gap-2">
            <span className="text-[2.25rem] leading-none font-bold tracking-[-0.045em] text-ink">
              410,000
            </span>
            <span className="text-[15px] font-medium text-[var(--summary-muted)]">
              sats
            </span>
          </p>
        </div>
        <Image
          src="/dark-stickman/talking-dark.svg"
          alt=""
          width={81}
          height={178}
          className="pointer-events-none mt-1 h-[5.65rem] w-auto shrink-0"
        />
      </div>
      <div className="mt-4 flex items-center">
        <p className="flex-1 text-[10px] font-medium tracking-[0.14em] text-[var(--summary-muted)] uppercase">
          2 Split Paid
        </p>
        <span aria-hidden="true" className="mx-3 h-3.5 w-px bg-[#d3dadf]" />
        <p className="flex-1 text-right text-[10px] font-medium tracking-[0.14em] text-[var(--summary-muted)] uppercase">
          1 Cancelled
        </p>
      </div>
    </section>
  );
}
