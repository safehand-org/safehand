import { BellIcon } from "./icons";

export function InviteBanner({ message }: { message: string }) {
  return (
    <aside className="mt-4 flex items-center gap-3 rounded-panel border border-dashed border-hairline px-3 py-3">
      <span className="flex size-9 shrink-0 items-center justify-center rounded-full border border-hairline bg-chip text-sheet">
        <BellIcon />
      </span>
      <p className="text-[12.5px] leading-[1.35] text-muted">{message}</p>
    </aside>
  );
}
