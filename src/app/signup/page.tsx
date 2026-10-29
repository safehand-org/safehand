import { SignupView } from "@/components/signup/SignupView";
import { PhoneShell } from "@/components/shell/PhoneShell";

export default function SignupPage() {
  return (
    <PhoneShell showNav={false}>
      <SignupView />
    </PhoneShell>
  );
}
