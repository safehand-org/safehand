"use client";

import { useRouter } from "next/navigation";
import { useEffect, useMemo, useState } from "react";
import { createDeps } from "@/lib/cardActions";
import {
  createAndSign,
  CreateStepError,
  NO_PROGRESS,
  type CreateProgress,
} from "@/lib/createCard";
import { errorMessage } from "@/lib/errors";
import { weightsFor } from "@/lib/shares";
import { cardLabel } from "@/lib/teammates";
import { useMe } from "@/lib/useMe";
import { loadWasm } from "@/lib/wasm";
import { StepBasics, basicsComplete, basicsCopy } from "./StepBasics";
import { leaveSummary, StepLeave } from "./StepLeave";
import { StepReview, type RunState } from "./StepReview";
import { sharesCopy, StepShares } from "./StepShares";
import { StepTeammates, TEAMMATES_COPY } from "./StepTeammates";
import { createInitialState, keepPercent, membersOf, type WizardState } from "./types";
import { PrimaryButton, WizardLayout } from "./WizardLayout";

const LOCK_NOTE =
  "Once everyone has signed, the card is locked and can't be changed. Until then, it's saved as a draft.";

export function CreateView() {
  const router = useRouter();
  const account = useMe();
  const me = account.status === "ready" ? account.me : null;
  const [state, setState] = useState<WizardState>(createInitialState);
  const [prefilled, setPrefilled] = useState(false);
  const [basisPoints, setBasisPoints] = useState<number[] | null>(null);
  const [progress, setProgress] = useState<CreateProgress>(NO_PROGRESS);
  const [run, setRun] = useState<RunState | null>(null);

  const ownPubkey = me?.pubkey ?? null;
  const members = useMemo(() => membersOf(state, ownPubkey ?? ""), [state, ownPubkey]);
  const shares = weightsFor(
    state.shareMode,
    members.map((member) => state.customShares[member.pubkey] ?? ""),
  );
  const weights = "weights" in shares ? shares.weights : null;
  const weightsKey = weights?.join(",") ?? "";

  // Start from the profile's name and payout address.
  useEffect(() => {
    if (!me || prefilled) return;
    // Prefill once, after `GET /me` arrives.
    setState((current) => ({
      ...current,
      yourName: current.yourName || me.profile?.name || "",
      lightningAddress: current.lightningAddress || me.profile?.lightning_address || "",
    }));
    setPrefilled(true);
  }, [me, prefilled]);

  // The percentages shown come from splitcard-core, over the card's weights.
  useEffect(() => {
    let current = true;
    if (!ownPubkey || !weights || members.length < 2) {
      setBasisPoints(null);
      return;
    }
    void loadWasm().then((wasm) => {
      try {
        const preview = wasm.new_card({
          card_id: "preview",
          event: "preview",
          team: "preview",
          members: members.map((member, index) => ({
            pubkey: member.pubkey,
            weight: weights[index],
          })),
        });
        if (current) setBasisPoints(wasm.display_basis_points(preview));
      } catch {
        if (current) setBasisPoints(null);
      }
    });
    return () => {
      current = false;
    };
    // `weightsKey` stands for `weights`, which is a new array each render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ownPubkey, members, weightsKey]);

  function patch(next: Partial<WizardState>) {
    setState((current) => ({ ...current, ...next }));
  }

  function goBack() {
    if (state.step === 1 || progress.team) {
      // Once the team exists, the draft lives on the cards tab.
      router.push(progress.team ? "/cards" : "/");
      return;
    }
    patch({ step: (state.step - 1) as WizardState["step"] });
  }

  function goNext() {
    patch({ step: (state.step + 1) as WizardState["step"] });
  }

  async function signAndCreate() {
    if (!weights || !ownPubkey) return;
    const keep = state.leavePolicy === "smaller" ? keepPercent(state) : undefined;
    if (keep === null) return;
    setRun({ done: progress.done, current: null, error: null });
    try {
      const deps = await createDeps();
      const finished = await createAndSign(
        {
          name: me?.profile?.name ?? cardLabel(state.yourName),
          teamName: state.teamName.trim(),
          event: state.hackathon.trim(),
          lightningAddress: state.lightningAddress.trim(),
          members: members.map((member, index) => ({
            pubkey: member.pubkey,
            weight: weights[index],
            ...(member.name ? { label: cardLabel(member.name) } : {}),
          })),
          ...(keep === undefined ? {} : { leaveKeepPercent: keep }),
        },
        progress,
        deps,
        (step) =>
          setRun((current) => ({
            done: current?.current ? [...current.done, current.current] : (current?.done ?? []),
            current: step,
            error: null,
          })),
      );
      setProgress(finished);
      setRun({ done: finished.done, current: null, error: null });
      if (finished.team) router.push(`/card/${encodeURIComponent(finished.team.slug)}`);
    } catch (error) {
      if (error instanceof CreateStepError) {
        setProgress(error.progress);
        setRun({ done: error.progress.done, current: error.step, error: errorMessage(error.reason) });
      } else {
        setRun((current) => ({
          done: current?.done ?? [],
          current: current?.current ?? null,
          error: errorMessage(error),
        }));
      }
    }
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
        subtitle={TEAMMATES_COPY}
        note="Teammates don't need an account yet. They sign in with the key behind their npub."
        onBack={goBack}
        actions={
          <PrimaryButton disabled={state.teammates.length < 1 || !ownPubkey} onClick={goNext}>
            Continue
          </PrimaryButton>
        }
      >
        <StepTeammates
          state={state}
          ownPubkey={ownPubkey}
          onAdd={(teammate) => patch({ teammates: [...state.teammates, teammate] })}
          onRemove={(pubkey) =>
            patch({ teammates: state.teammates.filter((item) => item.pubkey !== pubkey) })
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
            className={state.shareMode === "custom" ? "h-[3px] bg-ink" : "h-[3px] bg-[#d8d8d8]"}
          />
        }
        actions={
          <PrimaryButton disabled={!weights} onClick={goNext}>
            Continue
          </PrimaryButton>
        }
      >
        <StepShares state={state} members={members} basisPoints={basisPoints} onChange={patch} />
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
          <PrimaryButton
            disabled={state.leavePolicy === "smaller" && keepPercent(state) === null}
            onClick={goNext}
          >
            Continue
          </PrimaryButton>
        }
      >
        <StepLeave state={state} onChange={patch} />
      </WizardLayout>
    );
  }

  const running = run !== null && run.error === null;
  return (
    <WizardLayout
      step={5}
      title="Review And Sign"
      subtitle="Check everything. Once you sign, your teammates will be asked to sign too."
      note="Once everyone has signed, this card is locked and can't be changed."
      extraNote={leaveSummary(state)}
      onBack={goBack}
      actions={
        <PrimaryButton
          disabled={running || !weights || state.lightningAddress.trim().length === 0}
          onClick={() => void signAndCreate()}
        >
          {running ? "Signing…" : run?.error ? "Try Again" : "Sign with my Nostr key"}
        </PrimaryButton>
      }
    >
      <StepReview
        state={state}
        members={members}
        basisPoints={basisPoints}
        run={run}
        onLightningAddressChange={(lightningAddress) => patch({ lightningAddress })}
      />
    </WizardLayout>
  );
}
