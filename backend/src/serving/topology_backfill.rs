use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;

use chrono::{DateTime, Utc};
use geojson::{GeoJson, Value as GeoJsonValue};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::assets::{SkillFactRecord, SourceEntitySeed};
use crate::dag_config::{load_fact_registry_index, scoring_direction_from_hint};
use crate::knowledge::FactValue;

use super::{
    ServingEdgeRecord, ServingEntityRecord, ServingFactRecord, ServingReraEvidenceRecord,
    ServingSearchMetadataRecord, SourceObservation,
};

const LEGACY_SOCIETY_BOUNDARY_FACT_KEY: &str = "society.boundary_geojson";
const GEOMETRY_FACT_KEY: &str = "geo.geometry_geojson";
const AREA_NAME_FACT_KEY: &str = "place.name";
const EXPLICIT_AREA_NAME_FACT_KEY: &str = "geo.explicit_area_name";

#[derive(Debug, Clone)]
pub struct AreaTopologyBackfillRecords {
    pub entities: Vec<ServingEntityRecord>,
    pub facts: Vec<ServingFactRecord>,
    pub search_metadata: Vec<ServingSearchMetadataRecord>,
    pub edges: Vec<ServingEdgeRecord>,
    pub rera_evidence: Vec<ServingReraEvidenceRecord>,
    pub excluded_rera_evidence_society_ids: Vec<String>,
    pub prevalidated_entity_ids: BTreeSet<String>,
    pub report: AreaTopologyBackfillReport,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaTopologyBackfillReport {
    pub base_entity_count: usize,
    pub base_fact_count: usize,
    pub added_area_entity_ids: Vec<String>,
    pub added_area_fact_count: usize,
    pub rejected_area_entity_ids: Vec<String>,
    pub migrated_society_footprint_entity_ids: Vec<String>,
    pub imported_explicit_area_reference_entity_ids: Vec<String>,
    pub unmatched_explicit_area_references: BTreeMap<String, String>,
    pub ambiguous_area_names: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AreaTopologyBackfillInput {
    pub entities: Vec<ServingEntityRecord>,
    pub facts: Vec<ServingFactRecord>,
    pub search_metadata: Vec<ServingSearchMetadataRecord>,
    pub edges: Vec<ServingEdgeRecord>,
    pub rera_evidence: Vec<ServingReraEvidenceRecord>,
    pub excluded_rera_evidence_society_ids: Vec<String>,
    pub area_facts: Vec<SkillFactRecord>,
    pub society_geometry_facts: Vec<SkillFactRecord>,
    pub source_entity_seeds: Vec<SourceEntitySeed>,
    pub source_entity_seed_lineage: String,
    pub imported_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct AreaTopologyBackfillError(String);

impl fmt::Display for AreaTopologyBackfillError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for AreaTopologyBackfillError {}

/// Augment an immutable serving snapshot with qualified area inputs.
///
/// This deliberately performs no locality inference. The only non-geometric
/// binding input is the explicit `area` field from a source-entity seed.
pub fn backfill_area_topology(
    input: AreaTopologyBackfillInput,
) -> Result<AreaTopologyBackfillRecords, AreaTopologyBackfillError> {
    let base_entity_count = input.entities.len();
    let base_fact_count = input.facts.len();
    let prevalidated_entity_ids = input
        .entities
        .iter()
        .map(|entity| entity.entity_id.clone())
        .collect::<BTreeSet<_>>();
    let base_society_ids = input
        .entities
        .iter()
        .filter(|entity| entity.entity_type.eq_ignore_ascii_case("society"))
        .map(|entity| entity.entity_id.clone())
        .collect::<HashSet<_>>();
    let existing_fact_keys = input
        .facts
        .iter()
        .map(|fact| (fact.entity_id.clone(), fact.fact_key.to_ascii_lowercase()))
        .collect::<HashSet<_>>();

    let mut report = AreaTopologyBackfillReport {
        base_entity_count,
        base_fact_count,
        ..AreaTopologyBackfillReport::default()
    };
    let mut qualified_area_facts = qualified_area_facts(input.area_facts, &mut report)?;
    let area_names = unique_area_names(&qualified_area_facts, &mut report)?;
    if area_names.is_empty() {
        return Err(AreaTopologyBackfillError(
            "area-topology backfill found no uniquely named, observation-backed area polygons"
                .to_string(),
        ));
    }
    let unique_area_ids = area_names
        .values()
        .filter(|area_ids| area_ids.len() == 1)
        .flatten()
        .cloned()
        .collect::<HashSet<_>>();
    qualified_area_facts.retain(|fact| unique_area_ids.contains(&fact.entity_id));

    let mut added_entities = area_entities(&qualified_area_facts, &area_names)?;
    report.added_area_entity_ids = added_entities
        .iter()
        .map(|entity| entity.entity_id.clone())
        .collect();
    report.added_area_fact_count = qualified_area_facts.len();

    let mut migrated_footprints = Vec::new();
    for mut fact in input.society_geometry_facts {
        if fact.fact_key != LEGACY_SOCIETY_BOUNDARY_FACT_KEY
            || !base_society_ids.contains(&fact.entity_id)
            || existing_fact_keys.contains(&(fact.entity_id.clone(), GEOMETRY_FACT_KEY.to_string()))
        {
            continue;
        }
        if !skill_fact_is_polygon(&fact)? {
            continue;
        }
        qualify_legacy_source_fact(&mut fact)?;
        fact.fact_key = GEOMETRY_FACT_KEY.to_string();
        report
            .migrated_society_footprint_entity_ids
            .push(fact.entity_id.clone());
        migrated_footprints.push(fact);
    }
    report.migrated_society_footprint_entity_ids.sort();
    report.migrated_society_footprint_entity_ids.dedup();
    let footprint_society_ids = report
        .migrated_society_footprint_entity_ids
        .iter()
        .cloned()
        .collect::<HashSet<_>>();

    let mut explicit_area_facts = Vec::new();
    let seed_areas = unique_seed_area_assertions(input.source_entity_seeds)?;
    for (entity_id, area_name) in seed_areas {
        if !base_society_ids.contains(&entity_id)
            || footprint_society_ids.contains(&entity_id)
            || existing_fact_keys
                .contains(&(entity_id.clone(), EXPLICIT_AREA_NAME_FACT_KEY.to_string()))
        {
            continue;
        }
        let normalized = normalize_exact_area_name(&area_name);
        match area_names.get(&normalized) {
            Some(area_ids) if area_ids.len() == 1 => {
                explicit_area_facts.push(explicit_area_fact(
                    &entity_id,
                    &area_name,
                    &input.source_entity_seed_lineage,
                    input.imported_at,
                )?);
                report
                    .imported_explicit_area_reference_entity_ids
                    .push(entity_id);
            }
            Some(_) => {
                report.ambiguous_area_names.push(area_name);
            }
            None => {
                report
                    .unmatched_explicit_area_references
                    .insert(entity_id, area_name);
            }
        }
    }
    report.imported_explicit_area_reference_entity_ids.sort();
    report.imported_explicit_area_reference_entity_ids.dedup();
    report.ambiguous_area_names.sort();
    report.ambiguous_area_names.dedup();

    qualified_area_facts.extend(migrated_footprints);
    qualified_area_facts.extend(explicit_area_facts);
    let (mut added_facts, mut added_metadata) = serving_records(qualified_area_facts)?;

    let mut entities = input.entities;
    let existing_entity_ids = entities
        .iter()
        .map(|entity| entity.entity_id.clone())
        .collect::<HashSet<_>>();
    added_entities.retain(|entity| !existing_entity_ids.contains(&entity.entity_id));
    entities.extend(added_entities);
    entities.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));

    let mut facts = input.facts;
    facts.append(&mut added_facts);
    facts.sort_by(|left, right| {
        left.entity_id
            .cmp(&right.entity_id)
            .then_with(|| left.fact_key.cmp(&right.fact_key))
            .then_with(|| {
                left.stable_selection_key()
                    .cmp(&right.stable_selection_key())
            })
    });
    let mut search_metadata = input.search_metadata;
    search_metadata.append(&mut added_metadata);
    search_metadata.sort_by(|left, right| {
        left.entity_id
            .cmp(&right.entity_id)
            .then_with(|| left.fact_key.cmp(&right.fact_key))
    });

    if report.migrated_society_footprint_entity_ids.is_empty()
        && report
            .imported_explicit_area_reference_entity_ids
            .is_empty()
    {
        return Err(AreaTopologyBackfillError(
            "area-topology backfill produced no qualified society-to-area inputs".to_string(),
        ));
    }

    Ok(AreaTopologyBackfillRecords {
        entities,
        facts,
        search_metadata,
        edges: input.edges,
        rera_evidence: input.rera_evidence,
        excluded_rera_evidence_society_ids: input.excluded_rera_evidence_society_ids,
        prevalidated_entity_ids,
        report,
    })
}

fn qualified_area_facts(
    area_facts: Vec<SkillFactRecord>,
    report: &mut AreaTopologyBackfillReport,
) -> Result<Vec<SkillFactRecord>, AreaTopologyBackfillError> {
    let mut grouped = BTreeMap::<String, Vec<SkillFactRecord>>::new();
    for fact in area_facts
        .into_iter()
        .filter(|fact| fact.entity_id.starts_with("area:"))
    {
        grouped
            .entry(fact.entity_id.clone())
            .or_default()
            .push(fact);
    }
    let mut accepted = Vec::new();
    for (entity_id, mut facts) in grouped {
        let has_name = facts.iter().any(|fact| {
            fact.fact_key == AREA_NAME_FACT_KEY
                && matches!(fact_value(fact), Ok(FactValue::Text(name)) if !name.trim().is_empty())
                && skill_fact_has_valid_observation(fact)
        });
        let has_polygon = facts.iter().any(|fact| {
            fact.fact_key == GEOMETRY_FACT_KEY
                && skill_fact_has_valid_observation(fact)
                && skill_fact_is_polygon(fact).unwrap_or(false)
        });
        if !has_name || !has_polygon {
            report.rejected_area_entity_ids.push(entity_id);
            continue;
        }
        facts.retain(skill_fact_has_valid_observation);
        accepted.extend(facts);
    }
    report.rejected_area_entity_ids.sort();
    Ok(accepted)
}

fn unique_area_names(
    area_facts: &[SkillFactRecord],
    report: &mut AreaTopologyBackfillReport,
) -> Result<HashMap<String, Vec<String>>, AreaTopologyBackfillError> {
    let mut names = HashMap::<String, Vec<String>>::new();
    for fact in area_facts
        .iter()
        .filter(|fact| fact.fact_key == AREA_NAME_FACT_KEY)
    {
        let FactValue::Text(name) = fact_value(fact)? else {
            continue;
        };
        names
            .entry(normalize_exact_area_name(&name))
            .or_default()
            .push(fact.entity_id.clone());
    }
    for area_ids in names.values_mut() {
        area_ids.sort();
        area_ids.dedup();
    }
    report.ambiguous_area_names.extend(
        names
            .iter()
            .filter(|(_, area_ids)| area_ids.len() > 1)
            .map(|(name, _)| name.clone()),
    );
    Ok(names)
}

fn area_entities(
    facts: &[SkillFactRecord],
    area_names: &HashMap<String, Vec<String>>,
) -> Result<Vec<ServingEntityRecord>, AreaTopologyBackfillError> {
    let mut names_by_id = HashMap::<String, String>::new();
    let mut sources_by_id = HashMap::<String, String>::new();
    for fact in facts
        .iter()
        .filter(|fact| fact.fact_key == AREA_NAME_FACT_KEY)
    {
        let FactValue::Text(name) = fact_value(fact)? else {
            continue;
        };
        if area_names
            .get(&normalize_exact_area_name(&name))
            .is_some_and(|ids| ids.len() == 1)
        {
            names_by_id.insert(fact.entity_id.clone(), name.trim().to_string());
            sources_by_id.insert(
                fact.entity_id.clone(),
                fact.source_type.to_ascii_lowercase(),
            );
        }
    }
    let mut entities = names_by_id
        .into_iter()
        .map(|(entity_id, name)| ServingEntityRecord {
            searchable_text: format!("{entity_id} area {name}"),
            root_source: sources_by_id.remove(&entity_id),
            entity_id,
            entity_type: "area".to_string(),
            name,
        })
        .collect::<Vec<_>>();
    entities.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));
    Ok(entities)
}

fn qualify_legacy_source_fact(fact: &mut SkillFactRecord) -> Result<(), AreaTopologyBackfillError> {
    if skill_fact_has_valid_observation(fact) {
        return Ok(());
    }
    if fact.source_type.trim().is_empty()
        || fact
            .source_url
            .as_deref()
            .is_none_or(|url| url.trim().is_empty())
        || fact
            .skill_id
            .as_deref()
            .is_none_or(|id| id.trim().is_empty())
        || fact.run_id.trim().is_empty()
        || fact.input_hash.trim().is_empty()
    {
        return Err(AreaTopologyBackfillError(format!(
            "legacy fact {}/{} lacks structured provenance",
            fact.entity_id, fact.fact_key
        )));
    }
    let skill_id = fact.skill_id.as_deref().expect("validated skill id");
    fact.observation_provider = Some(fact.source_type.clone());
    fact.provider_observation_id = Some(format!("{skill_id}:{}:{}", fact.run_id, fact.input_hash));
    fact.asset_lineage = vec![format!("asset:{skill_id}/run:{}", fact.run_id)];
    if !skill_fact_has_valid_observation(fact) {
        return Err(AreaTopologyBackfillError(format!(
            "legacy fact {}/{} could not be qualified",
            fact.entity_id, fact.fact_key
        )));
    }
    Ok(())
}

fn explicit_area_fact(
    entity_id: &str,
    area_name: &str,
    lineage: &str,
    imported_at: DateTime<Utc>,
) -> Result<SkillFactRecord, AreaTopologyBackfillError> {
    if lineage.trim().is_empty() {
        return Err(AreaTopologyBackfillError(
            "source-entity seed lineage is required".to_string(),
        ));
    }
    let encoded = serde_json::to_vec(&(entity_id, area_name)).map_err(backfill_error)?;
    let digest = hex_digest(&Sha256::digest(&encoded));
    Ok(SkillFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: EXPLICIT_AREA_NAME_FACT_KEY.to_string(),
        value_type: "text".to_string(),
        value_json: serde_json::to_string(&FactValue::Text(area_name.trim().to_string()))
            .map_err(backfill_error)?,
        confidence: 0.95,
        source_type: "SourceEntitySeed".to_string(),
        source_url: None,
        model: None,
        skill_id: Some("area_topology_backfill".to_string()),
        triggered_by: Some("explicit_source_entity_area".to_string()),
        learned_at: imported_at,
        run_id: format!("source-entity-seed-{digest}"),
        input_hash: digest.clone(),
        observation_provider: Some("SourceEntitySeed".to_string()),
        provider_observation_id: Some(format!("source_entity_seed:sha256:{digest}")),
        asset_lineage: vec![lineage.to_string()],
    })
}

fn unique_seed_area_assertions(
    seeds: Vec<SourceEntitySeed>,
) -> Result<BTreeMap<String, String>, AreaTopologyBackfillError> {
    let mut assertions = BTreeMap::<String, String>::new();
    for seed in seeds {
        let Some(area_name) = seed
            .area
            .as_deref()
            .map(str::trim)
            .filter(|area| !area.is_empty())
        else {
            continue;
        };
        if let Some(existing) = assertions.get(&seed.entity_id) {
            if normalize_exact_area_name(existing) != normalize_exact_area_name(area_name) {
                return Err(AreaTopologyBackfillError(format!(
                    "source-entity seeds disagree on the explicit area for {}",
                    seed.entity_id
                )));
            }
            continue;
        }
        assertions.insert(seed.entity_id, area_name.to_string());
    }
    Ok(assertions)
}

fn serving_records(
    facts: Vec<SkillFactRecord>,
) -> Result<(Vec<ServingFactRecord>, Vec<ServingSearchMetadataRecord>), AreaTopologyBackfillError> {
    let registry = load_fact_registry_index().map_err(backfill_error)?;
    let mut metadata_keys = HashSet::new();
    let mut metadata = Vec::new();
    let mut serving_facts = Vec::new();
    for fact in facts {
        let value = fact_value(&fact)?;
        let observation = source_observation(&fact)?;
        let value_text = match &value {
            FactValue::Text(value) => Some(value.clone()),
            FactValue::Numeric(value) => Some(value.to_string()),
            FactValue::Bool(value) => Some(value.to_string()),
            _ => None,
        };
        if metadata_keys.insert((fact.entity_id.clone(), fact.fact_key.clone())) {
            if let Some(entry) = registry.lookup(&fact.fact_key) {
                let hint = entry.scoring_hint.as_ref();
                metadata.push(ServingSearchMetadataRecord {
                    entity_id: fact.entity_id.clone(),
                    fact_key: fact.fact_key.clone(),
                    display_template: entry.display_template.clone(),
                    answers_preferences: entry.answers_preferences.clone(),
                    scoring_direction: hint.map(scoring_direction_from_hint),
                    scoring_weight: hint.and_then(|hint| hint.weight),
                    scoring_thresholds: hint
                        .map(|hint| hint.thresholds.clone())
                        .unwrap_or_default(),
                });
            }
        }
        serving_facts.push(ServingFactRecord {
            entity_id: fact.entity_id,
            fact_key: fact.fact_key,
            value_type: fact.value_type,
            value_text,
            value,
            confidence: fact.confidence,
            source_type: fact.source_type,
            source_url: fact.source_url,
            model: fact.model,
            skill_id: fact.skill_id,
            learned_at: fact.learned_at,
            observation: Some(observation),
        });
    }
    Ok((serving_facts, metadata))
}

fn source_observation(
    fact: &SkillFactRecord,
) -> Result<SourceObservation, AreaTopologyBackfillError> {
    SourceObservation::new(
        fact.observation_provider.clone().unwrap_or_default(),
        fact.provider_observation_id.clone().unwrap_or_default(),
        &fact.entity_id,
        fact.learned_at,
        fact.source_url.clone(),
        fact.asset_lineage.clone(),
    )
    .map_err(backfill_error)
}

fn skill_fact_has_valid_observation(fact: &SkillFactRecord) -> bool {
    source_observation(fact).is_ok()
}

fn skill_fact_is_polygon(fact: &SkillFactRecord) -> Result<bool, AreaTopologyBackfillError> {
    let FactValue::Text(value) = fact_value(fact)? else {
        return Ok(false);
    };
    let parsed = value.parse::<GeoJson>().map_err(backfill_error)?;
    let geometry = match parsed {
        GeoJson::Geometry(geometry) => Some(geometry.value),
        GeoJson::Feature(feature) => feature.geometry.map(|geometry| geometry.value),
        GeoJson::FeatureCollection(_) => None,
    };
    Ok(matches!(
        geometry,
        Some(GeoJsonValue::Polygon(_) | GeoJsonValue::MultiPolygon(_))
    ))
}

fn fact_value(fact: &SkillFactRecord) -> Result<FactValue, AreaTopologyBackfillError> {
    serde_json::from_str(&fact.value_json).map_err(backfill_error)
}

fn normalize_exact_area_name(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to a string cannot fail");
    }
    output
}

fn backfill_error(error: impl fmt::Display) -> AreaTopologyBackfillError {
    AreaTopologyBackfillError(error.to_string())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::dag_config::SpatialTopologyPolicy;
    use crate::serving::{
        derive_spatial_topology, validate_serving_edge_evidence, ServingFactIndex,
    };

    fn entity(id: &str, kind: &str) -> ServingEntityRecord {
        ServingEntityRecord {
            entity_id: id.to_string(),
            entity_type: kind.to_string(),
            name: id.to_string(),
            root_source: Some("fixture".to_string()),
            searchable_text: id.to_string(),
        }
    }

    fn skill_fact(id: &str, key: &str, value: FactValue) -> SkillFactRecord {
        let learned_at = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let value_json = serde_json::to_string(&value).unwrap();
        SkillFactRecord {
            entity_id: id.to_string(),
            fact_key: key.to_string(),
            value_type: "text".to_string(),
            value_json: value_json.clone(),
            confidence: 0.9,
            source_type: "OpenStreetMap".to_string(),
            source_url: Some("https://www.openstreetmap.org/relation/1".to_string()),
            model: None,
            skill_id: Some("topology_fixture".to_string()),
            triggered_by: None,
            learned_at,
            run_id: "fixture-run".to_string(),
            input_hash: hex_digest(&Sha256::digest(value_json.as_bytes())),
            observation_provider: Some("OpenStreetMap".to_string()),
            provider_observation_id: Some(format!("fixture:{id}:{key}")),
            asset_lineage: vec!["asset:topology-fixture/run:fixture-run".to_string()],
        }
    }

    fn seed(id: &str, area: &str) -> SourceEntitySeed {
        SourceEntitySeed {
            entity_id: id.to_string(),
            alias_entity_id: None,
            name: id.to_string(),
            area: Some(area.to_string()),
            city: Some("Bengaluru".to_string()),
            project_key: None,
            latitude: None,
            longitude: None,
        }
    }

    #[test]
    fn backfill_uses_polygon_or_explicit_seed_area_and_ignores_unmatched_names() {
        let area_facts = vec![
            skill_fact(
                "area:osm:whitefield",
                AREA_NAME_FACT_KEY,
                FactValue::Text("Whitefield".to_string()),
            ),
            skill_fact(
                "area:osm:whitefield",
                GEOMETRY_FACT_KEY,
                FactValue::Text(
                    r#"{"type":"Polygon","coordinates":[[[0,0],[10,0],[10,10],[0,10],[0,0]]]}"#
                        .to_string(),
                ),
            ),
            skill_fact(
                "area:osm:whitefield",
                "area.admin_level",
                FactValue::Text("10".to_string()),
            ),
        ];
        let mut legacy_footprint = skill_fact(
            "society:polygon",
            LEGACY_SOCIETY_BOUNDARY_FACT_KEY,
            FactValue::Text(
                r#"{"type":"Polygon","coordinates":[[[1,1],[2,1],[2,2],[1,2],[1,1]]]}"#.to_string(),
            ),
        );
        legacy_footprint.observation_provider = None;
        legacy_footprint.provider_observation_id = None;
        legacy_footprint.asset_lineage.clear();
        let records = backfill_area_topology(AreaTopologyBackfillInput {
            entities: vec![
                entity("society:polygon", "society"),
                entity("society:explicit", "society"),
                entity("society:unmatched", "society"),
            ],
            facts: Vec::new(),
            search_metadata: Vec::new(),
            edges: Vec::new(),
            rera_evidence: Vec::new(),
            excluded_rera_evidence_society_ids: Vec::new(),
            area_facts,
            society_geometry_facts: vec![legacy_footprint],
            source_entity_seeds: vec![
                seed("society:polygon", "Whitefield"),
                seed("society:explicit", " Whitefield "),
                seed("society:unmatched", "Not In Boundary Data"),
            ],
            source_entity_seed_lineage: "source_entity_seed:file:sha256:fixture".to_string(),
            imported_at: Utc.timestamp_opt(1_700_000_100, 0).unwrap(),
        })
        .unwrap();

        assert_eq!(
            records.report.migrated_society_footprint_entity_ids,
            ["society:polygon"]
        );
        assert_eq!(
            records.report.imported_explicit_area_reference_entity_ids,
            ["society:explicit"]
        );
        assert_eq!(
            records
                .report
                .unmatched_explicit_area_references
                .get("society:unmatched")
                .map(String::as_str),
            Some("Not In Boundary Data")
        );
        assert!(!records.facts.iter().any(|fact| {
            fact.entity_id == "society:polygon" && fact.fact_key == EXPLICIT_AREA_NAME_FACT_KEY
        }));

        let mut topology_facts = records.facts.clone();
        let explicit_observed_at = Utc.timestamp_opt(1_700_000_200, 0).unwrap();
        topology_facts.push(ServingFactRecord {
            entity_id: "society:polygon".to_string(),
            fact_key: EXPLICIT_AREA_NAME_FACT_KEY.to_string(),
            value_type: "text".to_string(),
            value_text: Some("Whitefield".to_string()),
            value: FactValue::Text("Whitefield".to_string()),
            confidence: 0.95,
            source_type: "SourceEntitySeed".to_string(),
            source_url: None,
            model: None,
            skill_id: Some("topology_fixture".to_string()),
            learned_at: explicit_observed_at,
            observation: Some(
                SourceObservation::new(
                    "SourceEntitySeed",
                    "polygon-explicit-area",
                    "society:polygon",
                    explicit_observed_at,
                    None,
                    vec!["asset:topology-fixture/run:fixture-run".to_string()],
                )
                .unwrap(),
            ),
        });
        let index = ServingFactIndex::from_records(topology_facts, records.search_metadata.clone());
        let policy = SpatialTopologyPolicy {
            explicit_area_name_fact_keys: vec![EXPLICIT_AREA_NAME_FACT_KEY.to_string()],
            explicit_area_name_allowed_sources: vec!["source_entity_seed".to_string()],
            ..SpatialTopologyPolicy::default()
        };
        let topology = derive_spatial_topology(
            &records.entities,
            &index,
            &records.edges,
            &policy,
            "backfill-fixture",
        );
        assert!(topology.edges.iter().any(|edge| {
            edge.from_entity_id == "society:polygon"
                && edge.to_entity_id == "area:osm:whitefield"
                && edge
                    .derivation
                    .as_ref()
                    .is_some_and(|derivation| derivation.metric == "footprint_containment")
        }));
        assert!(topology.edges.iter().any(|edge| {
            edge.from_entity_id == "society:explicit"
                && edge.to_entity_id == "area:osm:whitefield"
                && edge
                    .derivation
                    .as_ref()
                    .is_some_and(|derivation| derivation.metric == "explicit_area_name_match")
        }));
        assert!(!topology
            .edges
            .iter()
            .any(|edge| edge.from_entity_id == "society:unmatched"));
        assert!(validate_serving_edge_evidence(
            &topology.edges,
            index.all_facts(),
            "backfill-fixture"
        )
        .is_ok());
    }

    #[test]
    fn conflicting_seed_area_assertions_fail_closed() {
        let error = unique_seed_area_assertions(vec![
            seed("society:conflict", "Whitefield"),
            seed("society:conflict", "Hoodi"),
        ])
        .unwrap_err();

        assert!(error.to_string().contains("disagree on the explicit area"));
    }
}
