use std::fmt;
use std::path::{Path, PathBuf};

use crate::assets::{
    AssetId, AssetMaterializationStore, AssetPartition, AssetPathBuilder, MaterializationRecord,
    MaterializationStatus,
};
use crate::lake::{LakeError, LakeKey, LakeStore};

use super::{
    hydrate_tantivy_index, read_edges_parquet, read_embeddings_parquet, read_entities_parquet,
    read_facts_parquet, read_search_metadata_parquet, ParquetReadError, ServingBundleManifest,
    ServingEdgeRecord, ServingEmbeddingRecord, ServingEntityRecord, ServingFactIndex,
    SpatialServingIndex, TantivyIndexError, TantivyRecallIndex, SEARCH_SERVING_BUNDLE_ASSET_ID,
};
use crate::graph::GraphIndex;
use crate::search::geo::GeoSearchIndex;

#[derive(Clone)]
pub struct ServingBundleLoader {
    lake: LakeStore,
    materializations: AssetMaterializationStore,
    cache_root: PathBuf,
}

pub struct LoadedServingBundle {
    pub manifest: ServingBundleManifest,
    pub entities: Vec<ServingEntityRecord>,
    pub edges: Vec<ServingEdgeRecord>,
    pub graph_index: GraphIndex,
    pub recall_index: TantivyRecallIndex,
    pub fact_index: ServingFactIndex,
    pub geo_index: GeoSearchIndex,
    pub spatial_index: SpatialServingIndex,
    pub semantic_embeddings: Vec<ServingEmbeddingRecord>,
    pub cache_dir: PathBuf,
}

impl ServingBundleLoader {
    pub fn new(lake: LakeStore, cache_root: impl Into<PathBuf>) -> Self {
        let materializations = AssetMaterializationStore::new(lake.clone());
        Self {
            lake,
            materializations,
            cache_root: cache_root.into(),
        }
    }

    pub async fn load_current_search_bundle(
        &self,
    ) -> Result<Option<LoadedServingBundle>, ServingBundleLoadError> {
        let asset_id = AssetId::new(SEARCH_SERVING_BUNDLE_ASSET_ID)
            .expect("search serving bundle asset id is static and valid");
        let partition = AssetPartition::global();
        let record = match self
            .materializations
            .current_record(&asset_id, &partition)
            .await
        {
            Ok(record) => record,
            Err(err) if err.is_not_found() => return Ok(None),
            Err(err) => return Err(ServingBundleLoadError::Lake(err)),
        };

        if record.status != MaterializationStatus::Succeeded {
            return Err(ServingBundleLoadError::CurrentMaterializationNotSucceeded {
                asset_id: record.asset_id.to_string(),
                status: record.status,
            });
        }

        let manifest_key = manifest_key_for_record(&record)?;
        let manifest: ServingBundleManifest = self.lake.get_json(&manifest_key).await?;
        let cache_dir = self.cache_dir_for(&record);

        if !cache_dir.exists() {
            hydrate_atomically(&self.lake, &manifest, &cache_dir).await?;
        }

        let recall_index = TantivyRecallIndex::open(&cache_dir)?;
        let entities = load_entities(&self.lake, &manifest).await?;
        let edges = load_edges(&self.lake, &manifest).await?;
        let fact_index = load_fact_index(&self.lake, &manifest).await?;
        let graph_index = GraphIndex::from_serving_edges(&edges);
        let geo_index = GeoSearchIndex::from_serving_bundle(&entities, &fact_index);
        let spatial_index = SpatialServingIndex::from_serving_bundle(&entities, &fact_index);
        let semantic_embeddings = load_semantic_embeddings(&self.lake, &manifest).await?;
        Ok(Some(LoadedServingBundle {
            manifest,
            entities,
            edges,
            graph_index,
            recall_index,
            fact_index,
            geo_index,
            spatial_index,
            semantic_embeddings,
            cache_dir,
        }))
    }

    fn cache_dir_for(&self, record: &MaterializationRecord) -> PathBuf {
        self.cache_root
            .join("search_bundle")
            .join(format!("materialization={}", record.materialization_id))
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

async fn load_edges(
    lake: &LakeStore,
    manifest: &ServingBundleManifest,
) -> Result<Vec<ServingEdgeRecord>, ServingBundleLoadError> {
    let Some(edge_key) = manifest.edge_parquet_key.as_ref() else {
        return Ok(Vec::new());
    };
    let edge_key = LakeKey::new(edge_key.clone()).map_err(ServingBundleLoadError::Key)?;
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

async fn load_semantic_embeddings(
    lake: &LakeStore,
    manifest: &ServingBundleManifest,
) -> Result<Vec<ServingEmbeddingRecord>, ServingBundleLoadError> {
    let Some(embedding_key) = manifest.semantic_embedding_parquet_key.as_ref() else {
        return Ok(Vec::new());
    };
    let embedding_key = LakeKey::new(embedding_key.clone()).map_err(ServingBundleLoadError::Key)?;
    let embedding_bytes = lake.get_bytes(&embedding_key).await?;
    Ok(read_embeddings_parquet(&embedding_bytes)?)
}

fn manifest_key_for_record(
    record: &MaterializationRecord,
) -> Result<LakeKey, ServingBundleLoadError> {
    let key = record
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.content_type == "application/json" && artifact.key.ends_with("/manifest.json")
        })
        .map(|artifact| artifact.key.clone())
        .unwrap_or_else(|| {
            AssetPathBuilder::serving_bundle_key(&record.version, "manifest.json").to_string()
        });
    LakeKey::new(key).map_err(ServingBundleLoadError::Key)
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
    Io(std::io::Error),
    Key(crate::lake::keys::KeyError),
    Lake(LakeError),
    Parquet(ParquetReadError),
    Tantivy(TantivyIndexError),
    CurrentMaterializationNotSucceeded {
        asset_id: String,
        status: MaterializationStatus,
    },
}

impl fmt::Display for ServingBundleLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "serving bundle load IO error: {err}"),
            Self::Key(err) => write!(f, "serving bundle manifest key error: {err}"),
            Self::Lake(err) => write!(f, "serving bundle load lake error: {err}"),
            Self::Parquet(err) => write!(f, "serving bundle Parquet load error: {err}"),
            Self::Tantivy(err) => write!(f, "serving bundle recall index error: {err}"),
            Self::CurrentMaterializationNotSucceeded { asset_id, status } => write!(
                f,
                "current materialization for {asset_id} is not succeeded: {status:?}"
            ),
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
