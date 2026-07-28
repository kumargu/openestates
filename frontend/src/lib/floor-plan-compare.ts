import type { PropertyCard } from "./types.ts";

type PlanComparableListing = Pick<
  PropertyCard,
  | "id"
  | "bhk"
  | "carpet_area_sqft"
  | "floor_plan_preview_url"
  | "plan_carpet_area_sqft"
  | "plan_sale_area_sqft"
  | "plan_configuration_type"
>;

export type FloorPlanComparePlan = {
  listingId: string;
  previewUrl: string;
  configurationType?: string;
  carpetAreaSqft?: number;
  saleAreaSqft?: number;
  usableAreaRatio?: number;
};

function positiveNumber(value: number | undefined): number | undefined {
  return typeof value === "number" && Number.isFinite(value) && value > 0 ? value : undefined;
}

function carpetDistance(listing: PlanComparableListing): number {
  const listingCarpet = positiveNumber(listing.carpet_area_sqft);
  const planCarpet = positiveNumber(listing.plan_carpet_area_sqft);
  if (!listingCarpet || !planCarpet) return Number.MAX_SAFE_INTEGER;
  return Math.abs(listingCarpet - planCarpet);
}

export function floorPlanForBhk(
  listings: PlanComparableListing[],
  activeBhk: number,
): FloorPlanComparePlan | null {
  const matches = listings
    .filter((listing) => listing.bhk === activeBhk && listing.floor_plan_preview_url)
    .sort((left, right) =>
      carpetDistance(left) - carpetDistance(right)
      || left.id.localeCompare(right.id)
    );
  const selected = matches[0];
  if (!selected?.floor_plan_preview_url) return null;

  const carpetAreaSqft = positiveNumber(selected.plan_carpet_area_sqft);
  const saleAreaSqft = positiveNumber(selected.plan_sale_area_sqft);
  return {
    listingId: selected.id,
    previewUrl: selected.floor_plan_preview_url,
    configurationType: selected.plan_configuration_type,
    carpetAreaSqft,
    saleAreaSqft,
    usableAreaRatio: carpetAreaSqft && saleAreaSqft
      ? Number((carpetAreaSqft / saleAreaSqft).toFixed(3))
      : undefined,
  };
}
