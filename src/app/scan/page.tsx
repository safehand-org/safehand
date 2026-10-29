import { ScanView } from "@/components/scan/ScanView";
import { PhoneShell } from "@/components/shell/PhoneShell";

export default function ScanPage() {
  return (
    <PhoneShell activeHref="/scan" showNav={false}>
      <ScanView />
    </PhoneShell>
  );
}
