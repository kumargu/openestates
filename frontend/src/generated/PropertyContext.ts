/* Generated from Rust public DTOs. Run npm run contracts:generate. */

export type ContextGeometry =
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
export type EvidenceId =
  | {
      id: string;
      kind: "observation";
    }
  | {
      id: string;
      kind: "derivation";
    }
  | {
      id: string;
      kind: "entity";
    }
  | {
      id: string;
      kind: "relationship";
    };
/**
 * What kind of value a fact holds.
 */
export type FactValue =
  | {
      data: number;
      type: "Numeric";
    }
  | {
      data: string;
      type: "Text";
    }
  | {
      data: boolean;
      type: "Bool";
    }
  | {
      data: string[];
      type: "Tags";
    }
  | {
      data: {
        explanation: string;
        value: number;
      };
      type: "Score";
    };
/**
 * Promoted catalog records support identity claims without fabricated observations.
 */
export type CatalogEvidence =
  | {
      entityId: string;
      entityType: string;
      kind: "entity";
      name: string;
    }
  | {
      confidence: number;
      fromEntityId: string;
      kind: "relationship";
      relation: string;
      relationshipId: string;
      sourceType: string;
      toEntityId: string;
    };
export type ConstraintOperator = "min" | "max";
export type ProofResolutionStatus = "resolved";

export interface PropertyContext {
  anchor: ContextEntity;
  contractVersion: number;
  entityRefs: KgEntityRefs;
  features: ContextFeature[];
  matchedProof: ProofResolution | null;
  propertyId: string;
  snapshotIdentity: string;
  truncated: boolean;
}
export interface ContextEntity {
  entityId: string;
  geometry: ContextGeometry | null;
  geometryEvidence: EvidenceRef[];
  geometrySource: ContextFact | null;
  name: string;
  /**
   * @minItems 2
   * @maxItems 2
   */
  point: [number, number] | null;
}
export interface EvidenceRef {
  evidence_id: EvidenceId;
  snapshot_identity: string;
  subject_entity_id: string;
}
export interface ContextFact {
  confidence: number;
  entityId: string;
  evidence: EvidenceRef;
  factKey: string;
  id: string;
  observedAt: string;
  sourceType: string;
  sourceUrl: string | null;
  value: FactValue;
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
export interface ContextFeature {
  attributes: ContextFact[];
  distance: DerivedEvidence | null;
  fact: ContextFact;
  target: ContextEntity | null;
}
export interface DerivedEvidence {
  algorithm_version: string;
  confidence: number;
  derivation_id: string;
  input_evidence: EvidenceRef[];
  metric: string;
  relation: string;
  snapshot_identity: string;
  subject_entity_id: string;
  target_entity_id: string | null;
  unit: string | null;
  value: number | null;
}
export interface ProofResolution {
  branchId: string;
  catalogEvidence?: CatalogEvidence[];
  claim?: EvaluatedClaim;
  constraint?: HardConstraint;
  contractVersion: number;
  derivationChain: DerivedEvidence[];
  factKey: string;
  geometry?: unknown;
  predicateId: string;
  propertyId: string;
  relation: string;
  resolutionStatus: ProofResolutionStatus;
  semanticFingerprint: string;
  snapshotIdentity: string;
  sourceObservations: ResolvedSourceObservation[];
  subjectEntityId: string;
  targetEntityId?: string;
  targetLabel?: string;
  unit?: string;
  value?: FactValue;
}
export interface EvaluatedClaim {
  dimension: string;
  unit: string;
  value: number;
}
export interface HardConstraint {
  /**
   * Registry dimension, e.g. "land_area".
   */
  field: string;
  operator: ConstraintOperator;
  raw_text: string;
  unit: string;
  value: number;
}
export interface ResolvedSourceObservation {
  assetLineage: string[];
  observationId: string;
  observedAt: string;
  provider: string;
  providerObservationId: string;
  sourceUrl?: string;
  subjectEntityId: string;
}
