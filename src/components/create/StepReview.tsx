import {
  CREATOR_ID,
  formatPercent,
  membersOf,
  type WizardState,
} from "./types";

const REVIEW_ROWS = [
  { bg: "bg-ink", text: "text-white", dot: "#34b6ee" },
  { bg: "bg-[#2a2826]", text: "text-white", dot: "#100b02" },
  { bg: "bg-[#ececec]", text: "text-ink", dot: "#d4d4d4" },
] as const;

export function StepReview({ state }: { state: WizardState }) {
  const members = membersOf(state);

  return (
    <div>
      <div>
        <h2 className="text-[22px] leading-none font-semibold tracking-[-0.03em]">
          {state.teamName.trim() || "Team"}
        </h2>
        <p className="mt-2 text-[14px] text-sheet-muted">
          {state.hackathon.trim() || "Hackathon"}
        </p>
      </div>

      <ul className="mt-5 flex flex-col gap-2">
        {members.map((member, index) => {
          const tone = REVIEW_ROWS[index] ?? REVIEW_ROWS[2];
          return (
            <li
              key={member.id}
              className={`flex items-center gap-3 rounded-full px-4 py-3.5 ${tone.bg} ${tone.text}`}
            >
              <span
                aria-hidden="true"
                className="size-2.5 shrink-0 rounded-full"
                style={{ background: tone.dot }}
              />
              <p className="min-w-0 flex-1 text-[15px] font-medium">
                {member.id === CREATOR_ID ? member.label : member.name}
              </p>
              <p className="text-[15px] font-medium">
                {formatPercent(member.percent)}
              </p>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
