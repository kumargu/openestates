use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use arrow::array::{Array, ArrayRef, Float64Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use datafusion::prelude::SessionContext;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::knowledge::FactValue;
use crate::lake::{LakeError, LakeStore};
use crate::parquet_data::optional_string_array;

use super::{
    read_canonical_society_rows, read_rera_project_rows, ArtifactRef, AssetId,
    AssetMaterializationStore, AssetPartition, AssetPathBuilder, AssetStage, MaterializationId,
    MaterializationRecord, ReraAssetError, SkillFactAnnotationRecord, SkillFactMaterializeError,
    SkillFactRecord, SkillFactsInput, SourceWatermark,
};

pub const EXTERNAL_LISTINGS_WEEKLY_ASSET_ID: &str = "external_listings_weekly";
pub const EXTERNAL_LISTING_FACTS_ASSET_ID: &str = "external_listing_facts";
pub const BUILDER_RERA_AGGREGATES_ASSET_ID: &str = "builder_rera_aggregates";

const OBSERVATION_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalListingObservationRecord {
    pub entity_id: String,
    pub project_key: Option<String>,
    pub source_name: String,
    pub source_url: Option<String>,
    #[serde(default)]
    pub listing_type: Option<String>,
    pub price: Option<f64>,
    #[serde(default)]
    pub price_min: Option<f64>,
    #[serde(default)]
    pub price_max: Option<f64>,
    pub area_sqft: Option<f64>,
    #[serde(default)]
    pub area_sqft_min: Option<f64>,
    #[serde(default)]
    pub area_sqft_max: Option<f64>,
    #[serde(default)]
    pub price_per_sqft_min: Option<f64>,
    #[serde(default)]
    pub price_per_sqft_max: Option<f64>,
    #[serde(default)]
    pub price_display: Option<String>,
    #[serde(default)]
    pub area_display: Option<String>,
    #[serde(default)]
    pub price_per_sqft_display: Option<String>,
    #[serde(default)]
    pub configuration: Option<String>,
    pub area_type: Option<String>,
    pub bhk: Option<f64>,
    pub bathrooms: Option<f64>,
    pub floor: Option<String>,
    pub society: Option<String>,
    pub locality: Option<String>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalListingsWeeklyInput {
    pub snapshot_date: String,
    #[serde(default)]
    pub records: Vec<ExternalListingObservationRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_watermarks: Vec<SourceWatermark>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservationSnapshotManifest {
    pub asset_id: String,
    pub format_version: u32,
    pub snapshot_date: String,
    pub run_id: String,
    pub created_at: DateTime<Utc>,
    pub row_count: u64,
    pub artifacts: Vec<ArtifactRef>,
}

#[derive(Clone)]
pub struct ProjectEnrichmentMaterializer {
    lake: LakeStore,
    materializations: AssetMaterializationStore,
}

impl ProjectEnrichmentMaterializer {
    pub fn new(lake: LakeStore) -> Self {
        Self {
            materializations: AssetMaterializationStore::new(lake.clone()),
            lake,
        }
    }

    pub async fn materialize_external_listings(
        &self,
        input: &ExternalListingsWeeklyInput,
        parent_materializations: Vec<MaterializationId>,
        dag_run_id: MaterializationId,
        record_partition: AssetPartition,
    ) -> Result<MaterializationRecord, ProjectEnrichmentAssetError> {
        validate_external_listing_input(input)?;
        self.materialize_observations(
            EXTERNAL_LISTINGS_WEEKLY_ASSET_ID,
            "external_listings",
            &input.snapshot_date,
            input.records.len(),
            write_external_listing_parquet(&input.records)?,
            "listings/part-00000.parquet",
            &input.source_watermarks,
            parent_materializations,
            dag_run_id,
            record_partition,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn materialize_observations(
        &self,
        asset_id: &str,
        source: &str,
        snapshot_date: &str,
        row_count: usize,
        parquet: Vec<u8>,
        parquet_name: &str,
        source_watermarks: &[SourceWatermark],
        parent_materializations: Vec<MaterializationId>,
        dag_run_id: MaterializationId,
        record_partition: AssetPartition,
    ) -> Result<MaterializationRecord, ProjectEnrichmentAssetError> {
        let run_id = dag_run_id.to_string();
        let artifact_partition = AssetPartition::new([("dt", snapshot_date)]);
        let parquet_key =
            AssetPathBuilder::raw_snapshot_key(source, &artifact_partition, &run_id, parquet_name);
        let parquet_meta = self.lake.put_bytes(&parquet_key, parquet).await?;
        let mut artifacts = vec![ArtifactRef::parquet(parquet_meta)];
        let manifest_key = AssetPathBuilder::raw_snapshot_key(
            source,
            &artifact_partition,
            &run_id,
            "manifest.json",
        );
        let manifest = ObservationSnapshotManifest {
            asset_id: asset_id.to_string(),
            format_version: OBSERVATION_FORMAT_VERSION,
            snapshot_date: snapshot_date.to_string(),
            run_id,
            created_at: Utc::now(),
            row_count: row_count as u64,
            artifacts: artifacts.clone(),
        };
        let manifest_meta = self.lake.put_json(&manifest_key, &manifest).await?;
        artifacts.push(ArtifactRef::json(manifest_meta));
        artifacts.sort_by(|left, right| left.key.cmp(&right.key));

        let record = MaterializationRecord::succeeded(
            asset_id_value(asset_id),
            AssetStage::Raw,
            record_partition,
            snapshot_date,
            artifacts,
        )
        .with_run_id(dag_run_id)
        .with_parent_materializations(parent_materializations)
        .with_source_watermarks(source_watermarks.to_vec())
        .with_row_count(row_count as u64);
        self.materializations.write_materialization(&record).await?;
        Ok(record)
    }
}

pub async fn external_listing_facts_input_with_aliases(
    lake: &LakeStore,
    listing_record: &MaterializationRecord,
    canonical_record: &MaterializationRecord,
    run_id: &MaterializationId,
) -> Result<SkillFactsInput, ProjectEnrichmentAssetError> {
    let rows = read_external_listing_rows(lake, listing_record).await?;
    let canonical = read_canonical_society_rows(lake, canonical_record).await?;
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
    let mut facts = Vec::new();
    let mut annotations = Vec::new();
    for row in rows {
        append_listing_facts(&row, &row.entity_id, run_id, &mut facts, &mut annotations)?;
        if let Some(alias) = aliases.get(&row.entity_id) {
            append_listing_facts(&row, alias, run_id, &mut facts, &mut annotations)?;
        }
    }
    Ok(SkillFactsInput {
        source: "external_listing".to_string(),
        snapshot_date: listing_record.version.clone(),
        facts,
        fact_annotations: annotations,
        source_watermarks: listing_record.source_watermarks.clone(),
    })
}

pub async fn builder_rera_aggregate_facts_input(
    lake: &LakeStore,
    rera_record: &MaterializationRecord,
    canonical_record: &MaterializationRecord,
    run_id: &MaterializationId,
) -> Result<SkillFactsInput, ProjectEnrichmentAssetError> {
    let projects = read_rera_project_rows(lake, rera_record).await?;
    let canonical = read_canonical_society_rows(lake, canonical_record).await?;
    let known_builders = canonical
        .entities
        .into_iter()
        .filter(|entity| entity.entity_type == "builder")
        .map(|entity| entity.entity_id)
        .collect::<HashSet<_>>();
    let batch = builder_input_batch(&projects)?;
    let context = SessionContext::new();
    context.register_batch("rera_projects", batch)?;
    let batches = context
        .sql(
            "SELECT promoter_name, COALESCE(status, 'Unknown') AS status, \
                    COUNT(*) AS project_count, \
                    SUM(COALESCE(total_land_area_sqm, 0.0)) AS total_land_area_sqm \
             FROM rera_projects \
             WHERE promoter_name IS NOT NULL AND trim(promoter_name) <> '' \
             GROUP BY promoter_name, COALESCE(status, 'Unknown')",
        )
        .await?
        .collect()
        .await?;

    #[derive(Default)]
    struct BuilderAggregate {
        project_count: u64,
        total_land_area_sqm: f64,
        statuses: BTreeMap<String, u64>,
    }
    let mut aggregates = BTreeMap::<String, BuilderAggregate>::new();
    for batch in batches {
        let promoter = string_column(&batch, "promoter_name")?;
        let status = string_column(&batch, "status")?;
        let project_count = int64_column(&batch, "project_count")?;
        let land_area = f64_column(&batch, "total_land_area_sqm")?;
        for row in 0..batch.num_rows() {
            let entry = aggregates
                .entry(promoter.value(row).to_string())
                .or_default();
            let count = project_count.value(row).max(0) as u64;
            entry.project_count += count;
            entry.total_land_area_sqm += land_area.value(row);
            entry.statuses.insert(status.value(row).to_string(), count);
        }
    }

    let learned_at = projects
        .iter()
        .map(|project| project.fetched_at)
        .max()
        .unwrap_or_else(Utc::now);
    let mut facts = Vec::new();
    let mut annotations = Vec::new();
    for (promoter, aggregate) in aggregates {
        let entity_id = format!("builder:{}", slug(&promoter));
        if !known_builders.contains(&entity_id) {
            continue;
        }
        append_derived_fact(
            &entity_id,
            "builder_project_count",
            FactValue::Numeric(aggregate.project_count as f64),
            1.0,
            "Computed",
            None,
            "builder_rera_aggregate",
            learned_at,
            run_id,
            "{value} RERA projects in the current registry",
            &[
                "experienced builder",
                "builder track record",
                "project count",
            ],
            Some(("HigherIsBetter", 0.5, vec![20.0, 5.0])),
            &mut facts,
            &mut annotations,
        )?;
        if aggregate.total_land_area_sqm > 0.0 {
            append_derived_fact(
                &entity_id,
                "builder_rera_registered_land_area_sqm",
                FactValue::Numeric(aggregate.total_land_area_sqm),
                1.0,
                "Computed",
                None,
                "builder_rera_aggregate",
                learned_at,
                run_id,
                "RERA portfolio land area: {value} sqm",
                &["builder scale", "builder portfolio"],
                None,
                &mut facts,
                &mut annotations,
            )?;
        }
        let status_tags = aggregate
            .statuses
            .into_iter()
            .map(|(status, count)| format!("{status}: {count}"))
            .collect::<Vec<_>>();
        append_derived_fact(
            &entity_id,
            "builder_rera_status_breakdown",
            FactValue::Tags(status_tags),
            1.0,
            "Computed",
            None,
            "builder_rera_aggregate",
            learned_at,
            run_id,
            "RERA project statuses: {value}",
            &["builder track record", "builder status"],
            Some(("TextMatch", 0.5, Vec::new())),
            &mut facts,
            &mut annotations,
        )?;
    }

    Ok(SkillFactsInput {
        source: "rera_aggregate".to_string(),
        snapshot_date: rera_record.version.clone(),
        facts,
        fact_annotations: annotations,
        source_watermarks: rera_record.source_watermarks.clone(),
    })
}

fn append_listing_facts(
    row: &ExternalListingObservationRecord,
    entity_id: &str,
    run_id: &MaterializationId,
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
) -> Result<(), ProjectEnrichmentAssetError> {
    let Some(bhk) = row.bhk else {
        return Ok(());
    };
    let Some(price) = row.price.filter(|value| value.is_finite() && *value > 0.0) else {
        return Ok(());
    };
    let Some(area_sqft) = row
        .area_sqft
        .filter(|value| value.is_finite() && *value > 0.0)
    else {
        return Ok(());
    };
    let bhk_key = bhk_fact_suffix(bhk);
    if bhk_key.is_empty() {
        return Ok(());
    }
    let source_url = row.source_url.clone();
    let source_type = "ExternalListing";
    let skill_id = "external_listing_facts";
    let price_inr = price.round();
    let area_sqft = area_sqft.round();
    let price_min = row.price_min.unwrap_or(price).round();
    let price_max = row.price_max.unwrap_or(price).round();
    let area_sqft_min = row.area_sqft_min.unwrap_or(area_sqft).round();
    let area_sqft_max = row.area_sqft_max.unwrap_or(area_sqft).round();
    let source_name = row.source_name.trim();
    let listing_type = row
        .listing_type
        .as_deref()
        .map(str::trim)
        .unwrap_or("sale")
        .to_ascii_lowercase();
    let is_rent = listing_type == "rent";
    let price_range_display = inr_range_display(price_min as u64, price_max as u64);
    let area_range_display = sqft_range_display(area_sqft_min as u64, area_sqft_max as u64);
    let ppsf_range_display = price_per_sqft_range_display(
        row.price_per_sqft_min,
        row.price_per_sqft_max,
        price_min,
        price_max,
        area_sqft_min,
        area_sqft_max,
    );
    let listing_payload = serde_json::json!({
        "price": price_inr as u64,
        "price_min": price_min as u64,
        "price_max": price_max as u64,
        "price_display": row.price_display.as_deref(),
        "area_sqft": area_sqft as u64,
        "area_sqft_min": area_sqft_min as u64,
        "area_sqft_max": area_sqft_max as u64,
        "area_display": row.area_display.as_deref(),
        "price_per_sqft_min": row.price_per_sqft_min,
        "price_per_sqft_max": row.price_per_sqft_max,
        "price_per_sqft_display": row.price_per_sqft_display.as_deref(),
        "area_type": row.area_type.as_deref().unwrap_or("unknown"),
        "bhk": bhk,
        "configuration": row.configuration.as_deref(),
        "bathrooms": row.bathrooms,
        "floor": row.floor.as_deref(),
        "society": row.society.as_deref(),
        "locality": row.locality.as_deref(),
        "source_url": row.source_url.as_deref(),
        "listing_type": listing_type,
        "observed_at": row.observed_at.to_rfc3339(),
    });
    if is_rent {
        append_rent_facts(
            row,
            entity_id,
            run_id,
            bhk,
            &bhk_key,
            price_inr,
            price_min,
            price_max,
            area_sqft,
            area_sqft_min,
            area_sqft_max,
            &price_range_display,
            &area_range_display,
            &ppsf_range_display,
            &listing_payload,
            source_name,
            facts,
            annotations,
        )?;
        return Ok(());
    }
    append_derived_fact(
        entity_id,
        &format!("listing_{bhk_key}"),
        FactValue::Text(listing_payload.to_string()),
        0.7,
        source_type,
        source_url.clone(),
        skill_id,
        row.observed_at,
        run_id,
        &format!(
            "{} listing: INR {} for {}",
            bhk_display(bhk),
            price_range_display,
            area_range_display
        ),
        &[
            "price",
            "budget",
            "listing price",
            "sqft",
            "bhk",
            "market listing",
        ],
        Some(("TextMatch", 2.0, Vec::new())),
        facts,
        annotations,
    )?;
    append_derived_fact(
        entity_id,
        &format!("listing_price_{bhk_key}"),
        FactValue::Numeric(price_inr),
        0.7,
        source_type,
        source_url.clone(),
        skill_id,
        row.observed_at,
        run_id,
        &format!("{} listing price: INR {{value}}", bhk_display(bhk)),
        &["price", "budget", "listing price"],
        Some(("LowerIsBetter", 1.5, Vec::new())),
        facts,
        annotations,
    )?;
    append_derived_fact(
        entity_id,
        &format!("listing_price_range_{bhk_key}"),
        FactValue::Text(price_range_display.clone()),
        0.7,
        source_type,
        source_url.clone(),
        skill_id,
        row.observed_at,
        run_id,
        &format!("{} listing price range: {{value}}", bhk_display(bhk)),
        &["price", "budget", "listing price", "price range"],
        Some(("TextMatch", 1.0, Vec::new())),
        facts,
        annotations,
    )?;
    append_derived_fact(
        entity_id,
        &format!("listing_area_sqft_{bhk_key}"),
        FactValue::Numeric(area_sqft),
        0.7,
        source_type,
        source_url.clone(),
        skill_id,
        row.observed_at,
        run_id,
        &format!("{} listing area: {{value}} sq ft", bhk_display(bhk)),
        &["sqft", "area", "large apartment"],
        Some(("HigherIsBetter", 0.7, Vec::new())),
        facts,
        annotations,
    )?;
    append_derived_fact(
        entity_id,
        &format!("listing_area_sqft_range_{bhk_key}"),
        FactValue::Text(area_range_display.clone()),
        0.7,
        source_type,
        source_url.clone(),
        skill_id,
        row.observed_at,
        run_id,
        &format!("{} listing area range: {{value}}", bhk_display(bhk)),
        &["sqft", "area", "large apartment", "area range"],
        Some(("TextMatch", 0.7, Vec::new())),
        facts,
        annotations,
    )?;
    if let Some(value) = ppsf_range_display {
        append_derived_fact(
            entity_id,
            &format!("listing_price_per_sqft_range_{bhk_key}"),
            FactValue::Text(value),
            0.7,
            source_type,
            source_url.clone(),
            skill_id,
            row.observed_at,
            run_id,
            &format!("{} listing rate range: {{value}}", bhk_display(bhk)),
            &["price per sqft", "market rate", "rate range"],
            Some(("TextMatch", 0.8, Vec::new())),
            facts,
            annotations,
        )?;
    }
    append_derived_fact(
        entity_id,
        &format!("listing_area_type_{bhk_key}"),
        FactValue::Text(
            row.area_type
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
        ),
        0.7,
        source_type,
        source_url.clone(),
        skill_id,
        row.observed_at,
        run_id,
        &format!("{} listing area type: {{value}}", bhk_display(bhk)),
        &["area type", "carpet area", "super built up"],
        None,
        facts,
        annotations,
    )?;
    if let Some(bathrooms) = row.bathrooms.filter(|value| value.is_finite()) {
        append_derived_fact(
            entity_id,
            &format!("listing_bathrooms_{bhk_key}"),
            FactValue::Numeric(bathrooms),
            0.7,
            source_type,
            source_url.clone(),
            skill_id,
            row.observed_at,
            run_id,
            &format!("{} listing bathrooms: {{value}}", bhk_display(bhk)),
            &["bathrooms"],
            None,
            facts,
            annotations,
        )?;
    }
    if let Some(floor) = row.floor.as_ref().filter(|floor| !floor.trim().is_empty()) {
        append_derived_fact(
            entity_id,
            &format!("listing_floor_{bhk_key}"),
            FactValue::Text(floor.clone()),
            0.7,
            source_type,
            source_url.clone(),
            skill_id,
            row.observed_at,
            run_id,
            &format!("{} listing floor: {{value}}", bhk_display(bhk)),
            &["floor"],
            None,
            facts,
            annotations,
        )?;
    }
    if let Some(value) = row.society.clone().filter(|value| !value.trim().is_empty()) {
        append_derived_fact(
            entity_id,
            "listing_society",
            FactValue::Text(value),
            0.7,
            source_type,
            source_url.clone(),
            skill_id,
            row.observed_at,
            run_id,
            "Listing society: {value}",
            &["society"],
            None,
            facts,
            annotations,
        )?;
    }
    if let Some(value) = row
        .locality
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        append_derived_fact(
            entity_id,
            "listing_locality",
            FactValue::Text(value),
            0.7,
            source_type,
            source_url.clone(),
            skill_id,
            row.observed_at,
            run_id,
            "Listing locality: {value}",
            &["locality", "area"],
            None,
            facts,
            annotations,
        )?;
    }
    if let Some(value) = row
        .source_url
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        append_derived_fact(
            entity_id,
            &format!("listing_source_url_{bhk_key}"),
            FactValue::Text(value),
            0.7,
            source_type,
            source_url.clone(),
            skill_id,
            row.observed_at,
            run_id,
            "Listing source: {value}",
            &["source", "listing source"],
            None,
            facts,
            annotations,
        )?;
    }
    append_derived_fact(
        entity_id,
        &format!("listing_observed_at_{bhk_key}"),
        FactValue::Text(row.observed_at.to_rfc3339()),
        0.7,
        source_type,
        source_url,
        skill_id,
        row.observed_at,
        run_id,
        &format!("{} listing observed at {{value}}", bhk_display(bhk)),
        &["fresh listing", "recent listing"],
        None,
        facts,
        annotations,
    )?;
    if !source_name.is_empty() {
        append_derived_fact(
            entity_id,
            &format!("listing_source_name_{bhk_key}"),
            FactValue::Text(source_name.to_string()),
            0.7,
            source_type,
            row.source_url.clone(),
            skill_id,
            row.observed_at,
            run_id,
            &format!("{} listing source: {{value}}", bhk_display(bhk)),
            &["source", "listing source"],
            None,
            facts,
            annotations,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn append_rent_facts(
    row: &ExternalListingObservationRecord,
    entity_id: &str,
    run_id: &MaterializationId,
    bhk: f64,
    bhk_key: &str,
    monthly_rent: f64,
    rent_min: f64,
    rent_max: f64,
    area_sqft: f64,
    area_sqft_min: f64,
    area_sqft_max: f64,
    rent_range_display: &str,
    area_range_display: &str,
    ppsf_range_display: &Option<String>,
    listing_payload: &serde_json::Value,
    source_name: &str,
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
) -> Result<(), ProjectEnrichmentAssetError> {
    let source_url = row.source_url.clone();
    let source_type = "ExternalListing";
    let skill_id = "external_listing_facts";
    append_derived_fact(
        entity_id,
        &format!("rent_{bhk_key}"),
        FactValue::Text(listing_payload.to_string()),
        0.68,
        source_type,
        source_url.clone(),
        skill_id,
        row.observed_at,
        run_id,
        &format!(
            "{} rent listing: INR {} per month for {}",
            bhk_display(bhk),
            rent_range_display,
            area_range_display
        ),
        &["rent", "monthly rent", "rental", "lease", "bhk"],
        Some(("TextMatch", 1.5, Vec::new())),
        facts,
        annotations,
    )?;
    append_derived_fact(
        entity_id,
        &format!("rent_monthly_{bhk_key}"),
        FactValue::Numeric(monthly_rent),
        0.68,
        source_type,
        source_url.clone(),
        skill_id,
        row.observed_at,
        run_id,
        &format!("{} monthly rent: INR {{value}}", bhk_display(bhk)),
        &["rent", "monthly rent", "rental budget"],
        Some(("LowerIsBetter", 1.0, Vec::new())),
        facts,
        annotations,
    )?;
    append_derived_fact(
        entity_id,
        &format!("rent_monthly_range_{bhk_key}"),
        FactValue::Text(rent_range_display.to_string()),
        0.68,
        source_type,
        source_url.clone(),
        skill_id,
        row.observed_at,
        run_id,
        &format!("{} monthly rent range: {{value}}", bhk_display(bhk)),
        &["rent", "monthly rent", "rent range"],
        Some(("TextMatch", 1.0, Vec::new())),
        facts,
        annotations,
    )?;
    append_derived_fact(
        entity_id,
        &format!("rent_area_sqft_{bhk_key}"),
        FactValue::Numeric(area_sqft),
        0.68,
        source_type,
        source_url.clone(),
        skill_id,
        row.observed_at,
        run_id,
        &format!("{} rent listing area: {{value}} sq ft", bhk_display(bhk)),
        &["rent area", "sqft", "rental"],
        None,
        facts,
        annotations,
    )?;
    append_derived_fact(
        entity_id,
        &format!("rent_area_sqft_range_{bhk_key}"),
        FactValue::Text(sqft_range_display(
            area_sqft_min as u64,
            area_sqft_max as u64,
        )),
        0.68,
        source_type,
        source_url.clone(),
        skill_id,
        row.observed_at,
        run_id,
        &format!("{} rent listing area range: {{value}}", bhk_display(bhk)),
        &["rent area", "sqft", "rental"],
        None,
        facts,
        annotations,
    )?;
    if let Some(value) = ppsf_range_display {
        append_derived_fact(
            entity_id,
            &format!("rent_per_sqft_range_{bhk_key}"),
            FactValue::Text(value.clone()),
            0.68,
            source_type,
            source_url.clone(),
            skill_id,
            row.observed_at,
            run_id,
            &format!("{} rent rate range: {{value}}", bhk_display(bhk)),
            &["rent per sqft", "rental rate"],
            None,
            facts,
            annotations,
        )?;
    }
    if let Some(value) = row
        .source_url
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        append_derived_fact(
            entity_id,
            &format!("rent_source_url_{bhk_key}"),
            FactValue::Text(value),
            0.68,
            source_type,
            source_url.clone(),
            skill_id,
            row.observed_at,
            run_id,
            "Rent source: {value}",
            &["rent source", "source"],
            None,
            facts,
            annotations,
        )?;
    }
    if !source_name.is_empty() {
        append_derived_fact(
            entity_id,
            &format!("rent_source_name_{bhk_key}"),
            FactValue::Text(source_name.to_string()),
            0.68,
            source_type,
            source_url,
            skill_id,
            row.observed_at,
            run_id,
            &format!("{} rent source: {{value}}", bhk_display(bhk)),
            &["rent source", "source"],
            None,
            facts,
            annotations,
        )?;
    }
    let _ = rent_min;
    let _ = rent_max;
    Ok(())
}

fn bhk_fact_suffix(bhk: f64) -> String {
    if !bhk.is_finite() || bhk <= 0.0 {
        return String::new();
    }
    let value = if bhk.fract().abs() < f64::EPSILON {
        format!("{}", bhk as u64)
    } else {
        format!("{bhk:.1}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    };
    format!("{}bhk", value.replace('.', "_"))
}

fn bhk_display(bhk: f64) -> String {
    if bhk.fract().abs() < f64::EPSILON {
        format!("{} BHK", bhk as u64)
    } else {
        format!("{bhk:.1} BHK")
    }
}

fn compact_inr(value: u64) -> String {
    if value >= 10_000_000 {
        format!("{:.2} Cr", value as f64 / 10_000_000.0)
    } else if value >= 100_000 {
        format!("{:.1} L", value as f64 / 100_000.0)
    } else {
        value.to_string()
    }
}

fn inr_range_display(low: u64, high: u64) -> String {
    if low == high {
        compact_inr(low)
    } else {
        format!("{}-{}", compact_inr(low), compact_inr(high))
    }
}

fn sqft_range_display(low: u64, high: u64) -> String {
    if low == high {
        format!("{low} sq ft")
    } else {
        format!("{low}-{high} sq ft")
    }
}

fn price_per_sqft_range_display(
    explicit_low: Option<f64>,
    explicit_high: Option<f64>,
    price_min: f64,
    price_max: f64,
    area_min: f64,
    area_max: f64,
) -> Option<String> {
    let low = explicit_low.or_else(|| (area_max > 0.0).then_some(price_min / area_max));
    let high = explicit_high.or_else(|| (area_min > 0.0).then_some(price_max / area_min));
    let low = low?.round() as u64;
    let high = high?.round() as u64;
    if low == high {
        Some(format!("INR {low}/sq ft"))
    } else {
        Some(format!("INR {low}-{high}/sq ft"))
    }
}

#[allow(clippy::too_many_arguments)]
fn append_derived_fact(
    entity_id: &str,
    fact_key: &str,
    value: FactValue,
    confidence: f32,
    source_type: &str,
    source_url: Option<String>,
    skill_id: &str,
    learned_at: DateTime<Utc>,
    run_id: &MaterializationId,
    display_template: &str,
    answers_preferences: &[&str],
    scoring: Option<(&str, f32, Vec<f64>)>,
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
) -> Result<(), ProjectEnrichmentAssetError> {
    let value_type = match value {
        FactValue::Numeric(_) => "numeric",
        FactValue::Text(_) => "text",
        FactValue::Bool(_) => "bool",
        FactValue::Tags(_) => "tags",
        FactValue::Score { .. } => "score",
    };
    let value_json = serde_json::to_string(&value)?;
    facts.push(SkillFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        value_type: value_type.to_string(),
        value_json: value_json.clone(),
        confidence,
        source_type: source_type.to_string(),
        source_url,
        model: None,
        skill_id: Some(skill_id.to_string()),
        triggered_by: Some("asset_dag".to_string()),
        learned_at,
        run_id: run_id.to_string(),
        input_hash: sha256_hex(format!("{entity_id}:{fact_key}:{value_json}").as_bytes()),
    });
    annotations.push(SkillFactAnnotationRecord {
        entity_id: entity_id.to_string(),
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

fn builder_input_batch(
    projects: &[super::ReraProjectSnapshotRecord],
) -> Result<RecordBatch, ProjectEnrichmentAssetError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("promoter_name", DataType::Utf8, true),
        Field::new("status", DataType::Utf8, true),
        Field::new("total_land_area_sqm", DataType::Float64, true),
    ]));
    Ok(RecordBatch::try_new(
        schema,
        vec![
            optional_string_array(projects.iter().map(|project| project.promoter_name.clone())),
            optional_string_array(projects.iter().map(|project| project.status.clone())),
            Arc::new(Float64Array::from(
                projects
                    .iter()
                    .map(|project| project.total_land_area_sqm)
                    .collect::<Vec<_>>(),
            )),
        ],
    )?)
}

fn validate_external_listing_input(
    input: &ExternalListingsWeeklyInput,
) -> Result<(), ProjectEnrichmentAssetError> {
    if input.snapshot_date.trim().is_empty() {
        return Err(ProjectEnrichmentAssetError::InvalidInput(
            "external listing snapshot date is empty".to_string(),
        ));
    }
    if input.records.is_empty()
        && !input.source_watermarks.iter().any(|watermark| {
            watermark.source.ends_with("_empty") || watermark.source.ends_with("_skipped")
        })
    {
        return Err(ProjectEnrichmentAssetError::InvalidInput(
            "external listing snapshot is empty".to_string(),
        ));
    }
    for record in &input.records {
        if record.entity_id.trim().is_empty() || record.source_name.trim().is_empty() {
            return Err(ProjectEnrichmentAssetError::InvalidInput(
                "external listing record is missing entity or source".to_string(),
            ));
        }
        if record.price.is_none() || record.area_sqft.is_none() || record.bhk.is_none() {
            return Err(ProjectEnrichmentAssetError::InvalidInput(format!(
                "external listing record for {} is missing price, area, or bhk",
                record.entity_id
            )));
        }
    }
    Ok(())
}

fn write_external_listing_parquet(
    records: &[ExternalListingObservationRecord],
) -> Result<Vec<u8>, ProjectEnrichmentAssetError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("project_key", DataType::Utf8, true),
        Field::new("source_name", DataType::Utf8, false),
        Field::new("source_url", DataType::Utf8, true),
        Field::new("listing_type", DataType::Utf8, true),
        Field::new("price", DataType::Float64, true),
        Field::new("price_min", DataType::Float64, true),
        Field::new("price_max", DataType::Float64, true),
        Field::new("area_sqft", DataType::Float64, true),
        Field::new("area_sqft_min", DataType::Float64, true),
        Field::new("area_sqft_max", DataType::Float64, true),
        Field::new("price_per_sqft_min", DataType::Float64, true),
        Field::new("price_per_sqft_max", DataType::Float64, true),
        Field::new("price_display", DataType::Utf8, true),
        Field::new("area_display", DataType::Utf8, true),
        Field::new("price_per_sqft_display", DataType::Utf8, true),
        Field::new("configuration", DataType::Utf8, true),
        Field::new("area_type", DataType::Utf8, true),
        Field::new("bhk", DataType::Float64, true),
        Field::new("bathrooms", DataType::Float64, true),
        Field::new("floor", DataType::Utf8, true),
        Field::new("society", DataType::Utf8, true),
        Field::new("locality", DataType::Utf8, true),
        Field::new("observed_at", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            strings(records.iter().map(|record| record.entity_id.clone())),
            optional_string_array(records.iter().map(|record| record.project_key.clone())),
            strings(records.iter().map(|record| record.source_name.clone())),
            optional_string_array(records.iter().map(|record| record.source_url.clone())),
            optional_string_array(records.iter().map(|record| record.listing_type.clone())),
            optional_f64s(records.iter().map(|record| record.price)),
            optional_f64s(records.iter().map(|record| record.price_min)),
            optional_f64s(records.iter().map(|record| record.price_max)),
            optional_f64s(records.iter().map(|record| record.area_sqft)),
            optional_f64s(records.iter().map(|record| record.area_sqft_min)),
            optional_f64s(records.iter().map(|record| record.area_sqft_max)),
            optional_f64s(records.iter().map(|record| record.price_per_sqft_min)),
            optional_f64s(records.iter().map(|record| record.price_per_sqft_max)),
            optional_string_array(records.iter().map(|record| record.price_display.clone())),
            optional_string_array(records.iter().map(|record| record.area_display.clone())),
            optional_string_array(
                records
                    .iter()
                    .map(|record| record.price_per_sqft_display.clone()),
            ),
            optional_string_array(records.iter().map(|record| record.configuration.clone())),
            optional_string_array(records.iter().map(|record| record.area_type.clone())),
            optional_f64s(records.iter().map(|record| record.bhk)),
            optional_f64s(records.iter().map(|record| record.bathrooms)),
            optional_string_array(records.iter().map(|record| record.floor.clone())),
            optional_string_array(records.iter().map(|record| record.society.clone())),
            optional_string_array(records.iter().map(|record| record.locality.clone())),
            strings(records.iter().map(|record| record.observed_at.to_rfc3339())),
        ],
    )?;
    write_batch(schema, batch)
}

async fn read_external_listing_rows(
    lake: &LakeStore,
    record: &MaterializationRecord,
) -> Result<Vec<ExternalListingObservationRecord>, ProjectEnrichmentAssetError> {
    let bytes = read_artifact(lake, record, "listings/part-00000.parquet").await?;
    let mut rows = Vec::new();
    for batch in parquet_batches(bytes)? {
        let entity_id = string_column(&batch, "entity_id")?;
        let project_key = string_column(&batch, "project_key")?;
        let source_name = string_column(&batch, "source_name")?;
        let source_url = string_column(&batch, "source_url")?;
        let listing_type = optional_string_column(&batch, "listing_type")?;
        let price = f64_column(&batch, "price")?;
        let price_min = optional_f64_column(&batch, "price_min")?;
        let price_max = optional_f64_column(&batch, "price_max")?;
        let area_sqft = f64_column(&batch, "area_sqft")?;
        let area_sqft_min = optional_f64_column(&batch, "area_sqft_min")?;
        let area_sqft_max = optional_f64_column(&batch, "area_sqft_max")?;
        let price_per_sqft_min = optional_f64_column(&batch, "price_per_sqft_min")?;
        let price_per_sqft_max = optional_f64_column(&batch, "price_per_sqft_max")?;
        let price_display = optional_string_column(&batch, "price_display")?;
        let area_display = optional_string_column(&batch, "area_display")?;
        let price_per_sqft_display = optional_string_column(&batch, "price_per_sqft_display")?;
        let configuration = optional_string_column(&batch, "configuration")?;
        let area_type = string_column(&batch, "area_type")?;
        let bhk = f64_column(&batch, "bhk")?;
        let bathrooms = f64_column(&batch, "bathrooms")?;
        let floor = string_column(&batch, "floor")?;
        let society = string_column(&batch, "society")?;
        let locality = string_column(&batch, "locality")?;
        let observed_at = string_column(&batch, "observed_at")?;
        for row in 0..batch.num_rows() {
            rows.push(ExternalListingObservationRecord {
                entity_id: required_string(entity_id, row, "entity_id")?,
                project_key: optional_string(project_key, row),
                source_name: required_string(source_name, row, "source_name")?,
                source_url: optional_string(source_url, row),
                listing_type: optional_column_string(listing_type, row),
                price: optional_f64(price, row),
                price_min: optional_column_f64(price_min, row),
                price_max: optional_column_f64(price_max, row),
                area_sqft: optional_f64(area_sqft, row),
                area_sqft_min: optional_column_f64(area_sqft_min, row),
                area_sqft_max: optional_column_f64(area_sqft_max, row),
                price_per_sqft_min: optional_column_f64(price_per_sqft_min, row),
                price_per_sqft_max: optional_column_f64(price_per_sqft_max, row),
                price_display: optional_column_string(price_display, row),
                area_display: optional_column_string(area_display, row),
                price_per_sqft_display: optional_column_string(price_per_sqft_display, row),
                configuration: optional_column_string(configuration, row),
                area_type: optional_string(area_type, row),
                bhk: optional_f64(bhk, row),
                bathrooms: optional_f64(bathrooms, row),
                floor: optional_string(floor, row),
                society: optional_string(society, row),
                locality: optional_string(locality, row),
                observed_at: parse_timestamp(observed_at, row)?,
            });
        }
    }
    Ok(rows)
}

fn write_batch(
    schema: Arc<Schema>,
    batch: RecordBatch,
) -> Result<Vec<u8>, ProjectEnrichmentAssetError> {
    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::try_new(3)?))
        .build();
    let mut bytes = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut bytes, schema, Some(props))?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(bytes)
}

fn parquet_batches(bytes: Vec<u8>) -> Result<Vec<RecordBatch>, ProjectEnrichmentAssetError> {
    ParquetRecordBatchReaderBuilder::try_new(Bytes::from(bytes))?
        .build()?
        .map(|batch| batch.map_err(ProjectEnrichmentAssetError::Arrow))
        .collect()
}

async fn read_artifact(
    lake: &LakeStore,
    record: &MaterializationRecord,
    suffix: &str,
) -> Result<Vec<u8>, ProjectEnrichmentAssetError> {
    let artifact = record
        .artifacts
        .iter()
        .find(|artifact| artifact.key.ends_with(suffix))
        .ok_or_else(|| ProjectEnrichmentAssetError::MissingArtifact(record.asset_id.clone()))?;
    let key = crate::lake::LakeKey::new(&artifact.key)?;
    Ok(lake.get_bytes(&key).await?.to_vec())
}

fn strings(values: impl IntoIterator<Item = String>) -> ArrayRef {
    Arc::new(StringArray::from(values.into_iter().collect::<Vec<_>>()))
}

fn optional_f64s(values: impl IntoIterator<Item = Option<f64>>) -> ArrayRef {
    Arc::new(Float64Array::from(values.into_iter().collect::<Vec<_>>()))
}

fn string_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a StringArray, ProjectEnrichmentAssetError> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| ProjectEnrichmentAssetError::InvalidSchema(name.to_string()))
}

fn optional_string_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<Option<&'a StringArray>, ProjectEnrichmentAssetError> {
    batch
        .column_by_name(name)
        .map(|column| {
            column
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| ProjectEnrichmentAssetError::InvalidSchema(name.to_string()))
        })
        .transpose()
}

fn f64_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a Float64Array, ProjectEnrichmentAssetError> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<Float64Array>())
        .ok_or_else(|| ProjectEnrichmentAssetError::InvalidSchema(name.to_string()))
}

fn optional_f64_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<Option<&'a Float64Array>, ProjectEnrichmentAssetError> {
    batch
        .column_by_name(name)
        .map(|column| {
            column
                .as_any()
                .downcast_ref::<Float64Array>()
                .ok_or_else(|| ProjectEnrichmentAssetError::InvalidSchema(name.to_string()))
        })
        .transpose()
}

fn int64_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a Int64Array, ProjectEnrichmentAssetError> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<Int64Array>())
        .ok_or_else(|| ProjectEnrichmentAssetError::InvalidSchema(name.to_string()))
}

fn required_string(
    column: &StringArray,
    row: usize,
    name: &str,
) -> Result<String, ProjectEnrichmentAssetError> {
    if column.is_null(row) {
        return Err(ProjectEnrichmentAssetError::InvalidSchema(name.to_string()));
    }
    Ok(column.value(row).to_string())
}

fn optional_string(column: &StringArray, row: usize) -> Option<String> {
    (!column.is_null(row)).then(|| column.value(row).to_string())
}

fn optional_column_string(column: Option<&StringArray>, row: usize) -> Option<String> {
    column.and_then(|column| optional_string(column, row))
}

fn optional_f64(column: &Float64Array, row: usize) -> Option<f64> {
    (!column.is_null(row)).then(|| column.value(row))
}

fn optional_column_f64(column: Option<&Float64Array>, row: usize) -> Option<f64> {
    column.and_then(|column| optional_f64(column, row))
}

fn parse_timestamp(
    column: &StringArray,
    row: usize,
) -> Result<DateTime<Utc>, ProjectEnrichmentAssetError> {
    Ok(
        DateTime::parse_from_rfc3339(&required_string(column, row, "observed_at")?)?
            .with_timezone(&Utc),
    )
}

fn slug(value: &str) -> String {
    let mut output = String::new();
    let mut pending_dash = false;
    for character in value.trim().to_lowercase().chars() {
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

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn asset_id_value(value: &str) -> AssetId {
    AssetId::new(value).expect("static project enrichment asset id is valid")
}

#[derive(Debug)]
pub enum ProjectEnrichmentAssetError {
    Arrow(arrow::error::ArrowError),
    Chrono(chrono::ParseError),
    DataFusion(datafusion::error::DataFusionError),
    InvalidInput(String),
    InvalidSchema(String),
    Lake(LakeError),
    MissingArtifact(AssetId),
    Parquet(parquet::errors::ParquetError),
    Json(serde_json::Error),
    Rera(ReraAssetError),
    SkillFacts(SkillFactMaterializeError),
    Key(crate::lake::keys::KeyError),
}

impl fmt::Display for ProjectEnrichmentAssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrow(error) => write!(f, "project enrichment Arrow error: {error}"),
            Self::Chrono(error) => write!(f, "project enrichment timestamp error: {error}"),
            Self::DataFusion(error) => write!(f, "project enrichment query error: {error}"),
            Self::InvalidInput(message) => write!(f, "invalid project enrichment input: {message}"),
            Self::InvalidSchema(column) => {
                write!(f, "invalid project enrichment Parquet column: {column}")
            }
            Self::Lake(error) => write!(f, "project enrichment lake error: {error}"),
            Self::MissingArtifact(asset_id) => {
                write!(f, "project enrichment artifact missing for {asset_id}")
            }
            Self::Parquet(error) => write!(f, "project enrichment Parquet error: {error}"),
            Self::Json(error) => write!(f, "project enrichment JSON error: {error}"),
            Self::Rera(error) => write!(f, "project enrichment RERA error: {error}"),
            Self::SkillFacts(error) => write!(f, "project enrichment fact error: {error}"),
            Self::Key(error) => write!(f, "project enrichment key error: {error}"),
        }
    }
}

impl std::error::Error for ProjectEnrichmentAssetError {}

macro_rules! from_error {
    ($source:ty, $variant:ident) => {
        impl From<$source> for ProjectEnrichmentAssetError {
            fn from(value: $source) -> Self {
                Self::$variant(value)
            }
        }
    };
}

from_error!(arrow::error::ArrowError, Arrow);
from_error!(chrono::ParseError, Chrono);
from_error!(datafusion::error::DataFusionError, DataFusion);
from_error!(LakeError, Lake);
from_error!(parquet::errors::ParquetError, Parquet);
from_error!(serde_json::Error, Json);
from_error!(ReraAssetError, Rera);
from_error!(SkillFactMaterializeError, SkillFacts);
from_error!(crate::lake::keys::KeyError, Key);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_source_snapshots_require_an_explicit_empty_watermark() {
        let listings = ExternalListingsWeeklyInput {
            snapshot_date: "2026-07-14".to_string(),
            records: Vec::new(),
            source_watermarks: Vec::new(),
        };

        assert!(matches!(
            validate_external_listing_input(&listings),
            Err(ProjectEnrichmentAssetError::InvalidInput(message))
                if message.contains("snapshot is empty")
        ));

        let empty_with_coverage = ExternalListingsWeeklyInput {
            snapshot_date: "2026-07-14".to_string(),
            records: Vec::new(),
            source_watermarks: vec![SourceWatermark {
                source: "external_listing_empty".to_string(),
                high_watermark: "2026-07-14T10:00:00Z".to_string(),
            }],
        };
        assert!(validate_external_listing_input(&empty_with_coverage).is_ok());
    }
}
