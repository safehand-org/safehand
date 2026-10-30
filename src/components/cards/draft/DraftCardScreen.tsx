"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import { CardsHeader } from "@/components/cards/CardsHeader";
import { getCard, getMe, type CardView, type Me, type TeamView } from "@/lib/api";
import { signAsMember } from "@/lib/cardActions";
import { memberName, shortNpub } from "@/lib/cardDetail";
import { errorMessage } from "@/lib/errors";
import { refreshPending } from "@/lib/pendingCards";
import { formatBasisPoints } from "@/lib/shares";
import { currentPubkey, hasSigner } from "@/lib/signer";
import { loadWasm } from "@/lib/wasm";
import { DeclineButton } from "./DeclineButton";

/** "This card was sent to npub1…, but you're signed in as npub1…". */
export function wrongKeyMessage(sentTo: string, signedInAs: string): string {
  return `This card was sent to ${sentTo}, but you're signed in as ${signedInAs}.`;
}

/** A draft card: who is on it, who has signed, and signing for the viewer. */
export function DraftCardScreen({ team, card: initial }: { team: TeamView; card: CardView }) {
  const router = useRouter();
  const [card, setCard] = useState(initial);
  const [viewer, setViewer] = useState<{ pubkey: string; npub: string } | null>(null);
  const [me, setMe] = useState<Me | null>(null);
  const [sentTo, setSentTo] = useState<string | null>(null);
  const [address, setAddress] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    // The link may name who it was sent to; only the browser sees the query.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setSentTo(new URLSearchParams(window.location.search).get("for"));
    if (!hasSigner()) return;
    void (async () => {
      const pubkey = await currentPubkey();
      if (!pubkey || !current) return;
      const wasm = await loadWasm();
      setViewer({ pubkey, npub: wasm.npub_encode(pubkey) });
      const account = await getMe().catch(() => null);
      if (!current || !account) return;
      setMe(account);
      setAddress((typed) => typed || account.profile?.lightning_address || "");
    })();
    return () => {
      current = false;
    };
  }, []);

  const signedCount = card.members.filter((member) => member.signed).length;
  const mine = viewer ? card.members.find((member) => member.pubkey === viewer.pubkey) : undefined;
  const open = card.status === "draft";
  const canSign = open && mine !== undefined && !mine.signed && !mine.declined;
  const shareLink = (npub?: string) =>
    `${window.location.origin}/card/${encodeURIComponent(team.slug)}${npub ? `?for=${npub}` : ""}`;

  async function copy(text: string, label: string) {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(label);
    } catch {
      setCopied(null);
      window.prompt("Copy this link", text);
    }
  }

  async function sign() {
    if (!me?.profile) return;
    setBusy(true);
    setError(null);
    try {
      await signAsMember(card.hash, me.profile, address);
      const updated = await getCard(card.hash);
      setCard(updated);
      void refreshPending().catch(() => null);
      if (updated.status === "active") router.refresh();
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setBusy(false);
    }
  }

  const waitingNpubs = card.members
    .filter((member) => !member.signed)
    .map((member) => shortNpub(member.npub));

  return (
    <div className="absolute inset-0 overflow-y-auto">
      <section className="rounded-b-sheet bg-sheet px-5 pt-5 pb-6 text-ink">
        <CardsHeader label="Draft split card" backHref="/cards" compact />
        <h1 className="mt-5 text-[2.35rem] leading-[0.95] font-semibold tracking-[-0.035em]">
          {team.name}
        </h1>
        <p className="mt-2 text-[14.5px] text-sheet-muted">{team.event_label}</p>
        <p className="mt-4 text-[15px] font-medium" data-testid="signed-count">
          {signedCount} of {card.members.length} signed
        </p>

        <ul className="mt-4 flex flex-col gap-2">
          {card.members.map((member) => (
            <li
              key={member.pubkey}
              className="flex items-center gap-3 rounded-[1.1rem] bg-[#f3f3f3] px-4 py-3"
            >
              <div className="min-w-0 flex-1">
                <p className="text-[15px] font-medium">
                  {memberName(member)}
                  {member.pubkey === viewer?.pubkey ? " (You)" : ""}
                </p>
                <p className="mt-0.5 truncate font-mono text-[11px] text-[#8a8a8a]">
                  {shortNpub(member.npub)}
                </p>
              </div>
              <span className="text-[14px] font-medium">
                {formatBasisPoints(member.share_basis_points)}
              </span>
              <span
                className={`rounded-full px-2.5 py-1 text-[10px] font-semibold tracking-[0.1em] uppercase ${
                  member.signed
                    ? "bg-ink text-white"
                    : member.declined
                      ? "bg-[#C62828] text-white"
                      : "bg-white text-[#6f6f6f]"
                }`}
              >
                {member.signed ? "Signed" : member.declined ? "Declined" : "Waiting"}
              </span>
              {open && !member.signed && !member.declined && member.pubkey !== viewer?.pubkey ? (
                <button
                  type="button"
                  aria-label={`Copy ${memberName(member)}'s link`}
                  onClick={() => void copy(shareLink(member.npub), member.pubkey)}
                  className="text-[11px] font-medium text-[#34b6ee]"
                >
                  {copied === member.pubkey ? "Copied" : "Link"}
                </button>
              ) : null}
            </li>
          ))}
        </ul>

        {open ? (
          <button
            type="button"
            onClick={() => void copy(shareLink(), "all")}
            className="mt-4 inline-flex h-11 w-full items-center justify-center rounded-full border border-[#e1e1e1] bg-white text-[15px] font-medium"
          >
            {copied === "all" ? "Link copied" : "Share link"}
          </button>
        ) : null}
      </section>

      <div className="flex flex-col gap-4 px-5 pt-5 pb-8">
        <ClosedNote card={card} />

        {open && viewer && !mine ? (
          <p className="rounded-[1.35rem] border border-dashed border-hairline px-5 py-3 text-[13px] leading-[1.4] text-muted">
            {wrongKeyMessage(sentTo ?? waitingNpubs.join(" or "), shortNpub(viewer.npub))}
          </p>
        ) : null}

        {open && !viewer ? (
          <p className="rounded-[1.35rem] border border-dashed border-hairline px-5 py-3 text-center text-[13px] leading-[1.4] text-muted">
            Are you on this card?{" "}
            <Link href="/signup" className="text-[#34b6ee]">
              Sign in to sign it
            </Link>
          </p>
        ) : null}

        {canSign && me && !me.profile ? (
          <p className="rounded-[1.35rem] border border-dashed border-hairline px-5 py-3 text-center text-[13px] leading-[1.4] text-muted">
            <Link href="/signup" className="text-[#34b6ee]">
              Set up your profile
            </Link>{" "}
            to sign this card.
          </p>
        ) : null}

        {canSign && me?.profile ? (
          <div>
            <label className="block">
              <span className="text-[10px] font-medium tracking-[0.16em] text-muted uppercase">
                Your Lightning address
              </span>
              <input
                type="text"
                inputMode="email"
                autoComplete="off"
                spellCheck={false}
                placeholder="you@wallet.com"
                value={address}
                onChange={(event) => setAddress(event.target.value)}
                className="mt-2.5 h-12 w-full rounded-full border border-hairline bg-transparent px-5 text-[15px] text-sheet outline-none focus-visible:ring-2 focus-visible:ring-sheet/20"
              />
              <span className="mt-2 block text-[12.5px] text-muted">
                This is where your share will be paid
              </span>
            </label>
            {error ? (
              <p role="alert" className="mt-3 text-[13px] text-[#E23B3B]">
                {error}
              </p>
            ) : null}
            <div className="mt-5 flex items-center justify-center gap-3">
              <DeclineButton
                hash={card.hash}
                onDeclined={(updated) => {
                  setCard(updated);
                  void refreshPending().catch(() => null);
                }}
              />
              <button
                type="button"
                disabled={busy || !address.trim()}
                onClick={() => void sign()}
                className="inline-flex h-12 items-center justify-center rounded-full bg-white px-7 text-[16px] font-medium text-ink disabled:opacity-50"
              >
                {busy ? "Signing…" : "Sign with my Nostr key"}
              </button>
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}

function ClosedNote({ card }: { card: CardView }) {
  if (card.status !== "declined" && card.status !== "stale") return null;
  const names = card.members.filter((member) => member.declined).map(memberName);
  const text =
    card.status === "declined"
      ? `${names.join(", ") || "Someone"} declined this card, so it can't be locked.`
      : "This card was replaced or can no longer be signed.";
  return (
    <div className="rounded-[1.35rem] border border-dashed border-hairline px-5 py-4 text-center">
      <p className="text-[13px] leading-[1.4] text-muted">{text}</p>
      <Link
        href="/create"
        className="mt-3 inline-flex h-11 items-center justify-center rounded-full bg-white px-6 text-[15px] font-medium text-ink"
      >
        Start a new card
      </Link>
    </div>
  );
}
