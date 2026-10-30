"use client";

import { useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import { ApiError, getMe, putProfile } from "@/lib/api";
import {
  backupKey as backupKeyOf,
  createIdentity,
  currentIdentity,
  restoreIdentity,
} from "@/lib/identity";
import { connectExtension, hasSigner } from "@/lib/signer";
import { SignupBackup } from "./SignupBackup";
import { SignupOnboarding } from "./SignupOnboarding";
import { SignupProfile } from "./SignupProfile";
import { SignupWelcome } from "./SignupWelcome";
import {
  SIGNUP_STORAGE_KEY,
  createSignupState,
  type AuthMode,
  type OnboardingSlide,
  type SignupState,
} from "./types";

/** The login saved in this browser, or a new one if there is none. */
async function loginForSignup() {
  return (await currentIdentity()) ?? (await createIdentity());
}

function profileErrorMessage(error: unknown): string {
  if (error instanceof ApiError) {
    if (error.code === "bio_too_long") return "Your bio is too long. Keep it under 280 characters.";
    if (error.code === "empty_field") return "Enter a display name.";
    return error.message;
  }
  return "Something went wrong. Try again.";
}

export function SignupView() {
  const router = useRouter();
  const [state, setState] = useState<SignupState>(createSignupState);
  const [hydrated, setHydrated] = useState(false);
  const [backupKey, setBackupKey] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let restored = createSignupState();
    const stored = window.sessionStorage.getItem(SIGNUP_STORAGE_KEY);
    if (stored) {
      try {
        restored = { ...restored, ...(JSON.parse(stored) as Partial<SignupState>) };
        // Session storage is only readable in the browser, after hydration.
        // eslint-disable-next-line react-hooks/set-state-in-effect
        setState(restored);
      } catch {
        window.sessionStorage.removeItem(SIGNUP_STORAGE_KEY);
      }
    }
    setHydrated(true);
    // A reload on the backup step shows the same key again.
    if (restored.step === "backup") {
      void loginForSignup()
        .then(backupKeyOf)
        .then(setBackupKey)
        .catch(() => setState((current) => ({ ...current, step: "welcome" })));
    }
  }, []);

  useEffect(() => {
    if (!hydrated) return;
    // The wizard checkpoint only: the key itself is never stored here.
    window.sessionStorage.setItem(SIGNUP_STORAGE_KEY, JSON.stringify(state));
  }, [hydrated, state]);

  function patch(next: Partial<SignupState>) {
    setState((current) => ({ ...current, ...next }));
  }

  function goHome() {
    window.sessionStorage.removeItem(SIGNUP_STORAGE_KEY);
    router.push("/");
  }

  async function createLogin() {
    setError(null);
    patch({ step: "backup", authMode: "create" });
    try {
      setBackupKey(await backupKeyOf(await loginForSignup()));
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : "Couldn't create your login.");
      patch({ step: "welcome" });
    }
  }

  async function submitProfile() {
    setSubmitting(true);
    setError(null);
    try {
      const bio = state.bio.trim();
      await putProfile({ name: state.displayName.trim(), ...(bio ? { bio } : {}) });
      patch({ step: "onboarding", onboardingSlide: 1 });
    } catch (failure) {
      if (failure instanceof ApiError && failure.code === "auth_missing") {
        patch({ step: "welcome", authMode: "create" });
      }
      setError(profileErrorMessage(failure));
    } finally {
      setSubmitting(false);
    }
  }

  /** Restores a login from its backup key; resolves to an error message or null. */
  async function signIn(input: string): Promise<string | null> {
    const identity = await restoreIdentity(input);
    if (!identity) return "That isn't a valid backup key. It starts with nsec1.";
    return finishSignIn();
  }

  /** Signs in with a Nostr extension; resolves to an error message or null. */
  async function signInWithExtension(): Promise<string | null> {
    const failure = await connectExtension();
    return failure ?? finishSignIn();
  }

  /** After choosing a signer: home if the profile exists, else set it up. */
  async function finishSignIn(): Promise<string | null> {
    try {
      const me = await getMe();
      if (me.profile) {
        goHome();
      } else {
        // A login without a profile yet: finish signing up.
        patch({ step: "profile", authMode: "create" });
      }
      return null;
    } catch (failure) {
      return failure instanceof Error ? failure.message : "Couldn't sign in. Try again.";
    }
  }

  if (!hydrated) {
    return <div className="min-h-full bg-background" />;
  }

  if (state.step === "onboarding") {
    return (
      <SignupOnboarding
        slide={state.onboardingSlide}
        onSkip={goHome}
        onNext={() =>
          patch({
            onboardingSlide: Math.min(3, state.onboardingSlide + 1) as OnboardingSlide,
          })
        }
        onDone={goHome}
      />
    );
  }

  if (state.step === "backup") {
    return <SignupBackup backupKey={backupKey} onContinue={() => patch({ step: "profile" })} />;
  }

  if (state.step === "profile") {
    return (
      <SignupProfile
        authMode={state.authMode}
        displayName={state.displayName}
        bio={state.bio}
        onAuthModeChange={(authMode: AuthMode) => patch({ authMode })}
        onDisplayNameChange={(displayName) => patch({ displayName })}
        onBioChange={(bio) => patch({ bio })}
        onSignIn={() => patch({ authMode: "signin" })}
        onSubmit={submitProfile}
        submitting={submitting}
        error={error}
        replacesSavedLogin={hasSigner()}
        onRestore={signIn}
        onConnectExtension={signInWithExtension}
      />
    );
  }

  return (
    <SignupWelcome
      authMode={state.authMode}
      onAuthModeChange={(authMode) => patch({ authMode })}
      onCreateLogin={createLogin}
      replacesSavedLogin={hasSigner()}
      onSignIn={signIn}
      onConnectExtension={signInWithExtension}
      error={error}
    />
  );
}
