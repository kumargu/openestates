import type { PropertyCard, RecommendationBranch } from "./types.ts";

export type RecommendationShelfItem = {
  id: string;
  property: PropertyCard;
};

export function recommendationShelfItems(
  branches: RecommendationBranch[],
  currentProperty: PropertyCard,
  excludedIds: ReadonlySet<string>,
  limit = 4,
): RecommendationShelfItem[] {
  const usedIds = new Set([currentProperty.id, ...excludedIds]);
  const usedSocieties = new Set([recommendationSocietyKey(currentProperty)]);
  const items: RecommendationShelfItem[] = [];

  for (const branch of branches) {
    const property = branch.property;
    const society = recommendationSocietyKey(property);
    if (
      usedIds.has(property.id)
      || usedSocieties.has(society)
      || (!property.hero_image && !property.society_name)
    ) {
      continue;
    }
    usedIds.add(property.id);
    usedSocieties.add(society);
    items.push({
      id: `${branch.branch_id}-${property.id}`,
      property,
    });
    if (items.length === limit) break;
  }

  return items;
}

function recommendationSocietyKey(property: PropertyCard): string {
  return property.kg_entity_refs?.society_entity_id
    || property.society_name.trim().toLocaleLowerCase("en-IN")
    || property.title.trim().toLocaleLowerCase("en-IN");
}
