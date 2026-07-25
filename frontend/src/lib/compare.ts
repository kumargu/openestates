import type { PropertyCard } from "./types.ts";

function societyIdentity(property: PropertyCard): string {
  return property.society_name?.trim().toLocaleLowerCase()
    || property.title.trim().toLocaleLowerCase();
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
