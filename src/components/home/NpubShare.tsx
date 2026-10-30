"use client";

import { useEffect, useRef, useState } from "react";
import { CopyIcon } from "./icons";

const COPIED_MS = 2000;

/** `npub1` plus the next 4 characters, an ellipsis, then the last 4. */
export function shortNpub(npub: string): string {
  if (npub.length < 13) return npub;
  return `${npub.slice(0, 9)}…${npub.slice(-4)}`;
}

function selectText(node: HTMLElement) {
  const selection = window.getSelection();
  if (!selection) return;
  const range = document.createRange();
  range.selectNodeContents(node);
  selection.removeAllRanges();
  selection.addRange(range);
}

/** Short npub under the home greeting, with copy-to-clipboard. */
export function NpubShare({ npub }: { npub: string }) {
  const fullRef = useRef<HTMLSpanElement>(null);
  const copiedTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [copied, setCopied] = useState(false);
  const [fallback, setFallback] = useState(false);

  useEffect(() => {
    return () => {
      if (copiedTimer.current) clearTimeout(copiedTimer.current);
    };
  }, []);

  useEffect(() => {
    if (fallback && fullRef.current) selectText(fullRef.current);
  }, [fallback]);

  async function copy() {
    try {
      await navigator.clipboard.writeText(npub);
      setFallback(false);
      setCopied(true);
      if (copiedTimer.current) clearTimeout(copiedTimer.current);
      copiedTimer.current = setTimeout(() => setCopied(false), COPIED_MS);
    } catch {
      setCopied(false);
      setFallback(true);
    }
  }

  const short = shortNpub(npub);

  return (
    <div className="mt-2 min-w-0">
      <div className="flex items-center gap-1.5">
        {fallback ? (
          <span
            ref={fullRef}
            className="font-mono text-[13px] leading-5 break-all tabular-nums text-ink select-all"
          >
            {npub}
          </span>
        ) : (
          <button
            type="button"
            onClick={() => void copy()}
            className="min-w-0 shrink-0 font-mono text-[13px] leading-5 whitespace-nowrap tabular-nums text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/30"
          >
            <span className="sr-only">{npub}</span>
            <span aria-hidden="true">{short}</span>
          </button>
        )}
        {copied ? (
          <span className="shrink-0 text-[12px] leading-none text-sheet-muted" aria-live="polite">
            Copied
          </span>
        ) : (
          <button
            type="button"
            onClick={() => void copy()}
            aria-label="Copy your npub"
            className="flex size-7 shrink-0 items-center justify-center text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/30"
          >
            <CopyIcon />
          </button>
        )}
      </div>
      <p className="mt-1 text-[11px] leading-4 text-sheet-muted">
        Share your npub so teammates can add you to a card.
      </p>
    </div>
  );
}
