import { Suspense } from "react";
import { notFound } from "next/navigation";
import { connection } from "next/server";
import { LockedCardScreen } from "@/components/cards/locked/LockedCardScreen";
import { PhoneShell } from "@/components/shell/PhoneShell";
import type { CardView, TeamView } from "@/lib/api";
import { serverGet } from "@/lib/serverApi";

export default function CardStatusPage({ params }: { params: Promise<{ id: string }> }) {
  return (
    <PhoneShell showNav={false}>
      <Suspense fallback={<LockedStatusFallback />}>
        <LockedCardStatusLoader params={params} />
      </Suspense>
    </PhoneShell>
  );
}

/** The route id is the team slug. Only a team with a locked card has a page. */
async function LockedCardStatusLoader({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  // The card and its signatures change at any time: read them per request.
  await connection();
  const team = await serverGet<TeamView>(`/teams/by-slug/${encodeURIComponent(id)}`);
  if (!team.ok) {
    if (team.status === 404) notFound();
    return <LockedStatusFallback />;
  }
  const card = await serverGet<CardView>(`/teams/${encodeURIComponent(team.data.id)}/card`);
  if (!card.ok) {
    if (card.status === 404) notFound();
    return <LockedStatusFallback />;
  }
  return (
    <LockedCardScreen team={team.data} card={card.data} fallback={<LockedStatusFallback />} />
  );
}

function LockedStatusFallback() {
  return (
    <div className="h-full">
      <div className="h-72 rounded-b-sheet bg-sheet" />
    </div>
  );
}
