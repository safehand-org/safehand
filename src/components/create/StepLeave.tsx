import type { WizardState } from "./types";

export function StepLeave({
  state,
  onChange,
}: {
  state: WizardState;
  onChange: (patch: Partial<WizardState>) => void;
}) {
  return (
    <div className="flex flex-col gap-3">
      <LeaveOption
        title="They leave with nothing"
        badge="Default"
        body="Their share is split equally among the rest of the team."
        selected={state.leavePolicy === "nothing"}
        onSelect={() => onChange({ leavePolicy: "nothing" })}
      />
      <LeaveOption
        title="They keep a smaller share"
        body="They keep a fixed part of the prize. The rest is split equally among everyone who stays."
        selected={state.leavePolicy === "smaller"}
        onSelect={() => onChange({ leavePolicy: "smaller" })}
      />
      {state.leavePolicy === "smaller" ? (
        <label className="mt-2 block">
          <span className="text-[10px] font-medium tracking-[0.16em] text-[#8a8a8a] uppercase">
            What they keep
          </span>
          <input
            type="text"
            value={`${state.leaveKeepPercent}% of the prize`}
            onChange={(event) => {
              const match = event.target.value.match(/\d+(\.\d+)?/);
              onChange({
                leaveKeepPercent: match ? Number(match[0]) : 0,
              });
            }}
            className="mt-2.5 h-12 w-full rounded-full border border-[#e7e7e7] bg-white px-5 text-[15px] text-ink outline-none focus-visible:ring-2 focus-visible:ring-ink/15"
          />
        </label>
      ) : null}
    </div>
  );
}

function LeaveOption({
  title,
  badge,
  body,
  selected,
  onSelect,
}: {
  title: string;
  badge?: string;
  body: string;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className="flex w-full items-center gap-3 rounded-[1.35rem] bg-[#f3f3f3] px-4 py-4 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/20"
    >
      <div className="min-w-0 flex-1">
        <p className="text-[16px] font-semibold tracking-[-0.02em] text-ink">
          {title}
          {badge ? (
            <span className="font-medium text-[#8a8a8a]">&nbsp;({badge})</span>
          ) : null}
        </p>
        <p className="mt-1.5 text-[13px] leading-5 text-sheet-muted">{body}</p>
      </div>
      <Toggle selected={selected} policy={title} />
    </button>
  );
}

function Toggle({
  selected,
  policy,
}: {
  selected: boolean;
  policy: string;
}) {
  return (
    <span
      role="switch"
      aria-checked={selected}
      aria-label={policy}
      className={
        selected
          ? "flex h-7 w-12 shrink-0 items-center justify-end rounded-full bg-ink p-0.5"
          : "flex h-7 w-12 shrink-0 items-center justify-start rounded-full bg-[#d9d9d9] p-0.5"
      }
    >
      <span className="size-6 rounded-full bg-white shadow-sm" />
    </span>
  );
}

export function leaveSummary(state: WizardState) {
  if (state.leavePolicy === "smaller") {
    return `If someone leaves before you submit, they keep ${state.leaveKeepPercent}% of the prize. The rest is split equally among the others.`;
  }

  return "If someone leaves before you submit, they leave with nothing. Their share is split equally among the rest of the team.";
}
