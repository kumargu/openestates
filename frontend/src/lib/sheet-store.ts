/**
 * Saved homes — a plain list of property IDs persisted in localStorage.
 * Kept intentionally minimal: saving is a lightweight bookmark. Any richer
 * "sheet" (tags, notes, comparison) will live in its own surface later.
 */
const STORAGE_KEY = "openestates_shortlist";
export const SAVED_UPDATED_EVENT = "oe-saved-changed";

function readIds(): string[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];

    const parsed = JSON.parse(raw);

    // Current format: a plain array of IDs.
    if (Array.isArray(parsed)) {
      return parsed.filter((id): id is string => typeof id === "string" && id.length > 0);
    }

    // Legacy format: { items: [{ id, tag, note, ... }] }. Migrate to IDs.
    if (parsed && Array.isArray(parsed.items)) {
      return parsed.items
        .map((item: unknown) =>
          item && typeof item === "object" ? (item as { id?: unknown }).id : item,
        )
        .filter((id: unknown): id is string => typeof id === "string" && id.length > 0);
    }
  } catch {
    return [];
  }

  return [];
}

function writeIds(ids: string[]): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(ids));
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent(SAVED_UPDATED_EVENT));
  }
}

export function getSavedIds(): string[] {
  return readIds();
}

export function isSaved(id: string): boolean {
  return readIds().includes(id);
}

export function removeSaved(id: string): void {
  const ids = readIds();
  if (!ids.includes(id)) return;
  writeIds(ids.filter((existing) => existing !== id));
}

/** Toggle a home's saved state. Returns the new state (true = now saved). */
export function toggleSaved(id: string): boolean {
  const ids = readIds();
  if (ids.includes(id)) {
    writeIds(ids.filter((existing) => existing !== id));
    return false;
  }
  writeIds([...ids, id]);
  return true;
}
