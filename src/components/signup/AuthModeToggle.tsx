import type { AuthMode } from "./types";

export function AuthModeToggle({
  mode,
  variant,
  onChange,
}: {
  mode: AuthMode;
  variant: "welcome" | "profile";
  onChange: (mode: AuthMode) => void;
}) {
  if (variant === "profile") {
    return (
      <div className="mt-8 grid grid-cols-2 gap-3">
        <ToggleButton
          selected={mode === "create"}
          radius="rounded-full"
          selectedClass="bg-ink text-white"
          idleClass="bg-white text-ink shadow-[0_0_0_1px_rgba(16,11,2,0.08)]"
          onClick={() => onChange("create")}
        >
          Create Account
        </ToggleButton>
        <ToggleButton
          selected={mode === "signin"}
          radius="rounded-full"
          selectedClass="bg-ink text-white"
          idleClass="bg-white text-ink shadow-[0_0_0_1px_rgba(16,11,2,0.08)]"
          onClick={() => onChange("signin")}
        >
          Sign In
        </ToggleButton>
      </div>
    );
  }

  return (
    <div className="grid grid-cols-2 gap-3">
      <ToggleButton
        selected={mode === "create"}
        selectedClass="border border-hairline bg-[#1c1a18] text-sheet"
        idleClass="border border-hairline/80 bg-transparent text-muted"
        onClick={() => onChange("create")}
      >
        Create Account
      </ToggleButton>
      <ToggleButton
        selected={mode === "signin"}
        selectedClass="border border-hairline bg-[#1c1a18] text-sheet"
        idleClass="border border-hairline/80 bg-transparent text-muted"
        onClick={() => onChange("signin")}
      >
        Sign In
      </ToggleButton>
    </div>
  );
}

function ToggleButton({
  selected,
  selectedClass,
  idleClass,
  radius = "rounded-[1.15rem]",
  onClick,
  children,
}: {
  selected: boolean;
  selectedClass: string;
  idleClass: string;
  radius?: string;
  onClick: () => void;
  children: string;
}) {
  return (
    <button
      type="button"
      aria-pressed={selected}
      onClick={onClick}
      className={`inline-flex h-12 items-center justify-center text-[15px] font-medium focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-sheet/40 ${radius} ${
        selected ? selectedClass : idleClass
      }`}
    >
      {children}
    </button>
  );
}
