export type AtlasPoint = Readonly<{ lat: number; lng: number }>;

export type AtlasSource = Readonly<{
  providerId: string;
  sourceId?: string;
  sourceUrl?: string;
  retrievedAt?: string;
}>;

export type AtlasDistanceEvidence = Readonly<{
  metres: number;
  method: "straight_line";
  target: "place_point" | "mapped_extent_centre" | "representative_segment_point";
}>;

export type AtlasEvidence = Readonly<{
  location: AtlasSource;
  geometry?: AtlasSource;
  distance: AtlasDistanceEvidence;
  caveat?: string;
}>;

export type AtlasSegment = Readonly<{
  id: string;
  path: readonly AtlasPoint[];
}>;

export type AtlasFeature = Readonly<{
  id: string;
  categoryId: string;
  name: string;
  position: AtlasPoint;
  elevationM?: number;
  boundary?: readonly AtlasPoint[];
  segments?: readonly AtlasSegment[];
  evidence: AtlasEvidence;
}>;

export type AtlasDocument = Readonly<{
  schemaVersion: string;
  homeId: string;
  generatedAt?: string;
  features: readonly AtlasFeature[];
  contextSegments?: Readonly<Record<string, readonly AtlasSegment[]>>;
}>;

export type AtlasCamera = Readonly<{
  center: AtlasPoint & Readonly<{ altitude: number }>;
  heading: number;
  tilt: number;
  range: number;
}>;

export type AtlasCameraTuning = Readonly<{
  mobileBreakpointPx: number;
  mobilePlaceScale: number;
  mobileGroupScale: number;
  maxContextDistanceM: number;
  minimumGroupRangeM: number;
  groupRangeMultiplier: number;
  minimumPairRangeM: number;
  pairRangeMultiplier: number;
  cameraAltitudeOffsetM: number;
}>;

export type AtlasCameraContext = Readonly<{
  home: AtlasFeature;
  defaultElevationM: number;
  viewportWidthPx: number;
  tuning?: Partial<AtlasCameraTuning>;
}>;

export type AtlasVisibility =
  | Readonly<{ mode: "home" }>
  | Readonly<{ mode: "category"; categoryId: string }>
  | Readonly<{ mode: "pair"; featureId: string }>
  | Readonly<{ mode: "feature"; featureId: string }>;

export type AtlasScene = Readonly<{
  id: string;
  targetFeatureId: string;
  camera: AtlasCamera;
  visibility: AtlasVisibility;
  caption: string;
  durationMs: number;
}>;

