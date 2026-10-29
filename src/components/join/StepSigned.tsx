import { MemberRow } from "./StepInviteCard";
import { DEMO_INVITE } from "./types";

export function StepSigned({ avatarColor }: { avatarColor: string }) {
  const members = DEMO_INVITE.members.map((member) =>
    member.you ? { ...member, signed: true } : member,
  );

  return (
    <ul className="flex flex-col gap-2.5">
      {members.map((member) => (
        <MemberRow
          key={member.id}
          member={member}
          swatch={member.you ? avatarColor : member.color}
          status={member.signed ? "Signed" : "Not yet signed"}
        />
      ))}
    </ul>
  );
}
