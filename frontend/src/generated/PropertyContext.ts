/* Generated from Rust public DTOs. Run npm run contracts:generate. */

export type SceneGeometry =
  | {
      /**
       * @minItems 2
       * @maxItems 2
       */
      coordinates: [number, number];
      type: "Point";
    }
  | {
      coordinates: [number, number][];
      type: "LineString";
    }
  | {
      coordinates: [number, number][][];
      type: "Polygon";
    }
  | {
      coordinates: [number, number][][][];
      type: "MultiPolygon";
    };
export type CoordinateQuality = "exact" | "derived" | "approximate" | "missing";
export type DisplayTone = "positive" | "neutral" | "caution" | "risk";
export type FillState = "filled" | "partial" | "empty";
export type ProofFocusStatus = "notRequested" | "applied" | "stale" | "retired" | "mismatch" | "unavailable";
export type EvidenceId =
  | {
      id: string;
      kind: "observation";
    }
  | {
      id: string;
      kind: "derivation";
    };

export interface SurfaceSceneResponse {
  anchor: SceneAnchor;
  callouts: SceneCallout[];
  contractVersion: number;
  entityRefs: KgEntityRefs;
  experience?: UiSurfaceSceneExperienceConfig;
  features: SceneFeature[];
  fillRate: SceneFillRate;
  gaps: SceneGap[];
  layers: SceneLayer[];
  proofFocus?: ResolvedProofFocus;
  proofFocusMessage?: string;
  proofFocusStatus: ProofFocusStatus;
  propertyId: string;
  receipts: SceneReceipt[];
  relations: SceneRelation[];
  servingBundleVersion?: string;
  snapshotIdentity: string;
  surfaceId: string;
  viewport: SceneViewport;
}
export interface SceneAnchor {
  area?: string;
  boundary?: SceneBoundary;
  coordinateQuality: CoordinateQuality;
  entityId: string;
  geometry?: SceneGeometry;
  label: string;
}
export interface SceneBoundary {
  confidence: number;
  geometry: SceneGeometry;
  sourceType: string;
  sourceUrl?: string;
}
export interface SceneCallout {
  featureIds: string[];
  id: string;
  label: string;
  receiptIds: string[];
  tone: DisplayTone;
}
/**
 * Minimal entity identity bundle attached to property/search/detail responses.
 *
 * These fields are stable API identifiers, not display copy and not a complete
 * serving export. They exist so the UI can ask follow-up endpoints for richer
 * context when a user shows intent: opens a property, expands a card, compares
 * homes, clicks a source trail, or requests a nearby/risk/community breakdown.
 *
 * Current usage pattern:
 * 1. Render fast listing data from `PropertyCard` or `PropertyDetailResponse`.
 * 2. Use `source_entity_ids` as opaque provenance handles for the property,
 *    society, area, and builder.
 * 3. Use source/evidence read models when the UI needs a larger drill-down
 *    such as builder portfolio, nearby projects, or lineage.
 * 4. Build optional UI sections from facts with source/confidence metadata.
 * 5. Hide sections that have no backed facts instead of rendering empty cards.
 *
 * Important distinction: these are KG node IDs, not necessarily canonical RERA
 * IDs. Some societies have an alias node such as `society:prestige-park-grove`
 * while lake artifacts may also contain a RERA-rooted canonical ID. The UI
 * should not infer canonicalization from the string shape. It should treat the
 * IDs as opaque handles and follow the API.
 */
export interface KgEntityRefs {
  /**
   * Area/locality node for traffic, waterlogging, metro, schools, price trend,
   * and other externalities.
   */
  area_entity_id: string;
  /**
   * Builder node when the society has a known BuiltBy edge in the KG.
   */
  builder_entity_id?: string;
  /**
   * Listing-level node for facts specific to this flat/unit/listing.
   */
  property_entity_id: string;
  /**
   * Society/project node for RERA, reviews, nearby places, amenities, and
   * community evidence.
   */
  society_entity_id: string;
  /**
   * Existing graph nodes the UI can safely prefetch first.
   *
   * This list is backend-filtered to nodes present in the current KG, sorted,
   * and deduplicated. It may omit an otherwise valid field ID if that node has
   * not been materialized yet. UI code should treat it as a convenient fetch
   * plan, not as a complete semantic model.
   */
  source_entity_ids?: string[];
}
export interface UiSurfaceSceneExperienceConfig {
  backToSocietyLabel: string | null;
  boundaryPadding: number;
  finalHeading: number;
  finalRangeM: number;
  finalTilt: number;
  googleUnavailableState: string | null;
  missingBoundaryState: string | null;
  mobileBoundaryPadding: number;
  revealDurationMs: number;
  rotationArcDegrees: number;
  searchContextLabel: string | null;
  searchContextViewHomeLabel: string | null;
  societyPauseLabel: string | null;
  societyPlayLabel: string | null;
  societyResumeLabel: string | null;
  startRangeM: number;
}
export interface SceneFeature {
  confidence: number;
  coordinateQuality: CoordinateQuality;
  details?: string[];
  display: SceneFeatureDisplay;
  entityId?: string;
  geometry: SceneGeometry;
  id: string;
  kind: string;
  label: string;
  layerId: string;
  metrics?: SceneMetrics;
  properties?: {
    [k: string]: string;
  };
  receiptIds: string[];
  shortLabel?: string;
}
export interface SceneFeatureDisplay {
  icon?: string;
  priority: number;
  tone: DisplayTone;
}
export interface SceneMetrics {
  distanceM?: number;
  rating?: number;
  reviewCount?: number;
  severity?: string;
  travelTimeMin?: number;
}
export interface SceneFillRate {
  availableFeatures: number;
  emptyLayers: number;
  filledLayers: number;
  partialLayers: number;
  shownFeatures: number;
  value: number;
}
export interface SceneGap {
  fillState: FillState;
  layerId: string;
}
export interface SceneLayer {
  availableCount: number;
  emptyState?: string;
  enabledByDefault: boolean;
  experience?: UiSurfaceLayerExperienceConfig;
  family: string;
  featureValueLabels?: {
    [k: string]: {
      [k: string]: string;
    };
  };
  fillState: FillState;
  id: string;
  label: string;
  mapPresentation?: string;
  rank: number;
  relationClass: string;
  renderKind: string;
  shownCount: number;
}
export interface UiSurfaceLayerExperienceConfig {
  anchorInteriorDwellMs: number | null;
  anchorInteriorPitch: number | null;
  anchorLookAheadM: number | null;
  anchorPitch: number | null;
  anchorPoseTransitionMs: number | null;
  cameraAltitudeM: number;
  cameraFov: number;
  cameraRangeM: number;
  cameraTilt: number;
  dwellMs: number;
  endsHereState: string | null;
  entranceDwellMs: number | null;
  kind: string;
  maximumDurationMs: number | null;
  maximumPanoramaGapM: number | null;
  minimumDurationMs: number | null;
  minimumFrameDwellMs: number | null;
  overviewDwellMs: number | null;
  panoramaCrossfadeMs: number | null;
  pauseLabel: string | null;
  replayLabel: string | null;
  resumeLabel: string | null;
  routeDirection: string | null;
  shortGapState: string | null;
  streetViewZoom: number;
  targetDurationMs: number | null;
  transitionMs: number;
  unavailableState: string | null;
  waypointSpacingM: number;
}
/**
 * Server-owned focus passed from proof resolution to a configured detail
 * surface. It is serializable in the scene but cannot be client-authored.
 */
export interface ResolvedProofFocus {
  destinationKind: string;
  distanceM?: number;
  entityId?: string;
  factKey: string;
  featureId?: string;
  layerId: string;
  matchedLabel?: string;
  matchedValue?: string;
  /**
   * Exact source identities; display text is never a substitute for these.
   */
  observationIds?: string[];
  receiptId?: string;
  surfaceId: string;
  targetId: string;
}
export interface SceneReceipt {
  claim: string;
  confidence: number;
  entityId: string;
  evidence: EvidenceRef;
  factKey: string;
  id: string;
  learnedAt: string;
  scope?: string;
  sourceType: string;
  sourceUrl?: string;
}
export interface EvidenceRef {
  evidence_id: EvidenceId;
  snapshot_identity: string;
  subject_entity_id: string;
}
export interface SceneRelation {
  confidence: number;
  direct: boolean;
  distanceM?: number;
  edgeType: string;
  fromId: string;
  receiptIds: string[];
  relationClass: string;
  toId: string;
}
export interface SceneViewport {
  bounds?: SceneBounds;
  /**
   * @minItems 2
   * @maxItems 2
   */
  center?: [number, number];
  radiusM?: number;
}
export interface SceneBounds {
  east: number;
  north: number;
  south: number;
  west: number;
}
