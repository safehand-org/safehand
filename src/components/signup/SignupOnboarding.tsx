import { SecondaryButton } from "@/components/create/WizardLayout";
import { emptyHomeData } from "@/components/home/types";
import { HomeView } from "@/components/home/HomeView";
import { Stickman } from "./Stickman";
import { ONBOARDING_SLIDES, type OnboardingSlide } from "./types";

export function SignupOnboarding({
  slide,
  onSkip,
  onNext,
  onDone,
}: {
  slide: OnboardingSlide;
  onSkip: () => void;
  onNext: () => void;
  onDone: () => void;
}) {
  const current = ONBOARDING_SLIDES[slide - 1];
  const isLast = slide === 3;

  return (
    <div className="relative min-h-full overflow-hidden bg-background">
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-0 origin-top scale-[1.02] blur-[10px] brightness-[0.55]"
      >
        <HomeView data={emptyHomeData} />
      </div>

      <div className="relative z-[1] flex min-h-full flex-col justify-end px-5 pt-16 pb-8">
        <article className="overflow-hidden rounded-[1.65rem] bg-[#141312] ring-1 ring-white/6">
          <div className="relative flex min-h-[20.5rem] items-center justify-center">
            <p className="pointer-events-none absolute inset-x-0 top-3 text-center text-[7.5rem] leading-none font-semibold tracking-[-0.08em] text-white/[0.14]">
              {current.number}
            </p>
            <Stickman
              src={current.stickman}
              width={current.stickmanSize.width}
              height={current.stickmanSize.height}
              className="relative z-[1] mt-8 h-[9.75rem] w-auto"
            />
          </div>

          <div className="bg-[#1c1a18] px-5 pt-4 pb-2">
            <h2 className="text-[1.35rem] leading-none font-semibold tracking-[-0.03em] text-sheet">
              {current.title}
            </h2>
          </div>
          <div className="px-5 pt-3 pb-5">
            <p className="text-[13.5px] leading-5 text-muted">{current.body}</p>
          </div>
        </article>

        {isLast ? (
          <button
            type="button"
            onClick={onDone}
            className="mt-6 inline-flex h-12 w-full items-center justify-center rounded-full bg-white text-[16px] font-medium text-ink transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/70 active:bg-neutral-200"
          >
            Let&apos;s Go!
          </button>
        ) : (
          <div className="mt-6 flex items-center justify-between gap-3">
            <button
              type="button"
              onClick={onSkip}
              className="inline-flex h-12 items-center justify-center rounded-full border border-hairline px-8 text-[16px] font-medium text-sheet focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/30"
            >
              Skip
            </button>
            <SecondaryButton onClick={onNext}>Next</SecondaryButton>
          </div>
        )}
      </div>
    </div>
  );
}
