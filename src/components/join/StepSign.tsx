import { DEMO_INVITE, DEMO_SHARE_DESTINATION, formatPercent } from "./types";

export function StepSign({ shareDestination }: { shareDestination: string }) {
  // TODO: show the payout destination saved with the signature, not the demo address
  const destination = shareDestination.trim() || DEMO_SHARE_DESTINATION;

  return (
    <div className="flex flex-col gap-3">
      <AgreementCard
        title={`Your share is ${formatPercent(DEMO_INVITE.sharePercent)}`}
        body={`Sent to ${destination} when the prize arrives.`}
      />
      <AgreementCard
        title="If you leave before you submit"
        body={`you keep ${DEMO_INVITE.leaveKeepPercent}% of the prize.`}
      />
      <AgreementCard
        title="Changes need everyone's vote"
        body="Once locked, any change needs everyone to agree."
      />
    </div>
  );
}

function AgreementCard({ title, body }: { title: string; body: string }) {
  return (
    <div className="flex items-center gap-3 rounded-[1.35rem] bg-[#f3f3f3] px-4 py-4">
      <div className="min-w-0 flex-1">
        <p className="text-[16px] font-semibold tracking-[-0.02em] text-ink">
          {title}
        </p>
        <p className="mt-1 text-[13px] leading-5 text-sheet-muted">{body}</p>
      </div>
      <CheckCircle />
    </div>
  );
}

function CheckCircle() {
  return (
    <svg
      viewBox="0 0 24 24"
      className="size-6 shrink-0 text-ink"
      fill="none"
      aria-hidden="true"
    >
      <circle cx="12" cy="12" r="9.25" stroke="currentColor" strokeWidth="1.4" />
      <path
        d="M8 12.15 10.55 14.7 16.1 9.2"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
