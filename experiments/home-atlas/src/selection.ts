import type { AtlasDocument, AtlasFeature, AtlasScene } from "./types.ts";

export type NumberedFeature = Readonly<{
  number: string;
  feature: AtlasFeature;
}>;

export function categoryFeatures(document: AtlasDocument, categoryId: string): readonly AtlasFeature[] {
  return document.features
    .filter((feature) => feature.categoryId === categoryId)
    .sort((left, right) => left.evidence.distance.metres - right.evidence.distance.metres || left.id.localeCompare(right.id));
}

export function numberedCategoryFeatures(document: AtlasDocument, categoryId: string): readonly NumberedFeature[] {
  return categoryFeatures(document, categoryId).map((feature, index) => ({
    number: String(index + 1).padStart(2, "0"),
    feature,
  }));
}

export function visibleFeaturesForScene(document: AtlasDocument, scene: AtlasScene): readonly AtlasFeature[] {
  const home = document.features.find((feature) => feature.id === document.homeId);
  if (!home) throw new Error(`Atlas home '${document.homeId}' is missing`);
  if (scene.visibility.mode === "home") return [home];
  if (scene.visibility.mode === "category") return [home, ...categoryFeatures(document, scene.visibility.categoryId)];

  const selected = document.features.find((feature) => feature.id === scene.visibility.featureId);
  if (!selected) throw new Error(`Atlas feature '${scene.visibility.featureId}' is missing`);
  return scene.visibility.mode === "pair" ? [home, selected] : [selected];
}

