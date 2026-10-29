export type SignupStep = "welcome" | "profile" | "onboarding";
export type AuthMode = "create" | "signin";
export type OnboardingSlide = 1 | 2 | 3;

export type SignupState = {
  step: SignupStep;
  authMode: AuthMode;
  displayName: string;
  bio: string;
  onboardingSlide: OnboardingSlide;
};

export const SIGNUP_STORAGE_KEY = "safehand.signup";

export function createSignupState(): SignupState {
  return {
    step: "welcome",
    authMode: "create",
    displayName: "",
    bio: "",
    onboardingSlide: 1,
  };
}

export const ONBOARDING_SLIDES = [
  {
    slide: 1 as const,
    number: "01",
    title: "Agree On Shares",
    body: "Decide who gets what before the hackathon starts, so there's nothing to argue about later.",
    stickman: "/white-stickman/innerpeace-white.svg",
    stickmanSize: { width: 91, height: 165 },
  },
  {
    slide: 2 as const,
    number: "02",
    title: "Everyone Signs On It",
    body: "Each teammate reads the card and signs it. Once everyone has signed, no one can change it alone.",
    stickman: "/white-stickman/running-white(1).svg",
    stickmanSize: { width: 119, height: 130 },
  },
  {
    slide: 3 as const,
    number: "03",
    title: "Get Paid Directly",
    body: "When the prize arrives, your share goes straight to you. No one holds your money.",
    stickman: "/white-stickman/running-white.svg",
    stickmanSize: { width: 119, height: 130 },
  },
] as const;
