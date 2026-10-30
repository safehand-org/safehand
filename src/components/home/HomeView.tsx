import { ActivityList } from "./ActivityList";
import { AttentionCard } from "./AttentionCard";
import { HomeGreeting } from "./HomeGreeting";
import { HomeHeader } from "./HomeHeader";
import { InviteBanner } from "./InviteBanner";
import type { HomeData } from "./types";

export function HomeView({ data, npub }: { data: HomeData; npub?: string | null }) {
  const hasAttention = data.attention !== null;

  return (
    <>
      <section
        className={
          hasAttention
            ? "relative overflow-hidden rounded-b-sheet bg-sheet px-5 pt-5 pb-4 text-ink"
            : "relative overflow-hidden rounded-b-sheet bg-sheet px-5 pt-5 pb-6 text-ink"
        }
      >
        <HomeHeader />
        <HomeGreeting greeting={data.greeting} compact={hasAttention} npub={npub} />
        {data.attention ? <AttentionCard attention={data.attention} /> : null}
      </section>

      <div className="px-5 pb-6">
        <InviteBanner message={data.inviteMessage} />
        <ActivityList items={data.activity} />
      </div>
    </>
  );
}
