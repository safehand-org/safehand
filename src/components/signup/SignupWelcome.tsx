import { DashedNote } from "@/components/join/JoinLayout";
import { AuthModeToggle } from "./AuthModeToggle";
import { SignupHeader } from "./SignupHeader";
import type { AuthMode } from "./types";

export function SignupWelcome({
  authMode,
  onAuthModeChange,
  onCreateLogin,
}: {
  authMode: AuthMode;
  onAuthModeChange: (mode: AuthMode) => void;
  onCreateLogin: () => void;
}) {
  return (
    <div className="flex min-h-full flex-col">
      <SignupHeader
        subtitle="Agree on your prize split before you win."
        stickmanSrc="/dark-stickman/standing-dark.svg"
        stickmanWidth={73}
        stickmanHeight={177}
        stickmanClassName="h-[9.5rem] w-auto"
      />

      <div className="flex flex-1 flex-col px-5 pt-4 pb-8">
        <AuthModeToggle
          mode={authMode}
          variant="welcome"
          onChange={onAuthModeChange}
        />

        {authMode === "create" ? (
          <CreateAccountMethods onCreateLogin={onCreateLogin} />
        ) : (
          <SignInStub />
        )}
      </div>
    </div>
  );
}

function CreateAccountMethods({ onCreateLogin }: { onCreateLogin: () => void }) {
  return (
    <>
      <div className="mt-4 rounded-[1.35rem] border border-dashed border-hairline px-4 py-4">
        <p className="text-[15px] font-medium text-sheet">
          New here? We&apos;ll get you set up.
        </p>
        <p className="mt-2 text-[12.5px] leading-[1.4] text-muted">
          We&apos;ll create your login in a few seconds. You&apos;ll save a
          backup so you can always get back in.
        </p>
      </div>

      <button
        type="button"
        onClick={onCreateLogin}
        className="mt-4 inline-flex h-12 w-full items-center justify-center rounded-full bg-white text-[16px] font-medium text-ink transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/70 active:bg-neutral-200"
      >
        Create My Login
      </button>

      <div className="mt-6 flex w-full items-center gap-3">
        <span aria-hidden="true" className="h-px flex-1 bg-[#2a2a2a]" />
        <p className="text-[10px] font-medium tracking-[0.16em] text-[#6e757b] uppercase">
          Or
        </p>
        <span aria-hidden="true" className="h-px flex-1 bg-[#2a2a2a]" />
      </div>

      {/* TODO: wire Nostr signup (nip-07 / nsec) to the auth backend */}
      <MethodButton onClick={() => {}}>Sign Up With Nostr</MethodButton>
      {/* TODO: wire Bitcoin wallet signup (lnurl / nwc / on-chain) to the auth backend */}
      <MethodButton onClick={() => {}}>Sign Up With A Bitcoin Wallet</MethodButton>
    </>
  );
}

function SignInStub() {
  return (
    <div className="mt-4">
      {/* TODO: wire returning-user sign-in (session, Nostr, or wallet) */}
      <DashedNote>
        Sign in isn&apos;t connected yet. Use Create Account to continue this
        mock flow.
      </DashedNote>
    </div>
  );
}

function MethodButton({
  children,
  onClick,
}: {
  children: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="mt-3 inline-flex h-12 w-full items-center justify-between rounded-[1.15rem] bg-[#1a1816] px-5 text-left text-[15px] font-medium text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/30"
    >
      <span>{children}</span>
      <ChevronRight />
    </button>
  );
}

function ChevronRight() {
  return (
    <svg
      viewBox="0 0 16 16"
      aria-hidden="true"
      className="size-4 text-muted"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M6 3.5 11 8l-5 4.5" />
    </svg>
  );
}
