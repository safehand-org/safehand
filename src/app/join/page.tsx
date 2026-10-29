import { JoinView } from "@/components/join/JoinView";
import { PhoneShell } from "@/components/shell/PhoneShell";

export default function JoinPage() {
  return (
    <PhoneShell showNav={false}>
      <JoinView />
    </PhoneShell>
  );
}
