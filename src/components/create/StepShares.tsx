import {
  equalTenths,
  formatBasisPoints,
  SHARES_PROBLEM_MESSAGES,
  tenthsToPercent,
  totalTenths,
  weightsFor,
} from "@/lib/shares";
import { SHARE_DOTS, type Member, type WizardState } from "./types";

export function StepShares({
  state,
  members,
  basisPoints,
  onChange,
}: {
  state: WizardState;
  members: Member[];
  /** Each member's share from splitcard-core, or null while it can't be computed. */
  basisPoints: number[] | null;
  onChange: (patch: Partial<WizardState>) => void;
}) {
  const custom = state.shareMode === "custom";
  const texts = members.map((member) => state.customShares[member.pubkey] ?? "");
  const total = totalTenths(texts);
  const result = weightsFor(state.shareMode, texts);

  return (
    <div>
      <div className="flex rounded-full bg-[#f3f3f3] p-1">
        <ModeButton
          label="Equal"
          selected={!custom}
          onClick={() => onChange({ shareMode: "equal" })}
        />
        <ModeButton
          label="Custom"
          selected={custom}
          onClick={() => {
            const start = equalTenths(members.length);
            onChange({
              shareMode: "custom",
              customShares: Object.fromEntries(
                members.map((member, index) => [member.pubkey, tenthsToPercent(start[index])]),
              ),
            });
          }}
        />
      </div>

      <ul className="mt-4 overflow-hidden rounded-[1.35rem] bg-[#f3f3f3]">
        {members.map((member, index) => (
          <li key={member.pubkey} className="flex items-center gap-3 px-4 py-3.5">
            <span
              aria-hidden="true"
              className="size-2.5 shrink-0 rounded-[3px]"
              style={{ background: SHARE_DOTS[index % SHARE_DOTS.length] }}
            />
            <p className="min-w-0 flex-1 text-[15px] font-medium text-ink">
              {member.you ? `${member.name || "You"} (You)` : member.name}
            </p>
            {custom ? (
              <label className="flex items-center text-[15px] font-medium text-ink">
                <input
                  type="text"
                  inputMode="decimal"
                  aria-label={`${member.name}'s share`}
                  value={texts[index]}
                  onChange={(event) =>
                    onChange({
                      customShares: { ...state.customShares, [member.pubkey]: event.target.value },
                    })
                  }
                  className="w-14 bg-transparent text-right outline-none"
                />
                <span>%</span>
              </label>
            ) : (
              <p className="text-[15px] font-medium text-ink">
                {basisPoints ? formatBasisPoints(basisPoints[index]) : "…"}
              </p>
            )}
          </li>
        ))}
      </ul>
      {custom ? (
        <p className="mt-3 text-[13px] leading-5 text-sheet-muted">
          Total: {total === null ? "—" : `${tenthsToPercent(total)}%`}
          {"problem" in result ? (
            <span className="block text-[#C62828]">{SHARES_PROBLEM_MESSAGES[result.problem]}</span>
          ) : null}
        </p>
      ) : null}
    </div>
  );
}

function ModeButton({
  label,
  selected,
  onClick,
}: {
  label: string;
  selected: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-pressed={selected}
      onClick={onClick}
      className={
        selected
          ? "h-10 flex-1 rounded-full bg-ink text-[12px] font-semibold tracking-[0.14em] text-white uppercase focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/30"
          : "h-10 flex-1 rounded-full text-[12px] font-semibold tracking-[0.14em] text-[#8a8a8a] uppercase focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/20"
      }
    >
      {label}
    </button>
  );
}

export function sharesCopy(state: WizardState) {
  if (state.shareMode === "custom") {
    return "Equal is the default. You can change it, as long as the total is exactly 100%.";
  }
  return "Everyone starts with an equal share. Switch to Custom to change it. The total must be 100%.";
}
