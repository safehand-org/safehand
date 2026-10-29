import { HomeScreen } from "@/components/home/HomeScreen";
import { PhoneShell } from "@/components/shell/PhoneShell";

export default function Home() {
  return (
    <PhoneShell>
      <HomeScreen />
    </PhoneShell>
  );
}
