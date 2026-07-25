/**
 * Search display utilities.
 * Note: All search logic (intent parsing, filtering, scoring) now lives in the backend.
 * This file only contains display formatting helpers.
 */

export type MatchLabel =
  | "Strong match"
  | "Good match"
  | "Partial match"
  | "Partial fit"
  | "Value pick"
  | "Premium match"
  | "Candidate"
  | "Similar profile";

export interface MatchResult {
  label: MatchLabel;
  reason: string;
}

/** Avoid repeating parsed search filters on every result card. */
export function isGenericFilterReason(reason: string): boolean {
  const distinctive = extractDistinctiveMatchParts(reason);
  return distinctive.length === 0;
}

function extractDistinctiveMatchParts(reason: string): string[] {
  const trimmed = reason.trim();
  if (!trimmed) return [];
  if (/^near\s+/i.test(trimmed) && !/^matches\s+/i.test(trimmed)) {
    return [trimmed];
  }
  if (!/^matches\s+/i.test(trimmed)) {
    return [trimmed];
  }

  const preferenceHints = /greenery|metro|quiet|premium|family|school|traffic|ready|verified|proof|society|builder|value|maintenance|commute|noise|water/i;

  return trimmed
    .replace(/^matches\s+/i, "")
    .split(/,\s*/)
    .map((part) => part.trim())
    .filter(Boolean)
    .filter((segment) => {
      if (/^\d+\s*bhk$/i.test(segment)) return false;
      if (/^under\s+/i.test(segment)) return false;
      if (/^avoid\s+/i.test(segment)) return true;
      if (preferenceHints.test(segment)) return true;
      if (segment.split(/\s+/).length <= 3) return false;
      return true;
    });
}

export function displayMatchReason(
  reason: string | undefined,
  fallback?: string | null,
): string | null {
  if (!reason?.trim()) return fallback ?? null;
  const distinctive = extractDistinctiveMatchParts(reason);
  if (distinctive.length > 0) return distinctive.join(" · ");
  return fallback ?? null;
}

export function friendlyMatchLabel(label: string): string {
  const normalized = label.trim().toLowerCase();
  if (normalized === "weak match") return "Partial fit";
  if (normalized === "partial match") return "Partial fit";
  return label;
}
