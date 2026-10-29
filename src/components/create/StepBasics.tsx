import { Field } from "./WizardLayout";
import type { WizardState } from "./types";

export function StepBasics({
  state,
  onChange,
}: {
  state: WizardState;
  onChange: (patch: Partial<WizardState>) => void;
}) {
  return (
    <div className="flex flex-col gap-5">
      <Field
        label="Your name"
        value={state.yourName}
        onChange={(yourName) => onChange({ yourName })}
      />
      <Field
        label="Team name"
        value={state.teamName}
        onChange={(teamName) => onChange({ teamName })}
      />
      <Field
        label="Which hackathon is this for?"
        value={state.hackathon}
        onChange={(hackathon) => onChange({ hackathon })}
      />
    </div>
  );
}

export function basicsComplete(state: WizardState) {
  return (
    state.yourName.trim().length > 0 &&
    state.teamName.trim().length > 0 &&
    state.hackathon.trim().length > 0
  );
}

export function basicsCopy(state: WizardState) {
  if (basicsComplete(state)) {
    return {
      title: "Create A Team Split Card",
      subtitle: "Takes about two minutes to complete fill form",
      note: "The card locks once all 3 have signed. Until then it is saved as a draft.",
    };
  }

  return {
    title: "Create Split Card",
    subtitle: "This takes about two minutes.",
    note: "Once everyone has signed, the card is locked and can't be changed. Until then, it's saved as a draft.",
  };
}
