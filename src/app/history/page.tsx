import { HistoryView } from "@/components/history/HistoryView";
import { PhoneShell } from "@/components/shell/PhoneShell";

export default function HistoryPage() {
  return (
    <PhoneShell activeHref="/history">
      <HistoryView />
    </PhoneShell>
  );
}
