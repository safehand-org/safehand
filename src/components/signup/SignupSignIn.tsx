import { useState } from "react";
import { PrimaryButton } from "@/components/create/WizardLayout";
import { FieldLabel } from "@/components/join/JoinLayout";

const PASTE_WARNING = "Only paste your key into apps you trust. A browser extension is safer.";

/** Sign in with a Nostr extension, a pasted nsec, or start a new account. */
export function SignupSignIn({
  replacesSavedLogin,
  onSignIn,
  onConnectExtension,
  onCreateAccount,
}: {
  /** This browser already has a login, which signing in replaces. */
  replacesSavedLogin: boolean;
  /** Resolves to an error message, or null once signed in. */
  onSignIn: (backupKey: string) => Promise<string | null>;
  /** Resolves to an error message, or null once signed in. */
  onConnectExtension: () => Promise<string | null>;
  onCreateAccount: () => void;
}) {
  const [pasting, setPasting] = useState(false);
  const [backupKey, setBackupKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function attempt(action: () => Promise<string | null>) {
    setBusy(true);
    setError(null);
    setError(await action());
    setBusy(false);
  }

  return (
    <div className="mt-4 flex flex-1 flex-col">
      <OptionButton disabled={busy} onClick={() => attempt(onConnectExtension)}>
        Use a Nostr extension
      </OptionButton>
      <OptionButton
        disabled={busy}
        selected={pasting}
        onClick={() => {
          setError(null);
          setPasting((open) => !open);
        }}
      >
        Paste your nsec
      </OptionButton>

      {pasting ? (
        <div className="mt-4">
          <label className="block">
            <FieldLabel>Your nsec</FieldLabel>
            <input
              type="password"
              autoComplete="off"
              spellCheck={false}
              placeholder="nsec1…"
              value={backupKey}
              onChange={(event) => setBackupKey(event.target.value)}
              className="mt-2.5 h-12 w-full rounded-full border border-hairline bg-transparent px-5 font-mono text-[14px] text-sheet outline-none placeholder:text-muted focus-visible:ring-2 focus-visible:ring-sheet/20"
            />
          </label>
          <p className="mt-3 text-[12.5px] leading-[1.4] text-[#E2A33B]">{PASTE_WARNING}</p>
          {replacesSavedLogin ? (
            <p className="mt-2 text-[12.5px] leading-[1.4] text-muted">
              This replaces the login saved in this browser. Make sure you have its backup key
              first.
            </p>
          ) : null}
          <div className="mt-5 flex justify-center">
            <PrimaryButton
              disabled={busy || backupKey.trim().length === 0}
              onClick={() => attempt(() => onSignIn(backupKey))}
            >
              {busy ? "Signing In…" : "Sign In"}
            </PrimaryButton>
          </div>
        </div>
      ) : null}

      <OptionButton disabled={busy} onClick={onCreateAccount}>
        Create a new account
      </OptionButton>
      {error ? <p className="mt-3 text-[12.5px] leading-[1.4] text-[#E23B3B]">{error}</p> : null}
    </div>
  );
}

function OptionButton({
  children,
  onClick,
  disabled,
  selected = false,
}: {
  children: string;
  onClick: () => void;
  disabled?: boolean;
  selected?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-expanded={selected || undefined}
      className="mt-3 inline-flex h-12 w-full items-center justify-between rounded-[1.15rem] bg-[#1a1816] px-5 text-left text-[15px] font-medium text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/30 disabled:opacity-50"
    >
      <span>{children}</span>
      <span aria-hidden="true" className="text-muted">
        ›
      </span>
    </button>
  );
}
