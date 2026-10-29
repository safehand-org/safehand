"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";
import { parseInviteCode } from "@/components/join/types";
import { ScanHeader } from "./ScanHeader";

export function ScanView() {
  const router = useRouter();
  const [invite, setInvite] = useState("");

  function openCard() {
    router.push(`/invite/card/${parseInviteCode(invite)}`);
  }

  return (
    <div className="flex min-h-full flex-col px-5 pt-8 pb-8 text-white">
      <ScanHeader />

      <section className="mt-8">
        <h1 className="text-[2.15rem] leading-none font-semibold tracking-[-0.03em]">
          Scan QR Code
        </h1>
        <p className="mt-3 text-[15px] leading-5 text-[#8b939a]">
          Point your camera at the code your teammate shared.
        </p>
      </section>

      <div className="mt-8 flex aspect-square w-full items-center justify-center rounded-[1.75rem] bg-[#1a1a1a]">
        <CameraIcon />
      </div>

      <div className="mt-8 flex items-center gap-3">
        <span aria-hidden="true" className="h-px flex-1 bg-[#2a2a2a]" />
        <p className="text-[10px] font-medium tracking-[0.16em] text-[#6e757b] uppercase">
          Or paste the invite link.
        </p>
        <span aria-hidden="true" className="h-px flex-1 bg-[#2a2a2a]" />
      </div>

      <form
        className="mt-6 block"
        onSubmit={(event) => {
          event.preventDefault();
          openCard();
        }}
      >
        <label className="block">
          <span className="text-[10px] font-medium tracking-[0.16em] text-[#6e757b] uppercase">
            Invite link or code
          </span>
          <input
            type="text"
            value={invite}
            onChange={(event) => setInvite(event.target.value)}
            placeholder="Paste the link or type the code."
            className="mt-2.5 h-12 w-full rounded-full bg-[#1a1a1a] px-5 text-[14px] text-white outline-none placeholder:text-[#5c6368] focus-visible:ring-2 focus-visible:ring-white/25"
          />
        </label>
      </form>

      <div className="mt-auto flex justify-center pt-10">
        <button
          type="button"
          onClick={openCard}
          className="h-12 rounded-full bg-white px-10 text-[16px] font-medium text-ink transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/70 active:bg-neutral-200"
        >
          Open card
        </button>
      </div>
    </div>
  );
}

function CameraIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      aria-hidden="true"
      className="size-16 text-white"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.4"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M4.5 8.25h3.05l1.1-1.85h6.7l1.1 1.85H19.5A1.5 1.5 0 0 1 21 9.75v8A1.5 1.5 0 0 1 19.5 19.25h-15A1.5 1.5 0 0 1 3 17.75v-8A1.5 1.5 0 0 1 4.5 8.25Z" />
      <circle cx="12" cy="13.5" r="3.15" />
    </svg>
  );
}
