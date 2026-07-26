#[cfg(feature = "fastembed")]
use std::path::PathBuf;

#[cfg(feature = "fastembed")]
use backend::assets::{
    ArtifactRef, AssetId, AssetMaterializationStore, AssetPartition, AssetPathBuilder, AssetStage,
    MaterializationRecord,
};
#[cfg(feature = "fastembed")]
use backend::data_loader::{properties_from_serving_bundle, semantic_serving_entities_for_bundle};
#[cfg(feature = "fastembed")]
use backend::lake::{ArtifactMetadata, LakeKey, LakePrefix, LakeStore, LakeStoreLocation};
#[cfg(feature = "fastembed")]
use backend::search::{
    semantic_embedding_documents_from_serving_entities, FastEmbedSemanticEmbedder, SemanticEmbedder,
};
#[cfg(feature = "fastembed")]
use backend::serving::builder::{serving_bundle_schema_descriptor, SERVING_BUNDLE_FORMAT_VERSION};
#[cfg(feature = "fastembed")]
use backend::serving::{
    write_embeddings_parquet, BundleArtifact, BundleArtifactKind, ServingBundleLoader,
    ServingBundleManifest, ServingEmbeddingRecord, SEARCH_SERVING_BUNDLE_ASSET_ID,
};
#[cfg(feature = "fastembed")]
use chrono::Utc;
#[cfg(feature = "fastembed")]
use sha2::{Digest, Sha256};

#[cfg(feature = "fastembed")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = CliOptions::parse()?;
    let project_root = options
        .project_root
        .clone()
        .unwrap_or_else(default_project_root);
    let lake_location = LakeStoreLocation::from_env(&project_root)?;
    let lake = lake_location.open()?;
    let materializations = AssetMaterializationStore::new(lake.clone());
    let asset_id = AssetId::new(SEARCH_SERVING_BUNDLE_ASSET_ID)?;
    let partition = AssetPartition::global();
    let current_record = materializations
        .current_record(&asset_id, &partition)
        .await?;
    let current_manifest_key = manifest_key_for_record(&current_record)?;
    let current_manifest: ServingBundleManifest = lake.get_json(&current_manifest_key).await?;

    let cache_root = project_root.join("data").join("cache").join("serving");
    let bundle = ServingBundleLoader::new(lake.clone(), cache_root)
        .load_current_search_bundle()
        .await?
        .ok_or("no promoted search serving bundle found")?;
    let properties = properties_from_serving_bundle(&bundle);
    let semantic_entities = semantic_serving_entities_for_bundle(&bundle, &properties);
    let documents = semantic_embedding_documents_from_serving_entities(&semantic_entities);
    if documents.is_empty() {
        return Err("current serving bundle produced no semantic documents".into());
    }

    let embedder = FastEmbedSemanticEmbedder::try_new_all_minilm_l6_v2()
        .map_err(|err| format!("failed to initialize fastembed: {err}"))?;
    let texts = documents
        .iter()
        .map(|document| document.text.clone())
        .collect::<Vec<_>>();
    eprintln!(
        "Embedding {} documents with {} ({} dimensions)",
        texts.len(),
        embedder.model_id(),
        embedder.dimensions()
    );
    let vectors = embedder.embed_batch(&texts);
    if vectors.len() != documents.len() {
        return Err(format!(
            "fastembed returned {} vectors for {} documents",
            vectors.len(),
            documents.len()
        )
        .into());
    }

    let dimensions = u32::try_from(embedder.dimensions())?;
    let records = documents
        .into_iter()
        .zip(vectors)
        .map(|(document, embedding)| {
            if embedding.len() != embedder.dimensions() {
                return Err(format!(
                    "{} produced vector length {}, expected {}",
                    document.entity_id,
                    embedding.len(),
                    embedder.dimensions()
                ));
            }
            Ok(ServingEmbeddingRecord {
                entity_id: document.entity_id,
                entity_type: document.entity_type,
                model_id: embedder.model_id().to_string(),
                dimensions,
                document_text_hash: sha256_hex(document.text.as_bytes()),
                embedding,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let new_version = options.bundle_version.unwrap_or_else(|| {
        format!(
            "{}-semantic-{}",
            current_manifest.bundle_version,
            Utc::now().format("%Y%m%d%H%M%S")
        )
    });
    let manifest =
        write_versioned_bundle_with_embeddings(&lake, &current_manifest, &new_version, &records)
            .await?;
    let manifest_key =
        AssetPathBuilder::serving_bundle_key(&manifest.bundle_version, "manifest.json");
    let manifest_meta = lake.put_json(&manifest_key, &manifest).await?;
    let record = MaterializationRecord::succeeded(
        asset_id,
        AssetStage::Serving,
        partition,
        manifest.bundle_version.clone(),
        vec![ArtifactRef::json(manifest_meta)],
    )
    .with_parent_materializations(vec![current_record.materialization_id.clone()])
    .with_source_watermarks(current_record.source_watermarks.clone())
    .with_row_count(manifest.entity_count);
    materializations.write_materialization(&record).await?;
    if options.promote {
        materializations.promote_current(&record).await?;
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "bundle_version": manifest.bundle_version,
            "promoted": options.promote,
            "embedding_key": manifest.semantic_embedding_parquet_key,
            "embedding_rows": records.len(),
            "model_id": embedder.model_id(),
            "dimensions": embedder.dimensions(),
        }))?
    );
    Ok(())
}

#[cfg(not(feature = "fastembed"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err("openestates-build-semantic-embeddings must be built with --features fastembed".into())
}

#[cfg(feature = "fastembed")]
async fn write_versioned_bundle_with_embeddings(
    lake: &LakeStore,
    current: &ServingBundleManifest,
    new_version: &str,
    records: &[ServingEmbeddingRecord],
) -> Result<ServingBundleManifest, Box<dyn std::error::Error>> {
    let mut artifacts = Vec::new();

    let entity_key =
        AssetPathBuilder::serving_bundle_key(new_version, "entities/part-00000.parquet");
    artifacts.push(
        copy_artifact(
            lake,
            &current.entity_parquet_key,
            &entity_key,
            BundleArtifactKind::EntitiesParquet,
            "application/vnd.apache.parquet",
            Some(current.entity_count),
        )
        .await?,
    );

    let fact_key = AssetPathBuilder::serving_bundle_key(new_version, "facts/part-00000.parquet");
    artifacts.push(
        copy_artifact(
            lake,
            &current.fact_parquet_key,
            &fact_key,
            BundleArtifactKind::FactsParquet,
            "application/vnd.apache.parquet",
            Some(current.fact_count),
        )
        .await?,
    );

    let search_metadata_key =
        AssetPathBuilder::serving_bundle_key(new_version, "search_metadata/part-00000.parquet");
    artifacts.push(
        copy_artifact(
            lake,
            &current.search_metadata_parquet_key,
            &search_metadata_key,
            BundleArtifactKind::SearchMetadataParquet,
            "application/vnd.apache.parquet",
            Some(current.search_metadata_count),
        )
        .await?,
    );

    let edge_key = if let Some(current_edge_key) = current.edge_parquet_key.as_ref() {
        let edge_key =
            AssetPathBuilder::serving_bundle_key(new_version, "edges/part-00000.parquet");
        artifacts.push(
            copy_artifact(
                lake,
                current_edge_key,
                &edge_key,
                BundleArtifactKind::EdgesParquet,
                "application/vnd.apache.parquet",
                Some(current.edge_count),
            )
            .await?,
        );
        Some(edge_key.to_string())
    } else {
        None
    };

    let embedding_key =
        AssetPathBuilder::serving_bundle_key(new_version, "semantic_embeddings/part-00000.parquet");
    let embedding_meta = lake
        .put_bytes(&embedding_key, write_embeddings_parquet(records)?)
        .await?;
    artifacts.push(bundle_artifact(
        BundleArtifactKind::SemanticEmbeddingsParquet,
        embedding_meta,
        "application/vnd.apache.parquet",
        Some(records.len() as u64),
    ));

    let schema_key = AssetPathBuilder::serving_bundle_key(new_version, "schema.json");
    let schema_meta = lake
        .put_json(
            &schema_key,
            &serving_bundle_schema_descriptor(SERVING_BUNDLE_FORMAT_VERSION),
        )
        .await?;
    artifacts.push(bundle_artifact(
        BundleArtifactKind::SchemaJson,
        schema_meta,
        "application/json",
        None,
    ));

    let trust_policy_key = AssetPathBuilder::serving_bundle_key(new_version, "trust_policy.json");
    artifacts.push(
        copy_artifact(
            lake,
            &current.trust_policy_key,
            &trust_policy_key,
            BundleArtifactKind::TrustPolicyJson,
            "application/json",
            None,
        )
        .await?,
    );

    let tantivy_index_prefix =
        AssetPathBuilder::serving_bundle_key(new_version, "tantivy_index").to_string();
    let current_tantivy_prefix = LakePrefix::new(current.tantivy_index_prefix.clone())?;
    let tantivy_keys = lake.list_keys(&current_tantivy_prefix).await?;
    if tantivy_keys.is_empty() {
        return Err(format!(
            "no Tantivy files found under {}",
            current.tantivy_index_prefix
        )
        .into());
    }
    for key in tantivy_keys {
        let relative = key
            .as_str()
            .strip_prefix(current_tantivy_prefix.as_str())
            .unwrap_or(key.as_str())
            .trim_start_matches('/');
        let target_key =
            AssetPathBuilder::serving_bundle_key(new_version, &format!("tantivy_index/{relative}"));
        artifacts.push(
            copy_artifact(
                lake,
                key.as_str(),
                &target_key,
                BundleArtifactKind::TantivyIndexFile,
                "application/octet-stream",
                None,
            )
            .await?,
        );
    }

    Ok(ServingBundleManifest {
        bundle_version: new_version.to_string(),
        format_version: SERVING_BUNDLE_FORMAT_VERSION,
        created_at: Utc::now(),
        entity_count: current.entity_count,
        fact_count: current.fact_count,
        search_metadata_count: current.search_metadata_count,
        edge_count: current.edge_count,
        entity_parquet_key: entity_key.to_string(),
        fact_parquet_key: fact_key.to_string(),
        search_metadata_parquet_key: search_metadata_key.to_string(),
        edge_parquet_key: edge_key,
        semantic_embedding_parquet_key: Some(embedding_key.to_string()),
        schema_key: schema_key.to_string(),
        trust_policy_key: trust_policy_key.to_string(),
        tantivy_index_prefix,
        artifacts,
    })
}

#[cfg(feature = "fastembed")]
async fn copy_artifact(
    lake: &LakeStore,
    source_key: &str,
    target_key: &LakeKey,
    kind: BundleArtifactKind,
    format: &str,
    row_count: Option<u64>,
) -> Result<BundleArtifact, Box<dyn std::error::Error>> {
    let source_key = LakeKey::new(source_key.to_string())?;
    let bytes = lake.get_bytes(&source_key).await?;
    let meta = lake.put_bytes(target_key, bytes).await?;
    Ok(bundle_artifact(kind, meta, format, row_count))
}

#[cfg(feature = "fastembed")]
fn bundle_artifact(
    kind: BundleArtifactKind,
    meta: ArtifactMetadata,
    format: &str,
    row_count: Option<u64>,
) -> BundleArtifact {
    BundleArtifact {
        kind,
        key: meta.key.to_string(),
        format: format.to_string(),
        content_hash: meta.content_hash,
        hash_algorithm: meta.hash_algorithm,
        size_bytes: meta.size_bytes,
        row_count,
    }
}

#[cfg(feature = "fastembed")]
fn manifest_key_for_record(
    record: &MaterializationRecord,
) -> Result<LakeKey, Box<dyn std::error::Error>> {
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
    Ok(LakeKey::new(key)?)
}

#[cfg(feature = "fastembed")]
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(feature = "fastembed")]
fn default_project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend crate should live under project root")
        .to_path_buf()
}

#[cfg(feature = "fastembed")]
#[derive(Default)]
struct CliOptions {
    project_root: Option<PathBuf>,
    bundle_version: Option<String>,
    promote: bool,
}

#[cfg(feature = "fastembed")]
impl CliOptions {
    fn parse() -> Result<Self, String> {
        let mut options = CliOptions {
            promote: true,
            ..Default::default()
        };
        let mut args = std::env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--project-root" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--project-root requires a value".to_string())?;
                    options.project_root = Some(PathBuf::from(value));
                }
                "--bundle-version" => {
                    options.bundle_version = Some(
                        args.next()
                            .ok_or_else(|| "--bundle-version requires a value".to_string())?,
                    );
                }
                "--no-promote" => {
                    options.promote = false;
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument: {other}")),
            }
        }
        Ok(options)
    }
}

#[cfg(feature = "fastembed")]
fn print_help() {
    println!("Build offline FastEmbed semantic embeddings for the promoted search serving bundle.");
    println!();
    println!("Usage:");
    println!(
        "  cargo run --features fastembed --bin openestates-build-semantic-embeddings -- [--project-root <path>] [--bundle-version <version>] [--no-promote]"
    );
    println!();
    println!("Environment:");
    println!("  OPENESTATES_ONNXRUNTIME_DYLIB=/absolute/path/to/libonnxruntime.so");
    println!(
        "  {env_name}=file:///absolute/path or s3://bucket/optional/prefix",
        env_name = backend::lake::LAKE_URL_ENV
    );
}
