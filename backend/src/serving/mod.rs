//! Read-optimized serving artifacts built from the knowledge graph.
//!
//! The lake remains the durable truth. Serving bundles are compiled snapshots:
//! Parquet tables for structured facts/entities, small JSON control files, and
//! a Tantivy index prefix for fast local recall.

pub mod builder;
pub mod coordinates;
pub mod loader;
pub mod materializer;
pub mod parquet;
pub mod projection;
pub mod proximity;
pub mod spatial_index;
pub mod tantivy_index;
pub mod types;

pub use builder::{ServingBundleBuilder, ServingBundleError};
pub use coordinates::{resolve_serving_coordinates, ServingCoordinates};
pub use loader::{LoadedServingBundle, ServingBundleLoadError, ServingBundleLoader};
pub use materializer::{
    SearchServingBundleMaterialization, SearchServingBundleMaterializeError,
    SearchServingBundleMaterializer,
};
pub use parquet::{
    read_edges_parquet, read_entities_parquet, read_facts_parquet, read_search_metadata_parquet,
    ParquetReadError,
};
pub use projection::{GoogleReviewEvidence, ProjectedFact, SocietyFactProjection};
pub use proximity::{derive_proximity_records, DerivedProximityRecords};
pub use spatial_index::{SpatialPoint, SpatialServingIndex};
pub use tantivy_index::{
    hydrate_tantivy_index, TantivyIndexError, TantivyRecallHit, TantivyRecallIndex,
};
pub use types::{
    unique_society_aliases, BundleArtifact, BundleArtifactKind, ServingBundleManifest,
    ServingBundleSchema, ServingColumnSchema, ServingEdgeRecord, ServingEntityFactRows,
    ServingEntityRecord, ServingFactIndex, ServingFactRecord, ServingSearchMetadataRecord,
    ServingTableSchema, TrustPolicy, SEARCH_SERVING_BUNDLE_ASSET_ID,
};
