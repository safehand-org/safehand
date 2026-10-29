import { DEMO_INVITE, formatPercent, type JoinMember } from "./types";

export function StepInviteCard() {
  return (
    <div>
      <div>
        <h2 className="text-[22px] leading-none font-semibold tracking-[-0.03em]">
          {DEMO_INVITE.teamName}
        </h2>
        <p className="mt-2 text-[14px] text-sheet-muted">
          {DEMO_INVITE.hackathon}
        </p>
      </div>

      <ul className="mt-5 flex flex-col gap-2.5">
        {DEMO_INVITE.members.map((member) => (
          <MemberRow key={member.id} member={member} />
        ))}
      </ul>
    </div>
  );
}

export function MemberRow({
  member,
  status,
  swatch,
}: {
  member: JoinMember;
  status?: string;
  swatch?: string;
}) {
  const you = Boolean(member.you);
  const color = swatch ?? member.color;

  return (
    <li
      className={`flex items-center gap-3 rounded-full px-4 py-3.5 ${
        you ? "bg-ink text-white" : "bg-[#f2f2f2] text-ink"
      }`}
    >
      <span
        aria-hidden="true"
        className="size-[9px] shrink-0 rounded-[2px]"
        style={{ background: color }}
      />
      <p className="min-w-0 flex-1 truncate text-[15px] font-medium">
        {member.name}
        {you && !status ? (
          <span className="font-normal text-[#a3a3a3]"> (You)</span>
        ) : null}
      </p>
      {status ? (
        <p
          className={`shrink-0 text-[10px] font-semibold tracking-[0.1em] uppercase ${
            member.signed ? "text-[#34b6ee]" : "text-[#9a9a9a]"
          }`}
        >
          {status}
        </p>
      ) : (
        <p className="shrink-0 text-[15px] font-semibold">
          {formatPercent(member.percent)}
        </p>
      )}
    </li>
  );
}
