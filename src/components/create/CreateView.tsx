"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";
import { PaperclipIcon } from "./icons";
import { StepBasics, basicsComplete, basicsCopy } from "./StepBasics";
import { StepInvite } from "./StepInvite";
import { leaveSummary, StepLeave } from "./StepLeave";
import { StepReview } from "./StepReview";
import { sharesComplete, sharesCopy, StepShares } from "./StepShares";
import { addTeammate, StepTeammates } from "./StepTeammates";
import {
  CREATOR_ID,
  createInitialState,
  equalPercent,
  type WizardState,
} from "./types";
import { PrimaryButton, SecondaryButton, WizardLayout } from "./WizardLayout";

const LOCK_NOTE =
  "Once everyone has signed, the card is locked and can't be changed. Until then, it's saved as a draft.";

export function CreateView() {
  const router = useRouter();
  const [state, setState] = useState<WizardState>(createInitialState);
  const [email, setEmail] = useState("");

  function patch(next: Partial<WizardState>) {
    setState((current) => ({ ...current, ...next }));
  }

  function goBack() {
    if (state.step === 1) {
      router.push("/");
      return;
    }
    patch({ step: (state.step - 1) as WizardState["step"] });
  }

  function goNext() {
    patch({ step: (state.step + 1) as WizardState["step"] });
  }

  if (state.step === 1) {
    const copy = basicsCopy(state);
    return (
      <WizardLayout
        step={1}
        title={copy.title}
        subtitle={copy.subtitle}
        note={copy.note}
        onBack={goBack}
        backHref="/"
        actions={
          <PrimaryButton disabled={!basicsComplete(state)} onClick={goNext}>
            Continue
          </PrimaryButton>
        }
      >
        <StepBasics state={state} onChange={patch} />
      </WizardLayout>
    );
  }

  if (state.step === 2) {
    return (
      <WizardLayout
        step={2}
        title="Who Are You Building With?"
        subtitle="Add everyone on your team. They'll get an invite to read the card and sign it."
        note="Add everyone on your team. Each person gets an invite by email, so there's a record of it."
        onBack={goBack}
        actions={
          <PrimaryButton
            disabled={state.teammates.length < 1}
            onClick={() => {
              const people = [CREATOR_ID, ...state.teammates.map((item) => item.id)];
              const equal = equalPercent(people.length);
              patch({
                step: 3,
                customShares: Object.fromEntries(people.map((id) => [id, equal])),
              });
            }}
          >
            Continue
          </PrimaryButton>
        }
      >
        <StepTeammates
          state={state}
          email={email}
          onEmailChange={setEmail}
          onAdd={() => {
            const teammate = addTeammate(state, email);
            if (!teammate) return;
            patch({ teammates: [...state.teammates, teammate] });
            setEmail("");
          }}
          onRemove={(id) =>
            patch({ teammates: state.teammates.filter((item) => item.id !== id) })
          }
        />
      </WizardLayout>
    );
  }

  if (state.step === 3) {
    return (
      <WizardLayout
        step={3}
        title="How Should The Prize Be Shared?"
        subtitle={sharesCopy(state)}
        note={LOCK_NOTE}
        onBack={goBack}
        sheetFooter={
          <div
            className={
              state.shareMode === "custom" ? "h-[3px] bg-ink" : "h-[3px] bg-[#d8d8d8]"
            }
          />
        }
        actions={
          <PrimaryButton disabled={!sharesComplete(state)} onClick={goNext}>
            Continue
          </PrimaryButton>
        }
      >
        <StepShares state={state} onChange={patch} />
      </WizardLayout>
    );
  }

  if (state.step === 4) {
    return (
      <WizardLayout
        step={4}
        title="What If Someone Leaves?"
        subtitle="Decide now what a teammate gets if they leave before you submit."
        note="Once the card is locked, any change needs everyone to agree."
        onBack={goBack}
        actions={
          <>
            <SecondaryButton href="/cards">Save As Draft</SecondaryButton>
            <PrimaryButton onClick={goNext}>Sign Card</PrimaryButton>
          </>
        }
      >
        <StepLeave state={state} onChange={patch} />
      </WizardLayout>
    );
  }

  if (state.step === 5) {
    return (
      <WizardLayout
        step={5}
        title="Review Your Split Card"
        subtitle="Check everything. Your teammates will be invited to sign this."
        note="Once everyone has signed, this card is locked and can't be changed."
        extraNote={leaveSummary(state)}
        onBack={goBack}
        actions={
          <>
            <SecondaryButton href="/cards">
              <PaperclipIcon />
              Save Draft
            </SecondaryButton>
            <PrimaryButton onClick={goNext}>Send Invites</PrimaryButton>
          </>
        }
      >
        <StepReview state={state} />
      </WizardLayout>
    );
  }

  return (
    <WizardLayout
      step={6}
      title="Invite Your Team"
      subtitle="We've emailed your teammates. You can also share the link or code yourself."
      note="Only the people you invited can sign. They'll sign in with the email you added."
      onBack={goBack}
      actions={<PrimaryButton href="/cards">Go To My Cards</PrimaryButton>}
    >
      <StepInvite state={state} />
    </WizardLayout>
  );
}
