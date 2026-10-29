import {
  CREATOR_ID,
  formatPercent,
  membersOf,
  SHARE_DOTS,
  shareTotal,
  type WizardState,
} from "./types";

export function StepShares({
  state,
  onChange,
}: {
  state: WizardState;
  onChange: (patch: Partial<WizardState>) => void;
}) {
  const members = membersOf(state);

  return (
    <div>
      <div className="flex rounded-full bg-[#f3f3f3] p-1">
        <ModeButton
          label="Equal"
          selected={state.shareMode === "equal"}
          onClick={() => onChange({ shareMode: "equal" })}
        />
        <ModeButton
          label="Custom"
          selected={state.shareMode === "custom"}
          onClick={() => {
            const next = Object.fromEntries(
              members.map((member) => [member.id, member.percent]),
            );
            onChange({ shareMode: "custom", customShares: next });
          }}
        />
      </div>

      <ul className="mt-4 overflow-hidden rounded-[1.35rem] bg-[#f3f3f3]">
        {members.map((member, index) => (
          <li
            key={member.id}
            className="flex items-center gap-3 px-4 py-3.5"
          >
            <span
              aria-hidden="true"
              className="size-2.5 shrink-0 rounded-[3px]"
              style={{ background: SHARE_DOTS[index] ?? "#100b02" }}
            />
            <p className="min-w-0 flex-1 text-[15px] font-medium text-ink">
              {member.id === CREATOR_ID ? member.label : member.name}
            </p>
            {state.shareMode === "custom" ? (
              <label className="flex items-center text-[15px] font-medium text-ink">
                <input
                  type="number"
                  min={0}
                  max={100}
                  step={0.1}
                  value={member.percent}
                  onChange={(event) => {
                    const value = Number(event.target.value);
                    onChange({
                      customShares: {
                        ...state.customShares,
                        [member.id]: Number.isFinite(value) ? value : 0,
                      },
                    });
                  }}
                  className="w-14 bg-transparent text-right outline-none [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
                />
                <span>%</span>
              </label>
            ) : (
              <p className="text-[15px] font-medium text-ink">
                {formatPercent(member.percent)}
              </p>
            )}
          </li>
        ))}
      </ul>
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
    return "Equal is the default. You can change it, as long as the total stays at 100%.";
  }

  return "Everyone starts with an equal share. Switch to Custom to change it. The total must be 100%.";
}

export function sharesComplete(state: WizardState) {
  const total = shareTotal(state);
  return total >= 99.8 && total <= 100.2;
}
