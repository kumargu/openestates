//! Read-optimized serving artifacts built from the knowledge graph.
//!
//! The lake remains the durable truth. Serving bundles are compiled snapshots:
//! Parquet tables for structured facts/entities, small JSON control files, and
//! a Tantivy index prefix for fast local recall.

pub mod builder;
pub mod loader;
pub mod materializer;
pub mod parquet;
pub mod tantivy_index;
pub mod types;

pub use builder::{ServingBundleBuilder, ServingBundleError};
pub use loader::{LoadedServingBundle, ServingBundleLoadError, ServingBundleLoader};
pub use materializer::{
    SearchServingBundleMaterialization, SearchServingBundleMaterializeError,
    SearchServingBundleMaterializer,
};
pub use tantivy_index::{
    hydrate_tantivy_index, TantivyIndexError, TantivyRecallHit, TantivyRecallIndex,
};
pub use types::{
    BundleArtifact, BundleArtifactKind, ServingBundleManifest, ServingEntityRecord,
    ServingFactRecord, ServingSearchMetadataRecord, TrustPolicy, SEARCH_SERVING_BUNDLE_ASSET_ID,
};
