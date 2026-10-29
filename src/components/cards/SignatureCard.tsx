export function SignatureCard() {
  return (
    <section className="mt-5">
      <p className="text-[11px] font-semibold tracking-[0.14em] text-[var(--accent)] uppercase">
        Needs your signature
      </p>
      <article className="mt-2.5 flex items-center gap-3 rounded-[1.6rem] bg-[#141416] px-4 py-4">
        <div className="min-w-0 flex-1">
          <h2 className="text-[16px] font-semibold tracking-[-0.015em] text-white">
            Bikini Bottom
          </h2>
          <p className="mt-0.5 text-[12px] leading-4 text-[#8b939a]">
            Goodness invited you. Your share is 45%.
          </p>
        </div>
        <button
          type="button"
          className="h-9 shrink-0 rounded-full bg-white px-3.5 text-[10px] font-semibold tracking-[0.06em] text-ink uppercase transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/70 active:bg-neutral-200"
        >
          Sign now
        </button>
      </article>
    </section>
  );
}
