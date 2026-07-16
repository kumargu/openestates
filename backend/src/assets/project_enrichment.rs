use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use arrow::array::{Array, ArrayRef, Float64Array, Int64Array, StringArray, UInt64Array};
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
use crate::parquet_data::{
    optional_string_array, optional_string_list_column_value, string_list_array, string_list_field,
    OptionalListColumn,
};

use super::{
    read_canonical_society_rows, read_rera_project_rows, read_skill_fact_artifact_rows,
    ArtifactRef, AssetId, AssetMaterializationStore, AssetPartition, AssetPathBuilder, AssetStage,
    MaterializationId, MaterializationRecord, ReraAssetError, SkillFactAnnotationRecord,
    SkillFactMaterializeError, SkillFactRecord, SkillFactsInput, SourceWatermark,
};

pub const PRESTIGE_INVENTORY_WEEKLY_ASSET_ID: &str = "prestige_inventory_weekly";
pub const MARKET_PROJECT_FACTS_ASSET_ID: &str = "market_project_facts";
pub const EXTERNAL_LISTINGS_WEEKLY_ASSET_ID: &str = "external_listings_weekly";
pub const EXTERNAL_LISTING_FACTS_ASSET_ID: &str = "external_listing_facts";
pub const METRO_STATIONS_MONTHLY_ASSET_ID: &str = "metro_stations_monthly";
pub const METRO_PROXIMITY_FACTS_ASSET_ID: &str = "metro_proximity_facts";
pub const BUILDER_RERA_AGGREGATES_ASSET_ID: &str = "builder_rera_aggregates";

const OBSERVATION_FORMAT_VERSION: u32 = 1;
const EARTH_RADIUS_KM: f64 = 6_371.008_8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrestigeInventoryObservationRecord {
    pub entity_id: String,
    pub project_key: Option<String>,
    pub source_project_id: String,
    pub source_project_name: String,
    pub source_project_slug: String,
    pub source_url: String,
    pub status: Option<String>,
    pub land_area_acres: Option<f64>,
    pub starting_price_inr: Option<f64>,
    pub price_display: Option<String>,
    #[serde(default)]
    pub bhk_options: Vec<String>,
    pub total_units: Option<u64>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub maps_url: Option<String>,
    pub address: Option<String>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrestigeInventoryWeeklyInput {
    pub snapshot_date: String,
    #[serde(default)]
    pub records: Vec<PrestigeInventoryObservationRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_watermarks: Vec<SourceWatermark>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalListingObservationRecord {
    pub entity_id: String,
    pub project_key: Option<String>,
    pub source_name: String,
    pub source_url: Option<String>,
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
pub struct MetroStationObservationRecord {
    pub station_id: String,
    pub name: String,
    pub network: Option<String>,
    pub operator: Option<String>,
    pub status: String,
    pub latitude: f64,
    pub longitude: f64,
    pub source_url: String,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetroStationsMonthlyInput {
    pub snapshot_date: String,
    #[serde(default)]
    pub records: Vec<MetroStationObservationRecord>,
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

    pub async fn materialize_prestige_inventory(
        &self,
        input: &PrestigeInventoryWeeklyInput,
        parent_materializations: Vec<MaterializationId>,
        dag_run_id: MaterializationId,
        record_partition: AssetPartition,
    ) -> Result<MaterializationRecord, ProjectEnrichmentAssetError> {
        validate_prestige_input(input)?;
        self.materialize_observations(
            PRESTIGE_INVENTORY_WEEKLY_ASSET_ID,
            "prestige",
            &input.snapshot_date,
            input.records.len(),
            write_prestige_inventory_parquet(&input.records)?,
            "projects/part-00000.parquet",
            &input.source_watermarks,
            parent_materializations,
            dag_run_id,
            record_partition,
        )
        .await
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

    pub async fn materialize_metro_stations(
        &self,
        input: &MetroStationsMonthlyInput,
        parent_materializations: Vec<MaterializationId>,
        dag_run_id: MaterializationId,
        record_partition: AssetPartition,
    ) -> Result<MaterializationRecord, ProjectEnrichmentAssetError> {
        validate_metro_input(input)?;
        self.materialize_observations(
            METRO_STATIONS_MONTHLY_ASSET_ID,
            "openstreetmap",
            &input.snapshot_date,
            input.records.len(),
            write_metro_stations_parquet(&input.records)?,
            "stations/part-00000.parquet",
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

pub async fn market_project_facts_input_with_aliases(
    lake: &LakeStore,
    inventory_record: &MaterializationRecord,
    canonical_record: &MaterializationRecord,
    run_id: &MaterializationId,
) -> Result<SkillFactsInput, ProjectEnrichmentAssetError> {
    let rows = read_prestige_inventory_rows(lake, inventory_record).await?;
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
        append_market_facts(&row, &row.entity_id, run_id, &mut facts, &mut annotations)?;
        if let Some(alias) = aliases.get(&row.entity_id) {
            append_market_facts(&row, alias, run_id, &mut facts, &mut annotations)?;
        }
    }
    Ok(SkillFactsInput {
        source: "prestige_builder".to_string(),
        snapshot_date: inventory_record.version.clone(),
        facts,
        fact_annotations: annotations,
        source_watermarks: inventory_record.source_watermarks.clone(),
    })
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

pub async fn metro_proximity_facts_input(
    lake: &LakeStore,
    metro_record: &MaterializationRecord,
    rera_facts_record: &MaterializationRecord,
    run_id: &MaterializationId,
) -> Result<SkillFactsInput, ProjectEnrichmentAssetError> {
    let stations = read_metro_station_rows(lake, metro_record)
        .await?
        .into_iter()
        .filter(|station| station.status.eq_ignore_ascii_case("operational"))
        .collect::<Vec<_>>();
    let rera_rows =
        read_skill_fact_artifact_rows(lake, std::slice::from_ref(rera_facts_record)).await?;
    let mut latest_coordinates = BTreeMap::<String, (DateTime<Utc>, f64, f64)>::new();
    for fact in rera_rows
        .facts
        .into_iter()
        .filter(|fact| fact.fact_key == "rera_lat_lng")
    {
        let Some((latitude, longitude)) = fact_coordinates(&fact)? else {
            continue;
        };
        let should_replace = latest_coordinates
            .get(&fact.entity_id)
            .is_none_or(|(learned_at, _, _)| fact.learned_at > *learned_at);
        if should_replace {
            latest_coordinates.insert(fact.entity_id, (fact.learned_at, latitude, longitude));
        }
    }

    let mut facts = Vec::new();
    let mut annotations = Vec::new();
    for (entity_id, (learned_at, latitude, longitude)) in latest_coordinates {
        let Some((station, distance_km)) = nearest_station(latitude, longitude, &stations) else {
            continue;
        };
        let observed_at = learned_at.max(station.observed_at);
        append_derived_fact(
            &entity_id,
            "nearest_operational_metro_station",
            FactValue::Text(station.name.clone()),
            0.9,
            "Computed",
            Some(station.source_url.clone()),
            "metro_proximity",
            observed_at,
            run_id,
            "Nearest operational metro: {value}",
            &["metro", "near metro", "operational metro"],
            Some(("TextMatch", 1.0, Vec::new())),
            &mut facts,
            &mut annotations,
        )?;
        append_derived_fact(
            &entity_id,
            "metro_distance_km",
            FactValue::Numeric(distance_km),
            0.9,
            "Computed",
            Some(station.source_url.clone()),
            "metro_proximity",
            observed_at,
            run_id,
            "Nearest operational metro is {value} km away",
            &["metro", "near metro", "walkable metro", "metro access"],
            Some(("LowerIsBetter", 2.0, vec![2.0, 5.0])),
            &mut facts,
            &mut annotations,
        )?;
        append_derived_fact(
            &entity_id,
            "metro_status",
            FactValue::Text(format!(
                "{} is {:.1} km away and operational",
                station.name, distance_km
            )),
            0.9,
            "Computed",
            Some(station.source_url.clone()),
            "metro_proximity",
            observed_at,
            run_id,
            "{value}",
            &["metro", "metro access", "operational metro"],
            Some(("TextMatch", 1.5, Vec::new())),
            &mut facts,
            &mut annotations,
        )?;
    }

    Ok(SkillFactsInput {
        source: "metro_proximity".to_string(),
        snapshot_date: metro_record.version.clone(),
        facts,
        fact_annotations: annotations,
        source_watermarks: metro_record.source_watermarks.clone(),
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

fn append_market_facts(
    row: &PrestigeInventoryObservationRecord,
    entity_id: &str,
    run_id: &MaterializationId,
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
) -> Result<(), ProjectEnrichmentAssetError> {
    let common = (
        0.95,
        "BuilderOfficial",
        Some(row.source_url.clone()),
        "prestige_inventory",
        row.observed_at,
        run_id,
    );
    if let Some(status) = &row.status {
        append_derived_fact(
            entity_id,
            "market_project_status",
            FactValue::Text(status.clone()),
            common.0,
            common.1,
            common.2.clone(),
            common.3,
            common.4,
            common.5,
            "Builder inventory status: {value}",
            &[
                "ready to move",
                "under construction",
                "new launch",
                "sold out",
            ],
            Some(("TextMatch", 1.0, Vec::new())),
            facts,
            annotations,
        )?;
    }
    if let Some(price) = row.starting_price_inr {
        append_derived_fact(
            entity_id,
            "market_starting_price_inr",
            FactValue::Numeric(price),
            common.0,
            common.1,
            common.2.clone(),
            common.3,
            common.4,
            common.5,
            "Builder-advertised starting price: INR {value}",
            &["price", "budget", "starting price", "premium"],
            None,
            facts,
            annotations,
        )?;
    }
    if !row.bhk_options.is_empty() {
        let preferences = row
            .bhk_options
            .iter()
            .map(|bhk| format!("{bhk} bhk"))
            .collect::<Vec<_>>();
        let preference_refs = preferences.iter().map(String::as_str).collect::<Vec<_>>();
        append_derived_fact(
            entity_id,
            "market_bhk_options",
            FactValue::Tags(preferences.clone()),
            common.0,
            common.1,
            common.2.clone(),
            common.3,
            common.4,
            common.5,
            "Builder-listed configurations: {value}",
            &preference_refs,
            Some(("TextMatch", 1.0, Vec::new())),
            facts,
            annotations,
        )?;
    }
    for (key, value, template) in [
        (
            "market_total_units",
            row.total_units
                .map(|value| FactValue::Numeric(value as f64)),
            "Builder-listed homes: {value}",
        ),
        (
            "builder_reported_land_area_acres",
            row.land_area_acres.map(FactValue::Numeric),
            "Builder-listed project area: {value} acres",
        ),
        (
            "project_latitude",
            row.latitude.map(FactValue::Numeric),
            "Project latitude: {value}",
        ),
        (
            "project_longitude",
            row.longitude.map(FactValue::Numeric),
            "Project longitude: {value}",
        ),
    ] {
        if let Some(value) = value {
            append_derived_fact(
                entity_id,
                key,
                value,
                common.0,
                common.1,
                common.2.clone(),
                common.3,
                common.4,
                common.5,
                template,
                &[],
                None,
                facts,
                annotations,
            )?;
        }
    }
    for (key, value, template, preferences) in [
        (
            "official_project_url",
            Some(row.source_url.clone()),
            "Official project page: {value}",
            vec!["official source", "builder website"],
        ),
        (
            "project_maps_url",
            row.maps_url.clone(),
            "Project map: {value}",
            vec!["map", "location"],
        ),
    ] {
        if let Some(value) = value {
            append_derived_fact(
                entity_id,
                key,
                FactValue::Text(value),
                common.0,
                common.1,
                common.2.clone(),
                common.3,
                common.4,
                common.5,
                template,
                &preferences,
                Some(("TextMatch", 0.2, Vec::new())),
                facts,
                annotations,
            )?;
        }
    }
    Ok(())
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
        "observed_at": row.observed_at.to_rfc3339(),
    });
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

fn fact_coordinates(
    fact: &SkillFactRecord,
) -> Result<Option<(f64, f64)>, ProjectEnrichmentAssetError> {
    let value: FactValue = serde_json::from_str(&fact.value_json)?;
    let FactValue::Text(value) = value else {
        return Ok(None);
    };
    let Some((latitude, longitude)) = value.split_once(',') else {
        return Ok(None);
    };
    let Ok(latitude) = latitude.trim().parse::<f64>() else {
        return Ok(None);
    };
    let Ok(longitude) = longitude.trim().parse::<f64>() else {
        return Ok(None);
    };
    Ok(valid_coordinate(latitude, longitude).then_some((latitude, longitude)))
}

fn nearest_station(
    latitude: f64,
    longitude: f64,
    stations: &[MetroStationObservationRecord],
) -> Option<(&MetroStationObservationRecord, f64)> {
    stations
        .iter()
        .map(|station| {
            (
                station,
                haversine_km(latitude, longitude, station.latitude, station.longitude),
            )
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))
}

fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let lat1 = lat1.to_radians();
    let lat2 = lat2.to_radians();
    let delta_lat = lat2 - lat1;
    let delta_lon = (lon2 - lon1).to_radians();
    let a =
        (delta_lat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2);
    EARTH_RADIUS_KM * 2.0 * a.sqrt().asin()
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

fn validate_prestige_input(
    input: &PrestigeInventoryWeeklyInput,
) -> Result<(), ProjectEnrichmentAssetError> {
    if input.snapshot_date.trim().is_empty() {
        return Err(ProjectEnrichmentAssetError::InvalidInput(
            "Prestige inventory snapshot date is empty".to_string(),
        ));
    }
    if input.records.is_empty() {
        return Err(ProjectEnrichmentAssetError::InvalidInput(
            "Prestige inventory snapshot is empty".to_string(),
        ));
    }
    for record in &input.records {
        if record.entity_id.trim().is_empty()
            || record.source_project_id.trim().is_empty()
            || record.source_project_name.trim().is_empty()
            || record.source_url.trim().is_empty()
        {
            return Err(ProjectEnrichmentAssetError::InvalidInput(
                "Prestige inventory record is missing identity or source".to_string(),
            ));
        }
        if let (Some(latitude), Some(longitude)) = (record.latitude, record.longitude) {
            if !valid_coordinate(latitude, longitude) {
                return Err(ProjectEnrichmentAssetError::InvalidInput(format!(
                    "invalid project coordinate for {}",
                    record.source_project_name
                )));
            }
        }
    }
    Ok(())
}

fn validate_external_listing_input(
    input: &ExternalListingsWeeklyInput,
) -> Result<(), ProjectEnrichmentAssetError> {
    if input.snapshot_date.trim().is_empty() {
        return Err(ProjectEnrichmentAssetError::InvalidInput(
            "external listing snapshot date is empty".to_string(),
        ));
    }
    if input.records.is_empty() {
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

fn validate_metro_input(
    input: &MetroStationsMonthlyInput,
) -> Result<(), ProjectEnrichmentAssetError> {
    if input.snapshot_date.trim().is_empty() {
        return Err(ProjectEnrichmentAssetError::InvalidInput(
            "metro station snapshot date is empty".to_string(),
        ));
    }
    if input.records.is_empty() {
        return Err(ProjectEnrichmentAssetError::InvalidInput(
            "metro station snapshot is empty".to_string(),
        ));
    }
    for record in &input.records {
        if record.station_id.trim().is_empty()
            || record.name.trim().is_empty()
            || record.source_url.trim().is_empty()
            || !valid_coordinate(record.latitude, record.longitude)
        {
            return Err(ProjectEnrichmentAssetError::InvalidInput(
                "metro station record is missing identity, source, or coordinates".to_string(),
            ));
        }
    }
    Ok(())
}

fn valid_coordinate(latitude: f64, longitude: f64) -> bool {
    latitude.is_finite()
        && longitude.is_finite()
        && (-90.0..=90.0).contains(&latitude)
        && (-180.0..=180.0).contains(&longitude)
}

fn write_prestige_inventory_parquet(
    records: &[PrestigeInventoryObservationRecord],
) -> Result<Vec<u8>, ProjectEnrichmentAssetError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("project_key", DataType::Utf8, true),
        Field::new("source_project_id", DataType::Utf8, false),
        Field::new("source_project_name", DataType::Utf8, false),
        Field::new("source_project_slug", DataType::Utf8, false),
        Field::new("source_url", DataType::Utf8, false),
        Field::new("status", DataType::Utf8, true),
        Field::new("land_area_acres", DataType::Float64, true),
        Field::new("starting_price_inr", DataType::Float64, true),
        Field::new("price_display", DataType::Utf8, true),
        string_list_field("bhk_options", false),
        Field::new("total_units", DataType::UInt64, true),
        Field::new("latitude", DataType::Float64, true),
        Field::new("longitude", DataType::Float64, true),
        Field::new("maps_url", DataType::Utf8, true),
        Field::new("address", DataType::Utf8, true),
        Field::new("observed_at", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            strings(records.iter().map(|record| record.entity_id.clone())),
            optional_string_array(records.iter().map(|record| record.project_key.clone())),
            strings(
                records
                    .iter()
                    .map(|record| record.source_project_id.clone()),
            ),
            strings(
                records
                    .iter()
                    .map(|record| record.source_project_name.clone()),
            ),
            strings(
                records
                    .iter()
                    .map(|record| record.source_project_slug.clone()),
            ),
            strings(records.iter().map(|record| record.source_url.clone())),
            optional_string_array(records.iter().map(|record| record.status.clone())),
            optional_f64s(records.iter().map(|record| record.land_area_acres)),
            optional_f64s(records.iter().map(|record| record.starting_price_inr)),
            optional_string_array(records.iter().map(|record| record.price_display.clone())),
            string_list_array(
                records
                    .iter()
                    .map(|record| Some(record.bhk_options.clone())),
            ),
            optional_u64s(records.iter().map(|record| record.total_units)),
            optional_f64s(records.iter().map(|record| record.latitude)),
            optional_f64s(records.iter().map(|record| record.longitude)),
            optional_string_array(records.iter().map(|record| record.maps_url.clone())),
            optional_string_array(records.iter().map(|record| record.address.clone())),
            strings(records.iter().map(|record| record.observed_at.to_rfc3339())),
        ],
    )?;
    write_batch(schema, batch)
}

fn write_external_listing_parquet(
    records: &[ExternalListingObservationRecord],
) -> Result<Vec<u8>, ProjectEnrichmentAssetError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("project_key", DataType::Utf8, true),
        Field::new("source_name", DataType::Utf8, false),
        Field::new("source_url", DataType::Utf8, true),
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

fn write_metro_stations_parquet(
    records: &[MetroStationObservationRecord],
) -> Result<Vec<u8>, ProjectEnrichmentAssetError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("station_id", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("network", DataType::Utf8, true),
        Field::new("operator", DataType::Utf8, true),
        Field::new("status", DataType::Utf8, false),
        Field::new("latitude", DataType::Float64, false),
        Field::new("longitude", DataType::Float64, false),
        Field::new("source_url", DataType::Utf8, false),
        Field::new("observed_at", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            strings(records.iter().map(|record| record.station_id.clone())),
            strings(records.iter().map(|record| record.name.clone())),
            optional_string_array(records.iter().map(|record| record.network.clone())),
            optional_string_array(records.iter().map(|record| record.operator.clone())),
            strings(records.iter().map(|record| record.status.clone())),
            f64s(records.iter().map(|record| record.latitude)),
            f64s(records.iter().map(|record| record.longitude)),
            strings(records.iter().map(|record| record.source_url.clone())),
            strings(records.iter().map(|record| record.observed_at.to_rfc3339())),
        ],
    )?;
    write_batch(schema, batch)
}

async fn read_prestige_inventory_rows(
    lake: &LakeStore,
    record: &MaterializationRecord,
) -> Result<Vec<PrestigeInventoryObservationRecord>, ProjectEnrichmentAssetError> {
    let bytes = read_artifact(lake, record, "projects/part-00000.parquet").await?;
    let mut rows = Vec::new();
    for batch in parquet_batches(bytes)? {
        let entity_id = string_column(&batch, "entity_id")?;
        let project_key = string_column(&batch, "project_key")?;
        let source_project_id = string_column(&batch, "source_project_id")?;
        let source_project_name = string_column(&batch, "source_project_name")?;
        let source_project_slug = string_column(&batch, "source_project_slug")?;
        let source_url = string_column(&batch, "source_url")?;
        let status = string_column(&batch, "status")?;
        let land_area_acres = f64_column(&batch, "land_area_acres")?;
        let starting_price_inr = f64_column(&batch, "starting_price_inr")?;
        let price_display = string_column(&batch, "price_display")?;
        let total_units = u64_column(&batch, "total_units")?;
        let latitude = f64_column(&batch, "latitude")?;
        let longitude = f64_column(&batch, "longitude")?;
        let maps_url = string_column(&batch, "maps_url")?;
        let address = string_column(&batch, "address")?;
        let observed_at = string_column(&batch, "observed_at")?;
        for row in 0..batch.num_rows() {
            let bhk_options = match optional_string_list_column_value(&batch, "bhk_options", row)
                .map_err(ProjectEnrichmentAssetError::InvalidSchema)?
            {
                OptionalListColumn::Values(values) => values,
                OptionalListColumn::Missing | OptionalListColumn::Null => Vec::new(),
            };
            rows.push(PrestigeInventoryObservationRecord {
                entity_id: required_string(entity_id, row, "entity_id")?,
                project_key: optional_string(project_key, row),
                source_project_id: required_string(source_project_id, row, "source_project_id")?,
                source_project_name: required_string(
                    source_project_name,
                    row,
                    "source_project_name",
                )?,
                source_project_slug: required_string(
                    source_project_slug,
                    row,
                    "source_project_slug",
                )?,
                source_url: required_string(source_url, row, "source_url")?,
                status: optional_string(status, row),
                land_area_acres: optional_f64(land_area_acres, row),
                starting_price_inr: optional_f64(starting_price_inr, row),
                price_display: optional_string(price_display, row),
                bhk_options,
                total_units: optional_u64(total_units, row),
                latitude: optional_f64(latitude, row),
                longitude: optional_f64(longitude, row),
                maps_url: optional_string(maps_url, row),
                address: optional_string(address, row),
                observed_at: parse_timestamp(observed_at, row)?,
            });
        }
    }
    Ok(rows)
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

async fn read_metro_station_rows(
    lake: &LakeStore,
    record: &MaterializationRecord,
) -> Result<Vec<MetroStationObservationRecord>, ProjectEnrichmentAssetError> {
    let bytes = read_artifact(lake, record, "stations/part-00000.parquet").await?;
    let mut rows = Vec::new();
    for batch in parquet_batches(bytes)? {
        let station_id = string_column(&batch, "station_id")?;
        let name = string_column(&batch, "name")?;
        let network = string_column(&batch, "network")?;
        let operator = string_column(&batch, "operator")?;
        let status = string_column(&batch, "status")?;
        let latitude = f64_column(&batch, "latitude")?;
        let longitude = f64_column(&batch, "longitude")?;
        let source_url = string_column(&batch, "source_url")?;
        let observed_at = string_column(&batch, "observed_at")?;
        for row in 0..batch.num_rows() {
            rows.push(MetroStationObservationRecord {
                station_id: required_string(station_id, row, "station_id")?,
                name: required_string(name, row, "name")?,
                network: optional_string(network, row),
                operator: optional_string(operator, row),
                status: required_string(status, row, "status")?,
                latitude: required_f64(latitude, row, "latitude")?,
                longitude: required_f64(longitude, row, "longitude")?,
                source_url: required_string(source_url, row, "source_url")?,
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

fn f64s(values: impl IntoIterator<Item = f64>) -> ArrayRef {
    Arc::new(Float64Array::from(values.into_iter().collect::<Vec<_>>()))
}

fn optional_u64s(values: impl IntoIterator<Item = Option<u64>>) -> ArrayRef {
    Arc::new(UInt64Array::from(values.into_iter().collect::<Vec<_>>()))
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

fn u64_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a UInt64Array, ProjectEnrichmentAssetError> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<UInt64Array>())
        .ok_or_else(|| ProjectEnrichmentAssetError::InvalidSchema(name.to_string()))
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

fn required_f64(
    column: &Float64Array,
    row: usize,
    name: &str,
) -> Result<f64, ProjectEnrichmentAssetError> {
    if column.is_null(row) {
        return Err(ProjectEnrichmentAssetError::InvalidSchema(name.to_string()));
    }
    Ok(column.value(row))
}

fn optional_u64(column: &UInt64Array, row: usize) -> Option<u64> {
    (!column.is_null(row)).then(|| column.value(row))
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
    fn empty_source_snapshots_are_rejected() {
        let prestige = PrestigeInventoryWeeklyInput {
            snapshot_date: "2026-07-14".to_string(),
            records: Vec::new(),
            source_watermarks: Vec::new(),
        };
        let metro = MetroStationsMonthlyInput {
            snapshot_date: "2026-07-14".to_string(),
            records: Vec::new(),
            source_watermarks: Vec::new(),
        };

        assert!(matches!(
            validate_prestige_input(&prestige),
            Err(ProjectEnrichmentAssetError::InvalidInput(message))
                if message.contains("snapshot is empty")
        ));
        assert!(matches!(
            validate_metro_input(&metro),
            Err(ProjectEnrichmentAssetError::InvalidInput(message))
                if message.contains("snapshot is empty")
        ));
    }
}
