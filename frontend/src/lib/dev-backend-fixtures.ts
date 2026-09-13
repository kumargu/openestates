import evidence from "./fixtures/prestige-waterford-api/evidence.json" with { type: "json" };
import property from "./fixtures/prestige-waterford-api/property.json" with { type: "json" };
import recommendations from "./fixtures/prestige-waterford-api/recommendations.json" with { type: "json" };
import rera from "./fixtures/prestige-waterford-api/rera.json" with { type: "json" };
import aroundThisHome from "./fixtures/prestige-waterford-api/surface-around-this-home.json" with { type: "json" };
import arrivalStory from "./fixtures/prestige-waterford-api/surface-arrival-story.json" with { type: "json" };
import surfacesBatch from "./fixtures/prestige-waterford-api/surfaces-batch.json" with { type: "json" };
import surfaces from "./fixtures/prestige-waterford-api/surfaces.json" with { type: "json" };

export const backendReplayFixtureId = "discovered-prestige-waterford-3bhk";

/**
 * Exact, redacted backend snapshots captured for the Prestige Waterford page.
 *
 * Keep this separate from dev-atlas-fixtures.ts: that fixture is intentionally
 * geometry-rich so every camera state can be exercised. This replay fixture is
 * intentionally faithful to the backend contract and exposes its real gaps.
 */
export function getBackendReplayFixture(path: string): unknown | null {
  const url = new URL(path, "http://fixture.local");
  const propertyPath = `/api/properties/${backendReplayFixtureId}`;

  if (url.pathname === propertyPath) return property;
  if (url.pathname === `${propertyPath}/evidence`) return evidence;
  if (url.pathname === `${propertyPath}/rera`) return rera;
  if (url.pathname === `${propertyPath}/recommendations`) return recommendations;
  if (url.pathname === `${propertyPath}/surfaces/around_this_home`) return aroundThisHome;
  if (url.pathname === `${propertyPath}/surfaces/arrival_story`) return arrivalStory;
  if (
    url.pathname === `${propertyPath}/surfaces`
    && url.searchParams.get("ids") === "around_this_home,arrival_story"
  ) {
    return surfaces;
  }
  if (url.pathname === "/api/properties/surfaces/batch") return surfacesBatch;

  return null;
}
