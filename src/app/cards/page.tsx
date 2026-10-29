import { CardsView } from "@/components/cards/CardsView";
import { PhoneShell } from "@/components/shell/PhoneShell";

export default function CardsPage() {
  return (
    <PhoneShell activeHref="/cards">
      <CardsView />
    </PhoneShell>
  );
}
