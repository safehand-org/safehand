import { useState } from "react";
import { PrimaryButton } from "@/components/create/WizardLayout";
import { FieldLabel } from "@/components/join/JoinLayout";

/** Sign in by pasting the backup key saved at signup. */
export function SignupSignIn({
  replacesSavedLogin,
  onSignIn,
}: {
  /** This browser already has a login, which signing in replaces. */
  replacesSavedLogin: boolean;
  /** Resolves to an error message, or null once signed in. */
  onSignIn: (backupKey: string) => Promise<string | null>;
}) {
  const [backupKey, setBackupKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit() {
    setBusy(true);
    setError(await onSignIn(backupKey));
    setBusy(false);
  }

  return (
    <div className="mt-4 flex flex-1 flex-col">
      <label className="block">
        <FieldLabel>Backup key</FieldLabel>
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
      {replacesSavedLogin ? (
        <p className="mt-3 text-[12.5px] leading-[1.4] text-muted">
          This replaces the login saved in this browser. Make sure you have its backup key first.
        </p>
      ) : null}
      {error ? <p className="mt-3 text-[12.5px] leading-[1.4] text-[#E23B3B]">{error}</p> : null}

      <div className="mt-auto flex items-center justify-center pt-10">
        <PrimaryButton disabled={busy || backupKey.trim().length === 0} onClick={submit}>
          {busy ? "Signing In…" : "Sign In"}
        </PrimaryButton>
      </div>
    </div>
  );
}
