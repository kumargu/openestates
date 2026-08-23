/**
 * Search display utilities.
 * Note: All search logic (intent parsing, filtering, scoring) now lives in the backend.
 * This file only contains display formatting helpers.
 */

import type { MatchReason, SearchResultItem, SearchSourceSpan } from "./types.ts";

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

export function searchResultsAnnouncement(
  query: string,
  eligibleCount: number,
  returnedCount: number,
  guidanceMode?: string | null,
): string {
  if (returnedCount > eligibleCount) {
    const resultKind = guidanceMode === "named_society_alternatives"
      ? "alternatives"
      : "broader matches";
    return `No exact matches for "${query}". Showing ${returnedCount} ${resultKind}.`;
  }
  if (returnedCount < eligibleCount) {
    return `Showing ${returnedCount} of ${eligibleCount} eligible properties for "${query}".`;
  }
  return `${eligibleCount} ${eligibleCount === 1 ? "property" : "properties"} found for "${query}".`;
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

  return splitLabelParts(trimmed.replace(/^matches\s+/i, ""))
    .map((part) => part.trim())
    .filter(Boolean)
    .filter((segment) => {
      if (isRequestedBhkClause(segment)) return false;
      if (/^under\s+/i.test(segment)) return false;
      if (/^avoid\s+/i.test(segment)) return true;
      if (preferenceHints.test(segment)) return true;
      if (segment.split(/\s+/).length <= 3) return false;
      return true;
    });
}

function isRequestedBhkClause(value: string): boolean {
  return /^\d+(?:\s+or\s+\d+)*\s*bhk$/i.test(value.trim());
}

/** Drop every include/exclude BHK mention, keeping the rest of the sentence. */
export function queryWithoutBhkClause(query: string, spans: SearchSourceSpan[]): string {
  const withoutBhks = removeUtf8ByteSpans(query, spans);
  return withoutBhks
    .replace(/\s+,/g, ",")
    .replace(/,\s*,/g, ",")
    .replace(/^[,;\s]+|[,;\s]+$/g, "")
    .replace(/\s{2,}/g, " ")
    .trim();
}

function removeUtf8ByteSpans(query: string, spans: SearchSourceSpan[]): string {
  const bytes = new TextEncoder().encode(query);
  const decoder = new TextDecoder();
  let result = query;
  const uniqueSpans = [...spans]
    .filter((span) => span.end > span.start)
    .sort((left, right) => left.start - right.start)
    .filter((span, index, all) => (
      index === 0
      || span.start !== all[index - 1].start
      || span.end !== all[index - 1].end
    ));
  for (const span of uniqueSpans.sort((left, right) => right.start - left.start)) {
    const start = decoder.decode(bytes.slice(0, span.start)).length;
    const end = decoder.decode(bytes.slice(0, span.end)).length;
    result = result.slice(0, start) + result.slice(end);
  }
  return result;
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
    "match_reason" | "match_explanation" | "title" | "society_name" | "builder_name" | "area"
  >,
): string[] {
  const labels: string[] = [];

  for (const reason of result.match_explanation?.reasons ?? []) {
    const label = compactExplanationLabel(reason);
    if (isAreaRestatingLabel(label, result)) continue;
    pushUniqueLabel(labels, label);
    if (labels.length >= 2) return labels;
  }

  const displayReason = displayMatchReason(result.match_reason);
  for (const part of splitLabelParts(displayReason ?? "")) {
    if (isNameOnlyReason(part, result)) continue;
    const label = compactReasonPart(part);
    if (isAreaRestatingLabel(label, result)) continue;
    pushUniqueLabel(labels, label);
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
  const display = reason.display.replace(/\s+/g, " ").trim();
  if (display) return truncateCompactDisplay(display);
  const preference = reason.preference.trim();
  if (!preference) return null;
  return compactLabel(preference);
}

function truncateCompactDisplay(value: string, maxChars = 64): string {
  if (value.length <= maxChars) return value;
  return `${value.slice(0, maxChars - 1).trimEnd()}…`;
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
  const withoutBrokenParen = collapsePlaceParenthetical(cleaned);
  const words = withoutBrokenParen.split(" ");
  const short = words.length > 4 && !withoutBrokenParen.includes("(")
    ? words.slice(0, 4).join(" ")
    : withoutBrokenParen;
  return short.replace(/\b\w/g, (letter) => letter.toUpperCase());
}

/** Keep "Near Whitefield (ITPL, Whitefield)" as one phrase. */
export function splitLabelParts(value: string): string[] {
  const parts: string[] = [];
  let current = "";
  let depth = 0;
  for (const char of value) {
    if (char === "(") depth += 1;
    if (char === ")") depth = Math.max(0, depth - 1);
    if (depth === 0 && /[;·,]/.test(char)) {
      if (current.trim()) parts.push(current.trim());
      current = "";
      continue;
    }
    current += char;
  }
  if (current.trim()) parts.push(current.trim());
  return parts;
}

function collapsePlaceParenthetical(value: string): string {
  return value.replace(/\s*\(([^)]*)\)\s*$/, (full, inner: string) => {
    const tokens = inner.split(/,\s*/).map((part) => part.trim()).filter(Boolean);
    if (tokens.length === 0) return "";
    const head = value.slice(0, value.length - full.length).trim();
    if (tokens.every((token) => head.toLowerCase().includes(token.toLowerCase()))) {
      return "";
    }
    return ` (${tokens[0]})`;
  });
}

function isAreaRestatingLabel(
  label: string | null,
  result: Pick<SearchResultItem, "area" | "title" | "society_name">,
): boolean {
  if (!label) return false;
  const tokens = label
    .toLowerCase()
    .replace(/^near\s+/i, "")
    .replace(/[()]/g, " ")
    .split(/[^a-z0-9]+/)
    .filter((token) => token.length >= 3);
  if (tokens.length === 0) return false;
  const haystack = [result.area, result.title, result.society_name]
    .join(" ")
    .toLowerCase();
  return tokens.every((token) => haystack.includes(token));
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
