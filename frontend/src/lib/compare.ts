export const SHORTLIST_STORAGE_KEY = "openestates:workspace-home-ids";
export const FOCUS_STORAGE_KEY = "openestates:workspace-focused-home";
export const MAX_SHORTLIST_HOMES = 10;
export const SHORTLIST_CHANGED_EVENT = "openestates:shortlist-changed";

export function completeSettledValues<T>(
  results: PromiseSettledResult<T>[],
  expectedCount: number,
): T[] | null {
  const values = results.flatMap((result) =>
    result.status === "fulfilled" ? [result.value] : []
  );
  return values.length === expectedCount ? values : null;
}

export function parseShortlistIds(value: string | null): string[] {
  if (!value) return [];
  return [...new Set(value.split(",").map((id) => id.trim()).filter(Boolean))]
    .slice(0, MAX_SHORTLIST_HOMES);
}

export function readShortlistIds(): string[] {
  return parseShortlistIds(window.localStorage.getItem(SHORTLIST_STORAGE_KEY));
}

export function writeShortlistIds(ids: string[]): string[] {
  const next = [...new Set(ids.map((id) => id.trim()).filter(Boolean))]
    .slice(0, MAX_SHORTLIST_HOMES);
  const nextValue = next.join(",");
  if (window.localStorage.getItem(SHORTLIST_STORAGE_KEY) === nextValue) {
    return next;
  }
  window.localStorage.setItem(SHORTLIST_STORAGE_KEY, nextValue);
  window.dispatchEvent(new CustomEvent(SHORTLIST_CHANGED_EVENT, { detail: next }));
  return next;
}

export function isShortlisted(propertyId: string, ids = readShortlistIds()): boolean {
  return ids.includes(propertyId);
}

export function toggleShortlistId(propertyId: string): string[] {
  const current = readShortlistIds();
  if (current.includes(propertyId)) {
    return writeShortlistIds(current.filter((id) => id !== propertyId));
  }
  return writeShortlistIds([propertyId, ...current]);
}
