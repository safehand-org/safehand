import { useState } from "react";
import { PrimaryButton, SecondaryButton } from "@/components/create/WizardLayout";
import { DashedNote, FieldLabel } from "@/components/join/JoinLayout";
import { SignupHeader } from "./SignupHeader";

/** Shows the new login's backup key, and waits until it has been saved. */
export function SignupBackup({
  backupKey,
  onContinue,
}: {
  /** The `nsec`, or null while it loads. */
  backupKey: string | null;
  onContinue: () => void;
}) {
  const [saved, setSaved] = useState(false);
  const [copied, setCopied] = useState(false);

  async function copy() {
    if (!backupKey) return;
    try {
      await navigator.clipboard.writeText(backupKey);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  }

  return (
    <div className="flex min-h-full flex-col">
      <SignupHeader
        subtitle="Save your backup key before you continue."
        stickmanSrc="/dark-stickman/standing-dark.svg"
        stickmanWidth={73}
        stickmanHeight={177}
        stickmanClassName="h-[9.5rem] w-auto"
      />

      <div className="flex flex-1 flex-col px-5 pt-4 pb-8">
        <DashedNote>
          Your login lives only in this browser. This key is the only way back in on another
          device, or if this browser&apos;s data is cleared. Keep it private: anyone with it can
          act as you.
        </DashedNote>

        <div className="mt-6">
          <FieldLabel>Backup key</FieldLabel>
          <p className="mt-2.5 rounded-[1.15rem] border border-hairline px-4 py-3 font-mono text-[13px] leading-[1.45] break-all text-sheet select-all">
            {backupKey ?? "Creating your login…"}
          </p>
        </div>

        <SecondaryButton className="mt-4 w-full" disabled={!backupKey} onClick={copy}>
          {copied ? "Copied" : "Copy Backup Key"}
        </SecondaryButton>

        <label className="mt-6 flex items-start gap-3 text-[13px] leading-[1.4] text-sheet">
          <input
            type="checkbox"
            checked={saved}
            onChange={(event) => setSaved(event.target.checked)}
            className="mt-0.5 size-4 accent-white"
          />
          I&apos;ve saved my backup key somewhere safe, like a password manager.
        </label>

        <div className="mt-auto flex items-center justify-center pt-10">
          <PrimaryButton disabled={!backupKey || !saved} onClick={onContinue}>
            Continue
          </PrimaryButton>
        </div>
      </div>
    </div>
  );
}
