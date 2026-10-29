"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";
import { PrimaryButton } from "@/components/create/WizardLayout";
import { Stickman } from "@/components/signup/Stickman";
import { JoinLayout, OutlineButton, TERMS_FOOTER } from "./JoinLayout";
import { StepInviteCard } from "./StepInviteCard";
import { StepProfile } from "./StepProfile";
import { StepSign } from "./StepSign";
import { StepSigned } from "./StepSigned";
import { createJoinState, DEMO_INVITE, type JoinState } from "./types";

export function JoinView() {
  const router = useRouter();
  const [state, setState] = useState<JoinState>(createJoinState);

  function patch(next: Partial<JoinState>) {
    setState((current) => ({ ...current, ...next }));
  }

  function goHome() {
    router.push("/");
  }

  function goBack() {
    if (state.step <= 1) {
      goHome();
      return;
    }
    patch({ step: (state.step - 1) as JoinState["step"] });
  }

  function goNext() {
    // TODO: persist profile colour, payout destination, and signature via the split-card API
    patch({ step: (state.step + 1) as JoinState["step"] });
  }

  if (state.step === 1) {
    return (
      <JoinLayout
        showLogo
        title={
          <>
            {DEMO_INVITE.inviter} Invited You
            <br />
            To Sign A Split Card
          </>
        }
        subtitle={
          <>
            Read the card carefully.{" "}
            <span className="font-semibold text-ink">
              Nothing is final until you sign.
            </span>
          </>
        }
        sheetNote={
          <>
            Once everyone has signed, this card is locked and
            <br />
            can&apos;t be changed.
          </>
        }
        note={`If someone leaves before you submit, they keep ${DEMO_INVITE.leaveKeepPercent}% of the prize. The rest is split equally among the others.`}
        actions={
          <>
            <OutlineButton href="/">Decline</OutlineButton>
            <PrimaryButton onClick={goNext}>Continue</PrimaryButton>
          </>
        }
      >
        <StepInviteCard />
      </JoinLayout>
    );
  }

  if (state.step === 2) {
    return (
      <JoinLayout
        title={
          <>
            Set Up Your
            <br />
            Safehand Profile
          </>
        }
        subtitle={
          <>
            Your teammates will see your name
            <br />
            and colour.
          </>
        }
        figure={
          <Stickman
            src="/dark-stickman/standing-dark.svg"
            width={73}
            height={177}
            className="h-[8.35rem] w-auto"
          />
        }
        figureClassName="pointer-events-none absolute top-2 right-0"
        stepLabel="Step: 1 of 2"
        stepPill="Basic Info"
        progress={50}
        onBack={goBack}
        terms={TERMS_FOOTER}
        actionsAlign="end"
        actions={<OutlineButton onClick={goNext}>Continue</OutlineButton>}
      >
        <StepProfile
          avatarColor={state.avatarColor}
          shareDestination={state.shareDestination}
          onColorChange={(avatarColor) => patch({ avatarColor })}
          onDestinationChange={(shareDestination) => patch({ shareDestination })}
        />
      </JoinLayout>
    );
  }

  if (state.step === 3) {
    return (
      <JoinLayout
        title="Sign Split Card"
        subtitle="By signing, you agree to these three things."
        figure={
          <Stickman
            src="/dark-stickman/standing-dark.svg"
            width={73}
            height={177}
            className="h-[5.15rem] w-auto"
          />
        }
        figureSize="sm"
        figureClassName="pointer-events-none absolute top-4 right-0"
        stepLabel="Step: 2 of 2"
        stepPill="Signature"
        progress={100}
        onBack={goBack}
        terms={TERMS_FOOTER}
        actionsAlign="end"
        actions={<PrimaryButton onClick={goNext}>Sign Card</PrimaryButton>}
      >
        <StepSign shareDestination={state.shareDestination} />
      </JoinLayout>
    );
  }

  return (
    <JoinLayout
      showLogo
      title="You've Signed"
      subtitle="Your signature is saved on the card."
      figure={
        <Stickman
          src="/dark-stickman/handsup-dark.svg"
          width={108}
          height={173}
          className="h-[8rem] w-auto"
        />
      }
      figureClassName="pointer-events-none absolute -top-7 right-0"
      note="We'll email you when everyone has signed."
      actionsAlign="end"
      actions={<PrimaryButton href="/">Go Home</PrimaryButton>}
    >
      <StepSigned avatarColor={state.avatarColor} />
    </JoinLayout>
  );
}
