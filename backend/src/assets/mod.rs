//! Asset graph contracts for crawling, enrichment, KG views, and serving bundles.
//!
//! An asset is a durable data product, not a Python or Rust function. Crawler
//! and enrichment code can change, but the asset IDs, partitions, manifests,
//! and current pointers are the stable contract.
//!
//! Storage contract: raw, silver, gold, and serving tables are Parquet part
//! files. JSON is reserved for small control-plane files such as manifests,
//! schema descriptors, trust policy, and current pointers.

pub mod kg_view;
pub mod materialization;
pub mod paths;
pub mod planner;
pub mod reddit;
pub mod registry;
pub mod skill_facts;
pub mod types;

pub use kg_view::{
    KgSocietyViewMaterialization, KgSocietyViewMaterializeError, KgSocietyViewMaterializer,
    KgViewArtifact, KgViewArtifactKind, KgViewEdgeRecord, KgViewEntityRecord,
    KgViewFactAnnotationRecord, KgViewFactRecord, KgViewManifest, KgViewRecords,
    KG_SOCIETY_VIEW_ASSET_ID,
};
pub use materialization::AssetMaterializationStore;
pub use paths::AssetPathBuilder;
pub use planner::{AssetPlanner, PlanReason, PlannedAsset, PlannerError};
pub use reddit::{
    RedditThreadSnapshotManifest, RedditThreadSnapshotMaterialization,
    RedditThreadSnapshotMaterializeError, RedditThreadSnapshotMaterializer,
    RedditThreadSnapshotRecord, REDDIT_THREADS_DAILY_ASSET_ID,
};
pub use registry::{
    default_openestates_registry, AssetDefinition, AssetRegistry, CostTier, RefreshCadence,
    RegistryError, TrustTier,
};
pub use skill_facts::{
    SkillFactAnnotationRecord, SkillFactManifest, SkillFactMaterialization,
    SkillFactMaterializeError, SkillFactMaterializer, SkillFactRecord,
};
pub use types::{
    ArtifactRef, AssetId, AssetPartition, AssetStage, CurrentAssetPointer, MaterializationId,
    MaterializationRecord, MaterializationStatus, SourceWatermark,
};
