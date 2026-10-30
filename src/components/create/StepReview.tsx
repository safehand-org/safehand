import { CREATE_STEPS, type CreateStepId } from "@/lib/createCard";
import { formatBasisPoints } from "@/lib/shares";
import type { Member, WizardState } from "./types";

const REVIEW_ROWS = [
  { bg: "bg-ink", text: "text-white", dot: "#34b6ee" },
  { bg: "bg-[#2a2826]", text: "text-white", dot: "#100b02" },
  { bg: "bg-[#ececec]", text: "text-ink", dot: "#d4d4d4" },
] as const;

export type RunState = {
  /** Steps finished so far. */
  done: CreateStepId[];
  /** The step running now, or the one that failed. */
  current: CreateStepId | null;
  error: string | null;
};

export function StepReview({
  state,
  members,
  basisPoints,
  run,
  onLightningAddressChange,
}: {
  state: WizardState;
  members: Member[];
  basisPoints: number[] | null;
  run: RunState | null;
  onLightningAddressChange: (value: string) => void;
}) {
  return (
    <div>
      <div>
        <h2 className="text-[22px] leading-none font-semibold tracking-[-0.03em]">
          {state.teamName.trim()}
        </h2>
        <p className="mt-2 text-[14px] text-sheet-muted">{state.hackathon.trim()}</p>
      </div>

      <ul className="mt-5 flex flex-col gap-2">
        {members.map((member, index) => {
          const tone = REVIEW_ROWS[index] ?? REVIEW_ROWS[2];
          return (
            <li
              key={member.pubkey}
              className={`flex items-center gap-3 rounded-full px-4 py-3.5 ${tone.bg} ${tone.text}`}
            >
              <span
                aria-hidden="true"
                className="size-2.5 shrink-0 rounded-full"
                style={{ background: tone.dot }}
              />
              <p className="min-w-0 flex-1 text-[15px] font-medium">
                {member.you ? `${member.name} (You)` : member.name}
              </p>
              <p className="text-[15px] font-medium">
                {basisPoints ? formatBasisPoints(basisPoints[index]) : "…"}
              </p>
            </li>
          );
        })}
      </ul>

      <label className="mt-6 block">
        <span className="text-[10px] font-medium tracking-[0.16em] text-[#8a8a8a] uppercase">
          Your Lightning address
        </span>
        <input
          type="text"
          inputMode="email"
          autoComplete="off"
          spellCheck={false}
          placeholder="you@wallet.com"
          value={state.lightningAddress}
          disabled={run !== null && run.error === null}
          onChange={(event) => onLightningAddressChange(event.target.value)}
          className="mt-2.5 h-12 w-full rounded-full border border-[#e7e7e7] bg-white px-5 text-[15px] text-ink outline-none focus-visible:ring-2 focus-visible:ring-ink/15"
        />
        <span className="mt-2 block text-[13px] text-sheet-muted">
          This is where your share will be paid
        </span>
      </label>

      {run ? <Progress run={run} /> : null}
    </div>
  );
}

function Progress({ run }: { run: RunState }) {
  return (
    <div className="mt-5">
      <ol className="flex flex-col gap-1.5" aria-label="Progress">
        {CREATE_STEPS.map((step) => {
          const done = run.done.includes(step.id);
          const failed = run.error !== null && run.current === step.id;
          const active = !done && !failed && run.current === step.id;
          return (
            <li key={step.id} className="flex items-center gap-2 text-[13px]">
              <span aria-hidden="true" className="w-4 text-center">
                {done ? "✓" : failed ? "✕" : active ? "…" : "·"}
              </span>
              <span className={done || active ? "text-ink" : "text-sheet-muted"}>
                {step.label}
              </span>
            </li>
          );
        })}
      </ol>
      {run.error ? (
        <p role="alert" className="mt-3 text-[13px] leading-5 text-[#C62828]">
          {run.error}
        </p>
      ) : null}
    </div>
  );
}
