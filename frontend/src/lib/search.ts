/**
 * Search display utilities.
 * Note: All search logic (intent parsing, filtering, scoring) now lives in the backend.
 * This file only contains display formatting helpers.
 */

export type MatchLabel =
  | "Strong match"
  | "Good match"
  | "Value pick"
  | "Premium match"
  | "Candidate"
  | "Similar profile";

export interface MatchResult {
  label: MatchLabel;
  reason: string;
}

/** Format a search summary for display from backend-parsed intent. */
export function formatSearchSummary(intent: {
  query: string;
  area?: string;
  bhk?: number;
  budgetMax?: number;
  preferences: string[];
}): string {
  const parts: string[] = [];
  if (intent.area) parts.push(intent.area);
  if (intent.bhk) parts.push(`${intent.bhk} BHK`);
  if (intent.budgetMax) {
    const cr = intent.budgetMax / 10_000_000;
    parts.push(cr >= 1 ? `under ${cr} Cr` : `under ${(intent.budgetMax / 100_000).toFixed(0)}L`);
  }
  parts.push(...intent.preferences);

  if (parts.length === 0) return "Showing all properties ranked by transparency signals";
  return `Ranking based on ${parts.join(", ")}`;
}
