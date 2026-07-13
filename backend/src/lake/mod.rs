//! Object-store shaped storage for OpenEstates data artifacts.
//!
//! Local development and S3 production should use the same logical keys. This
//! module keeps path construction and metadata handling out of crawlers,
//! materializers, and request handlers.

pub mod keys;
pub mod store;

pub use keys::{LakeKey, LakePrefix};
pub use store::{ArtifactMetadata, LakeError, LakeStore};
