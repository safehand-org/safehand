export type WizardStep = 1 | 2 | 3 | 4 | 5 | 6;

export type Teammate = {
  id: string;
  name: string;
  email: string;
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
  customShares: Record<string, number>;
  leavePolicy: LeavePolicy;
  leaveKeepPercent: number;
};

export type Member = {
  id: string;
  name: string;
  label: string;
  email: string;
  you: boolean;
  color: string;
  percent: number;
};

export const CREATOR_ID = "creator";

export const AVATAR_PALETTE = ["#8B2C2C", "#5C1518", "#1F4D3A", "#1E3A5F", "#6B3FA0"];

const KNOWN_NAMES: Record<string, string> = {
  "goodness@friedrice.com": "Goodness",
  "spongebobeddie@pineapple.com": "Spongebob Squarepants",
};

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
    leaveKeepPercent: 15,
  };
}

export function nameFromEmail(email: string) {
  const normalized = email.trim().toLowerCase();
  if (KNOWN_NAMES[normalized]) return KNOWN_NAMES[normalized];

  const local = normalized.split("@")[0] ?? "";
  const spaced = local.replace(/[._-]+/g, " ").trim();
  if (!spaced) return "Teammate";

  return spaced.replace(/\b\w/g, (char) => char.toUpperCase());
}

export function inviteSlug(teamName: string) {
  return (
    teamName
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "team"
  );
}

export function invitePath(teamName: string) {
  return `/invite/card/${inviteSlug(teamName)}`;
}

export function creatorName(state: WizardState) {
  return state.yourName.trim() || "Flora";
}

export function memberCount(state: WizardState) {
  return 1 + state.teammates.length;
}

export function equalPercent(count: number) {
  return Number((100 / Math.max(count, 1)).toFixed(1));
}

export function membersOf(state: WizardState): Member[] {
  const people: Omit<Member, "percent">[] = [
    {
      id: CREATOR_ID,
      name: creatorName(state),
      label: `${creatorName(state)} (You)`,
      email: "",
      you: true,
      color: "#100b02",
    },
    ...state.teammates.map((teammate) => ({
      id: teammate.id,
      name: teammate.name,
      label: teammate.name,
      email: teammate.email,
      you: false,
      color: teammate.color,
    })),
  ];

  const equal = equalPercent(people.length);

  return people.map((person) => ({
    ...person,
    percent:
      state.shareMode === "custom"
        ? (state.customShares[person.id] ?? equal)
        : equal,
  }));
}

export function shareTotal(state: WizardState) {
  return membersOf(state).reduce((sum, member) => sum + member.percent, 0);
}

export function formatPercent(value: number) {
  return `${value.toFixed(1)}%`;
}

export function isValidEmail(value: string) {
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value.trim());
}
