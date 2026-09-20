const STORAGE_KEY = "oe_recent_searches";
const MAX_ITEMS = 5;

export function getRecentSearches(): string[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    const saved: unknown = raw ? JSON.parse(raw) : [];
    return Array.isArray(saved) ? saved.filter((value): value is string => typeof value === "string") : [];
  } catch {
    return [];
  }
}

export function addRecentSearch(query: string): void {
  const q = query.trim();
  if (!q) return;
  const list = getRecentSearches().filter((s) => s !== q);
  list.unshift(q);
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(list.slice(0, MAX_ITEMS))); }
  catch { /* Search does not depend on browser persistence. */ }
}

export function clearRecentSearches(): void {
  try { localStorage.removeItem(STORAGE_KEY); }
  catch { /* Storage is optional. */ }
}
