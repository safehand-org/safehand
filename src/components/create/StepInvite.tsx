"use client";

import { useEffect, useState } from "react";
import { ShareIcon } from "./icons";
import { QrCode } from "./qr";
import { invitePath, type WizardState } from "./types";

export function StepInvite({ state }: { state: WizardState }) {
  const path = invitePath(state.teamName);
  const [value, setValue] = useState(path);

  useEffect(() => {
    setValue(`${window.location.origin}${path}`);
  }, [path]);

  const waiting = state.teammates.length;

  async function share() {
    const payload = { title: state.teamName, text: "Join this split card.", url: value };
    if (navigator.share) {
      try {
        await navigator.share(payload);
        return;
      } catch {
        // Fall through to clipboard when the share sheet is dismissed or unavailable.
      }
    }
    await navigator.clipboard.writeText(value);
  }

  return (
    <div>
      <div className="mx-auto mt-4 flex aspect-square w-[min(100%,17.25rem)] items-center justify-center">
        <QrCode value={value} />
      </div>

      <p className="mt-6 text-center text-[13px] text-sheet-muted">
        Waiting for {waiting} teammate{waiting === 1 ? "" : "s"} to sign.
      </p>

      <div className="mt-5 flex items-center gap-2">
        <p className="min-w-0 flex-1 truncate text-[14px] text-[#6f6f6f]">
          {path}
        </p>
        <button
          type="button"
          onClick={() => void share()}
          className="inline-flex h-10 shrink-0 items-center gap-1.5 rounded-full bg-ink px-4 text-[13px] font-medium text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/30"
        >
          Share
          <ShareIcon />
        </button>
      </div>
    </div>
  );
}
