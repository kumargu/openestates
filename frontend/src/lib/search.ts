/**
 * Search display utilities.
 * Note: All search logic (intent parsing, filtering, scoring) now lives in the backend.
 * This file only contains display formatting helpers.
 */

import type { MatchReason, SearchResultItem } from "./types.ts";

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

export function searchResultReasonLabels(
  result: Pick<
    SearchResultItem,
    "match_reason" | "match_explanation" | "title" | "society_name" | "builder_name"
  >,
): string[] {
  const labels: string[] = [];

  for (const reason of result.match_explanation?.reasons ?? []) {
    pushUniqueLabel(labels, compactExplanationLabel(reason));
    if (labels.length >= 2) return labels;
  }

  const displayReason = displayMatchReason(result.match_reason);
  for (const part of displayReason?.split(/\s*[;·,]\s*/) ?? []) {
    if (isNameOnlyReason(part, result)) continue;
    pushUniqueLabel(labels, compactReasonPart(part));
    if (labels.length >= 2) return labels;
  }

  return labels;
}

export function friendlyMatchLabel(label: string): string {
  const normalized = label.trim().toLowerCase();
  if (normalized === "weak match") return "Partial fit";
  if (normalized === "partial match") return "Partial fit";
  return label;
}

function compactExplanationLabel(reason: MatchReason): string | null {
  if (reason.score <= 0) return null;
  const preference = reason.preference.trim();
  if (!preference) return null;
  return compactLabel(preference);
}

function compactReasonPart(part: string): string | null {
  const trimmed = part.trim();
  if (!trimmed) return null;
  return compactLabel(trimmed.replace(/^matches\s+/i, ""));
}

function compactLabel(value: string): string {
  const cleaned = value
    .replace(/^avoid\s+/i, "Avoid ")
    .replace(/\s+/g, " ")
    .trim();
  const words = cleaned.split(" ");
  const short = words.length > 4 ? words.slice(0, 4).join(" ") : cleaned;
  return short.replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function pushUniqueLabel(labels: string[], label: string | null) {
  if (!label) return;
  if (labels.some((existing) => existing.toLowerCase() === label.toLowerCase())) return;
  labels.push(label);
}

function isNameOnlyReason(
  reason: string,
  result: Pick<SearchResultItem, "title" | "society_name" | "builder_name">,
): boolean {
  const match = reason.match(/^matched\s+'([^']+)'\s+in\s+(title|society|builder)$/i);
  if (!match) return false;
  const token = match[1]?.toLowerCase() ?? "";
  const field = match[2]?.toLowerCase();
  const value = field === "builder"
    ? result.builder_name
    : field === "society"
      ? result.society_name
      : result.title;
  return value.toLowerCase().includes(token);
}
