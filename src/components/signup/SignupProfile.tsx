import { PrimaryButton } from "@/components/create/WizardLayout";
import { FieldLabel } from "@/components/join/JoinLayout";
import { AuthModeToggle } from "./AuthModeToggle";
import { SignupHeader } from "./SignupHeader";
import type { AuthMode } from "./types";

export function SignupProfile({
  authMode,
  displayName,
  bio,
  onAuthModeChange,
  onDisplayNameChange,
  onBioChange,
  onSignIn,
  onSubmit,
}: {
  authMode: AuthMode;
  displayName: string;
  bio: string;
  onAuthModeChange: (mode: AuthMode) => void;
  onDisplayNameChange: (value: string) => void;
  onBioChange: (value: string) => void;
  onSignIn: () => void;
  onSubmit: () => void;
}) {
  const canSubmit = displayName.trim().length > 0;

  return (
    <div className="flex min-h-full flex-col">
      <SignupHeader
        subtitle="Create an account to continue."
        stickmanSrc="/dark-stickman/sitdown-dark.svg"
        stickmanWidth={87}
        stickmanHeight={126}
        stickmanClassName="h-[8.35rem] w-auto"
      >
        <AuthModeToggle
          mode={authMode}
          variant="profile"
          onChange={onAuthModeChange}
        />
      </SignupHeader>

      <div className="flex flex-1 flex-col px-5 pt-4 pb-8">
        {authMode === "create" ? (
          <>
            <p className="rounded-[1.35rem] border border-dashed border-hairline px-4 py-3 text-center text-[12.5px] leading-[1.4] text-muted">
              Already have an account?{" "}
              <button
                type="button"
                onClick={onSignIn}
                className="text-[#34b6ee] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#34b6ee]/40"
              >
                Sign in
              </button>
            </p>

            <label className="mt-6 block">
              <FieldLabel>Display name</FieldLabel>
              <input
                type="text"
                value={displayName}
                onChange={(event) => onDisplayNameChange(event.target.value)}
                className="mt-2.5 h-12 w-full rounded-full border border-hairline bg-transparent px-5 text-[15px] text-sheet outline-none focus-visible:ring-2 focus-visible:ring-sheet/20"
              />
            </label>

            <label className="mt-5 block">
              <FieldLabel>Bio (optional)</FieldLabel>
              <input
                type="text"
                value={bio}
                onChange={(event) => onBioChange(event.target.value)}
                className="mt-2.5 h-12 w-full rounded-full border border-hairline bg-transparent px-5 text-[15px] text-sheet outline-none focus-visible:ring-2 focus-visible:ring-sheet/20"
              />
            </label>
          </>
        ) : (
          <SignInStub onCreate={() => onAuthModeChange("create")} />
        )}

        <div className="mt-auto flex items-center justify-center pt-10">
          <PrimaryButton
            disabled={authMode !== "create" || !canSubmit}
            onClick={onSubmit}
          >
            Sign Up
          </PrimaryButton>
        </div>
      </div>
    </div>
  );
}

function SignInStub({ onCreate }: { onCreate: () => void }) {
  return (
    <p className="rounded-[1.35rem] border border-dashed border-hairline px-4 py-3 text-center text-[12.5px] leading-[1.4] text-muted">
      {/* TODO: wire returning-user sign-in (session, Nostr, or wallet) */}
      Sign in isn&apos;t connected yet.{" "}
      <button
        type="button"
        onClick={onCreate}
        className="text-[#34b6ee] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#34b6ee]/40"
      >
        Create an account
      </button>{" "}
      to continue this mock flow.
    </p>
  );
}
