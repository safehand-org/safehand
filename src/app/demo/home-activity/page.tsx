import { activityHomeData } from "@/components/home/types";
import { HomeView } from "@/components/home/HomeView";
import { PhoneShell } from "@/components/shell/PhoneShell";

export default function DemoHome() {
  return (
    <PhoneShell>
      <HomeView data={activityHomeData} />
    </PhoneShell>
  );
}
