import type { PropertyCard } from "./types.ts";

export const SHORTLIST_STORAGE_KEY = "openestates:workspace-home-ids";
export const FOCUS_STORAGE_KEY = "openestates:workspace-focused-home";
export const MAX_SHORTLIST_HOMES = 10;
export const SHORTLIST_CHANGED_EVENT = "openestates:shortlist-changed";

function societyIdentity(property: PropertyCard): string {
  return property.society_name?.trim().toLocaleLowerCase()
    || property.title.trim().toLocaleLowerCase();
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

export function defaultComparedHomes(
  properties: PropertyCard[],
  limit: number,
): PropertyCard[] {
  if (properties.length === 0 || limit <= 0) return [];

  const preferredBhk = properties[0].bhk;
  const selected: PropertyCard[] = [];
  const societyKeys = new Set<string>();

  function addDistinct(property: PropertyCard) {
    const key = societyIdentity(property);
    if (societyKeys.has(key) || selected.length >= limit) return;
    societyKeys.add(key);
    selected.push(property);
  }

  properties
    .filter((property) => property.bhk === preferredBhk)
    .forEach(addDistinct);
  properties.forEach(addDistinct);

  if (selected.length < limit) {
    for (const property of properties) {
      if (selected.some((home) => home.id === property.id)) continue;
      selected.push(property);
      if (selected.length >= limit) break;
    }
  }

  return selected;
}

export function normalizeComparedSocieties(
  selectedHomes: PropertyCard[],
  catalog: PropertyCard[],
  minimumSocieties: number,
  limit: number,
): PropertyCard[] {
  const normalized: PropertyCard[] = [];
  const societyKeys = new Set<string>();

  function addDistinct(property: PropertyCard) {
    const key = societyIdentity(property);
    if (societyKeys.has(key) || normalized.length >= limit) return;
    societyKeys.add(key);
    normalized.push(property);
  }

  selectedHomes.forEach(addDistinct);
  if (normalized.length >= minimumSocieties) return normalized;

  const preferredBhk = selectedHomes[0]?.bhk ?? catalog[0]?.bhk;
  catalog
    .filter((property) => property.bhk === preferredBhk)
    .forEach(addDistinct);
  catalog.forEach(addDistinct);

  return normalized;
}
