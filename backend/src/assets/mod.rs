//! Asset graph contracts for crawling, enrichment, KG views, and serving bundles.
//!
//! An asset is a durable data product, not a Python or Rust function. Crawler
//! and enrichment code can change, but the asset IDs, partitions, manifests,
//! and current pointers are the stable contract.
//!
//! Storage contract: raw, silver, gold, and serving tables are Parquet part
//! files. JSON is reserved for small control-plane files such as manifests,
//! schema descriptors, trust policy, and current pointers.

pub mod approach_road;
pub mod canonical_nodes;
pub mod compaction;
pub mod environment;
pub mod executor;
pub mod fan_in;
pub mod google;
pub mod home_state;
pub mod kg_view;
pub mod materialization;
pub mod media;
pub mod paths;
pub mod planner;
pub mod project_enrichment;
pub mod reddit;
pub mod registry;
pub mod rera;
pub mod run_manifest;
pub mod skill_facts;
pub mod source_inputs;
pub mod source_provider;
pub mod transit;
pub mod types;

pub use approach_road::{
    read_approach_road_graph_rows, ApproachRoadGraphError, ApproachRoadGraphMaterialization,
    ApproachRoadGraphMaterializer, ApproachRoadGraphRows, APPROACH_ROAD_GRAPH_FACTS_ASSET_ID,
};
pub use canonical_nodes::{read_canonical_node_rows, CanonicalNodeRows, CanonicalNodesError};
pub use compaction::{
    CurrentProjectFactsError, CurrentProjectFactsMaterialization, CurrentProjectFactsMaterializer,
    CURRENT_PROJECT_FACTS_ASSET_ID,
};
pub use environment::{
    society_groundwater_potential_facts_input, EnvironmentGroundwaterPotentialInput,
    EnvironmentGroundwaterPotentialZone, EnvironmentRingPoint, EnvironmentalAssetError,
    SOCIETY_GROUNDWATER_POTENTIAL_FACTS_ASSET_ID,
};
pub use executor::{
    AssetDagExecutionOptions, AssetDagExecutionReport, AssetDagExecutor, AssetDagExecutorError,
    AssetRetryPolicy,
};
pub use fan_in::{
    all_current_materialization_records_for_dependency,
    all_current_partition_dependency_records_for_asset, sort_materialization_records,
    AssetFanInError,
};
pub use google::{
    canonicalize_google_nearby_places_input, canonicalize_google_places_input,
    google_nearby_place_facts_input, google_nearby_place_facts_input_with_aliases,
    google_review_facts_input, google_review_facts_input_with_aliases,
    read_google_nearby_place_rows, read_google_place_rows, GoogleNearbyPlaceRecord,
    GoogleNearbyPlaceSnapshotManifest, GoogleNearbyPlaceSnapshotMaterialization,
    GoogleNearbyPlacesWeeklyInput, GooglePlaceAssetError, GooglePlaceSnapshotManifest,
    GooglePlaceSnapshotMaterialization, GooglePlaceSnapshotMaterializer, GooglePlaceSnapshotRecord,
    GooglePlacesWeeklyInput, GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID, GOOGLE_PLACES_WEEKLY_ASSET_ID,
};
pub use home_state::{home_state_signals_input, HOME_STATE_SIGNALS_ASSET_ID};
pub use kg_view::{
    KgSocietyViewMaterialization, KgSocietyViewMaterializeError, KgSocietyViewMaterializer,
    KgViewArtifact, KgViewArtifactKind, KgViewEdgeRecord, KgViewEntityRecord,
    KgViewFactAnnotationRecord, KgViewFactRecord, KgViewManifest, KgViewRecords,
    KG_SOCIETY_VIEW_ASSET_ID,
};
pub use materialization::AssetMaterializationStore;
pub use media::{
    image_media_facts_input_with_aliases, ExternalImageObservationRecord,
    ExternalImageSnapshotManifest, ExternalImagesWeeklyInput, MediaAssetError,
    MediaAssetMaterializer, EXTERNAL_IMAGES_WEEKLY_ASSET_ID, IMAGE_MEDIA_FACTS_ASSET_ID,
};
pub use paths::AssetPathBuilder;
pub use planner::{
    AssetDagPlan, AssetFreshness, AssetPlanEntry, AssetPlanner, FreshnessPolicy,
    FreshnessReferenceKind, PlanDecision, PlanReason, PlannedAsset, PlannerError,
};
pub use project_enrichment::{
    builder_rera_aggregate_facts_input, external_listing_facts_input_with_aliases,
    ExternalListingObservationRecord, ExternalListingsWeeklyInput, ObservationSnapshotManifest,
    ProjectEnrichmentAssetError, ProjectEnrichmentMaterializer, BUILDER_RERA_AGGREGATES_ASSET_ID,
    EXTERNAL_LISTINGS_WEEKLY_ASSET_ID, EXTERNAL_LISTING_FACTS_ASSET_ID,
};
pub use reddit::{
    RedditThreadSnapshotManifest, RedditThreadSnapshotMaterialization,
    RedditThreadSnapshotMaterializeError, RedditThreadSnapshotMaterializer,
    RedditThreadSnapshotRecord, REDDIT_THREADS_DAILY_ASSET_ID,
};
pub use registry::{
    default_openestates_registry, openestates_registry, AssetDefinition, AssetPartitionPolicy,
    AssetRegistry, CostTier, DependencyFanInPolicy, DependencyFanInRule, PartitionCoordinate,
    PartitionResolutionError, RefreshCadence, RegistryError, TrustTier,
};
pub use rera::{
    read_canonical_society_rows, read_rera_project_rows, rera_legal_facts_input,
    CanonicalSocietyMaterializer, CanonicalSocietyRows, ReraAssetError, ReraCanonicalMappingRecord,
    ReraProjectSnapshotRecord, ReraRegistryMaterializer, ReraRegistryMonthlyInput,
    CANONICAL_SOCIETY_NODES_ASSET_ID, RERA_LEGAL_FACTS_ASSET_ID, RERA_REGISTRY_MONTHLY_ASSET_ID,
};
pub use run_manifest::{
    AssetDagResumeLease, AssetDagRunManifest, AssetRunAttempt, AssetRunManifestStore, AssetRunStep,
    AssetRunStepStatus, CurrentDagRunPointer, DagRunStatus, RunManifestError,
    DEFAULT_RESUME_LEASE_SECONDS,
};
pub use skill_facts::{
    read_skill_fact_artifact_rows, SkillFactAnnotationRecord, SkillFactArtifactRows,
    SkillFactManifest, SkillFactMaterialization, SkillFactMaterializeError, SkillFactMaterializer,
    SkillFactRecord, GOOGLE_NEARBY_PLACE_FACTS_ASSET_ID, GOOGLE_REVIEW_FACTS_ASSET_ID,
    REDDIT_RESIDENT_FACTS_ASSET_ID,
};
pub use source_inputs::{
    AssetSourceInputs, RedditThreadsDailyInput, SkillFactsInput, SourceInputCollectionPlan,
};
pub use source_provider::{
    CommandSourceInputProvider, LakeObjectSourceInputProvider, LocalFileSourceInputProvider,
    SourceEntitySeed, SourceInputProvider, SourceInputProviderError, SourceInputRequest,
};
pub use transit::{
    bengaluru_metro_station_facts_input, BengaluruMetroStationInput, BengaluruMetroStationsInput,
    TransitAssetError, BENGALURU_METRO_STATION_FACTS_ASSET_ID,
};
pub use types::{
    ArtifactRef, AssetId, AssetPartition, AssetStage, CurrentAssetPointer, MaterializationId,
    MaterializationRecord, MaterializationStatus, SourceWatermark,
};
