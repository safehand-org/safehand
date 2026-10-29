import { JoinView } from "@/components/join/JoinView";
import { PhoneShell } from "@/components/shell/PhoneShell";

export default function InviteCardPage() {
  return (
    <PhoneShell showNav={false}>
      <JoinView />
    </PhoneShell>
  );
}
