export type WizardStep = 1 | 2 | 3 | 4 | 5;

/** A teammate added by npub. `name` is their profile name or what you typed. */
export type Teammate = {
  pubkey: string;
  npub: string;
  name: string;
  color: string;
};

export type ShareMode = "equal" | "custom";
export type LeavePolicy = "nothing" | "smaller";

export type WizardState = {
  step: WizardStep;
  yourName: string;
  teamName: string;
  hackathon: string;
  teammates: Teammate[];
  shareMode: ShareMode;
  /** Typed custom percents by pubkey, such as `33.4`. */
  customShares: Record<string, string>;
  leavePolicy: LeavePolicy;
  /** Whole percent a leaver keeps, as typed. */
  leaveKeepPercent: string;
  lightningAddress: string;
};

/** Someone on the card, the creator first. */
export type Member = {
  pubkey: string;
  name: string;
  you: boolean;
  color: string;
};

export const AVATAR_PALETTE = ["#8B2C2C", "#5C1518", "#1F4D3A", "#1E3A5F", "#6B3FA0"];

export const SHARE_DOTS = ["#100b02", "#8d8d8d", "#d4d4d4", "#5c5c5c", "#b0b0b0"];

export function createInitialState(): WizardState {
  return {
    step: 1,
    yourName: "",
    teamName: "",
    hackathon: "",
    teammates: [],
    shareMode: "equal",
    customShares: {},
    leavePolicy: "nothing",
    leaveKeepPercent: "15",
    lightningAddress: "",
  };
}

export function membersOf(state: WizardState, creatorPubkey: string): Member[] {
  return [
    { pubkey: creatorPubkey, name: state.yourName.trim(), you: true, color: "#100b02" },
    ...state.teammates.map((teammate) => ({
      pubkey: teammate.pubkey,
      name: teammate.name,
      you: false,
      color: teammate.color,
    })),
  ];
}

/** The whole percent a leaver keeps, 1 to 99, or null if what's typed isn't one. */
export function keepPercent(state: WizardState): number | null {
  const text = state.leaveKeepPercent.trim();
  if (!/^\d{1,2}$/.test(text)) return null;
  const value = Number(text);
  return value >= 1 && value <= 99 ? value : null;
}
