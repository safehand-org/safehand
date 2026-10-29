import { CloseIcon, PersonIcon } from "./icons";
import {
  AVATAR_PALETTE,
  creatorName,
  isValidEmail,
  nameFromEmail,
  type Teammate,
  type WizardState,
} from "./types";

export function StepTeammates({
  state,
  email,
  onEmailChange,
  onAdd,
  onRemove,
}: {
  state: WizardState;
  email: string;
  onEmailChange: (value: string) => void;
  onAdd: () => void;
  onRemove: (id: string) => void;
}) {
  const hasTeammates = state.teammates.length > 0;

  return (
    <div>
      <div className="flex flex-col gap-2.5">
        <CreatorRow name={creatorName(state)} />
        {state.teammates.map((teammate) => (
          <TeammateRow
            key={teammate.id}
            teammate={teammate}
            onRemove={() => onRemove(teammate.id)}
          />
        ))}
      </div>

      {hasTeammates ? (
        <p className="mt-4 text-[13px] leading-5 text-sheet-muted">
          We&apos;ll email each person an invite once the card is created.
        </p>
      ) : (
        <p className="mt-5 text-[13px] leading-5 text-sheet-muted">
          Add at least one teammate to continue.
        </p>
      )}

      <form
        className="mt-4 flex items-center gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          onAdd();
        }}
      >
        <input
          type="email"
          value={email}
          onChange={(event) => onEmailChange(event.target.value)}
          placeholder="Teammate's email"
          className="h-12 min-w-0 flex-1 rounded-full border border-dashed border-[#c6c6c6] bg-white px-5 text-[15px] text-ink outline-none placeholder:text-[#b0b0b0] focus-visible:ring-2 focus-visible:ring-ink/15"
        />
        <button
          type="submit"
          className="flex size-12 shrink-0 items-center justify-center rounded-full bg-ink text-[11px] font-medium tracking-[0.01em] text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/30"
        >
          + Add
        </button>
      </form>
    </div>
  );
}

function CreatorRow({ name }: { name: string }) {
  return (
    <div className="flex items-center gap-3 rounded-full bg-ink px-2 py-2 text-white">
      <span className="flex size-10 shrink-0 items-center justify-center rounded-full bg-[#2a2a2a]">
        <PersonIcon />
      </span>
      <div className="min-w-0">
        <p className="text-[15px] leading-none font-medium">{name}</p>
        <p className="mt-1 text-[12px] leading-none text-[#b8b8b8]">You</p>
      </div>
    </div>
  );
}

function TeammateRow({
  teammate,
  onRemove,
}: {
  teammate: Teammate;
  onRemove: () => void;
}) {
  return (
    <div className="flex items-center gap-3 rounded-full bg-[#f3f3f3] px-2 py-2">
      <span
        className="flex size-10 shrink-0 items-center justify-center rounded-full text-[13px] font-semibold text-white"
        style={{ background: teammate.color }}
      >
        {teammate.name.slice(0, 1)}
      </span>
      <div className="min-w-0 flex-1">
        <p className="text-[15px] leading-none font-medium text-ink">
          {teammate.name}
        </p>
        <p className="mt-1 truncate text-[12px] leading-none text-[#8a8a8a]">
          {teammate.email}
        </p>
      </div>
      <button
        type="button"
        aria-label={`Remove ${teammate.name}`}
        onClick={onRemove}
        className="mr-2 flex size-8 items-center justify-center text-[#7a7a7a] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ink/20"
      >
        <CloseIcon />
      </button>
    </div>
  );
}

export function addTeammate(state: WizardState, rawEmail: string): Teammate | null {
  const email = rawEmail.trim().toLowerCase();
  if (!isValidEmail(email)) return null;
  if (state.teammates.some((item) => item.email === email)) return null;

  return {
    id: `teammate-${email}`,
    name: nameFromEmail(email),
    email,
    color: AVATAR_PALETTE[state.teammates.length % AVATAR_PALETTE.length],
  };
}
