import Link from "next/link";
import type { SignatureRequest } from "@/lib/account";

export function SignatureCard({ request }: { request: SignatureRequest }) {
  return (
    <section className="mt-5">
      <p className="text-[11px] font-semibold tracking-[0.14em] text-[var(--accent)] uppercase">
        Needs your signature
      </p>
      <article className="mt-2.5 flex items-center gap-3 rounded-[1.6rem] bg-[#141416] px-4 py-4">
        <div className="min-w-0 flex-1">
          <h2 className="text-[16px] font-semibold tracking-[-0.015em] text-white">
            {request.title}
          </h2>
          <p className="mt-0.5 text-[12px] leading-4 text-[#8b939a]">
            {request.body}
          </p>
        </div>
        <Link
          href={request.href}
          prefetch={false}
          className="inline-flex h-9 shrink-0 items-center rounded-full bg-white px-3.5 text-[10px] font-semibold tracking-[0.06em] text-ink uppercase transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/70 active:bg-neutral-200"
        >
          Sign now
        </Link>
      </article>
    </section>
  );
}
