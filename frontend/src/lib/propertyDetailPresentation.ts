import type { DecisionLabel, ExternalReviewCard, PropertyCard } from "./types.ts";

const REVIEW_DATE_FORMAT = new Intl.DateTimeFormat("en-IN", {
  day: "numeric",
  month: "short",
  year: "numeric",
  timeZone: "UTC",
});

export const PROPERTY_REVIEW_EXCERPT_LIMIT = 3;

type SocietyIdentity = Pick<
  PropertyCard,
  "id" | "kg_entity_refs" | "society_name"
>;

export function propertyDisplayName(
  title: string,
  societyName: string | undefined,
): string {
  const society = societyName?.trim();
  if (society) return society;
  return title.replace(/^\s*\d+(?:\.\d+)?\s*BHK\s+(?:in|at)\s+/i, "").trim();
}

export function decisionSummaryLabels(
  labels: readonly DecisionLabel[] | undefined,
): DecisionLabel[] {
  return (labels ?? []).filter((label) => label.label.trim().length > 0);
}

export function formatReviewDate(value: string | undefined): string | null {
  const normalized = value?.trim();
  if (!normalized) return null;
  if (!/^\d{4}-\d{2}-\d{2}(?:T.*)?$/.test(normalized)) return normalized;

  const timestamp = Date.parse(normalized);
  return Number.isFinite(timestamp)
    ? REVIEW_DATE_FORMAT.format(new Date(timestamp))
    : normalized;
}

export function reviewExcerpts(
  reviews: readonly ExternalReviewCard[] | undefined,
): ExternalReviewCard[] {
  return (reviews ?? [])
    .filter((review) => review.text.trim().length > 0)
    .slice(0, PROPERTY_REVIEW_EXCERPT_LIMIT);
}

export function societyIdentityKey(property: SocietyIdentity): string {
  const societyId = property.kg_entity_refs?.society_entity_id?.trim();
  if (societyId) return `society:${societyId}`;

  const societyName = property.society_name.trim().toLocaleLowerCase("en-IN");
  return societyName ? `society-name:${societyName}` : `property:${property.id}`;
}

export function uniqueBySociety<T>(
  items: readonly T[],
  propertyFor: (item: T) => SocietyIdentity,
): T[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    const key = societyIdentityKey(propertyFor(item));
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}
