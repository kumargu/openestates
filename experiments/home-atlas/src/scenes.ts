import type { AtlasCamera, AtlasFeature, AtlasScene } from "./types.ts";

export type CategoryTourCopy = Readonly<{
  overview: (features: readonly AtlasFeature[]) => string;
  pair: (feature: AtlasFeature) => string;
  focus: (feature: AtlasFeature) => string;
  returnHome: () => string;
}>;

export type CategoryTourCameras = Readonly<{
  group: (features: readonly AtlasFeature[]) => AtlasCamera;
  pair: (feature: AtlasFeature) => AtlasCamera;
  focus: (feature: AtlasFeature) => AtlasCamera;
  home: () => AtlasCamera;
  segment?: (feature: AtlasFeature) => AtlasCamera;
}>;

export type CategoryTourTiming = Readonly<{
  overviewMs: number;
  pairMs: number;
  focusMs: number;
  segmentMs: number;
  returnHomeMs: number;
}>;

export const DEFAULT_CATEGORY_TOUR_TIMING: CategoryTourTiming = Object.freeze({
  overviewMs: 4_500,
  pairMs: 5_500,
  focusMs: 6_500,
  segmentMs: 8_000,
  returnHomeMs: 5_500,
});

export function buildCategoryTour(input: Readonly<{
  categoryId: string;
  home: AtlasFeature;
  features: readonly AtlasFeature[];
  cameras: CategoryTourCameras;
  copy: CategoryTourCopy;
  timing?: Partial<CategoryTourTiming>;
}>): readonly AtlasScene[] {
  const timing = { ...DEFAULT_CATEGORY_TOUR_TIMING, ...input.timing };
  const scenes: AtlasScene[] = [{
    id: `${input.categoryId}:overview`,
    targetFeatureId: input.home.id,
    camera: input.cameras.group(input.features),
    visibility: { mode: "category", categoryId: input.categoryId },
    caption: input.copy.overview(input.features),
    durationMs: timing.overviewMs,
  }];

  for (const feature of input.features) {
    scenes.push({
      id: `${input.categoryId}:${feature.id}:pair`,
      targetFeatureId: feature.id,
      camera: input.cameras.pair(feature),
      visibility: { mode: "pair", featureId: feature.id },
      caption: input.copy.pair(feature),
      durationMs: timing.pairMs,
    });
    scenes.push({
      id: `${input.categoryId}:${feature.id}:focus`,
      targetFeatureId: feature.id,
      camera: input.cameras.focus(feature),
      visibility: { mode: "pair", featureId: feature.id },
      caption: input.copy.focus(feature),
      durationMs: timing.focusMs,
    });
    if (feature.segments && input.cameras.segment) {
      scenes.push({
        id: `${input.categoryId}:${feature.id}:segment`,
        targetFeatureId: feature.id,
        camera: input.cameras.segment(feature),
        visibility: { mode: "pair", featureId: feature.id },
        caption: input.copy.focus(feature),
        durationMs: timing.segmentMs,
      });
    }
  }

  scenes.push({
    id: `${input.categoryId}:home`,
    targetFeatureId: input.home.id,
    camera: input.cameras.home(),
    visibility: { mode: "home" },
    caption: input.copy.returnHome(),
    durationMs: timing.returnHomeMs,
  });
  return scenes;
}

