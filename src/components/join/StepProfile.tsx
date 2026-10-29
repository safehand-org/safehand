import { FieldLabel } from "./JoinLayout";
import { PROFILE_COLORS } from "./types";

export function StepProfile({
  avatarColor,
  shareDestination,
  onColorChange,
  onDestinationChange,
}: {
  avatarColor: string;
  shareDestination: string;
  onColorChange: (value: string) => void;
  onDestinationChange: (value: string) => void;
}) {
  return (
    <div className="flex flex-col gap-6">
      <div>
        <FieldLabel>Display color</FieldLabel>
        <div className="mt-3 flex items-center gap-2.5">
          {PROFILE_COLORS.map((color) => {
            const selected = color === avatarColor;
            return (
              <button
                key={color}
                type="button"
                aria-label={`Choose ${color}`}
                aria-pressed={selected}
                onClick={() => onColorChange(color)}
                className="flex size-9 items-center justify-center rounded-full"
              >
                <span
                  className={selected ? "size-8 rounded-full" : "size-3.5 rounded-full"}
                  style={{ background: color }}
                />
              </button>
            );
          })}
        </div>
      </div>

      <label className="block">
        <FieldLabel>Where should your share go?</FieldLabel>
        <input
          type="email"
          value={shareDestination}
          onChange={(event) => onDestinationChange(event.target.value)}
          className="mt-2.5 h-[3.25rem] w-full rounded-full border border-[#e8e8e8] bg-white px-5 text-[15px] text-ink outline-none focus-visible:ring-2 focus-visible:ring-ink/15"
        />
      </label>
    </div>
  );
}
