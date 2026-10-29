export type HomeGreeting = {
  title: string;
  name: string;
  subtitle: string;
};

export type HomeAttention = {
  title: string;
  body: string;
  timeAgo: string;
  teammatesLabel: string;
  actionLabel: string;
  /** Where the action goes: the card's invite link. */
  href?: string;
};

export type HomeActivityItem = {
  id: string;
  title: string;
  detail: string;
  status: string;
  href?: string;
};

export type HomeData = {
  greeting: HomeGreeting;
  attention: HomeAttention | null;
  inviteMessage: string;
  activity: HomeActivityItem[];
};

export const activityHomeData: HomeData = {
  greeting: {
    title: "Welcome Back,",
    name: "Flora.",
    subtitle: "You have 1 card waiting for your signature.",
  },
  attention: {
    title: "Orbit Pay",
    body: "Goodness invited you to a split. Your share is 33%.",
    timeAgo: "2h ago",
    teammatesLabel: "3 teammates",
    actionLabel: "Review And Sign",
  },
  inviteMessage: "Join in with the link or scan the qr code.",
  activity: [
    {
      id: "team-42",
      title: "Team 42",
      detail: "1 of 3 signed",
      status: "Draft",
    },
    {
      id: "lagos-build",
      title: "Lagos Build",
      detail: "Everyone has signed. Waiting for the prize",
      status: "Locked",
      href: "/card/lagos-build",
    },
  ],
};

export const emptyHomeData: HomeData = {
  greeting: {
    title: "Hello,",
    name: "Flora.",
    subtitle: "No split cards yet. Let's make your first one in two minutes.",
  },
  attention: null,
  inviteMessage:
    "Got an invite? Open the link or scan the code to join your team.",
  activity: [],
};
