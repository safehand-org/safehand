import { NotificationsView } from "@/components/notifications/NotificationsView";
import { PhoneShell } from "@/components/shell/PhoneShell";

export default function NotificationsPage() {
  return (
    <PhoneShell activeHref="/notifications">
      <NotificationsView />
    </PhoneShell>
  );
}
