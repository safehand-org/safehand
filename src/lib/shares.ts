// Turns the wizard's share choices into card weights. Weights are whole
// numbers: 1 each for an equal split, or tenths of a percent for a custom
// one, which must total exactly 100.0%. The percentages shown come from
// splitcard-core (`display_basis_points`), not from arithmetic here.

/** 100.0% in tenths of a percent. */
export const WHOLE_TENTHS = 1000;

/** A typed percent with at most one decimal, in tenths; null if it is not one. */
export function percentToTenths(text: string): number | null {
  const match = /^(\d{1,3})(?:\.(\d))?$/.exec(text.trim());
  if (!match) return null;
  const tenths = Number(match[1]) * 10 + Number(match[2] ?? "0");
  return tenths <= WHOLE_TENTHS ? tenths : null;
}

/** Tenths of a percent as typed text, such as `33.4`. */
export function tenthsToPercent(tenths: number): string {
  return `${Math.floor(tenths / 10)}.${tenths % 10}`;
}

/** Custom shares to start from: as equal as tenths allow, extras first. */
export function equalTenths(count: number): number[] {
  if (count < 1) return [];
  const base = Math.floor(WHOLE_TENTHS / count);
  const extra = WHOLE_TENTHS - base * count;
  return Array.from({ length: count }, (_, index) => base + (index < extra ? 1 : 0));
}

/** The total of typed custom shares in tenths, or null if any is not a percent. */
export function totalTenths(texts: string[]): number | null {
  let total = 0;
  for (const text of texts) {
    const tenths = percentToTenths(text);
    if (tenths === null) return null;
    total += tenths;
  }
  return total;
}

export type SharesProblem = "not_a_percent" | "zero_share" | "wrong_total";

export const SHARES_PROBLEM_MESSAGES: Record<SharesProblem, string> = {
  not_a_percent: "Enter each share as a percent with at most one decimal, like 33.3.",
  zero_share: "Every share must be more than 0%.",
  wrong_total: "The shares must add up to exactly 100.0%.",
};

/** Card weights for the members, in order, or the problem with the shares. */
export function weightsFor(
  mode: "equal" | "custom",
  texts: string[],
): { weights: number[] } | { problem: SharesProblem } {
  if (mode === "equal") return { weights: texts.map(() => 1) };
  const weights: number[] = [];
  for (const text of texts) {
    const tenths = percentToTenths(text);
    if (tenths === null) return { problem: "not_a_percent" };
    if (tenths === 0) return { problem: "zero_share" };
    weights.push(tenths);
  }
  const total = weights.reduce((sum, weight) => sum + weight, 0);
  return total === WHOLE_TENTHS ? { weights } : { problem: "wrong_total" };
}

/** Basis points from splitcard-core as a percent with one decimal: `33.3%`. */
export function formatBasisPoints(basisPoints: number): string {
  return `${tenthsToPercent(Math.floor((basisPoints + 5) / 10))}%`;
}
