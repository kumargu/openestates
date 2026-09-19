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

export type AtlasCamera = Readonly<{
  center: AtlasPoint & Readonly<{ altitude: number }>;
  heading: number;
  tilt: number;
  range: number;
}>;

export type AtlasVisibility =
  | Readonly<{ mode: "home" }>
  | Readonly<{ mode: "category"; categoryId: string }>
  | Readonly<{ mode: "pair"; featureId: string }>
  | Readonly<{ mode: "feature"; featureId: string }>;

export type AtlasScene = Readonly<{
  id: string;
  phase: "overview" | "pair" | "inspect" | "segment" | "home";
  targetFeatureId: string;
  camera: AtlasCamera;
  visibility: AtlasVisibility;
  caption: string;
  durationMs: number;
}>;
