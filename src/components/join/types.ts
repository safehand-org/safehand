export type JoinStep = 1 | 2 | 3 | 4;

export type JoinMember = {
  id: string;
  name: string;
  percent: number;
  signed: boolean;
  color: string;
  you?: boolean;
};

export type JoinState = {
  step: JoinStep;
  avatarColor: string;
  shareDestination: string;
};

export const DEMO_SHARE_DESTINATION = "greedy@lion.com";

export const DEMO_INVITE = {
  code: "team-42",
  email: "goodness@friedrice.com",
  inviter: "Flora",
  teamName: "Team 42",
  hackathon: "Lagos Builders Hackathon",
  leaveKeepPercent: 15,
  sharePercent: 33.3,
  members: [
    { id: "flora", name: "Flora", percent: 33.3, signed: true, color: "#34b6ee" },
    {
      id: "goodness",
      name: "Goodness",
      percent: 33.3,
      signed: false,
      you: true,
      color: "#ffffff",
    },
    {
      id: "spongebob",
      name: "Spongebob Squarepants",
      percent: 33.3,
      signed: false,
      color: "#c8c8c8",
    },
  ] satisfies JoinMember[],
};

export const PROFILE_COLORS = [
  "#7A5CFF",
  "#F59A3A",
  "#3EE06A",
  "#E23B3B",
  "#FF4DA6",
];

export function createJoinState(): JoinState {
  return {
    step: 1,
    avatarColor: PROFILE_COLORS[0],
    shareDestination: "",
  };
}

export function formatPercent(value: number) {
  return `${value.toFixed(1)}%`;
}

export function parseInviteCode(raw: string) {
  const value = raw.trim();
  if (!value) return DEMO_INVITE.code;

  const pathMatch = value.match(/invite\/card\/([^/?#]+)/i);
  if (pathMatch?.[1]) {
    try {
      return decodeURIComponent(pathMatch[1]);
    } catch {
      return pathMatch[1];
    }
  }

  return value.replace(/^\/+|\/+$/g, "") || DEMO_INVITE.code;
}
