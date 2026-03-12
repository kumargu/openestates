const STORAGE_KEY = "oe_recent_searches";
const MAX_ITEMS = 5;

export function getRecentSearches(): string[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

export function addRecentSearch(query: string): void {
  const q = query.trim();
  if (!q) return;
  const list = getRecentSearches().filter((s) => s !== q);
  list.unshift(q);
  localStorage.setItem(STORAGE_KEY, JSON.stringify(list.slice(0, MAX_ITEMS)));
}

export function clearRecentSearches(): void {
  localStorage.removeItem(STORAGE_KEY);
}
