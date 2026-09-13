import propertyDetail from "./fixtures/prestige-waterford-api/property-detail.json" with { type: "json" };
import arrivalStory from "./fixtures/prestige-waterford-api/arrival-story.json" with { type: "json" };
import aroundThisHome from "./fixtures/prestige-waterford-api/around-this-home.json" with { type: "json" };
import type {
  PropertyDetailResponse,
  SurfaceSceneResponse,
} from "./types.ts";

export const waterfordFixturePropertyId =
  "discovered-prestige-waterford-3bhk";
export const waterfordFixtureServingBundleVersion =
  "catalog-71-ffb4dc50-117e-453c-b26f-41822430324e";

const detail = propertyDetail as unknown as PropertyDetailResponse;
const scenes = new Map<string, SurfaceSceneResponse>([
  ["arrival_story", arrivalStory as unknown as SurfaceSceneResponse],
  ["around_this_home", aroundThisHome as unknown as SurfaceSceneResponse],
]);

function copy<T>(value: T): T {
  return structuredClone(value);
}

/**
 * Production-shaped Waterford responses captured from one promoted serving
 * bundle. This is reachable only through the existing fixture-mode API seam.
 */
export function getWaterfordApiFixtureResponse(path: string): unknown | null {
  const [pathname] = path.split("?");
  const propertyPath = `/api/properties/${waterfordFixturePropertyId}`;

  if (pathname === propertyPath) return copy(detail);

  const surfaceMatch = pathname.match(
    new RegExp(`^${propertyPath}/surfaces/([^/]+)$`),
  );
  if (!surfaceMatch) return null;

  const scene = scenes.get(decodeURIComponent(surfaceMatch[1]));
  return scene ? copy(scene) : null;
}
