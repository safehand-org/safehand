import { useState } from "react";
import { ApiError, getProfile } from "@/lib/api";
import { shortNpub } from "@/lib/cardDetail";
import { cardLabel, checkTeammateKey, MAX_LABEL_CHARS } from "@/lib/teammates";
import { loadWasm } from "@/lib/wasm";
import { CloseIcon, PersonIcon } from "./icons";
import { AVATAR_PALETTE, type Teammate, type WizardState } from "./types";

export const TEAMMATES_COPY =
  "Add everyone on your team by their npub. They'll be asked to sign the card in the app.";

const FIELD_CLASS =
  "h-12 min-w-0 flex-1 rounded-full border border-dashed border-[#c6c6c6] bg-white px-5 text-[15px] text-ink outline-none placeholder:text-[#b0b0b0] focus-visible:ring-2 focus-visible:ring-ink/15";
const ADD_CLASS =
  "flex size-12 shrink-0 items-center justify-center rounded-full bg-ink text-[11px] font-medium tracking-[0.01em] text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/30 disabled:opacity-50";

export function StepTeammates({
  state,
  ownPubkey,
  onAdd,
  onRemove,
}: {
  state: WizardState;
  ownPubkey: string | null;
  onAdd: (teammate: Teammate) => void;
  onRemove: (pubkey: string) => void;
}) {
  const [input, setInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  /** A key with no profile, waiting for a name. */
  const [unnamed, setUnnamed] = useState<{ pubkey: string; npub: string } | null>(null);
  const [name, setName] = useState("");

  function add(pubkey: string, npub: string, teammateName: string) {
    onAdd({
      pubkey,
      npub,
      name: cardLabel(teammateName),
      color: AVATAR_PALETTE[state.teammates.length % AVATAR_PALETTE.length],
    });
    setInput("");
    setUnnamed(null);
    setName("");
  }

  async function lookUp() {
    setError(null);
    setBusy(true);
    try {
      const wasm = await loadWasm();
      const existing = [
        ...(ownPubkey ? [{ pubkey: ownPubkey, name: "You" }] : []),
        ...state.teammates,
      ];
      const check = checkTeammateKey(wasm.parse_public_key, input, ownPubkey, existing);
      if (!check.ok) {
        setError(check.message);
        if (check.clear) setInput("");
        return;
      }
      const npub = wasm.npub_encode(check.pubkey);
      try {
        const profile = await getProfile(check.pubkey);
        add(check.pubkey, npub, profile.name);
      } catch (failure) {
        if (failure instanceof ApiError && failure.code === "profile_not_found") {
          setUnnamed({ pubkey: check.pubkey, npub });
          return;
        }
        throw failure;
      }
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : "Couldn't add them. Try again.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div>
      <div className="flex flex-col gap-2.5">
        <CreatorRow name={state.yourName.trim() || "You"} />
        {state.teammates.map((teammate) => (
          <TeammateRow
            key={teammate.pubkey}
            teammate={teammate}
            onRemove={() => onRemove(teammate.pubkey)}
          />
        ))}
      </div>

      {state.teammates.length === 0 ? (
        <p className="mt-5 text-[13px] leading-5 text-sheet-muted">
          Add at least one teammate to continue.
        </p>
      ) : null}

      {unnamed ? (
        <form
          className="mt-4"
          onSubmit={(event) => {
            event.preventDefault();
            if (name.trim()) add(unnamed.pubkey, unnamed.npub, name);
          }}
        >
          <p className="text-[13px] leading-5 text-sheet-muted">
            {shortNpub(unnamed.npub)} hasn&apos;t used Safehand yet. What should we call them?
          </p>
          <div className="mt-2 flex items-center gap-2">
            <input
              type="text"
              autoFocus
              value={name}
              maxLength={MAX_LABEL_CHARS}
              onChange={(event) => setName(event.target.value)}
              placeholder="Their name"
              className={FIELD_CLASS}
            />
            <button type="submit" disabled={!name.trim()} className={ADD_CLASS}>
              + Add
            </button>
          </div>
          <button
            type="button"
            onClick={() => setUnnamed(null)}
            className="mt-2 text-[12.5px] text-sheet-muted underline"
          >
            Cancel
          </button>
        </form>
      ) : (
        <form
          className="mt-4 flex items-center gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            if (input.trim()) void lookUp();
          }}
        >
          <input
            type="text"
            autoComplete="off"
            spellCheck={false}
            value={input}
            onChange={(event) => {
              setInput(event.target.value);
              setError(null);
            }}
            placeholder="Teammate's npub1…"
            aria-label="Teammate's npub"
            className={`${FIELD_CLASS} font-mono text-[13px]`}
          />
          <button type="submit" disabled={busy || !input.trim()} className={ADD_CLASS}>
            {busy ? "…" : "+ Add"}
          </button>
        </form>
      )}
      {error ? (
        <p role="alert" className="mt-3 text-[13px] leading-5 text-[#C62828]">
          {error}
        </p>
      ) : null}
    </div>
  );
}

function CreatorRow({ name }: { name: string }) {
  return (
    <div className="flex items-center gap-3 rounded-full bg-ink px-2 py-2 text-white">
      <span className="flex size-10 shrink-0 items-center justify-center rounded-full bg-[#2a2a2a]">
        <PersonIcon />
      </span>
      <div className="min-w-0">
        <p className="text-[15px] leading-none font-medium">{name}</p>
        <p className="mt-1 text-[12px] leading-none text-[#b8b8b8]">You</p>
      </div>
    </div>
  );
}

function TeammateRow({ teammate, onRemove }: { teammate: Teammate; onRemove: () => void }) {
  return (
    <div className="flex items-center gap-3 rounded-full bg-[#f3f3f3] px-2 py-2">
      <span
        className="flex size-10 shrink-0 items-center justify-center rounded-full text-[13px] font-semibold text-white"
        style={{ background: teammate.color }}
      >
        {teammate.name.slice(0, 1)}
      </span>
      <div className="min-w-0 flex-1">
        <p className="text-[15px] leading-none font-medium text-ink">{teammate.name}</p>
        <p className="mt-1 truncate font-mono text-[11px] leading-none text-[#8a8a8a]">
          {shortNpub(teammate.npub)}
        </p>
      </div>
      <button
        type="button"
        aria-label={`Remove ${teammate.name}`}
        onClick={onRemove}
        className="mr-2 flex size-8 items-center justify-center text-[#7a7a7a] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/20"
      >
        <CloseIcon />
      </button>
    </div>
  );
}
