const STORAGE_KEY = "openestates_shortlist";

export function getShortlistedIds(): string[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

export function isShortlisted(id: string): boolean {
  return getShortlistedIds().includes(id);
}

export function toggleShortlist(id: string): boolean {
  const ids = getShortlistedIds();
  const index = ids.indexOf(id);
  if (index >= 0) {
    ids.splice(index, 1);
    localStorage.setItem(STORAGE_KEY, JSON.stringify(ids));
    return false; // removed
  } else {
    ids.push(id);
    localStorage.setItem(STORAGE_KEY, JSON.stringify(ids));
    return true; // added
  }
}
