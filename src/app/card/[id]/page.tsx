import { Suspense } from "react";
import { notFound } from "next/navigation";
import { LockedCardStatusView } from "@/components/cards/locked/LockedCardStatusView";
import { getLockedCard, lockedCards } from "@/components/cards/locked/types";
import { PhoneShell } from "@/components/shell/PhoneShell";

export function generateStaticParams() {
  return Object.keys(lockedCards).map((id) => ({ id }));
}

export default function CardStatusPage({
  params,
  searchParams,
}: {
  params: Promise<{ id: string }>;
  searchParams: Promise<{ phase?: string | string[] }>;
}) {
  return (
    <PhoneShell showNav={false}>
      <Suspense fallback={<LockedStatusFallback />}>
        <LockedCardStatusLoader params={params} searchParams={searchParams} />
      </Suspense>
    </PhoneShell>
  );
}

async function LockedCardStatusLoader({
  params,
  searchParams,
}: {
  params: Promise<{ id: string }>;
  searchParams: Promise<{ phase?: string | string[] }>;
}) {
  const { id } = await params;
  const query = await searchParams;
  const phase = Array.isArray(query.phase) ? query.phase[0] : query.phase;
  // TODO: load the split card, prize detection, and payout execution from the cards API
  const card = getLockedCard(id, phase);
  if (!card) notFound();

  return <LockedCardStatusView card={card} />;
}

function LockedStatusFallback() {
  return (
    <div className="h-full">
      <div className="h-72 rounded-b-sheet bg-sheet" />
    </div>
  );
}
