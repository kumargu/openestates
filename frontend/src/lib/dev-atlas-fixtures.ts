import data from "./fixtures/home-atlas.json" with { type: "json" };
import type {
  SurfaceSceneResponse,
  SceneFeature,
  PropertyCard,
} from "./types.ts";

export const atlasFixtureId = "fixture-prestige-waterford-3bhk";
export function atlasFixtureCard(base: PropertyCard): PropertyCard {
  return {
    ...base,
    id: atlasFixtureId,
    title: "Prestige Waterford · local demo",
    society_name: "Prestige Waterford",
    description_summary:
      "Local integration fixture. Listing values are illustrative; map geometry is archived OSM/Places evidence.",
    kg_entity_refs: {
      ...base.kg_entity_refs,
      property_entity_id: atlasFixtureId,
      society_entity_id: "society:prestige-waterford",
    },
  };
}

export function atlasFixtureScene(surfaceId: string): SurfaceSceneResponse {
  const home = data.places.find((p) => p.id === "home")!;
  const features: SceneFeature[] = data.places
    .filter((p) => p.id !== "home")
    .map((p) => ({
      id: p.id,
      entityId: p.id,
      layerId: p.kind,
      kind: p.kind,
      label: p.name,
      geometry: { type: "Point", coordinates: [p.lng, p.lat] },
      coordinateQuality: "exact",
      metrics: { distanceM: p.distanceM },
      display: { tone: "neutral", priority: 1, icon: p.kind },
      confidence: 1,
      receiptIds: [p.kind === "road" ? "osm" : "places"],
    }));
  features.push({
    id: "ecc-alignment",
    layerId: "approach",
    kind: "road",
    label: data.road.name,
    geometry: {
      type: "LineString",
      coordinates: data.road.path.map((p) => [p.lng, p.lat]),
    },
    coordinateQuality: "exact",
    display: { tone: "neutral", priority: 1 },
    confidence: 1,
    receiptIds: ["osm"],
  });
  for (const place of data.places) {
    for (const segment of place.segments ?? []) {
      features.push({
        id: `${place.id}:${segment.id}`, entityId: place.id, layerId: place.kind,
        kind: place.kind, label: place.name,
        geometry: {type:'LineString', coordinates:segment.path.map(p => [p.lng,p.lat])},
        coordinateQuality:'exact', display:{tone:'neutral',priority:1}, confidence:1, receiptIds:['osm'],
      });
    }
    if (place.id === "home" || !place.boundary?.length) continue;
    features.push({
      id: `${place.id}-boundary`,
      entityId: place.id,
      layerId: place.kind,
      kind: place.kind,
      label: place.name,
      geometry: {
        type: "Polygon",
        coordinates: [place.boundary.map((p) => [p.lng, p.lat])],
      },
      coordinateQuality: "exact",
      display: { tone: "neutral", priority: 1 },
      confidence: 1,
      receiptIds: ["osm"],
    });
  }
  for (const segment of data.metroSegments)
    features.push({
      id: segment.id,
      layerId: "metro",
      kind: "metro",
      label: "Purple Line",
      geometry: {
        type: "LineString",
        coordinates: segment.path.map((p) => [p.lng, p.lat]),
      },
      coordinateQuality: "exact",
      display: { tone: "neutral", priority: 1 },
      confidence: 1,
      receiptIds: ["osm"],
    });
  const labels: Record<string, string> = {
    metro: "Metro",
    school: "Schools",
    hospital: "Hospitals",
    lake: "Lakes",
    society: "Societies",
    road: "Roads",
    approach: "Approach road",
  };
  return {
    contractVersion: 1,
    surfaceId,
    propertyId: atlasFixtureId,
    servingBundleVersion: "local-atlas-fixture",
    entityRefs: {
      property_entity_id: atlasFixtureId,
      area_entity_id: "area:whitefield",
      society_entity_id: "society:prestige-waterford",
    },
    anchor: {
      entityId: "society:prestige-waterford",
      label: home.name,
      area: "Whitefield",
      coordinateQuality: "exact",
      geometry: { type: "Point", coordinates: [home.lng, home.lat] },
      boundary: {
        sourceType: "OSM",
        confidence: 1,
        sourceUrl: "https://www.openstreetmap.org/way/133630420",
        geometry: {
          type: "Polygon",
          coordinates: [home.boundary!.map((p) => [p.lng, p.lat])],
        },
      },
    },
    experience: data.experience,
    viewport: { center: [home.lng, home.lat], radiusM: 3200 },
    layers: Object.entries(labels).map(([id, label], rank) => ({
      id,
      label,
      rank,
      family: "access",
      relationClass: "context",
      renderKind: id === "approach" ? "terrain_corridor" : "pin",
      mapPresentation: "immersive_3d",
      experience: id === "approach" ? data.roadExperience : undefined,
      enabledByDefault: true,
      availableCount: features.filter((f) => f.layerId === id).length,
      shownCount: features.filter((f) => f.layerId === id).length,
      fillState: "filled",
    })),
    features,
    relations: [],
    callouts: [],
    receipts: [
      {
        id: "osm",
        entityId: "society:prestige-waterford",
        factKey: "geo.geometry_geojson",
        claim: "Archived mapped alignment",
        learnedAt: "2026-09-08",
        confidence: 1,
        sourceType: "OSM",
        sourceUrl: "https://www.openstreetmap.org/",
      },
      {
        id: "places",
        entityId: "society:prestige-waterford",
        factKey: "nearby_places",
        claim: "Archived nearby locations",
        learnedAt: "2026-09-08",
        confidence: 1,
        sourceType: "Google Places",
        sourceUrl: "https://maps.google.com/",
      },
    ],
    fillRate: {
      filledLayers: Object.keys(labels).length,
      partialLayers: 0,
      emptyLayers: 0,
      shownFeatures: features.length,
      availableFeatures: features.length,
      value: 1,
    },
    gaps: [],
  };
}
