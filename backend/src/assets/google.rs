use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use arrow::array::{Array, ArrayRef, Float32Array, Float64Array, StringArray, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::dag_config::{dag_root, load_json, DagConfigError};
use crate::knowledge::FactValue;
use crate::lake::{LakeError, LakeKey, LakeStore};
use crate::parquet_data::{
    optional_string_list_column_value, string_list_array, string_list_field, OptionalListColumn,
};

use super::{
    ArtifactRef, AssetId, AssetMaterializationStore, AssetPartition, AssetPathBuilder, AssetStage,
    MaterializationId, MaterializationRecord, ReraAssetError, SkillFactAnnotationRecord,
    SkillFactRecord, SkillFactsInput, SourceWatermark,
};

pub const GOOGLE_PLACES_WEEKLY_ASSET_ID: &str = "google_places_weekly";
pub const GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID: &str = "google_nearby_places_weekly";
const GOOGLE_PLACE_FORMAT_VERSION: u32 = 2;
const GOOGLE_NEARBY_PLACE_FORMAT_VERSION: u32 = 3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GooglePlaceSnapshotRecord {
    pub entity_id: String,
    pub project_key: Option<String>,
    pub query: String,
    pub place_name: Option<String>,
    pub place_id: Option<String>,
    pub reviews_url: String,
    pub rating: Option<f64>,
    pub review_count: Option<u64>,
    #[serde(default)]
    pub review_snippets: Vec<String>,
    #[serde(default)]
    pub latitude: Option<f64>,
    #[serde(default)]
    pub longitude: Option<f64>,
    pub address: Option<String>,
    pub confidence: f32,
    pub fetched_at: DateTime<Utc>,
    pub fetch_source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoogleNearbyPlaceRecord {
    pub entity_id: String,
    pub project_key: Option<String>,
    pub query: String,
    pub category: String,
    pub place_name: String,
    pub place_id: Option<String>,
    pub place_url: String,
    pub distance_km: Option<f64>,
    #[serde(default)]
    pub latitude: Option<f64>,
    #[serde(default)]
    pub longitude: Option<f64>,
    pub rating: Option<f64>,
    pub review_count: Option<u64>,
    pub primary_type: Option<String>,
    #[serde(default)]
    pub place_types: Vec<String>,
    pub confidence: f32,
    pub fetched_at: DateTime<Utc>,
    pub fetch_source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GooglePlacesWeeklyInput {
    pub snapshot_date: String,
    #[serde(default)]
    pub records: Vec<GooglePlaceSnapshotRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_watermarks: Vec<SourceWatermark>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoogleNearbyPlacesWeeklyInput {
    pub snapshot_date: String,
    #[serde(default)]
    pub records: Vec<GoogleNearbyPlaceRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_watermarks: Vec<SourceWatermark>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GooglePlaceSnapshotManifest {
    pub asset_id: String,
    pub format_version: u32,
    pub snapshot_date: String,
    pub run_id: String,
    pub created_at: DateTime<Utc>,
    pub row_count: u64,
    pub artifacts: Vec<ArtifactRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoogleNearbyPlaceSnapshotManifest {
    pub asset_id: String,
    pub format_version: u32,
    pub snapshot_date: String,
    pub run_id: String,
    pub created_at: DateTime<Utc>,
    pub row_count: u64,
    pub artifacts: Vec<ArtifactRef>,
}

#[derive(Debug, Clone)]
pub struct GooglePlaceSnapshotMaterialization {
    pub manifest: GooglePlaceSnapshotManifest,
    pub record: MaterializationRecord,
}

#[derive(Debug, Clone)]
pub struct GoogleNearbyPlaceSnapshotMaterialization {
    pub manifest: GoogleNearbyPlaceSnapshotManifest,
    pub record: MaterializationRecord,
}

#[derive(Clone)]
pub struct GooglePlaceSnapshotMaterializer {
    lake: LakeStore,
    materializations: AssetMaterializationStore,
}

impl GooglePlaceSnapshotMaterializer {
    pub fn new(lake: LakeStore) -> Self {
        Self {
            materializations: AssetMaterializationStore::new(lake.clone()),
            lake,
        }
    }

    pub async fn materialize_and_promote(
        &self,
        input: &GooglePlacesWeeklyInput,
        run_id: impl Into<String>,
    ) -> Result<GooglePlaceSnapshotMaterialization, GooglePlaceAssetError> {
        let partition = AssetPartition::new([("source", "google")]);
        let materialization = self
            .materialize_for_run(
                input,
                run_id,
                Vec::new(),
                MaterializationId::new(),
                partition,
            )
            .await?;
        self.materializations
            .promote_current(&materialization.record)
            .await?;
        Ok(materialization)
    }

    pub async fn materialize_for_run(
        &self,
        input: &GooglePlacesWeeklyInput,
        run_id: impl Into<String>,
        parent_materializations: Vec<MaterializationId>,
        dag_run_id: MaterializationId,
        record_partition: AssetPartition,
    ) -> Result<GooglePlaceSnapshotMaterialization, GooglePlaceAssetError> {
        validate_input(input)?;
        let run_id = run_id.into();
        let artifact_partition = AssetPartition::new([("dt", input.snapshot_date.as_str())]);
        let place_key = AssetPathBuilder::raw_snapshot_key(
            "google",
            &artifact_partition,
            &run_id,
            "places/part-00000.parquet",
        );
        let place_meta = self
            .lake
            .put_bytes(&place_key, write_places_parquet(&input.records)?)
            .await?;
        let mut artifacts = vec![ArtifactRef::parquet(place_meta)];

        let manifest_key = AssetPathBuilder::raw_snapshot_key(
            "google",
            &artifact_partition,
            &run_id,
            "manifest.json",
        );
        let manifest = GooglePlaceSnapshotManifest {
            asset_id: GOOGLE_PLACES_WEEKLY_ASSET_ID.to_string(),
            format_version: GOOGLE_PLACE_FORMAT_VERSION,
            snapshot_date: input.snapshot_date.clone(),
            run_id,
            created_at: Utc::now(),
            row_count: input.records.len() as u64,
            artifacts: artifacts.clone(),
        };
        let manifest_meta = self.lake.put_json(&manifest_key, &manifest).await?;
        artifacts.push(ArtifactRef::json(manifest_meta));
        artifacts.sort_by(|left, right| left.key.cmp(&right.key));

        let record = MaterializationRecord::succeeded(
            asset_id(GOOGLE_PLACES_WEEKLY_ASSET_ID),
            AssetStage::Raw,
            record_partition,
            input.snapshot_date.clone(),
            artifacts,
        )
        .with_run_id(dag_run_id)
        .with_parent_materializations(parent_materializations)
        .with_source_watermarks(place_watermarks(input))
        .with_row_count(input.records.len() as u64);
        self.materializations.write_materialization(&record).await?;

        Ok(GooglePlaceSnapshotMaterialization { manifest, record })
    }

    pub async fn materialize_nearby_and_promote(
        &self,
        input: &GoogleNearbyPlacesWeeklyInput,
        run_id: impl Into<String>,
    ) -> Result<GoogleNearbyPlaceSnapshotMaterialization, GooglePlaceAssetError> {
        let partition = AssetPartition::new([("source", "google")]);
        let materialization = self
            .materialize_nearby_for_run(
                input,
                run_id,
                Vec::new(),
                MaterializationId::new(),
                partition,
            )
            .await?;
        self.materializations
            .promote_current(&materialization.record)
            .await?;
        Ok(materialization)
    }

    pub async fn materialize_nearby_for_run(
        &self,
        input: &GoogleNearbyPlacesWeeklyInput,
        run_id: impl Into<String>,
        parent_materializations: Vec<MaterializationId>,
        dag_run_id: MaterializationId,
        record_partition: AssetPartition,
    ) -> Result<GoogleNearbyPlaceSnapshotMaterialization, GooglePlaceAssetError> {
        validate_nearby_input(input)?;
        let run_id = run_id.into();
        let artifact_partition = AssetPartition::new([("dt", input.snapshot_date.as_str())]);
        let nearby_key = AssetPathBuilder::raw_snapshot_key(
            "google",
            &artifact_partition,
            &run_id,
            "nearby_places/part-00000.parquet",
        );
        let nearby_meta = self
            .lake
            .put_bytes(&nearby_key, write_nearby_places_parquet(&input.records)?)
            .await?;
        let mut artifacts = vec![ArtifactRef::parquet(nearby_meta)];

        let manifest_key = AssetPathBuilder::raw_snapshot_key(
            "google",
            &artifact_partition,
            &run_id,
            "nearby_places/manifest.json",
        );
        let manifest = GoogleNearbyPlaceSnapshotManifest {
            asset_id: GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID.to_string(),
            format_version: GOOGLE_NEARBY_PLACE_FORMAT_VERSION,
            snapshot_date: input.snapshot_date.clone(),
            run_id,
            created_at: Utc::now(),
            row_count: input.records.len() as u64,
            artifacts: artifacts.clone(),
        };
        let manifest_meta = self.lake.put_json(&manifest_key, &manifest).await?;
        artifacts.push(ArtifactRef::json(manifest_meta));
        artifacts.sort_by(|left, right| left.key.cmp(&right.key));

        let record = MaterializationRecord::succeeded(
            asset_id(GOOGLE_NEARBY_PLACES_WEEKLY_ASSET_ID),
            AssetStage::Raw,
            record_partition,
            input.snapshot_date.clone(),
            artifacts,
        )
        .with_run_id(dag_run_id)
        .with_parent_materializations(parent_materializations)
        .with_source_watermarks(nearby_watermarks(input))
        .with_row_count(input.records.len() as u64);
        self.materializations.write_materialization(&record).await?;

        Ok(GoogleNearbyPlaceSnapshotMaterialization { manifest, record })
    }
}

pub async fn read_google_place_rows(
    lake: &LakeStore,
    record: &MaterializationRecord,
) -> Result<Vec<GooglePlaceSnapshotRecord>, GooglePlaceAssetError> {
    let artifact = record
        .artifacts
        .iter()
        .find(|artifact| artifact.key.ends_with("places/part-00000.parquet"))
        .ok_or_else(|| GooglePlaceAssetError::MissingArtifact(record.asset_id.clone()))?;
    let key = LakeKey::new(&artifact.key).map_err(LakeError::Key)?;
    read_places_parquet(lake.get_bytes(&key).await?)
}

pub async fn read_google_nearby_place_rows(
    lake: &LakeStore,
    record: &MaterializationRecord,
) -> Result<Vec<GoogleNearbyPlaceRecord>, GooglePlaceAssetError> {
    let artifact = record
        .artifacts
        .iter()
        .find(|artifact| artifact.key.ends_with("nearby_places/part-00000.parquet"))
        .ok_or_else(|| GooglePlaceAssetError::MissingArtifact(record.asset_id.clone()))?;
    let key = LakeKey::new(&artifact.key).map_err(LakeError::Key)?;
    read_nearby_places_parquet(lake.get_bytes(&key).await?)
}

pub async fn google_review_facts_input(
    lake: &LakeStore,
    google_record: &MaterializationRecord,
    run_id: &MaterializationId,
) -> Result<SkillFactsInput, GooglePlaceAssetError> {
    let rows = read_google_place_rows(lake, google_record).await?;
    google_review_facts_from_rows(rows, google_record, run_id, &HashMap::new())
}

pub async fn google_nearby_place_facts_input(
    lake: &LakeStore,
    nearby_record: &MaterializationRecord,
    run_id: &MaterializationId,
) -> Result<SkillFactsInput, GooglePlaceAssetError> {
    let rows = read_google_nearby_place_rows(lake, nearby_record).await?;
    google_nearby_place_facts_from_rows(rows, nearby_record, run_id, &HashMap::new())
}

pub async fn google_review_facts_input_with_aliases(
    lake: &LakeStore,
    google_record: &MaterializationRecord,
    canonical_record: &MaterializationRecord,
    run_id: &MaterializationId,
) -> Result<SkillFactsInput, GooglePlaceAssetError> {
    let rows = read_google_place_rows(lake, google_record).await?;
    let canonical = super::read_canonical_society_rows(lake, canonical_record).await?;
    let aliases = canonical
        .mappings
        .into_iter()
        .filter_map(|mapping| {
            mapping
                .alias_entity_id
                .filter(|alias| alias != &mapping.canonical_entity_id)
                .map(|alias| (mapping.canonical_entity_id, alias))
        })
        .collect::<HashMap<_, _>>();
    google_review_facts_from_rows(rows, google_record, run_id, &aliases)
}

pub async fn google_nearby_place_facts_input_with_aliases(
    lake: &LakeStore,
    nearby_record: &MaterializationRecord,
    canonical_record: &MaterializationRecord,
    run_id: &MaterializationId,
) -> Result<SkillFactsInput, GooglePlaceAssetError> {
    let rows = read_google_nearby_place_rows(lake, nearby_record).await?;
    let canonical = super::read_canonical_society_rows(lake, canonical_record).await?;
    let aliases = canonical
        .mappings
        .into_iter()
        .filter_map(|mapping| {
            mapping
                .alias_entity_id
                .filter(|alias| alias != &mapping.canonical_entity_id)
                .map(|alias| (mapping.canonical_entity_id, alias))
        })
        .collect::<HashMap<_, _>>();
    google_nearby_place_facts_from_rows(rows, nearby_record, run_id, &aliases)
}

fn google_review_facts_from_rows(
    rows: Vec<GooglePlaceSnapshotRecord>,
    google_record: &MaterializationRecord,
    run_id: &MaterializationId,
    aliases: &HashMap<String, String>,
) -> Result<SkillFactsInput, GooglePlaceAssetError> {
    let mut facts = Vec::new();
    let mut annotations = Vec::new();
    for row in rows {
        append_google_review_facts(&row, run_id, &mut facts, &mut annotations)?;
        if let Some(alias) = aliases.get(&row.entity_id) {
            let mut alias_row = row.clone();
            alias_row.entity_id.clone_from(alias);
            append_google_review_facts(&alias_row, run_id, &mut facts, &mut annotations)?;
        }
    }
    Ok(SkillFactsInput {
        source: "google".to_string(),
        snapshot_date: google_record.version.clone(),
        facts,
        fact_annotations: annotations,
        source_watermarks: google_record.source_watermarks.clone(),
    })
}

fn google_nearby_place_facts_from_rows(
    rows: Vec<GoogleNearbyPlaceRecord>,
    nearby_record: &MaterializationRecord,
    run_id: &MaterializationId,
    aliases: &HashMap<String, String>,
) -> Result<SkillFactsInput, GooglePlaceAssetError> {
    let config = load_nearby_place_category_config()?;
    let mut rows_by_fact = BTreeMap::<(String, String), Vec<GoogleNearbyPlaceRecord>>::new();
    let mut facts = Vec::new();
    let mut annotations = Vec::new();
    let mut emitted_place_fact_keys = HashSet::<(String, String)>::new();
    for row in rows {
        if let Some(category_config) = config.category_for_alias(&row.category) {
            let fact_key = category_config.fact_key.as_str();
            rows_by_fact
                .entry((row.entity_id.clone(), fact_key.to_string()))
                .or_default()
                .push(row.clone());
            if let Some(alias) = aliases.get(&row.entity_id) {
                let mut alias_row = row;
                alias_row.entity_id.clone_from(alias);
                rows_by_fact
                    .entry((alias_row.entity_id.clone(), fact_key.to_string()))
                    .or_default()
                    .push(alias_row);
            }
        }
    }

    for ((entity_id, fact_key), mut grouped_rows) in rows_by_fact {
        let Some(category_config) = config.category_for_fact_key(&fact_key) else {
            continue;
        };
        grouped_rows.retain(|row| nearby_row_has_serving_evidence(category_config, row));
        if grouped_rows.is_empty() {
            continue;
        }
        for row in &grouped_rows {
            append_google_nearby_place_identity_facts(
                row,
                run_id,
                &mut facts,
                &mut annotations,
                &mut emitted_place_fact_keys,
            )?;
        }
        grouped_rows.sort_by(|left, right| {
            left.distance_km
                .partial_cmp(&right.distance_km)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    right
                        .rating
                        .partial_cmp(&left.rating)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| left.place_name.cmp(&right.place_name))
        });
        let values = grouped_rows.into_iter().take(5).collect::<Vec<_>>();
        if values.is_empty() {
            continue;
        }
        for row in values {
            let display = nearby_place_display(&row);
            let value = FactValue::Text(display.clone());
            push_nearby_society_fact(
                &mut facts, &entity_id, &fact_key, value, &row, run_id, &display,
            )?;
            for risk in &category_config.derived_distance_risks {
                if row
                    .distance_km
                    .is_some_and(|distance_km| distance_km <= risk.max_distance_km)
                {
                    let risk_value = FactValue::Text(display.clone());
                    push_nearby_society_fact(
                        &mut facts,
                        &entity_id,
                        &risk.fact_key,
                        risk_value,
                        &row,
                        run_id,
                        &display,
                    )?;
                }
            }
        }
        annotations.push(SkillFactAnnotationRecord {
            entity_id: entity_id.clone(),
            fact_key: fact_key.clone(),
            display_template: Some(format!("{}: {{value}}", category_config.display_label)),
            answers_preferences_json: serde_json::to_string(&nearby_answers_preferences(
                category_config,
            ))?,
            scoring_direction: Some("TextMatch".to_string()),
            scoring_weight: Some(0.8),
            scoring_thresholds_json: "[]".to_string(),
        });
        for risk in &category_config.derived_distance_risks {
            if facts
                .iter()
                .any(|fact| fact.entity_id == entity_id && fact.fact_key == risk.fact_key)
            {
                annotations.push(SkillFactAnnotationRecord {
                    entity_id: entity_id.clone(),
                    fact_key: risk.fact_key.clone(),
                    display_template: Some(format!("{}: {{value}}", risk.display_label)),
                    answers_preferences_json: serde_json::to_string(
                        &risk
                            .answers_preferences
                            .iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>(),
                    )?,
                    scoring_direction: Some("TextMatch".to_string()),
                    scoring_weight: Some(risk.scoring_weight),
                    scoring_thresholds_json: "[]".to_string(),
                });
            }
        }
    }
    Ok(SkillFactsInput {
        source: "google".to_string(),
        snapshot_date: nearby_record.version.clone(),
        facts,
        fact_annotations: annotations,
        source_watermarks: nearby_record.source_watermarks.clone(),
    })
}

fn push_nearby_society_fact(
    facts: &mut Vec<SkillFactRecord>,
    entity_id: &str,
    fact_key: &str,
    value: FactValue,
    row: &GoogleNearbyPlaceRecord,
    run_id: &MaterializationId,
    display: &str,
) -> Result<(), GooglePlaceAssetError> {
    facts.push(SkillFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        value_type: fact_value_type(&value).to_string(),
        value_json: serde_json::to_string(&value)?,
        confidence: row.confidence,
        source_type: "Google".to_string(),
        source_url: Some(row.place_url.clone()),
        model: None,
        skill_id: Some("fetch_google_nearby_places".to_string()),
        triggered_by: Some(row.query.clone()),
        learned_at: row.fetched_at,
        run_id: run_id.to_string(),
        input_hash: format!(
            "sha256:{}",
            sha256_hex(format!("{entity_id}:{fact_key}:{}:{display}", row.place_url).as_bytes())
        ),
    });
    Ok(())
}

fn append_google_nearby_place_identity_facts(
    row: &GoogleNearbyPlaceRecord,
    run_id: &MaterializationId,
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
    emitted_fact_keys: &mut HashSet<(String, String)>,
) -> Result<(), GooglePlaceAssetError> {
    if !valid_optional_coordinate(row.latitude, row.longitude) {
        return Ok(());
    }
    let (Some(latitude), Some(longitude)) = (row.latitude, row.longitude) else {
        return Ok(());
    };

    let entity_id = nearby_place_entity_id(row);
    push_nearby_place_fact_once(
        row,
        run_id,
        &entity_id,
        "place.name",
        FactValue::Text(row.place_name.clone()),
        "Place name: {value}",
        &["nearby", "landmark", "place"],
        facts,
        annotations,
        emitted_fact_keys,
    )?;
    push_nearby_place_fact_once(
        row,
        run_id,
        &entity_id,
        "place.category",
        FactValue::Text(row.category.clone()),
        "Place category: {value}",
        &["nearby", "landmark", "place"],
        facts,
        annotations,
        emitted_fact_keys,
    )?;
    push_nearby_place_fact_once(
        row,
        run_id,
        &entity_id,
        "geo.latitude",
        FactValue::Numeric(latitude),
        "Latitude: {value}",
        &["coordinates", "location", "latitude"],
        facts,
        annotations,
        emitted_fact_keys,
    )?;
    push_nearby_place_fact_once(
        row,
        run_id,
        &entity_id,
        "geo.longitude",
        FactValue::Numeric(longitude),
        "Longitude: {value}",
        &["coordinates", "location", "longitude"],
        facts,
        annotations,
        emitted_fact_keys,
    )?;
    push_nearby_place_fact_once(
        row,
        run_id,
        &entity_id,
        "google_place_url",
        FactValue::Text(row.place_url.clone()),
        "Google place: {value}",
        &["google maps", "nearby", "landmark"],
        facts,
        annotations,
        emitted_fact_keys,
    )?;

    if let Some(place_id) = row
        .place_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        push_nearby_place_fact_once(
            row,
            run_id,
            &entity_id,
            "google_place_id",
            FactValue::Text(place_id.to_string()),
            "Google place id: {value}",
            &["google maps", "nearby", "landmark"],
            facts,
            annotations,
            emitted_fact_keys,
        )?;
    }
    if let Some(rating) = row.rating {
        push_nearby_place_fact_once(
            row,
            run_id,
            &entity_id,
            "google_rating",
            FactValue::Numeric(rating),
            "Google rating: {value}",
            &["google rating", "reviews"],
            facts,
            annotations,
            emitted_fact_keys,
        )?;
    }
    if let Some(review_count) = row.review_count {
        push_nearby_place_fact_once(
            row,
            run_id,
            &entity_id,
            "google_review_count",
            FactValue::Numeric(review_count as f64),
            "Google reviews: {value}",
            &["google reviews", "review count"],
            facts,
            annotations,
            emitted_fact_keys,
        )?;
    }
    if !row.place_types.is_empty() {
        push_nearby_place_fact_once(
            row,
            run_id,
            &entity_id,
            "place.types",
            FactValue::Tags(row.place_types.clone()),
            "Place types: {value}",
            &["nearby", "landmark", "place"],
            facts,
            annotations,
            emitted_fact_keys,
        )?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn push_nearby_place_fact_once(
    row: &GoogleNearbyPlaceRecord,
    run_id: &MaterializationId,
    entity_id: &str,
    fact_key: &str,
    value: FactValue,
    display_template: &str,
    answers_preferences: &[&str],
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
    emitted_fact_keys: &mut HashSet<(String, String)>,
) -> Result<(), GooglePlaceAssetError> {
    if !emitted_fact_keys.insert((entity_id.to_string(), fact_key.to_string())) {
        return Ok(());
    }
    let value_type = fact_value_type(&value);
    let value_json = serde_json::to_string(&value)?;
    facts.push(SkillFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        value_type: value_type.to_string(),
        value_json: value_json.clone(),
        confidence: row.confidence,
        source_type: "Google".to_string(),
        source_url: Some(row.place_url.clone()),
        model: None,
        skill_id: Some("fetch_google_nearby_places".to_string()),
        triggered_by: Some(row.query.clone()),
        learned_at: row.fetched_at,
        run_id: run_id.to_string(),
        input_hash: format!(
            "sha256:{}",
            sha256_hex(format!("{entity_id}:{fact_key}:{value_json}").as_bytes())
        ),
    });
    annotations.push(SkillFactAnnotationRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        display_template: Some(display_template.to_string()),
        answers_preferences_json: serde_json::to_string(answers_preferences)?,
        scoring_direction: None,
        scoring_weight: None,
        scoring_thresholds_json: "[]".to_string(),
    });
    Ok(())
}

fn nearby_place_entity_id(row: &GoogleNearbyPlaceRecord) -> String {
    let stable_source = row
        .place_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&row.place_url);
    let mut stable_slug = slug(stable_source);
    if stable_slug.is_empty() {
        stable_slug = sha256_hex(stable_source.as_bytes())
            .chars()
            .take(12)
            .collect();
    }
    format!("place:google:{stable_slug}")
}

fn fact_value_type(value: &FactValue) -> &'static str {
    match value {
        FactValue::Numeric(_) => "numeric",
        FactValue::Text(_) => "text",
        FactValue::Bool(_) => "bool",
        FactValue::Tags(_) => "tags",
        FactValue::Score { .. } => "score",
    }
}

fn append_google_review_facts(
    row: &GooglePlaceSnapshotRecord,
    run_id: &MaterializationId,
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
) -> Result<(), GooglePlaceAssetError> {
    push_fact(
        row,
        run_id,
        "google_reviews_url",
        FactValue::Text(row.reviews_url.clone()),
        "Google reviews: {value}",
        &["google reviews", "resident reviews", "reviews"],
        Some(("TextMatch", 0.8, Vec::new())),
        facts,
        annotations,
    )?;
    if let Some(place_id) = &row.place_id {
        push_fact(
            row,
            run_id,
            "google_place_id",
            FactValue::Text(place_id.clone()),
            "Google place id: {value}",
            &["google reviews", "maps"],
            None,
            facts,
            annotations,
        )?;
    }
    if let Some(address) = row
        .address
        .as_deref()
        .filter(|address| !address.trim().is_empty())
    {
        push_fact(
            row,
            run_id,
            "google_place_address",
            FactValue::Text(address.trim().to_string()),
            "Google address: {value}",
            &["address", "location", "approach road", "access road"],
            Some(("TextMatch", 0.4, Vec::new())),
            facts,
            annotations,
        )?;
    }
    if valid_optional_coordinate(row.latitude, row.longitude) {
        if let (Some(latitude), Some(longitude)) = (row.latitude, row.longitude) {
            push_fact(
                row,
                run_id,
                "geo.latitude",
                FactValue::Numeric(latitude),
                "Latitude: {value}",
                &["coordinates", "location", "latitude"],
                None,
                facts,
                annotations,
            )?;
            push_fact(
                row,
                run_id,
                "geo.longitude",
                FactValue::Numeric(longitude),
                "Longitude: {value}",
                &["coordinates", "location", "longitude"],
                None,
                facts,
                annotations,
            )?;
        }
    }
    if let Some(rating) = row.rating {
        push_fact(
            row,
            run_id,
            "google_rating",
            FactValue::Numeric(rating),
            "Google rating: {value}",
            &["high rating", "good reviews", "google rating"],
            Some(("HigherIsBetter", 1.0, vec![4.2, 3.8])),
            facts,
            annotations,
        )?;
    }
    if let Some(review_count) = row.review_count {
        push_fact(
            row,
            run_id,
            "google_review_count",
            FactValue::Numeric(review_count as f64),
            "Google reviews: {value}",
            &["many reviews", "review count", "google reviews"],
            Some(("HigherIsBetter", 0.5, vec![100.0, 20.0])),
            facts,
            annotations,
        )?;
    }
    if !row.review_snippets.is_empty() {
        push_fact(
            row,
            run_id,
            "google_review_snippets",
            FactValue::Tags(row.review_snippets.clone()),
            "Google review highlights: {value}",
            &[
                "review highlights",
                "resident feedback",
                "google reviews",
                "community signal",
            ],
            Some(("TextMatch", 1.2, Vec::new())),
            facts,
            annotations,
        )?;
        append_review_signal_fact(
            row,
            run_id,
            "stp_concern",
            "STP review signal: {value}",
            &["stp", "sewage treatment plant", "sewage smell"],
            &[
                "stp",
                "sewage treatment plant",
                "sewage smell",
                "stp smell",
                "sewage issue",
            ],
            facts,
            annotations,
        )?;
        append_review_signal_fact(
            row,
            run_id,
            "high_tension_wire_concern",
            "High-tension wire review signal: {value}",
            &[
                "high tension wires",
                "high tension line",
                "ht line",
                "power lines",
            ],
            &[
                "high tension wires",
                "high tension line",
                "ht line",
                "power lines",
                "transmission line",
            ],
            facts,
            annotations,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn append_review_signal_fact(
    row: &GooglePlaceSnapshotRecord,
    run_id: &MaterializationId,
    fact_key: &str,
    display_template: &str,
    answers_preferences: &[&str],
    terms: &[&str],
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
) -> Result<(), GooglePlaceAssetError> {
    let snippets = matching_review_snippets(&row.review_snippets, terms);
    if snippets.is_empty() {
        return Ok(());
    }

    push_fact(
        row,
        run_id,
        fact_key,
        FactValue::Tags(snippets),
        display_template,
        answers_preferences,
        None,
        facts,
        annotations,
    )
}

fn matching_review_snippets(snippets: &[String], terms: &[&str]) -> Vec<String> {
    snippets
        .iter()
        .filter(|snippet| {
            let lower = snippet.to_lowercase();
            terms.iter().any(|term| lower.contains(term))
        })
        .cloned()
        .collect()
}

pub async fn canonicalize_google_places_input(
    lake: &LakeStore,
    input: &GooglePlacesWeeklyInput,
    canonical_record: &MaterializationRecord,
) -> Result<GooglePlacesWeeklyInput, GooglePlaceAssetError> {
    let canonical = super::read_canonical_society_rows(lake, canonical_record).await?;
    let canonical_ids: HashSet<_> = canonical
        .entities
        .iter()
        .filter(|entity| entity.entity_type == "society")
        .map(|entity| entity.entity_id.as_str())
        .collect();
    let by_project_key: HashMap<_, _> = canonical
        .mappings
        .iter()
        .map(|mapping| {
            (
                mapping.project_key.as_str(),
                mapping.canonical_entity_id.as_str(),
            )
        })
        .collect();
    let by_alias: HashMap<_, _> = canonical
        .mappings
        .iter()
        .filter_map(|mapping| {
            mapping
                .alias_entity_id
                .as_deref()
                .map(|alias| (alias, mapping.canonical_entity_id.as_str()))
        })
        .collect();
    let mut resolved = input.clone();
    let mut records = Vec::with_capacity(resolved.records.len());
    for mut record in resolved.records {
        if canonical_ids.contains(record.entity_id.as_str()) {
            records.push(record);
            continue;
        }
        let Some(entity_id) = by_alias
            .get(record.entity_id.as_str())
            .copied()
            .or_else(|| {
                record
                    .project_key
                    .as_deref()
                    .and_then(|key| by_project_key.get(key).copied())
            })
        else {
            eprintln!(
                "WARN: Skipping Google place row without canonical society evidence: {}",
                record.query
            );
            continue;
        };
        record.entity_id = entity_id.to_string();
        records.push(record);
    }
    resolved.records = records;
    Ok(resolved)
}

pub async fn canonicalize_google_nearby_places_input(
    lake: &LakeStore,
    input: &GoogleNearbyPlacesWeeklyInput,
    canonical_record: &MaterializationRecord,
) -> Result<GoogleNearbyPlacesWeeklyInput, GooglePlaceAssetError> {
    let canonical = super::read_canonical_society_rows(lake, canonical_record).await?;
    let canonical_ids: HashSet<_> = canonical
        .entities
        .iter()
        .filter(|entity| entity.entity_type == "society")
        .map(|entity| entity.entity_id.as_str())
        .collect();
    let by_project_key: HashMap<_, _> = canonical
        .mappings
        .iter()
        .map(|mapping| {
            (
                mapping.project_key.as_str(),
                mapping.canonical_entity_id.as_str(),
            )
        })
        .collect();
    let by_alias: HashMap<_, _> = canonical
        .mappings
        .iter()
        .filter_map(|mapping| {
            mapping
                .alias_entity_id
                .as_deref()
                .map(|alias| (alias, mapping.canonical_entity_id.as_str()))
        })
        .collect();
    let mut resolved = input.clone();
    let mut records = Vec::with_capacity(resolved.records.len());
    for mut record in resolved.records {
        if canonical_ids.contains(record.entity_id.as_str()) {
            records.push(record);
            continue;
        }
        let Some(entity_id) = by_alias
            .get(record.entity_id.as_str())
            .copied()
            .or_else(|| {
                record
                    .project_key
                    .as_deref()
                    .and_then(|key| by_project_key.get(key).copied())
            })
        else {
            eprintln!(
                "WARN: Skipping Google nearby row without canonical society evidence: {}",
                record.query
            );
            continue;
        };
        record.entity_id = entity_id.to_string();
        records.push(record);
    }
    resolved.records = records;
    Ok(resolved)
}

#[allow(clippy::too_many_arguments)]
fn push_fact(
    row: &GooglePlaceSnapshotRecord,
    run_id: &MaterializationId,
    fact_key: &str,
    value: FactValue,
    display_template: &str,
    answers_preferences: &[&str],
    scoring: Option<(&str, f32, Vec<f64>)>,
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
) -> Result<(), GooglePlaceAssetError> {
    let value_type = match &value {
        FactValue::Numeric(_) => "numeric",
        FactValue::Text(_) => "text",
        FactValue::Bool(_) => "bool",
        FactValue::Tags(_) => "tags",
        FactValue::Score { .. } => "score",
    };
    let value_json = serde_json::to_string(&value)?;
    facts.push(SkillFactRecord {
        entity_id: row.entity_id.clone(),
        fact_key: fact_key.to_string(),
        value_type: value_type.to_string(),
        value_json: value_json.clone(),
        confidence: row.confidence,
        source_type: "Google".to_string(),
        source_url: Some(row.reviews_url.clone()),
        model: None,
        skill_id: Some("fetch_google_review_links".to_string()),
        triggered_by: Some(row.query.clone()),
        learned_at: row.fetched_at,
        run_id: run_id.to_string(),
        input_hash: format!(
            "sha256:{}",
            sha256_hex(format!("{}:{fact_key}:{value_json}", row.entity_id).as_bytes())
        ),
    });
    annotations.push(SkillFactAnnotationRecord {
        entity_id: row.entity_id.clone(),
        fact_key: fact_key.to_string(),
        display_template: Some(display_template.to_string()),
        answers_preferences_json: serde_json::to_string(answers_preferences)?,
        scoring_direction: scoring
            .as_ref()
            .map(|(direction, _, _)| direction.to_string()),
        scoring_weight: scoring.as_ref().map(|(_, weight, _)| *weight),
        scoring_thresholds_json: serde_json::to_string(
            &scoring.map_or_else(Vec::new, |(_, _, thresholds)| thresholds),
        )?,
    });
    Ok(())
}

fn write_places_parquet(
    records: &[GooglePlaceSnapshotRecord],
) -> Result<Vec<u8>, GooglePlaceAssetError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("project_key", DataType::Utf8, true),
        Field::new("query", DataType::Utf8, false),
        Field::new("place_name", DataType::Utf8, true),
        Field::new("place_id", DataType::Utf8, true),
        Field::new("reviews_url", DataType::Utf8, false),
        Field::new("rating", DataType::Float64, true),
        Field::new("review_count", DataType::UInt64, true),
        string_list_field("review_snippets", false),
        Field::new("latitude", DataType::Float64, true),
        Field::new("longitude", DataType::Float64, true),
        Field::new("address", DataType::Utf8, true),
        Field::new("confidence", DataType::Float32, false),
        Field::new("fetched_at", DataType::Utf8, false),
        Field::new("fetch_source", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            string_array(records.iter().map(|record| record.entity_id.clone())),
            optional_string_array(records.iter().map(|record| record.project_key.clone())),
            string_array(records.iter().map(|record| record.query.clone())),
            optional_string_array(records.iter().map(|record| record.place_name.clone())),
            optional_string_array(records.iter().map(|record| record.place_id.clone())),
            string_array(records.iter().map(|record| record.reviews_url.clone())),
            optional_f64_array(records.iter().map(|record| record.rating)),
            optional_u64_array(records.iter().map(|record| record.review_count)),
            string_list_array(
                records
                    .iter()
                    .map(|record| Some(record.review_snippets.clone())),
            ),
            optional_f64_array(records.iter().map(|record| record.latitude)),
            optional_f64_array(records.iter().map(|record| record.longitude)),
            optional_string_array(records.iter().map(|record| record.address.clone())),
            Arc::new(Float32Array::from(
                records
                    .iter()
                    .map(|record| record.confidence)
                    .collect::<Vec<_>>(),
            )),
            string_array(records.iter().map(|record| record.fetched_at.to_rfc3339())),
            string_array(records.iter().map(|record| record.fetch_source.clone())),
        ],
    )?;
    write_batch(batch)
}

fn read_places_parquet(
    bytes: Vec<u8>,
) -> Result<Vec<GooglePlaceSnapshotRecord>, GooglePlaceAssetError> {
    let mut records = Vec::new();
    for batch in ParquetRecordBatchReaderBuilder::try_new(Bytes::from(bytes))?.build()? {
        let batch = batch?;
        let entity_id = string_column(&batch, "entity_id")?;
        let project_key = string_column(&batch, "project_key")?;
        let query = string_column(&batch, "query")?;
        let place_name = string_column(&batch, "place_name")?;
        let place_id = string_column(&batch, "place_id")?;
        let reviews_url = string_column(&batch, "reviews_url")?;
        let rating = f64_column(&batch, "rating")?;
        let review_count = u64_column(&batch, "review_count")?;
        let latitude = optional_f64_column(&batch, "latitude");
        let longitude = optional_f64_column(&batch, "longitude");
        let address = string_column(&batch, "address")?;
        let confidence = f32_column(&batch, "confidence")?;
        let fetched_at = string_column(&batch, "fetched_at")?;
        let fetch_source = string_column(&batch, "fetch_source")?;
        for row in 0..batch.num_rows() {
            records.push(GooglePlaceSnapshotRecord {
                entity_id: required_string(entity_id, row, "entity_id")?,
                project_key: optional_string(project_key, row),
                query: required_string(query, row, "query")?,
                place_name: optional_string(place_name, row),
                place_id: optional_string(place_id, row),
                reviews_url: required_string(reviews_url, row, "reviews_url")?,
                rating: optional_f64(rating, row),
                review_count: optional_u64(review_count, row),
                review_snippets: string_list_column(&batch, "review_snippets", row)?,
                latitude: optional_f64_from_column(latitude, row),
                longitude: optional_f64_from_column(longitude, row),
                address: optional_string(address, row),
                confidence: required_f32(confidence, row, "confidence")?,
                fetched_at: DateTime::parse_from_rfc3339(&required_string(
                    fetched_at,
                    row,
                    "fetched_at",
                )?)?
                .with_timezone(&Utc),
                fetch_source: required_string(fetch_source, row, "fetch_source")?,
            });
        }
    }
    Ok(records)
}

fn write_nearby_places_parquet(
    records: &[GoogleNearbyPlaceRecord],
) -> Result<Vec<u8>, GooglePlaceAssetError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("project_key", DataType::Utf8, true),
        Field::new("query", DataType::Utf8, false),
        Field::new("category", DataType::Utf8, false),
        Field::new("place_name", DataType::Utf8, false),
        Field::new("place_id", DataType::Utf8, true),
        Field::new("place_url", DataType::Utf8, false),
        Field::new("distance_km", DataType::Float64, true),
        Field::new("latitude", DataType::Float64, true),
        Field::new("longitude", DataType::Float64, true),
        Field::new("rating", DataType::Float64, true),
        Field::new("review_count", DataType::UInt64, true),
        Field::new("primary_type", DataType::Utf8, true),
        string_list_field("place_types", false),
        Field::new("confidence", DataType::Float32, false),
        Field::new("fetched_at", DataType::Utf8, false),
        Field::new("fetch_source", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            string_array(records.iter().map(|record| record.entity_id.clone())),
            optional_string_array(records.iter().map(|record| record.project_key.clone())),
            string_array(records.iter().map(|record| record.query.clone())),
            string_array(records.iter().map(|record| record.category.clone())),
            string_array(records.iter().map(|record| record.place_name.clone())),
            optional_string_array(records.iter().map(|record| record.place_id.clone())),
            string_array(records.iter().map(|record| record.place_url.clone())),
            optional_f64_array(records.iter().map(|record| record.distance_km)),
            optional_f64_array(records.iter().map(|record| record.latitude)),
            optional_f64_array(records.iter().map(|record| record.longitude)),
            optional_f64_array(records.iter().map(|record| record.rating)),
            optional_u64_array(records.iter().map(|record| record.review_count)),
            optional_string_array(records.iter().map(|record| record.primary_type.clone())),
            string_list_array(
                records
                    .iter()
                    .map(|record| Some(record.place_types.clone())),
            ),
            Arc::new(Float32Array::from(
                records
                    .iter()
                    .map(|record| record.confidence)
                    .collect::<Vec<_>>(),
            )),
            string_array(records.iter().map(|record| record.fetched_at.to_rfc3339())),
            string_array(records.iter().map(|record| record.fetch_source.clone())),
        ],
    )?;
    write_batch(batch)
}

fn read_nearby_places_parquet(
    bytes: Vec<u8>,
) -> Result<Vec<GoogleNearbyPlaceRecord>, GooglePlaceAssetError> {
    let mut records = Vec::new();
    for batch in ParquetRecordBatchReaderBuilder::try_new(Bytes::from(bytes))?.build()? {
        let batch = batch?;
        let entity_id = string_column(&batch, "entity_id")?;
        let project_key = string_column(&batch, "project_key")?;
        let query = string_column(&batch, "query")?;
        let category = string_column(&batch, "category")?;
        let place_name = string_column(&batch, "place_name")?;
        let place_id = string_column(&batch, "place_id")?;
        let place_url = string_column(&batch, "place_url")?;
        let distance_km = f64_column(&batch, "distance_km")?;
        let latitude = optional_f64_column(&batch, "latitude");
        let longitude = optional_f64_column(&batch, "longitude");
        let rating = f64_column(&batch, "rating")?;
        let review_count = u64_column(&batch, "review_count")?;
        let primary_type = optional_string_column(&batch, "primary_type");
        let confidence = f32_column(&batch, "confidence")?;
        let fetched_at = string_column(&batch, "fetched_at")?;
        let fetch_source = string_column(&batch, "fetch_source")?;
        for row in 0..batch.num_rows() {
            records.push(GoogleNearbyPlaceRecord {
                entity_id: required_string(entity_id, row, "entity_id")?,
                project_key: optional_string(project_key, row),
                query: required_string(query, row, "query")?,
                category: required_string(category, row, "category")?,
                place_name: required_string(place_name, row, "place_name")?,
                place_id: optional_string(place_id, row),
                place_url: required_string(place_url, row, "place_url")?,
                distance_km: optional_f64(distance_km, row),
                latitude: optional_f64_from_column(latitude, row),
                longitude: optional_f64_from_column(longitude, row),
                rating: optional_f64(rating, row),
                review_count: optional_u64(review_count, row),
                primary_type: optional_string_from_column(primary_type, row),
                place_types: string_list_column(&batch, "place_types", row)?,
                confidence: required_f32(confidence, row, "confidence")?,
                fetched_at: DateTime::parse_from_rfc3339(&required_string(
                    fetched_at,
                    row,
                    "fetched_at",
                )?)?
                .with_timezone(&Utc),
                fetch_source: required_string(fetch_source, row, "fetch_source")?,
            });
        }
    }
    Ok(records)
}

fn write_batch(batch: RecordBatch) -> Result<Vec<u8>, GooglePlaceAssetError> {
    let properties = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::try_new(3)?))
        .build();
    let mut output = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut output, batch.schema(), Some(properties))?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(output)
}

fn validate_input(input: &GooglePlacesWeeklyInput) -> Result<(), GooglePlaceAssetError> {
    if input.snapshot_date.trim().is_empty() {
        return Err(GooglePlaceAssetError::InvalidInput(
            "Google place snapshot date cannot be empty".to_string(),
        ));
    }
    if input.records.is_empty() {
        return Err(GooglePlaceAssetError::InvalidInput(
            "Google place snapshot cannot be empty".to_string(),
        ));
    }
    for record in &input.records {
        if record.entity_id.trim().is_empty()
            || record.query.trim().is_empty()
            || record.reviews_url.trim().is_empty()
            || record.fetch_source.trim().is_empty()
        {
            return Err(GooglePlaceAssetError::InvalidInput(format!(
                "Google place row for {} is missing required provenance",
                record.entity_id
            )));
        }
        if !(record.reviews_url.starts_with("https://")
            || record.reviews_url.starts_with("http://"))
        {
            return Err(GooglePlaceAssetError::InvalidInput(format!(
                "Google place row for {} has a non-navigable reviews URL",
                record.entity_id
            )));
        }
        if !record.confidence.is_finite() || !(0.0..=1.0).contains(&record.confidence) {
            return Err(GooglePlaceAssetError::InvalidInput(format!(
                "Google place row for {} has invalid confidence",
                record.entity_id
            )));
        }
        if record
            .rating
            .is_some_and(|rating| !rating.is_finite() || !(0.0..=5.0).contains(&rating))
        {
            return Err(GooglePlaceAssetError::InvalidInput(format!(
                "Google place row for {} has invalid rating",
                record.entity_id
            )));
        }
    }
    Ok(())
}

fn validate_nearby_input(
    input: &GoogleNearbyPlacesWeeklyInput,
) -> Result<(), GooglePlaceAssetError> {
    let config = load_nearby_place_category_config()?;
    if input.snapshot_date.trim().is_empty() {
        return Err(GooglePlaceAssetError::InvalidInput(
            "Google nearby snapshot date cannot be empty".to_string(),
        ));
    }
    if input.records.is_empty() {
        return Err(GooglePlaceAssetError::InvalidInput(
            "Google nearby snapshot cannot be empty".to_string(),
        ));
    }
    for record in &input.records {
        if record.entity_id.trim().is_empty()
            || record.query.trim().is_empty()
            || record.place_name.trim().is_empty()
            || record.place_url.trim().is_empty()
            || record.fetch_source.trim().is_empty()
        {
            return Err(GooglePlaceAssetError::InvalidInput(format!(
                "Google nearby row for {} is missing required provenance",
                record.entity_id
            )));
        }
        if config.category_for_alias(&record.category).is_none() {
            return Err(GooglePlaceAssetError::InvalidInput(format!(
                "Google nearby row for {} has unsupported category {}",
                record.entity_id, record.category
            )));
        }
        if !is_web_url(&record.place_url) {
            return Err(GooglePlaceAssetError::InvalidInput(format!(
                "Google nearby row for {} has a non-navigable place URL",
                record.entity_id
            )));
        }
        if !record.confidence.is_finite() || !(0.0..=1.0).contains(&record.confidence) {
            return Err(GooglePlaceAssetError::InvalidInput(format!(
                "Google nearby row for {} has invalid confidence",
                record.entity_id
            )));
        }
        if record
            .distance_km
            .is_some_and(|distance| !distance.is_finite() || distance < 0.0)
        {
            return Err(GooglePlaceAssetError::InvalidInput(format!(
                "Google nearby row for {} has invalid distance",
                record.entity_id
            )));
        }
        if !valid_optional_coordinate(record.latitude, record.longitude) {
            return Err(GooglePlaceAssetError::InvalidInput(format!(
                "Google nearby row for {} has invalid coordinates",
                record.entity_id
            )));
        }
        if record
            .rating
            .is_some_and(|rating| !rating.is_finite() || !(0.0..=5.0).contains(&rating))
        {
            return Err(GooglePlaceAssetError::InvalidInput(format!(
                "Google nearby row for {} has invalid rating",
                record.entity_id
            )));
        }
    }
    Ok(())
}

fn place_watermarks(input: &GooglePlacesWeeklyInput) -> Vec<SourceWatermark> {
    if !input.source_watermarks.is_empty() {
        return input.source_watermarks.clone();
    }
    vec![SourceWatermark {
        source: "google_places".to_string(),
        high_watermark: input
            .records
            .iter()
            .map(|record| record.fetched_at)
            .max()
            .map(|time| time.to_rfc3339())
            .unwrap_or_else(|| input.snapshot_date.clone()),
    }]
}

fn nearby_watermarks(input: &GoogleNearbyPlacesWeeklyInput) -> Vec<SourceWatermark> {
    if !input.source_watermarks.is_empty() {
        return input.source_watermarks.clone();
    }
    vec![SourceWatermark {
        source: "google_nearby_places".to_string(),
        high_watermark: input
            .records
            .iter()
            .map(|record| record.fetched_at)
            .max()
            .map(|time| time.to_rfc3339())
            .unwrap_or_else(|| input.snapshot_date.clone()),
    }]
}

#[derive(Debug, Clone, Deserialize)]
struct NearbyPlaceCategoryFile {
    categories: Vec<NearbyPlaceCategory>,
}

#[derive(Debug, Clone, Deserialize)]
struct NearbyPlaceCategory {
    fact_key: String,
    #[serde(default)]
    category_aliases: Vec<String>,
    display_label: String,
    #[serde(default)]
    answers_preferences: Vec<String>,
    max_distance_km: f64,
    #[serde(default)]
    strong_local_distance_km: Option<f64>,
    #[serde(default)]
    min_review_count_for_far: Option<u64>,
    #[serde(default)]
    allow_missing_place_types: bool,
    #[serde(default)]
    accepted_place_types: Vec<String>,
    #[serde(default)]
    name_markers: Vec<String>,
    #[serde(default)]
    name_block_markers: Vec<String>,
    #[serde(default)]
    require_name_marker: bool,
    #[serde(default)]
    derived_distance_risks: Vec<NearbyDerivedDistanceRisk>,
}

#[derive(Debug, Clone, Deserialize)]
struct NearbyDerivedDistanceRisk {
    fact_key: String,
    display_label: String,
    #[serde(default)]
    answers_preferences: Vec<String>,
    max_distance_km: f64,
    #[serde(default = "default_nearby_derived_risk_weight")]
    scoring_weight: f32,
}

fn default_nearby_derived_risk_weight() -> f32 {
    1.0
}

impl NearbyPlaceCategoryFile {
    fn category_for_alias(&self, category: &str) -> Option<&NearbyPlaceCategory> {
        let normalized = normalize_category_key(category);
        self.categories.iter().find(|config| {
            config
                .category_aliases
                .iter()
                .any(|alias| normalize_category_key(alias) == normalized)
        })
    }

    fn category_for_fact_key(&self, fact_key: &str) -> Option<&NearbyPlaceCategory> {
        self.categories
            .iter()
            .find(|config| config.fact_key == fact_key)
    }
}

fn load_nearby_place_category_config() -> Result<NearbyPlaceCategoryFile, GooglePlaceAssetError> {
    let config: NearbyPlaceCategoryFile =
        load_json(&dag_root().join("nearby_place_categories.json"))?;
    validate_nearby_place_category_config(&config)?;
    Ok(config)
}

fn validate_nearby_place_category_config(
    config: &NearbyPlaceCategoryFile,
) -> Result<(), GooglePlaceAssetError> {
    if config.categories.is_empty() {
        return Err(GooglePlaceAssetError::InvalidInput(
            "nearby place category config must define at least one category".to_string(),
        ));
    }
    let mut aliases = HashSet::new();
    let mut fact_keys = HashSet::new();
    for category in &config.categories {
        if category.fact_key.trim().is_empty()
            || category.display_label.trim().is_empty()
            || category.category_aliases.is_empty()
            || !category.max_distance_km.is_finite()
            || category.max_distance_km < 0.0
        {
            return Err(GooglePlaceAssetError::InvalidInput(format!(
                "nearby category {} is missing required config",
                category.fact_key
            )));
        }
        if !fact_keys.insert(category.fact_key.clone()) {
            return Err(GooglePlaceAssetError::InvalidInput(format!(
                "nearby category fact key {} is duplicated",
                category.fact_key
            )));
        }
        for alias in &category.category_aliases {
            let alias = normalize_category_key(alias);
            if alias.is_empty() || !aliases.insert(alias.clone()) {
                return Err(GooglePlaceAssetError::InvalidInput(format!(
                    "nearby category alias {alias} is empty or duplicated"
                )));
            }
        }
        for risk in &category.derived_distance_risks {
            if risk.fact_key.trim().is_empty()
                || risk.display_label.trim().is_empty()
                || !risk.max_distance_km.is_finite()
                || risk.max_distance_km < 0.0
                || !risk.scoring_weight.is_finite()
                || risk.scoring_weight < 0.0
            {
                return Err(GooglePlaceAssetError::InvalidInput(format!(
                    "nearby category {} has invalid derived distance risk config",
                    category.fact_key
                )));
            }
        }
    }
    Ok(())
}

fn normalize_category_key(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace([' ', '-'], "_")
}

fn nearby_row_has_serving_evidence(
    category_config: &NearbyPlaceCategory,
    row: &GoogleNearbyPlaceRecord,
) -> bool {
    if !nearby_row_matches_fact_key(category_config, row) {
        return false;
    }
    let Some(distance_km) = row.distance_km else {
        return false;
    };
    if distance_km > category_config.max_distance_km {
        return false;
    }
    if let Some(strong_local_distance_km) = category_config.strong_local_distance_km {
        if distance_km <= strong_local_distance_km {
            return true;
        }
    }
    if let Some(min_review_count) = category_config.min_review_count_for_far {
        return row.review_count.unwrap_or(0) >= min_review_count;
    }
    true
}

fn nearby_row_matches_fact_key(
    category_config: &NearbyPlaceCategory,
    row: &GoogleNearbyPlaceRecord,
) -> bool {
    let mut place_types = row
        .place_types
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<HashSet<_>>();
    if let Some(primary_type) = row.primary_type.as_deref() {
        let primary_type = primary_type.trim().to_ascii_lowercase();
        if !primary_type.is_empty() {
            place_types.insert(primary_type);
        }
    }
    let has_place_types = !place_types.is_empty();
    let name = row.place_name.to_ascii_lowercase();
    if category_config
        .name_block_markers
        .iter()
        .any(|blocked| name.contains(&blocked.to_ascii_lowercase()))
    {
        return false;
    }
    let name_marker_match = category_config
        .name_markers
        .iter()
        .any(|marker| name.contains(&marker.to_ascii_lowercase()));
    if category_config.require_name_marker {
        return name_marker_match;
    }
    if category_config.allow_missing_place_types && !has_place_types {
        return true;
    }
    if category_config
        .accepted_place_types
        .iter()
        .any(|accepted| place_types.contains(&accepted.to_ascii_lowercase()))
    {
        return true;
    }
    name_marker_match
}

fn nearby_answers_preferences(category_config: &NearbyPlaceCategory) -> Vec<&str> {
    category_config
        .answers_preferences
        .iter()
        .map(String::as_str)
        .collect()
}

fn nearby_place_display(row: &GoogleNearbyPlaceRecord) -> String {
    let mut parts = Vec::new();
    if let Some(distance) = row.distance_km {
        parts.push(format!("{distance:.1} km"));
    }
    if let Some(rating) = row.rating {
        parts.push(format!("{rating:.1} rating"));
    }
    if let Some(review_count) = row.review_count {
        parts.push(format!("{review_count} reviews"));
    }
    if parts.is_empty() {
        row.place_name.clone()
    } else {
        format!("{} ({})", row.place_name, parts.join(", "))
    }
}

fn is_web_url(url: &str) -> bool {
    let url = url.trim();
    url.starts_with("https://") || url.starts_with("http://")
}

fn string_array(values: impl IntoIterator<Item = String>) -> ArrayRef {
    Arc::new(StringArray::from_iter_values(values))
}

fn optional_string_array(values: impl IntoIterator<Item = Option<String>>) -> ArrayRef {
    Arc::new(StringArray::from(values.into_iter().collect::<Vec<_>>()))
}

fn optional_f64_array(values: impl IntoIterator<Item = Option<f64>>) -> ArrayRef {
    Arc::new(Float64Array::from(values.into_iter().collect::<Vec<_>>()))
}

fn optional_u64_array(values: impl IntoIterator<Item = Option<u64>>) -> ArrayRef {
    Arc::new(UInt64Array::from(values.into_iter().collect::<Vec<_>>()))
}

fn string_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a StringArray, GooglePlaceAssetError> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| GooglePlaceAssetError::InvalidSchema(name.to_string()))
}

fn optional_string_column<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a StringArray> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<StringArray>())
}

fn optional_f64_column<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a Float64Array> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<Float64Array>())
}

fn f64_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a Float64Array, GooglePlaceAssetError> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<Float64Array>())
        .ok_or_else(|| GooglePlaceAssetError::InvalidSchema(name.to_string()))
}

fn u64_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a UInt64Array, GooglePlaceAssetError> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<UInt64Array>())
        .ok_or_else(|| GooglePlaceAssetError::InvalidSchema(name.to_string()))
}

fn f32_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a Float32Array, GooglePlaceAssetError> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<Float32Array>())
        .ok_or_else(|| GooglePlaceAssetError::InvalidSchema(name.to_string()))
}

fn required_string(
    column: &StringArray,
    row: usize,
    name: &str,
) -> Result<String, GooglePlaceAssetError> {
    if column.is_null(row) {
        return Err(GooglePlaceAssetError::InvalidSchema(name.to_string()));
    }
    Ok(column.value(row).to_string())
}

fn optional_string(column: &StringArray, row: usize) -> Option<String> {
    (!column.is_null(row)).then(|| column.value(row).to_string())
}

fn optional_string_from_column(column: Option<&StringArray>, row: usize) -> Option<String> {
    column.and_then(|column| optional_string(column, row))
}

fn string_list_column(
    batch: &RecordBatch,
    name: &str,
    row: usize,
) -> Result<Vec<String>, GooglePlaceAssetError> {
    match optional_string_list_column_value(batch, name, row)
        .map_err(GooglePlaceAssetError::InvalidSchema)?
    {
        OptionalListColumn::Values(values) => Ok(values),
        OptionalListColumn::Missing | OptionalListColumn::Null => Ok(Vec::new()),
    }
}

fn optional_f64(column: &Float64Array, row: usize) -> Option<f64> {
    (!column.is_null(row)).then(|| column.value(row))
}

fn optional_f64_from_column(column: Option<&Float64Array>, row: usize) -> Option<f64> {
    column.and_then(|column| optional_f64(column, row))
}

fn valid_optional_coordinate(latitude: Option<f64>, longitude: Option<f64>) -> bool {
    match (latitude, longitude) {
        (Some(latitude), Some(longitude)) => {
            latitude.is_finite()
                && longitude.is_finite()
                && (-90.0..=90.0).contains(&latitude)
                && (-180.0..=180.0).contains(&longitude)
        }
        (None, None) => true,
        _ => false,
    }
}

fn slug(value: &str) -> String {
    let mut output = String::new();
    let mut pending_dash = false;
    for character in value.trim().to_ascii_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            if pending_dash && !output.is_empty() {
                output.push('-');
            }
            output.push(character);
            pending_dash = false;
        } else {
            pending_dash = true;
        }
    }
    output
}

fn optional_u64(column: &UInt64Array, row: usize) -> Option<u64> {
    (!column.is_null(row)).then(|| column.value(row))
}

fn required_f32(
    column: &Float32Array,
    row: usize,
    name: &str,
) -> Result<f32, GooglePlaceAssetError> {
    if column.is_null(row) {
        return Err(GooglePlaceAssetError::InvalidSchema(name.to_string()));
    }
    Ok(column.value(row))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn asset_id(value: &str) -> AssetId {
    AssetId::new(value).expect("static Google asset id is valid")
}

#[derive(Debug)]
pub enum GooglePlaceAssetError {
    Arrow(arrow::error::ArrowError),
    Chrono(chrono::ParseError),
    Config(DagConfigError),
    InvalidInput(String),
    InvalidSchema(String),
    Lake(LakeError),
    MissingArtifact(AssetId),
    Parquet(parquet::errors::ParquetError),
    Json(serde_json::Error),
    Canonical(ReraAssetError),
}

impl fmt::Display for GooglePlaceAssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrow(err) => write!(f, "Google place Arrow error: {err}"),
            Self::Chrono(err) => write!(f, "Google place timestamp error: {err}"),
            Self::Config(err) => write!(f, "Google nearby place config error: {err}"),
            Self::InvalidInput(message) => write!(f, "invalid Google place input: {message}"),
            Self::InvalidSchema(column) => {
                write!(f, "invalid Google place Parquet column: {column}")
            }
            Self::Lake(err) => write!(f, "Google place lake error: {err}"),
            Self::MissingArtifact(asset_id) => {
                write!(f, "Google place artifact missing for {asset_id}")
            }
            Self::Parquet(err) => write!(f, "Google place Parquet error: {err}"),
            Self::Json(err) => write!(f, "Google place JSON error: {err}"),
            Self::Canonical(err) => write!(f, "Google canonical society lookup failed: {err}"),
        }
    }
}

impl std::error::Error for GooglePlaceAssetError {}

impl From<arrow::error::ArrowError> for GooglePlaceAssetError {
    fn from(value: arrow::error::ArrowError) -> Self {
        Self::Arrow(value)
    }
}

impl From<chrono::ParseError> for GooglePlaceAssetError {
    fn from(value: chrono::ParseError) -> Self {
        Self::Chrono(value)
    }
}

impl From<DagConfigError> for GooglePlaceAssetError {
    fn from(value: DagConfigError) -> Self {
        Self::Config(value)
    }
}

impl From<LakeError> for GooglePlaceAssetError {
    fn from(value: LakeError) -> Self {
        Self::Lake(value)
    }
}

impl From<parquet::errors::ParquetError> for GooglePlaceAssetError {
    fn from(value: parquet::errors::ParquetError) -> Self {
        Self::Parquet(value)
    }
}

impl From<serde_json::Error> for GooglePlaceAssetError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<ReraAssetError> for GooglePlaceAssetError {
    fn from(value: ReraAssetError) -> Self {
        Self::Canonical(value)
    }
}
