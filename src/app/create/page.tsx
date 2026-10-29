import { CreateView } from "@/components/create/CreateView";
import { PhoneShell } from "@/components/shell/PhoneShell";

export default function CreatePage() {
  return (
    <PhoneShell activeHref="/create" showNav={false}>
      <CreateView />
    </PhoneShell>
  );
}
