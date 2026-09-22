/* Generated from Rust public DTOs. Run npm run contracts:generate. */

export type Availability = "available" | "unavailable";
export type IntentExpression =
  | {
      clauses: IntentExpression[];
      kind: "all";
    }
  | {
      clauses: IntentExpression[];
      kind: "any";
    }
  | {
      clause: IntentExpression;
      kind: "not";
    }
  | {
      kind: "predicate";
      predicate: IntentPredicatePresentation;
    };
export type Polarity = "positive" | "negative";
export type BoolExpr =
  | {
      op: "all";
      value: BoolExpr[];
    }
  | {
      op: "any";
      value: BoolExpr[];
    }
  | {
      op: "not";
      value: BoolExpr;
    }
  | {
      op: "leaf";
      value: string;
    };
export type SearchJourneyResults =
  | {
      guidance?: SearchGuidance;
      kind: "current";
      orderedResultIds: string[];
      resultSets: JourneyResultSet[];
      state: SearchResultState;
      totalMatches: number;
    }
  | {
      kind: "retained";
      orderedResultIds: string[];
      resultFingerprint: string;
    };
export type MatchTier = "exact" | "supported" | "contextual";
export type SearchResultState = "results" | "no_matches";
export type SearchRevisionOperation =
  "initial" | "resume" | "refine" | "rephrase" | "expand" | "replace" | "exclude" | "correct";
export type ResultMovementCause = "catalogRefresh" | "intentRefinement";
export type SearchJourneyAttemptKind = "initial" | "revision" | "resume";
export type SearchJourneyOutcome =
  "activated" | "preservedParent" | "clarificationRequired" | "limitReached" | "resumed";
export type SelectedPropertyCause = "catalogRefresh" | "intentRefinement";
export type SelectedPropertyOutcome = "retained" | "excluded";

export interface SearchJourneyEnvelope {
  active: SearchJourneyActive;
  attempt: SearchJourneyAttempt;
  contractVersion: number;
  runtimeVersion: SearchRuntimeVersion;
}
export interface SearchJourneyActive {
  buyerBrief: string;
  collections: JourneyCollection[];
  intent: IntentPresentation;
  latestUtterance: string;
  results: SearchJourneyResults;
  revision: SearchJourneyRevision;
}
export interface JourneyCollection {
  cards: BrowsePropertyCard[];
  id: string;
  note: string;
  priceBand: CollectionPriceBand;
  strategy: string;
  title: string;
}
/**
 * Bounded landing-card projection built once with the immutable search runtime.
 * It deliberately excludes evidence blobs, descriptions, tags, and source panels.
 */
export interface BrowsePropertyCard {
  area: string;
  availability: InventoryAvailability;
  bhk?: number;
  detail_href: string;
  google_rating?: number;
  google_review_count?: number;
  id: string;
  image: string;
  price?: number;
  price_max?: number;
  price_min?: number;
  save_id: string;
  society_id: string;
  society_name: string;
  sqft?: number;
  title: string;
}
export interface InventoryAvailability {
  area: Availability;
  bedrooms: Availability;
  price: Availability;
}
export interface CollectionPriceBand {
  currency: string;
  label: string;
  max: number;
  min: number;
}
export interface IntentPresentation {
  branches: IntentBranchPresentation[];
  root: BoolExpr;
}
export interface IntentBranchPresentation {
  constraints: IntentExpression;
  id: string;
  preferences: IntentPreferencePresentation[];
  unresolvedRequirements?: string[];
}
export interface IntentPredicatePresentation {
  dimension: string;
  id: string;
  label: string;
  operator: string;
  polarity: string;
  required: boolean;
  resolvedLabel?: string;
  unit?: string;
  value: unknown;
}
export interface IntentPreferencePresentation {
  id: string;
  label: string;
  polarity: Polarity;
  priority?: number;
  required: boolean;
  weight: number;
}
export interface SearchGuidance {
  message: string;
  mode: string;
  suggestions: string[];
  title: string;
}
export interface JourneyResultSet {
  branchId: string;
  label: string;
  results: JourneyResultCard[];
}
/**
 * Bounded landing-card projection built once with the immutable search runtime.
 * It deliberately excludes evidence blobs, descriptions, tags, and source panels.
 */
export interface JourneyResultCard {
  area: string;
  availability: InventoryAvailability;
  bhk?: number;
  detail_href: string;
  google_rating?: number;
  google_review_count?: number;
  homeStateDisplay?: string;
  id: string;
  image: string;
  matchTier: MatchTier;
  price?: number;
  price_max?: number;
  price_min?: number;
  reasons: SearchMatchReason[];
  save_id: string;
  society_id: string;
  society_name: string;
  sqft?: number;
  title: string;
}
export interface SearchMatchReason {
  branchId: string;
  explanation: string;
  predicateId: string;
  proofToken: string;
  showOnCard: boolean;
}
export interface SearchJourneyRevision {
  depth: number;
  id: string;
  operation: SearchRevisionOperation;
  parentId?: string;
  resultFingerprint: string;
  semanticFingerprint: string;
  stateToken: string;
}
export interface SearchJourneyAttempt {
  attemptedIntent?: IntentPresentation;
  catalogDelta?: ResultDelta;
  catalogRebased: boolean;
  clarification?: JourneyClarification;
  intentDelta?: ResultDelta;
  kind: SearchJourneyAttemptKind;
  operation: SearchRevisionOperation;
  outcome: SearchJourneyOutcome;
  selectedPropertyConsequence?: SelectedPropertyConsequence;
}
export interface ResultDelta {
  added: string[];
  moved: ResultMovement[];
  removed: string[];
  retained: string[];
}
export interface ResultMovement {
  cause: ResultMovementCause;
  explanation: string;
  from: number;
  propertyId: string;
  to: number;
}
export interface JourneyClarification {
  code: string;
  message: string;
}
export interface SelectedPropertyConsequence {
  branchIds?: string[];
  cause: SelectedPropertyCause;
  explanation: string;
  failedPredicateIds?: string[];
  outcome: SelectedPropertyOutcome;
  proofReferences?: SearchMatchReason[];
  propertyId: string;
}
export interface SearchRuntimeVersion {
  scoringPolicyVersion: number;
  searchEngineVersion: string;
  semanticContractDigest: string;
  servingBundleVersion: string;
  snapshotIdentity: string;
}
