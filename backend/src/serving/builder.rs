use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::assets::{
    AssetPathBuilder, KgViewEdgeRecord, KgViewFactAnnotationRecord, KgViewFactRecord, KgViewRecords,
};
use crate::dag_config::{load_fact_registry_index, scoring_direction_from_hint, DagConfigError};
use crate::knowledge::{FactValue, KnowledgeGraph};
use crate::lake::{ArtifactMetadata, LakeError, LakeKey, LakeStore};
use crate::search::schema;

use super::parquet::{
    write_edges_parquet, write_entities_parquet, write_facts_parquet,
    write_search_metadata_parquet, ParquetWriteError,
};
use super::proximity::derive_proximity_records;
use super::tantivy_index::{TantivyIndexError, TantivyRecallIndex};
use super::{
    BundleArtifact, BundleArtifactKind, ServingBundleManifest, ServingBundleSchema,
    ServingColumnSchema, ServingEdgeRecord, ServingEntityRecord, ServingFactRecord,
    ServingSearchMetadataRecord, ServingTableSchema, TrustPolicy,
};

pub const SERVING_BUNDLE_FORMAT_VERSION: u32 = 5;

#[derive(Clone)]
pub struct ServingBundleBuilder {
    lake: LakeStore,
}

impl ServingBundleBuilder {
    pub fn new(lake: LakeStore) -> Self {
        Self { lake }
    }

    pub async fn build_from_graph(
        &self,
        graph: &KnowledgeGraph,
        bundle_version: impl Into<String>,
    ) -> Result<ServingBundleManifest, ServingBundleError> {
        let records = KgViewRecords::from_graph(graph)?;
        self.build_from_kg_view_records(&records, bundle_version)
            .await
    }

    pub async fn build_from_kg_view_records(
        &self,
        records: &KgViewRecords,
        bundle_version: impl Into<String>,
    ) -> Result<ServingBundleManifest, ServingBundleError> {
        let current_facts = current_serving_facts(&records.facts);
        let entities = serving_entity_records(records, &current_facts);
        let facts = serving_fact_records(&current_facts)?;
        let current_annotations = current_serving_annotations(&records.fact_annotations);
        let search_metadata =
            serving_search_metadata_records(&current_facts, &current_annotations)?;
        let edges = serving_edge_records(&records.edges);
        self.build_from_serving_records(
            entities,
            facts,
            search_metadata,
            edges,
            bundle_version,
            true,
        )
        .await
    }

    pub async fn build_child_from_serving_records(
        &self,
        mut entities: Vec<ServingEntityRecord>,
        facts: Vec<ServingFactRecord>,
        search_metadata: Vec<ServingSearchMetadataRecord>,
        edges: Vec<ServingEdgeRecord>,
        bundle_version: impl Into<String>,
    ) -> Result<ServingBundleManifest, ServingBundleError> {
        rebuild_serving_entity_searchable_text(&mut entities, &facts);
        self.build_from_serving_records(
            entities,
            facts,
            search_metadata,
            edges,
            bundle_version,
            false,
        )
        .await
    }

    async fn build_from_serving_records(
        &self,
        entities: Vec<ServingEntityRecord>,
        mut facts: Vec<ServingFactRecord>,
        mut search_metadata: Vec<ServingSearchMetadataRecord>,
        mut edges: Vec<ServingEdgeRecord>,
        bundle_version: impl Into<String>,
        derive_proximity: bool,
    ) -> Result<ServingBundleManifest, ServingBundleError> {
        let bundle_version = bundle_version.into();
        let mut artifacts = Vec::new();
        if derive_proximity {
            let base_index =
                super::ServingFactIndex::from_records(facts.clone(), search_metadata.clone());
            let derived = derive_proximity_records(&entities, &base_index, &edges)?;
            facts.extend(derived.facts);
            search_metadata.extend(derived.search_metadata);
            edges.extend(derived.edges);
        }
        if let Err(err) = write_preference_coverage_report(&entities, &facts, &search_metadata) {
            eprintln!("preference coverage report skipped: {err}");
        }

        let entity_key =
            AssetPathBuilder::serving_bundle_key(&bundle_version, "entities/part-00000.parquet");
        let entity_bytes = write_entities_parquet(&entities)?;
        let entity_meta = self.lake.put_bytes(&entity_key, entity_bytes).await?;
        artifacts.push(artifact(
            BundleArtifactKind::EntitiesParquet,
            entity_meta,
            "application/vnd.apache.parquet",
            Some(entities.len() as u64),
        ));

        let fact_key =
            AssetPathBuilder::serving_bundle_key(&bundle_version, "facts/part-00000.parquet");
        let fact_bytes = write_facts_parquet(&facts)?;
        let fact_meta = self.lake.put_bytes(&fact_key, fact_bytes).await?;
        artifacts.push(artifact(
            BundleArtifactKind::FactsParquet,
            fact_meta,
            "application/vnd.apache.parquet",
            Some(facts.len() as u64),
        ));

        let edge_key =
            AssetPathBuilder::serving_bundle_key(&bundle_version, "edges/part-00000.parquet");
        let edge_bytes = write_edges_parquet(&edges)?;
        let edge_meta = self.lake.put_bytes(&edge_key, edge_bytes).await?;
        artifacts.push(artifact(
            BundleArtifactKind::EdgesParquet,
            edge_meta,
            "application/vnd.apache.parquet",
            Some(edges.len() as u64),
        ));

        let search_metadata_key = AssetPathBuilder::serving_bundle_key(
            &bundle_version,
            "search_metadata/part-00000.parquet",
        );
        let search_metadata_bytes = write_search_metadata_parquet(&search_metadata)?;
        let search_metadata_meta = self
            .lake
            .put_bytes(&search_metadata_key, search_metadata_bytes)
            .await?;
        artifacts.push(artifact(
            BundleArtifactKind::SearchMetadataParquet,
            search_metadata_meta,
            "application/vnd.apache.parquet",
            Some(search_metadata.len() as u64),
        ));

        let schema_key = AssetPathBuilder::serving_bundle_key(&bundle_version, "schema.json");
        let schema_descriptor = serving_bundle_schema_descriptor(SERVING_BUNDLE_FORMAT_VERSION);
        let schema_meta = self.lake.put_json(&schema_key, &schema_descriptor).await?;
        artifacts.push(artifact(
            BundleArtifactKind::SchemaJson,
            schema_meta,
            "application/json",
            None,
        ));

        let trust_policy = TrustPolicy::default();
        let trust_policy_key =
            AssetPathBuilder::serving_bundle_key(&bundle_version, "trust_policy.json");
        let trust_policy_meta = self.lake.put_json(&trust_policy_key, &trust_policy).await?;
        artifacts.push(artifact(
            BundleArtifactKind::TrustPolicyJson,
            trust_policy_meta,
            "application/json",
            None,
        ));

        let tantivy_prefix =
            AssetPathBuilder::serving_bundle_key(&bundle_version, "tantivy_index").to_string();
        let temp_index_dir = temp_index_dir(&bundle_version);
        if temp_index_dir.exists() {
            std::fs::remove_dir_all(&temp_index_dir).map_err(ServingBundleError::Io)?;
        }
        TantivyRecallIndex::build_in_dir(&temp_index_dir, &entities, &facts, &search_metadata)?;
        artifacts.extend(upload_tantivy_dir(&self.lake, &temp_index_dir, &tantivy_prefix).await?);
        let _ = std::fs::remove_dir_all(&temp_index_dir);

        let manifest = ServingBundleManifest {
            bundle_version,
            format_version: SERVING_BUNDLE_FORMAT_VERSION,
            created_at: Utc::now(),
            entity_count: entities.len() as u64,
            fact_count: facts.len() as u64,
            search_metadata_count: search_metadata.len() as u64,
            edge_count: edges.len() as u64,
            entity_parquet_key: entity_key.to_string(),
            fact_parquet_key: fact_key.to_string(),
            search_metadata_parquet_key: search_metadata_key.to_string(),
            edge_parquet_key: Some(edge_key.to_string()),
            schema_key: schema_key.to_string(),
            trust_policy_key: trust_policy_key.to_string(),
            tantivy_index_prefix: tantivy_prefix,
            artifacts,
        };

        let manifest_key =
            AssetPathBuilder::serving_bundle_key(&manifest.bundle_version, "manifest.json");
        self.lake.put_json(&manifest_key, &manifest).await?;
        Ok(manifest)
    }
}

fn rebuild_serving_entity_searchable_text(
    entities: &mut [ServingEntityRecord],
    facts: &[ServingFactRecord],
) {
    let mut fact_text_by_entity = HashMap::<&str, String>::new();
    for fact in facts {
        let text = fact_text_by_entity
            .entry(fact.entity_id.as_str())
            .or_default();
        text.push(' ');
        text.push_str(&fact.fact_key);
        match &fact.value {
            FactValue::Text(value) => {
                text.push(' ');
                text.push_str(value);
            }
            FactValue::Tags(values) => {
                for value in values {
                    text.push(' ');
                    text.push_str(value);
                }
            }
            FactValue::Score { explanation, .. } => {
                text.push(' ');
                text.push_str(explanation);
            }
            FactValue::Numeric(_) | FactValue::Bool(_) => {
                if let Some(value) = &fact.value_text {
                    text.push(' ');
                    text.push_str(value);
                }
            }
        }
    }
    for entity in entities {
        entity.searchable_text = format!(
            "{} {} {} {}",
            entity.entity_id,
            entity.entity_type,
            entity.name,
            fact_text_by_entity
                .get(entity.entity_id.as_str())
                .map(String::as_str)
                .unwrap_or("")
        );
    }
}

pub fn serving_bundle_schema_descriptor(format_version: u32) -> ServingBundleSchema {
    ServingBundleSchema {
        format_version,
        storage_format: "parquet+tantivy".to_string(),
        fact_schema_registry_version: schema::registry().version,
        tables: vec![
            ServingTableSchema {
                name: "entities".to_string(),
                path: "entities/part-00000.parquet".to_string(),
                columns: vec![
                    required_column("entity_id", "utf8"),
                    required_column("entity_type", "utf8"),
                    required_column("name", "utf8"),
                    optional_column("root_source", "utf8"),
                    required_column("searchable_text", "utf8"),
                ],
            },
            ServingTableSchema {
                name: "facts".to_string(),
                path: "facts/part-00000.parquet".to_string(),
                columns: vec![
                    required_column("entity_id", "utf8"),
                    required_column("fact_key", "utf8"),
                    required_column("value_type", "utf8"),
                    optional_column("value_text", "utf8"),
                    optional_column("value_number", "float64"),
                    optional_column("value_bool", "bool"),
                    optional_column("value_tags", "list<utf8>"),
                    optional_column("value_score", "float64"),
                    optional_column("value_score_explanation", "utf8"),
                    required_column("confidence", "float32"),
                    required_column("source_type", "utf8"),
                    optional_column("source_url", "utf8"),
                    optional_column("model", "utf8"),
                    optional_column("skill_id", "utf8"),
                    required_column("learned_at", "timestamp_rfc3339"),
                ],
            },
            ServingTableSchema {
                name: "edges".to_string(),
                path: "edges/part-00000.parquet".to_string(),
                columns: vec![
                    required_column("from_entity_id", "utf8"),
                    required_column("edge_type", "utf8"),
                    required_column("to_entity_id", "utf8"),
                    required_column("confidence", "float32"),
                    required_column("source_type", "utf8"),
                ],
            },
            ServingTableSchema {
                name: "search_metadata".to_string(),
                path: "search_metadata/part-00000.parquet".to_string(),
                columns: vec![
                    required_column("entity_id", "utf8"),
                    required_column("fact_key", "utf8"),
                    optional_column("display_template", "utf8"),
                    required_column("answers_preferences", "list<utf8>"),
                    optional_column("scoring_direction", "utf8"),
                    optional_column("scoring_weight", "float32"),
                    required_column("scoring_thresholds", "list<float64>"),
                ],
            },
        ],
    }
}

fn required_column(name: &str, logical_type: &str) -> ServingColumnSchema {
    ServingColumnSchema {
        name: name.to_string(),
        logical_type: logical_type.to_string(),
        required: true,
    }
}

fn optional_column(name: &str, logical_type: &str) -> ServingColumnSchema {
    ServingColumnSchema {
        name: name.to_string(),
        logical_type: logical_type.to_string(),
        required: false,
    }
}

fn serving_edge_records(edges: &[KgViewEdgeRecord]) -> Vec<ServingEdgeRecord> {
    edges
        .iter()
        .map(|edge| ServingEdgeRecord {
            from_entity_id: edge.from_entity_id.clone(),
            edge_type: serving_edge_type(&edge.relation),
            to_entity_id: edge.to_entity_id.clone(),
            confidence: edge.weight,
            source_type: edge.source_type.clone(),
        })
        .collect()
}

fn serving_edge_type(relation: &str) -> String {
    match relation {
        "SocietyInArea" => "in_area".to_string(),
        "BuiltBy" => "built_by".to_string(),
        "PropertyInSociety" => "in_society".to_string(),
        other => other.to_string(),
    }
}

fn serving_entity_records(
    records: &KgViewRecords,
    current_facts: &[KgViewFactRecord],
) -> Vec<ServingEntityRecord> {
    let fact_text_by_entity = serving_fact_text_by_entity(current_facts);
    records
        .entities
        .iter()
        .map(|node| ServingEntityRecord {
            entity_id: node.entity_id.clone(),
            entity_type: node.entity_type.clone(),
            name: node.name.clone(),
            root_source: node.root_source.clone(),
            searchable_text: format!(
                "{} {} {} {}",
                node.entity_id,
                node.entity_type,
                node.name,
                fact_text_by_entity
                    .get(&node.entity_id)
                    .map(String::as_str)
                    .unwrap_or("")
            ),
        })
        .collect()
}

fn current_serving_facts(facts: &[KgViewFactRecord]) -> Vec<KgViewFactRecord> {
    let mut current = BTreeMap::<ServingFactKey, &KgViewFactRecord>::new();
    for fact in facts {
        let key = ServingFactKey {
            entity_id: fact.entity_id.as_str(),
            fact_key: fact.fact_key.as_str(),
            source_type: fact.source_type.as_str(),
            source_url: fact.source_url.as_deref(),
            skill_id: fact.skill_id.as_deref(),
        };
        match current.get(&key) {
            Some(existing)
                if fact.fact_version > existing.fact_version
                    || (fact.fact_version == existing.fact_version
                        && (fact.confidence > existing.confidence
                            || ((fact.confidence - existing.confidence).abs() < f32::EPSILON
                                && fact.learned_at > existing.learned_at))) =>
            {
                current.insert(key, fact);
            }
            None => {
                current.insert(key, fact);
            }
            Some(_) => {}
        }
    }
    current.into_values().cloned().collect()
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ServingFactKey<'a> {
    entity_id: &'a str,
    fact_key: &'a str,
    source_type: &'a str,
    source_url: Option<&'a str>,
    skill_id: Option<&'a str>,
}

fn current_serving_annotations(
    annotations: &[KgViewFactAnnotationRecord],
) -> Vec<KgViewFactAnnotationRecord> {
    let mut current = BTreeMap::<(&str, &str), &KgViewFactAnnotationRecord>::new();
    for annotation in annotations {
        current.insert(
            (annotation.entity_id.as_str(), annotation.fact_key.as_str()),
            annotation,
        );
    }
    current.into_values().cloned().collect()
}

fn serving_fact_text_by_entity(facts: &[KgViewFactRecord]) -> HashMap<String, String> {
    let mut by_entity = HashMap::<String, String>::new();
    for fact in facts {
        let entry = by_entity.entry(fact.entity_id.clone()).or_default();
        entry.push(' ');
        entry.push_str(&fact.fact_key);
        if let Some(value) = &fact.value_text {
            entry.push(' ');
            entry.push_str(value);
        }
    }
    by_entity
}

fn serving_fact_records(
    facts: &[KgViewFactRecord],
) -> Result<Vec<ServingFactRecord>, serde_json::Error> {
    facts
        .iter()
        .map(|fact| {
            let value = serde_json::from_str::<FactValue>(&fact.value_json)?;
            Ok(ServingFactRecord {
                entity_id: fact.entity_id.clone(),
                fact_key: fact.fact_key.clone(),
                value_type: fact.value_type.clone(),
                value_text: fact.value_text.clone(),
                value,
                confidence: fact.confidence,
                source_type: fact.source_type.clone(),
                source_url: fact.source_url.clone(),
                model: fact.model.clone(),
                skill_id: fact.skill_id.clone(),
                learned_at: fact.learned_at,
            })
        })
        .collect()
}

fn serving_search_metadata_records(
    facts: &[KgViewFactRecord],
    annotations: &[KgViewFactAnnotationRecord],
) -> Result<Vec<ServingSearchMetadataRecord>, ServingBundleError> {
    let registry = load_fact_registry_index().map_err(ServingBundleError::DagConfig)?;
    let annotation_by_key = annotations
        .iter()
        .map(|annotation| {
            (
                (annotation.entity_id.as_str(), annotation.fact_key.as_str()),
                annotation,
            )
        })
        .collect::<HashMap<(&str, &str), &KgViewFactAnnotationRecord>>();

    let mut seen = BTreeMap::<(&str, &str), ()>::new();
    let mut records = Vec::with_capacity(facts.len());

    for fact in facts {
        let key = (fact.entity_id.as_str(), fact.fact_key.as_str());
        if seen.contains_key(&key) {
            continue;
        }
        seen.insert(key, ());

        if let Some(entry) = registry.lookup(&fact.fact_key) {
            let hint = entry.scoring_hint.as_ref();
            records.push(ServingSearchMetadataRecord {
                entity_id: fact.entity_id.clone(),
                fact_key: fact.fact_key.clone(),
                display_template: entry.display_template.clone(),
                answers_preferences: entry.answers_preferences.clone(),
                scoring_direction: hint.map(scoring_direction_from_hint),
                scoring_weight: hint.and_then(|hint| hint.weight),
                scoring_thresholds: hint.map(|hint| hint.thresholds.clone()).unwrap_or_default(),
            });
            continue;
        }

        if let Some(annotation) = annotation_by_key.get(&key) {
            records.push(ServingSearchMetadataRecord {
                entity_id: fact.entity_id.clone(),
                fact_key: fact.fact_key.clone(),
                display_template: annotation.display_template.clone(),
                answers_preferences: serde_json::from_str(&annotation.answers_preferences_json)?,
                scoring_direction: annotation.scoring_direction.clone(),
                scoring_weight: annotation.scoring_weight,
                scoring_thresholds: serde_json::from_str(&annotation.scoring_thresholds_json)?,
            });
        }
    }

    records.sort_by(|left, right| {
        left.entity_id
            .cmp(&right.entity_id)
            .then_with(|| left.fact_key.cmp(&right.fact_key))
    });
    Ok(records)
}

#[derive(Debug, serde::Serialize)]
struct PreferenceCoverageReport {
    generated_at: String,
    society_count: usize,
    preference_labels: Vec<PreferenceLabelCoverage>,
    registry_gaps: Vec<RegistryGap>,
}

#[derive(Debug, serde::Serialize)]
struct PreferenceLabelCoverage {
    label: String,
    societies_with_match: u32,
    society_coverage_pct: f64,
}

#[derive(Debug, serde::Serialize)]
struct RegistryGap {
    fact_key: String,
    entity_count: u32,
}

fn write_preference_coverage_report(
    entities: &[ServingEntityRecord],
    facts: &[ServingFactRecord],
    search_metadata: &[ServingSearchMetadataRecord],
) -> Result<(), std::io::Error> {
    let society_count = entities
        .iter()
        .filter(|entity| entity.entity_type == "Society")
        .count();
    if society_count == 0 {
        return Ok(());
    }

    let metadata_by_entity_fact = search_metadata
        .iter()
        .map(|row| ((row.entity_id.as_str(), row.fact_key.as_str()), row))
        .collect::<HashMap<(&str, &str), &ServingSearchMetadataRecord>>();

    let mut label_hits = BTreeMap::<String, std::collections::HashSet<String>>::new();
    for fact in facts {
        if let Some(metadata) =
            metadata_by_entity_fact.get(&(fact.entity_id.as_str(), fact.fact_key.as_str()))
        {
            for label in &metadata.answers_preferences {
                label_hits
                    .entry(label.to_lowercase())
                    .or_default()
                    .insert(fact.entity_id.clone());
            }
        }
    }

    let preference_labels = label_hits
        .into_iter()
        .map(|(label, societies)| {
            let societies_with_match = societies.len() as u32;
            PreferenceLabelCoverage {
                label,
                societies_with_match,
                society_coverage_pct: (societies_with_match as f64 / society_count as f64) * 100.0,
            }
        })
        .collect::<Vec<_>>();

    let mut registry_gaps = BTreeMap::<String, u32>::new();
    for fact in facts {
        if !metadata_by_entity_fact.contains_key(&(fact.entity_id.as_str(), fact.fact_key.as_str()))
        {
            *registry_gaps.entry(fact.fact_key.clone()).or_default() += 1;
        }
    }
    let registry_gaps = registry_gaps
        .into_iter()
        .map(|(fact_key, entity_count)| RegistryGap {
            fact_key,
            entity_count,
        })
        .collect::<Vec<_>>();

    let report = PreferenceCoverageReport {
        generated_at: Utc::now().to_rfc3339(),
        society_count,
        preference_labels,
        registry_gaps,
    };

    let output_path = preference_coverage_output_path();
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_string_pretty(&report)?;
    std::fs::write(output_path, payload)?;
    Ok(())
}

fn preference_coverage_output_path() -> PathBuf {
    if let Ok(path) = std::env::var("OPENESTATES_PREFERENCE_COVERAGE_PATH") {
        return PathBuf::from(path);
    }
    PathBuf::from("data/validation/preference_coverage.json")
}

async fn upload_tantivy_dir(
    lake: &LakeStore,
    dir: &Path,
    prefix: &str,
) -> Result<Vec<BundleArtifact>, ServingBundleError> {
    let mut files = Vec::new();
    collect_files(dir, &mut files)?;
    files.sort();

    let mut artifacts = Vec::new();
    for path in files {
        let relative = path
            .strip_prefix(dir)
            .map_err(|_| ServingBundleError::InvalidIndexPath(path.clone()))?;
        let relative_key = relative.to_string_lossy().replace('\\', "/");
        let key = LakeKey::new(format!("{}/{}", prefix.trim_end_matches('/'), relative_key))?;
        let bytes = std::fs::read(&path).map_err(ServingBundleError::Io)?;
        let meta = lake.put_bytes(&key, bytes).await?;
        artifacts.push(artifact(
            BundleArtifactKind::TantivyIndexFile,
            meta,
            "application/octet-stream",
            None,
        ));
    }

    Ok(artifacts)
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), ServingBundleError> {
    for entry in std::fs::read_dir(dir).map_err(ServingBundleError::Io)? {
        let entry = entry.map_err(ServingBundleError::Io)?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn artifact(
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

fn temp_index_dir(bundle_version: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "openestates-tantivy-{}-{}",
        slug(bundle_version),
        uuid::Uuid::new_v4()
    ))
}

fn slug(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[derive(Debug)]
pub enum ServingBundleError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Lake(LakeError),
    Key(crate::lake::keys::KeyError),
    Parquet(ParquetWriteError),
    Tantivy(TantivyIndexError),
    DagConfig(DagConfigError),
    InvalidIndexPath(PathBuf),
}

impl fmt::Display for ServingBundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "serving bundle IO error: {err}"),
            Self::Json(err) => write!(f, "serving bundle JSON error: {err}"),
            Self::Lake(err) => write!(f, "serving bundle lake error: {err}"),
            Self::Key(err) => write!(f, "serving bundle key error: {err}"),
            Self::Parquet(err) => write!(f, "serving bundle Parquet error: {err}"),
            Self::Tantivy(err) => write!(f, "serving bundle Tantivy error: {err}"),
            Self::DagConfig(err) => write!(f, "serving bundle DAG config error: {err}"),
            Self::InvalidIndexPath(path) => {
                write!(
                    f,
                    "Tantivy index path is outside index dir: {}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ServingBundleError {}

impl From<std::io::Error> for ServingBundleError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<serde_json::Error> for ServingBundleError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

impl From<LakeError> for ServingBundleError {
    fn from(err: LakeError) -> Self {
        Self::Lake(err)
    }
}

impl From<crate::lake::keys::KeyError> for ServingBundleError {
    fn from(err: crate::lake::keys::KeyError) -> Self {
        Self::Key(err)
    }
}

impl From<ParquetWriteError> for ServingBundleError {
    fn from(err: ParquetWriteError) -> Self {
        Self::Parquet(err)
    }
}

impl From<TantivyIndexError> for ServingBundleError {
    fn from(err: TantivyIndexError) -> Self {
        Self::Tantivy(err)
    }
}

impl From<DagConfigError> for ServingBundleError {
    fn from(err: DagConfigError) -> Self {
        Self::DagConfig(err)
    }
}
