"use client";

import { useRouter } from "next/navigation";
import { useEffect, useState } from "react";
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

export function SignupView() {
  const router = useRouter();
  const [state, setState] = useState<SignupState>(createSignupState);
  const [hydrated, setHydrated] = useState(false);

  useEffect(() => {
    const stored = window.sessionStorage.getItem(SIGNUP_STORAGE_KEY);
    if (stored) {
      try {
        const parsed = JSON.parse(stored) as Partial<SignupState>;
        setState((current) => ({ ...current, ...parsed }));
      } catch {
        window.sessionStorage.removeItem(SIGNUP_STORAGE_KEY);
      }
    }
    setHydrated(true);
  }, []);

  useEffect(() => {
    if (!hydrated) return;
    window.sessionStorage.setItem(SIGNUP_STORAGE_KEY, JSON.stringify(state));
  }, [hydrated, state]);

  function patch(next: Partial<SignupState>) {
    setState((current) => ({ ...current, ...next }));
  }

  function goHome() {
    window.sessionStorage.removeItem(SIGNUP_STORAGE_KEY);
    router.push("/");
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
        onSubmit={() => {
          // TODO: POST displayName + bio to the registration API
          patch({ step: "onboarding", onboardingSlide: 1 });
        }}
      />
    );
  }

  return (
    <SignupWelcome
      authMode={state.authMode}
      onAuthModeChange={(authMode) => patch({ authMode })}
      onCreateLogin={() => {
        // TODO: create a local login / backup key before collecting profile
        patch({ step: "profile", authMode: "create" });
      }}
    />
  );
}
