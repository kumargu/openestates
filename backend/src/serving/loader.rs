use std::fmt;
use std::path::{Path, PathBuf};

use crate::assets::AssetPathBuilder;
use crate::lake::{LakeError, LakeKey, LakeStore};

use super::SERVING_BUNDLE_FORMAT_VERSION;
use super::{
    hydrate_tantivy_index, read_edges_parquet, read_entities_parquet, read_entity_aliases_parquet,
    read_facts_parquet, read_rera_evidence_parquet, read_search_metadata_parquet,
    validate_serving_edge_evidence, validate_society_aliases, ParquetReadError, ReraEvidenceIndex,
    ServingBundleManifest, ServingEdgeRecord, ServingEntityAliasIndex, ServingEntityAliasRecord,
    ServingEntityRecord, ServingEvidenceIndex, ServingFactIndex, SpatialServingIndex,
    TantivyIndexError, TantivyRecallIndex,
};
use crate::graph::GraphIndex;
use crate::search::geo::SpatialEntityIndex;

#[derive(Clone)]
pub struct ServingBundleLoader {
    lake: LakeStore,
    cache_root: PathBuf,
}

pub struct LoadedServingBundle {
    pub manifest: ServingBundleManifest,
    pub entities: Vec<ServingEntityRecord>,
    pub entity_alias_index: ServingEntityAliasIndex,
    pub edges: Vec<ServingEdgeRecord>,
    pub graph_index: GraphIndex,
    pub recall_index: TantivyRecallIndex,
    pub fact_index: ServingFactIndex,
    pub evidence_index: ServingEvidenceIndex,
    pub rera_evidence_index: ReraEvidenceIndex,
    pub entity_index: SpatialEntityIndex,
    pub spatial_index: SpatialServingIndex,
    pub search_capabilities: crate::search::SearchCapabilityIndex,
    pub cache_dir: PathBuf,
}

impl ServingBundleLoader {
    pub fn new(lake: LakeStore, cache_root: impl Into<PathBuf>) -> Self {
        Self {
            lake,
            cache_root: cache_root.into(),
        }
    }

    pub(crate) fn lake(&self) -> &LakeStore {
        &self.lake
    }

    pub async fn load_search_bundle(
        &self,
        bundle_version: &str,
    ) -> Result<LoadedServingBundle, ServingBundleLoadError> {
        let manifest_key = AssetPathBuilder::serving_bundle_key(bundle_version, "manifest.json");
        let manifest: ServingBundleManifest = self.lake.get_json(&manifest_key).await?;
        if manifest.bundle_version != bundle_version {
            return Err(ServingBundleLoadError::Configuration(format!(
                "serving manifest is bundle {}, expected {bundle_version}",
                manifest.bundle_version
            )));
        }
        if manifest.format_version != SERVING_BUNDLE_FORMAT_VERSION {
            return Err(ServingBundleLoadError::Configuration(format!(
                "serving bundle format is {}, expected {SERVING_BUNDLE_FORMAT_VERSION}",
                manifest.format_version
            )));
        }
        let cache_dir = self.cache_dir_for(&manifest);

        if !cache_dir.exists() {
            hydrate_atomically(&self.lake, &manifest, &cache_dir).await?;
        }

        let recall_index = TantivyRecallIndex::open(&cache_dir)?;
        let entities = load_entities(&self.lake, &manifest).await?;
        let entity_aliases = load_entity_aliases(&self.lake, &manifest).await?;
        validate_society_aliases(&entity_aliases, &entities)
            .map_err(|err| ServingBundleLoadError::Configuration(err.to_string()))?;
        let entity_alias_index = ServingEntityAliasIndex::from_records(entity_aliases)
            .map_err(|err| ServingBundleLoadError::Configuration(err.to_string()))?;
        let edges = load_edges(&self.lake, &manifest).await?;
        let aliases = super::types::unique_society_aliases(&entities);
        let mut fact_index = load_fact_index(&self.lake, &manifest).await?;
        validate_serving_edge_evidence(&edges, fact_index.all_facts(), &manifest.bundle_version)
            .map_err(ServingBundleLoadError::Configuration)?;
        let evidence_index = ServingEvidenceIndex::from_records(fact_index.all_facts(), &edges)
            .map_err(ServingBundleLoadError::Configuration)?;
        fact_index.add_society_aliases(&entities);
        fact_index.add_canonical_spatial_bindings(&edges);
        let mut rera_evidence_index = load_rera_evidence_index(&self.lake, &manifest).await?;
        rera_evidence_index.add_aliases(&aliases);
        let mut graph_index =
            GraphIndex::from_serving_bundle(&entities, &edges, &manifest.bundle_version);
        graph_index.add_entity_aliases(&aliases);
        let entity_index =
            SpatialEntityIndex::from_serving_bundle_with_edges(&entities, &fact_index, &edges);
        let spatial_index =
            SpatialServingIndex::from_serving_bundle_with_edges(&entities, &fact_index, &edges);
        let search_capabilities =
            crate::search::SearchCapabilityIndex::from_bundle(&entities, &fact_index);
        Ok(LoadedServingBundle {
            manifest,
            entities,
            entity_alias_index,
            edges,
            graph_index,
            recall_index,
            fact_index,
            evidence_index,
            rera_evidence_index,
            entity_index,
            spatial_index,
            search_capabilities,
            cache_dir,
        })
    }

    fn cache_dir_for(&self, manifest: &ServingBundleManifest) -> PathBuf {
        self.cache_root
            .join("search_bundle")
            .join(format!("version={}", manifest.bundle_version))
            .join("tantivy_index")
    }
}

async fn load_entities(
    lake: &LakeStore,
    manifest: &ServingBundleManifest,
) -> Result<Vec<ServingEntityRecord>, ServingBundleLoadError> {
    let entity_key =
        LakeKey::new(manifest.entity_parquet_key.clone()).map_err(ServingBundleLoadError::Key)?;
    let entity_bytes = lake.get_bytes(&entity_key).await?;
    Ok(read_entities_parquet(&entity_bytes)?)
}

async fn load_entity_aliases(
    lake: &LakeStore,
    manifest: &ServingBundleManifest,
) -> Result<Vec<ServingEntityAliasRecord>, ServingBundleLoadError> {
    let alias_key = LakeKey::new(manifest.entity_alias_parquet_key.clone())
        .map_err(ServingBundleLoadError::Key)?;
    let alias_bytes = lake.get_bytes(&alias_key).await?;
    Ok(read_entity_aliases_parquet(&alias_bytes)?)
}

async fn load_edges(
    lake: &LakeStore,
    manifest: &ServingBundleManifest,
) -> Result<Vec<ServingEdgeRecord>, ServingBundleLoadError> {
    let edge_key =
        LakeKey::new(manifest.edge_parquet_key.clone()).map_err(ServingBundleLoadError::Key)?;
    let edge_bytes = lake.get_bytes(&edge_key).await?;
    Ok(read_edges_parquet(&edge_bytes)?)
}

async fn load_fact_index(
    lake: &LakeStore,
    manifest: &ServingBundleManifest,
) -> Result<ServingFactIndex, ServingBundleLoadError> {
    let fact_key =
        LakeKey::new(manifest.fact_parquet_key.clone()).map_err(ServingBundleLoadError::Key)?;
    let search_metadata_key = LakeKey::new(manifest.search_metadata_parquet_key.clone())
        .map_err(ServingBundleLoadError::Key)?;
    let fact_bytes = lake.get_bytes(&fact_key).await?;
    let search_metadata_bytes = lake.get_bytes(&search_metadata_key).await?;
    let facts = read_facts_parquet(&fact_bytes)?;
    let search_metadata = read_search_metadata_parquet(&search_metadata_bytes)?;
    Ok(ServingFactIndex::from_records(facts, search_metadata))
}

async fn load_rera_evidence_index(
    lake: &LakeStore,
    manifest: &ServingBundleManifest,
) -> Result<ReraEvidenceIndex, ServingBundleLoadError> {
    let key = LakeKey::new(manifest.rera_evidence_parquet_key.clone())
        .map_err(ServingBundleLoadError::Key)?;
    let bytes = lake.get_bytes(&key).await?;
    Ok(ReraEvidenceIndex::from_records(read_rera_evidence_parquet(
        &bytes,
    )?))
}

async fn hydrate_atomically(
    lake: &LakeStore,
    manifest: &ServingBundleManifest,
    cache_dir: &Path,
) -> Result<(), ServingBundleLoadError> {
    if let Some(parent) = cache_dir.parent() {
        std::fs::create_dir_all(parent).map_err(ServingBundleLoadError::Io)?;
    }

    let temp_dir = cache_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("_hydrating-{}", uuid::Uuid::new_v4()));
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir).map_err(ServingBundleLoadError::Io)?;
    }

    hydrate_tantivy_index(lake, manifest, &temp_dir).await?;
    match std::fs::rename(&temp_dir, cache_dir) {
        Ok(()) => Ok(()),
        Err(_err) if cache_dir.exists() => {
            let _ = std::fs::remove_dir_all(&temp_dir);
            Ok(())
        }
        Err(err) => {
            let _ = std::fs::remove_dir_all(&temp_dir);
            Err(ServingBundleLoadError::Io(err))
        }
    }
}

#[derive(Debug)]
pub enum ServingBundleLoadError {
    Configuration(String),
    Io(std::io::Error),
    Key(crate::lake::keys::KeyError),
    Lake(LakeError),
    Parquet(ParquetReadError),
    Tantivy(TantivyIndexError),
}

impl fmt::Display for ServingBundleLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(message) => {
                write!(f, "serving bundle configuration error: {message}")
            }
            Self::Io(err) => write!(f, "serving bundle load IO error: {err}"),
            Self::Key(err) => write!(f, "serving bundle manifest key error: {err}"),
            Self::Lake(err) => write!(f, "serving bundle load lake error: {err}"),
            Self::Parquet(err) => write!(f, "serving bundle Parquet load error: {err}"),
            Self::Tantivy(err) => write!(f, "serving bundle recall index error: {err}"),
        }
    }
}

impl std::error::Error for ServingBundleLoadError {}

impl From<std::io::Error> for ServingBundleLoadError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<LakeError> for ServingBundleLoadError {
    fn from(err: LakeError) -> Self {
        Self::Lake(err)
    }
}

impl From<TantivyIndexError> for ServingBundleLoadError {
    fn from(err: TantivyIndexError) -> Self {
        Self::Tantivy(err)
    }
}

impl From<ParquetReadError> for ServingBundleLoadError {
    fn from(err: ParquetReadError) -> Self {
        Self::Parquet(err)
    }
}
