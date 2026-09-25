/* Generated from Rust public DTOs. Run npm run contracts:generate. */

export type ProofFailureStatus = "staleSnapshot" | "missingEvidence" | "invalidReference";

export interface ProofResolutionFailure {
  code: string;
  contractVersion: number;
  message: string;
  resolutionStatus: ProofFailureStatus;
  runtimeVersion: SearchRuntimeVersion;
}
export interface SearchRuntimeVersion {
  scoringPolicyVersion: number;
  searchEngineVersion: string;
  semanticContractDigest: string;
  servingBundleVersion: string;
  snapshotIdentity: string;
}
