//! Asset graph contracts for crawling, enrichment, KG views, and serving bundles.
//!
//! An asset is a durable data product, not a Python or Rust function. Crawler
//! and enrichment code can change, but the asset IDs, partitions, manifests,
//! and current pointers are the stable contract.
//!
//! Storage contract: raw, silver, gold, and serving tables are Parquet part
//! files. JSON is reserved for small control-plane files such as manifests,
//! schema descriptors, trust policy, and current pointers.

pub mod executor;
pub mod fan_in;
pub mod kg_view;
pub mod materialization;
pub mod paths;
pub mod planner;
pub mod reddit;
pub mod registry;
pub mod rera;
pub mod run_manifest;
pub mod skill_facts;
pub mod source_inputs;
pub mod types;

pub use executor::{
    AssetDagExecutionOptions, AssetDagExecutionReport, AssetDagExecutor, AssetDagExecutorError,
};
pub use fan_in::{
    all_current_materialization_records_for_dependency,
    all_current_partition_dependency_records_for_asset, sort_materialization_records,
    AssetFanInError,
};
pub use kg_view::{
    KgSocietyViewMaterialization, KgSocietyViewMaterializeError, KgSocietyViewMaterializer,
    KgViewArtifact, KgViewArtifactKind, KgViewEdgeRecord, KgViewEntityRecord,
    KgViewFactAnnotationRecord, KgViewFactRecord, KgViewManifest, KgViewRecords,
    KG_SOCIETY_VIEW_ASSET_ID,
};
pub use materialization::AssetMaterializationStore;
pub use paths::AssetPathBuilder;
pub use planner::{
    AssetDagPlan, AssetFreshness, AssetPlanEntry, AssetPlanner, FreshnessPolicy,
    FreshnessReferenceKind, PlanDecision, PlanReason, PlannedAsset, PlannerError,
};
pub use reddit::{
    RedditThreadSnapshotManifest, RedditThreadSnapshotMaterialization,
    RedditThreadSnapshotMaterializeError, RedditThreadSnapshotMaterializer,
    RedditThreadSnapshotRecord, REDDIT_THREADS_DAILY_ASSET_ID,
};
pub use registry::{
    default_openestates_registry, AssetDefinition, AssetPartitionPolicy, AssetRegistry, CostTier,
    DependencyFanInPolicy, DependencyFanInRule, PartitionCoordinate, PartitionResolutionError,
    RefreshCadence, RegistryError, TrustTier,
};
pub use rera::{
    read_canonical_society_rows, read_rera_project_rows, rera_legal_facts_input,
    CanonicalSocietyMaterializer, CanonicalSocietyRows, ReraAssetError, ReraCanonicalMappingRecord,
    ReraProjectSnapshotRecord, ReraRegistryMaterializer, ReraRegistryMonthlyInput,
    CANONICAL_SOCIETY_NODES_ASSET_ID, RERA_LEGAL_FACTS_ASSET_ID, RERA_REGISTRY_MONTHLY_ASSET_ID,
};
pub use run_manifest::{
    AssetDagRunManifest, AssetRunManifestStore, AssetRunStep, AssetRunStepStatus,
    CurrentDagRunPointer, DagRunStatus, RunManifestError,
};
pub use skill_facts::{
    read_skill_fact_artifact_rows, SkillFactAnnotationRecord, SkillFactArtifactRows,
    SkillFactManifest, SkillFactMaterialization, SkillFactMaterializeError, SkillFactMaterializer,
    SkillFactRecord, GOOGLE_REVIEW_FACTS_ASSET_ID, REDDIT_RESIDENT_FACTS_ASSET_ID,
};
pub use source_inputs::{AssetSourceInputs, RedditThreadsDailyInput, SkillFactsInput};
pub use types::{
    ArtifactRef, AssetId, AssetPartition, AssetStage, CurrentAssetPointer, MaterializationId,
    MaterializationRecord, MaterializationStatus, SourceWatermark,
};
