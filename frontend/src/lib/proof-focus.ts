import type { ProofFocus, SearchResultItem } from "./types.ts";

type ProofFocusSource = Pick<
  SearchResultItem,
  "proof_focuses" | "match_reason" | "match_explanation"
>;

export function primaryProofFocus(
  result: ProofFocusSource,
  query?: string,
): ProofFocus | undefined {
  const focuses = result.proof_focuses ?? [];
  if (focuses.length === 0) return undefined;
  if (focuses.length === 1) return focuses[0];

  const queryHaystack = normalizeHaystack(query);
  const reasonHaystack = normalizeHaystack(
    [
      result.match_reason,
      ...(result.match_explanation?.reasons.map((reason) => reason.display) ?? []),
    ]
      .filter((value): value is string => Boolean(value?.trim()))
      .join(" "),
  );

  let best = focuses[0];
  let bestScore = Number.NEGATIVE_INFINITY;
  for (const focus of focuses) {
    const score = scoreProofFocus(focus, queryHaystack, reasonHaystack);
    if (score > bestScore) {
      best = focus;
      bestScore = score;
    }
  }
  return best;
}

function scoreProofFocus(
  focus: ProofFocus,
  queryHaystack: string,
  reasonHaystack: string,
): number {
  let score = 0;
  const label = normalizeHaystack(focus.matchedLabel);
  const constraint = normalizeHaystack(focus.requestedConstraint);
  const layer = normalizeHaystack(focus.layerId.replace(/[_-]+/g, " "));
  const factToken = normalizeHaystack(
    focus.factKey.replace(/^nearby[_-]?/i, "").replace(/[_-]+/g, " "),
  );

  if (label && queryHaystack.includes(label)) score += 200;
  if (constraint && queryHaystack.includes(constraint)) score += 160;
  if (layer && haystackHasToken(queryHaystack, layer)) score += 120;
  if (factToken && haystackHasToken(queryHaystack, factToken)) score += 100;
  if (label && reasonHaystack.includes(label)) score += 40;
  if (constraint && reasonHaystack.includes(constraint)) score += 30;
  if (focus.entityId) score += 10;
  if (label && queryHaystack.includes(label)) {
    score += Math.min(label.length, 20);
  }
  return score;
}

function normalizeHaystack(value: string | null | undefined): string {
  return (value ?? "").trim().toLocaleLowerCase("en-IN");
}

function haystackHasToken(haystack: string, token: string): boolean {
  if (!haystack || !token) return false;
  if (haystack.includes(token)) return true;
  const first = token.split(/\s+/)[0];
  if (!first || first.length < 4) return false;
  return new RegExp(`\\b${escapeRegExp(first)}\\b`, "i").test(haystack);
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
