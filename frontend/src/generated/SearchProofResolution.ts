/* Generated from Rust public DTOs. Run npm run contracts:generate. */

export type ConstraintOperator = "min" | "max";
export type EvidenceId =
  | {
      id: string;
      kind: "observation";
    }
  | {
      id: string;
      kind: "derivation";
    };
export type ProofResolutionStatus = "resolved";
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

export interface ProofResolution {
  branchId: string;
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
export interface EvidenceRef {
  evidence_id: EvidenceId;
  snapshot_identity: string;
  subject_entity_id: string;
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
