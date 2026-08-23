use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::dag_config::{
    EligibilityValuePredicate, ProjectedPropertyRequirement, ServingEligibilityFile,
    SocietyEvidenceRequirement,
};
use crate::knowledge::FactValue;

use super::{
    QuarantinedSociety, ServingEdgeRecord, ServingEntityRecord, ServingFactIndex,
    ServingFactRecord, ServingQuarantineReport, ServingSearchMetadataRecord,
};

pub(crate) struct EligibleServingRecords {
    pub entities: Vec<ServingEntityRecord>,
    pub facts: Vec<ServingFactRecord>,
    pub search_metadata: Vec<ServingSearchMetadataRecord>,
    pub edges: Vec<ServingEdgeRecord>,
    pub quarantine: ServingQuarantineReport,
}

#[derive(Default)]
struct SocietyGroup {
    entity_ids: BTreeSet<String>,
    names: BTreeSet<String>,
    property_entity_ids: BTreeSet<String>,
    projected_property_ids: BTreeSet<String>,
    reasons: BTreeSet<String>,
}

pub(crate) fn classify_and_prune(
    entities: Vec<ServingEntityRecord>,
    facts: Vec<ServingFactRecord>,
    search_metadata: Vec<ServingSearchMetadataRecord>,
    edges: Vec<ServingEdgeRecord>,
    bundle_version: &str,
    config: &ServingEligibilityFile,
) -> Result<EligibleServingRecords, serde_json::Error> {
    let mut groups = society_groups(&entities);
    let runtime_id_by_entity_id = groups
        .iter()
        .flat_map(|(runtime_id, group)| {
            group
                .entity_ids
                .iter()
                .map(move |entity_id| (entity_id.clone(), runtime_id.clone()))
        })
        .collect::<BTreeMap<_, _>>();

    for group in groups
        .values_mut()
        .filter(|group| group.entity_ids.len() > 1)
    {
        group
            .reasons
            .insert("ambiguous_canonical_identity".to_string());
    }

    let mut property_runtime_ids = property_society_runtime_ids(&edges, &runtime_id_by_entity_id);
    let mut fact_index = ServingFactIndex::from_records(facts.clone(), search_metadata.clone());
    fact_index.add_society_aliases(&entities);
    let projected_properties = crate::data_loader::properties_from_serving_records_with_edges(
        &entities,
        &edges,
        &fact_index,
        bundle_version,
    );

    for property in &projected_properties {
        let property_entity_id = format!("property:{}", property.id);
        let runtime_ids = property_runtime_ids
            .entry(property_entity_id.clone())
            .or_insert_with(|| BTreeSet::from([property.society_id.clone()]));
        for runtime_id in runtime_ids.iter() {
            if let Some(group) = groups.get_mut(runtime_id) {
                group.property_entity_ids.insert(property_entity_id.clone());
                group.projected_property_ids.insert(property.id.clone());
            }
        }
    }

    for (property_entity_id, runtime_ids) in &property_runtime_ids {
        if runtime_ids.len() > 1 {
            for runtime_id in runtime_ids {
                if let Some(group) = groups.get_mut(runtime_id) {
                    group
                        .reasons
                        .insert("ambiguous_property_society".to_string());
                }
            }
        }
        for runtime_id in runtime_ids {
            if let Some(group) = groups.get_mut(runtime_id) {
                group.property_entity_ids.insert(property_entity_id.clone());
            }
        }
    }

    evaluate_property_requirements(
        &projected_properties,
        &property_runtime_ids,
        config,
        &mut groups,
    )?;
    evaluate_society_requirements(&facts, &edges, config, &mut groups);
    classify_missing_search_metadata(
        &facts,
        &search_metadata,
        &edges,
        &runtime_id_by_entity_id,
        &property_runtime_ids,
        &mut groups,
    );

    let quarantine = quarantine_report(bundle_version, config.version, groups);
    let removed_society_ids = quarantine
        .societies
        .iter()
        .flat_map(|society| society.society_entity_ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    let removed_property_ids = quarantine
        .societies
        .iter()
        .flat_map(|society| society.property_entity_ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    let removed_entity_ids = entities_to_remove(
        &entities,
        &edges,
        &removed_society_ids,
        &removed_property_ids,
    );

    Ok(EligibleServingRecords {
        entities: entities
            .into_iter()
            .filter(|entity| !removed_entity_ids.contains(&entity.entity_id))
            .collect(),
        facts: facts
            .into_iter()
            .filter(|fact| !removed_entity_ids.contains(&fact.entity_id))
            .collect(),
        search_metadata: search_metadata
            .into_iter()
            .filter(|metadata| !removed_entity_ids.contains(&metadata.entity_id))
            .collect(),
        edges: edges
            .into_iter()
            .filter(|edge| {
                !removed_entity_ids.contains(&edge.from_entity_id)
                    && !removed_entity_ids.contains(&edge.to_entity_id)
            })
            .collect(),
        quarantine,
    })
}

fn society_groups(entities: &[ServingEntityRecord]) -> BTreeMap<String, SocietyGroup> {
    let mut groups = BTreeMap::<String, SocietyGroup>::new();
    for entity in entities
        .iter()
        .filter(|entity| entity.entity_type == "society")
    {
        let runtime_id = format!("soc-{}", entity_slug(&entity.name));
        let group = groups.entry(runtime_id).or_default();
        group.entity_ids.insert(entity.entity_id.clone());
        group.names.insert(entity.name.clone());
    }
    groups
}

fn property_society_runtime_ids(
    edges: &[ServingEdgeRecord],
    runtime_id_by_entity_id: &BTreeMap<String, String>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut property_runtime_ids = BTreeMap::<String, BTreeSet<String>>::new();
    for edge in edges.iter().filter(|edge| edge.edge_type == "in_society") {
        if let Some(runtime_id) = runtime_id_by_entity_id.get(&edge.to_entity_id) {
            property_runtime_ids
                .entry(edge.from_entity_id.clone())
                .or_default()
                .insert(runtime_id.clone());
        }
    }
    property_runtime_ids
}

fn evaluate_property_requirements(
    properties: &[crate::models::Property],
    property_runtime_ids: &BTreeMap<String, BTreeSet<String>>,
    config: &ServingEligibilityFile,
    groups: &mut BTreeMap<String, SocietyGroup>,
) -> Result<(), serde_json::Error> {
    for group in groups.values_mut() {
        let has_unprojected_property_entity = group.property_entity_ids.iter().any(|entity_id| {
            let property_id = entity_id.strip_prefix("property:").unwrap_or(entity_id);
            !group.projected_property_ids.contains(property_id)
        });
        if group.projected_property_ids.len() < config.minimum_projected_properties
            || has_unprojected_property_entity
        {
            group
                .reasons
                .insert(config.missing_projection_reason_code.clone());
        }
    }

    for property in properties {
        let property_entity_id = format!("property:{}", property.id);
        let runtime_ids = property_runtime_ids
            .get(&property_entity_id)
            .cloned()
            .unwrap_or_else(|| BTreeSet::from([property.society_id.clone()]));
        let projected = serde_json::to_value(property)?;
        for requirement in &config.property_requirements {
            if projected_property_satisfies(&projected, requirement) {
                continue;
            }
            for runtime_id in &runtime_ids {
                if let Some(group) = groups.get_mut(runtime_id) {
                    group.reasons.insert(requirement.reason_code.clone());
                }
            }
        }
    }
    Ok(())
}

fn projected_property_satisfies(
    projected: &Value,
    requirement: &ProjectedPropertyRequirement,
) -> bool {
    let matches = |field: &String| {
        let value = projected.get(field).unwrap_or(&Value::Null);
        match requirement.predicate {
            EligibilityValuePredicate::AnyNonEmpty | EligibilityValuePredicate::AllNonEmpty => {
                value_is_non_empty(value)
            }
            EligibilityValuePredicate::AnyPositive => {
                value.as_f64().is_some_and(|value| value > 0.0)
            }
        }
    };
    match requirement.predicate {
        EligibilityValuePredicate::AllNonEmpty => requirement.fields.iter().all(matches),
        EligibilityValuePredicate::AnyNonEmpty | EligibilityValuePredicate::AnyPositive => {
            requirement.fields.iter().any(matches)
        }
    }
}

fn value_is_non_empty(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value > 0.0),
        Value::String(value) => !value.trim().is_empty(),
        Value::Array(values) => values.iter().any(value_is_non_empty),
        Value::Object(values) => !values.is_empty(),
    }
}

fn evaluate_society_requirements(
    facts: &[ServingFactRecord],
    edges: &[ServingEdgeRecord],
    config: &ServingEligibilityFile,
    groups: &mut BTreeMap<String, SocietyGroup>,
) {
    let mut facts_by_entity = BTreeMap::<&str, Vec<&ServingFactRecord>>::new();
    for fact in facts {
        facts_by_entity
            .entry(&fact.entity_id)
            .or_default()
            .push(fact);
    }

    for group in groups.values_mut() {
        for requirement in &config.society_requirements {
            if society_has_evidence(&group.entity_ids, requirement, edges, &facts_by_entity) {
                continue;
            }
            group.reasons.insert(requirement.reason_code.clone());
        }
    }
}

fn society_has_evidence(
    society_entity_ids: &BTreeSet<String>,
    requirement: &SocietyEvidenceRequirement,
    edges: &[ServingEdgeRecord],
    facts_by_entity: &BTreeMap<&str, Vec<&ServingFactRecord>>,
) -> bool {
    if society_entity_ids.iter().any(|entity_id| {
        facts_by_entity
            .get(entity_id.as_str())
            .into_iter()
            .flatten()
            .any(|fact| {
                requirement.fact_keys.contains(&fact.fact_key) && meaningful_fact_value(&fact.value)
            })
    }) {
        return true;
    }

    edges.iter().any(|edge| {
        let related_entity_id = if society_entity_ids.contains(&edge.from_entity_id)
            && requirement.related_edge_types.contains(&edge.edge_type)
        {
            Some(edge.to_entity_id.as_str())
        } else if society_entity_ids.contains(&edge.to_entity_id)
            && requirement.related_edge_types.contains(&edge.edge_type)
        {
            Some(edge.from_entity_id.as_str())
        } else {
            None
        };
        let Some(related_entity_id) = related_entity_id else {
            return false;
        };
        requirement.edge_presence_counts
            || facts_by_entity
                .get(related_entity_id)
                .into_iter()
                .flatten()
                .any(|fact| {
                    requirement.related_fact_keys.contains(&fact.fact_key)
                        && meaningful_fact_value(&fact.value)
                })
    })
}

fn meaningful_fact_value(value: &FactValue) -> bool {
    match value {
        FactValue::Bool(value) => *value,
        FactValue::Numeric(value) => value.is_finite() && *value > 0.0,
        FactValue::Text(value) => !value.trim().is_empty(),
        FactValue::Tags(values) => values.iter().any(|value| !value.trim().is_empty()),
        FactValue::Score { value, explanation } => {
            value.is_finite() || !explanation.trim().is_empty()
        }
    }
}

fn classify_missing_search_metadata(
    facts: &[ServingFactRecord],
    search_metadata: &[ServingSearchMetadataRecord],
    edges: &[ServingEdgeRecord],
    runtime_id_by_entity_id: &BTreeMap<String, String>,
    property_runtime_ids: &BTreeMap<String, BTreeSet<String>>,
    groups: &mut BTreeMap<String, SocietyGroup>,
) {
    let linked_runtime_ids = edges.iter().fold(
        BTreeMap::<&str, BTreeSet<&str>>::new(),
        |mut linked, edge| {
            if let Some(runtime_id) = runtime_id_by_entity_id.get(&edge.from_entity_id) {
                linked
                    .entry(&edge.to_entity_id)
                    .or_default()
                    .insert(runtime_id);
            }
            if let Some(runtime_id) = runtime_id_by_entity_id.get(&edge.to_entity_id) {
                linked
                    .entry(&edge.from_entity_id)
                    .or_default()
                    .insert(runtime_id);
            }
            linked
        },
    );
    let metadata_pairs = search_metadata
        .iter()
        .map(|metadata| (metadata.entity_id.as_str(), metadata.fact_key.as_str()))
        .collect::<BTreeSet<_>>();
    for fact in facts {
        if metadata_pairs.contains(&(fact.entity_id.as_str(), fact.fact_key.as_str())) {
            continue;
        }
        if let Some(runtime_id) = runtime_id_by_entity_id.get(&fact.entity_id) {
            if let Some(group) = groups.get_mut(runtime_id) {
                group.reasons.insert("missing_search_metadata".to_string());
            }
        }
        if let Some(runtime_ids) = property_runtime_ids.get(&fact.entity_id) {
            for runtime_id in runtime_ids {
                if let Some(group) = groups.get_mut(runtime_id) {
                    group.reasons.insert("missing_search_metadata".to_string());
                }
            }
        }
        if let Some(runtime_ids) = linked_runtime_ids.get(fact.entity_id.as_str()) {
            for runtime_id in runtime_ids {
                if let Some(group) = groups.get_mut(*runtime_id) {
                    group.reasons.insert("missing_search_metadata".to_string());
                }
            }
        }
    }
}

fn quarantine_report(
    bundle_version: &str,
    eligibility_policy_version: u32,
    groups: BTreeMap<String, SocietyGroup>,
) -> ServingQuarantineReport {
    let societies = groups
        .into_iter()
        .filter(|(_, group)| !group.reasons.is_empty())
        .map(|(runtime_society_id, group)| QuarantinedSociety {
            runtime_society_id,
            society_entity_ids: group.entity_ids.into_iter().collect(),
            society_names: group.names.into_iter().collect(),
            property_entity_ids: group.property_entity_ids.into_iter().collect(),
            projected_property_ids: group.projected_property_ids.into_iter().collect(),
            reason_codes: group.reasons.into_iter().collect(),
        })
        .collect::<Vec<_>>();
    let mut reason_counts = BTreeMap::<String, u64>::new();
    for society in &societies {
        for reason in &society.reason_codes {
            *reason_counts.entry(reason.clone()).or_default() += 1;
        }
    }
    ServingQuarantineReport {
        format_version: 1,
        eligibility_policy_version,
        source_bundle_version: bundle_version.to_string(),
        excluded_society_count: societies.len() as u64,
        reason_counts,
        societies,
    }
}

fn entities_to_remove(
    entities: &[ServingEntityRecord],
    edges: &[ServingEdgeRecord],
    removed_society_ids: &BTreeSet<String>,
    removed_property_ids: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut removed = removed_society_ids
        .union(removed_property_ids)
        .cloned()
        .collect::<BTreeSet<_>>();

    for edge in edges.iter().filter(|edge| edge.edge_type == "for_property") {
        if removed_property_ids.contains(&edge.to_entity_id) {
            removed.insert(edge.from_entity_id.clone());
        }
    }

    let entity_type_by_id = entities
        .iter()
        .map(|entity| (entity.entity_id.as_str(), entity.entity_type.as_str()))
        .collect::<BTreeMap<_, _>>();
    let linked_societies = edges.iter().fold(
        BTreeMap::<&str, BTreeSet<&str>>::new(),
        |mut links, edge| {
            if entity_type_by_id.get(edge.from_entity_id.as_str()) == Some(&"society") {
                links
                    .entry(&edge.to_entity_id)
                    .or_default()
                    .insert(&edge.from_entity_id);
            }
            if entity_type_by_id.get(edge.to_entity_id.as_str()) == Some(&"society") {
                links
                    .entry(&edge.from_entity_id)
                    .or_default()
                    .insert(&edge.to_entity_id);
            }
            links
        },
    );
    for (entity_id, society_ids) in linked_societies {
        if society_ids
            .iter()
            .all(|society_id| removed_society_ids.contains(*society_id))
        {
            removed.insert(entity_id.to_string());
        }
    }
    removed
}

fn entity_slug(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
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

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::dag_config::{EligibilityValuePredicate, ProjectedPropertyRequirement};

    fn policy() -> ServingEligibilityFile {
        ServingEligibilityFile {
            version: 1,
            minimum_projected_properties: 1,
            missing_projection_reason_code: "missing_property_projection".to_string(),
            property_requirements: vec![
                ProjectedPropertyRequirement {
                    reason_code: "missing_property_area".to_string(),
                    predicate: EligibilityValuePredicate::AnyNonEmpty,
                    fields: vec!["area".to_string()],
                },
                ProjectedPropertyRequirement {
                    reason_code: "missing_property_size".to_string(),
                    predicate: EligibilityValuePredicate::AnyPositive,
                    fields: vec!["carpet_area_sqft".to_string()],
                },
                ProjectedPropertyRequirement {
                    reason_code: "missing_property_builder".to_string(),
                    predicate: EligibilityValuePredicate::AnyNonEmpty,
                    fields: vec!["builder_name".to_string()],
                },
                ProjectedPropertyRequirement {
                    reason_code: "missing_property_media".to_string(),
                    predicate: EligibilityValuePredicate::AllNonEmpty,
                    fields: vec!["hero_image".to_string(), "images".to_string()],
                },
            ],
            society_requirements: vec![
                SocietyEvidenceRequirement {
                    reason_code: "missing_rera_registration".to_string(),
                    fact_keys: vec!["rera_registered".to_string()],
                    related_edge_types: Vec::new(),
                    related_fact_keys: Vec::new(),
                    edge_presence_counts: false,
                },
                SocietyEvidenceRequirement {
                    reason_code: "missing_approach_road_evidence".to_string(),
                    fact_keys: Vec::new(),
                    related_edge_types: vec!["served_by_road".to_string()],
                    related_fact_keys: Vec::new(),
                    edge_presence_counts: true,
                },
            ],
        }
    }

    fn entity(entity_id: &str, entity_type: &str, name: &str) -> ServingEntityRecord {
        ServingEntityRecord {
            entity_id: entity_id.to_string(),
            entity_type: entity_type.to_string(),
            name: name.to_string(),
            root_source: Some("test".to_string()),
            searchable_text: String::new(),
        }
    }

    fn fact(entity_id: &str, fact_key: &str, value: FactValue) -> ServingFactRecord {
        ServingFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            value_type: "test".to_string(),
            value_text: None,
            value,
            confidence: 1.0,
            source_type: "Manual".to_string(),
            source_url: None,
            model: None,
            skill_id: None,
            learned_at: Utc::now(),
        }
    }

    fn metadata(fact: &ServingFactRecord) -> ServingSearchMetadataRecord {
        ServingSearchMetadataRecord {
            entity_id: fact.entity_id.clone(),
            fact_key: fact.fact_key.clone(),
            display_template: None,
            answers_preferences: Vec::new(),
            scoring_direction: None,
            scoring_weight: None,
            scoring_thresholds: Vec::new(),
        }
    }

    fn edge(from: &str, edge_type: &str, to: &str) -> ServingEdgeRecord {
        ServingEdgeRecord {
            from_entity_id: from.to_string(),
            edge_type: edge_type.to_string(),
            to_entity_id: to.to_string(),
            confidence: 1.0,
            source_type: "Manual".to_string(),
        }
    }

    fn two_society_records(
        include_incomplete_media: bool,
    ) -> (
        Vec<ServingEntityRecord>,
        Vec<ServingFactRecord>,
        Vec<ServingSearchMetadataRecord>,
        Vec<ServingEdgeRecord>,
    ) {
        let entities = vec![
            entity("society:complete", "society", "Complete"),
            entity("society:incomplete", "society", "Incomplete"),
            entity("property:complete-2bhk", "property", "Complete 2 BHK"),
            entity("property:incomplete-2bhk", "property", "Incomplete 2 BHK"),
            entity("area:shared", "area", "Shared Area"),
            entity("road:shared", "road_segment", "Shared Road"),
        ];
        let mut facts = Vec::new();
        for society_id in ["society:complete", "society:incomplete"] {
            facts.push(fact(society_id, "rera_registered", FactValue::Bool(true)));
        }
        for property_id in ["property:complete-2bhk", "property:incomplete-2bhk"] {
            facts.extend([
                fact(property_id, "price", FactValue::Numeric(10_000_000.0)),
                fact(property_id, "bhk", FactValue::Numeric(2.0)),
                fact(property_id, "carpet_area_sqft", FactValue::Numeric(1_100.0)),
                fact(
                    property_id,
                    "area",
                    FactValue::Text("Shared Area".to_string()),
                ),
                fact(
                    property_id,
                    "builder_name",
                    FactValue::Text("Test Builder".to_string()),
                ),
            ]);
        }
        facts.push(fact(
            "property:complete-2bhk",
            "hero_image",
            FactValue::Text("/media/complete.webp".to_string()),
        ));
        facts.push(fact(
            "property:complete-2bhk",
            "images",
            FactValue::Tags(vec!["/media/complete.webp".to_string()]),
        ));
        if include_incomplete_media {
            facts.push(fact(
                "property:incomplete-2bhk",
                "hero_image",
                FactValue::Text("/media/incomplete.webp".to_string()),
            ));
            facts.push(fact(
                "property:incomplete-2bhk",
                "images",
                FactValue::Tags(vec!["/media/incomplete.webp".to_string()]),
            ));
        }
        let search_metadata = facts.iter().map(metadata).collect();
        let edges = vec![
            edge("property:complete-2bhk", "in_society", "society:complete"),
            edge(
                "property:incomplete-2bhk",
                "in_society",
                "society:incomplete",
            ),
            edge("society:complete", "in_area", "area:shared"),
            edge("society:incomplete", "in_area", "area:shared"),
            edge("society:complete", "served_by_road", "road:shared"),
            edge("society:incomplete", "served_by_road", "road:shared"),
        ];
        (entities, facts, search_metadata, edges)
    }

    #[test]
    fn incomplete_society_is_atomically_quarantined_and_shared_entities_remain() {
        let (entities, facts, metadata, edges) = two_society_records(false);
        let result = classify_and_prune(entities, facts, metadata, edges, "test-v1", &policy())
            .expect("eligibility classification should succeed");

        let clean_ids = result
            .entities
            .iter()
            .map(|entity| entity.entity_id.as_str())
            .collect::<BTreeSet<_>>();
        assert!(clean_ids.contains("society:complete"));
        assert!(clean_ids.contains("property:complete-2bhk"));
        assert!(clean_ids.contains("area:shared"));
        assert!(clean_ids.contains("road:shared"));
        assert!(!clean_ids.contains("society:incomplete"));
        assert!(!clean_ids.contains("property:incomplete-2bhk"));
        assert!(result
            .facts
            .iter()
            .all(|fact| fact.entity_id != "society:incomplete"
                && fact.entity_id != "property:incomplete-2bhk"));
        assert!(result.edges.iter().all(|edge| {
            edge.from_entity_id != "society:incomplete" && edge.to_entity_id != "society:incomplete"
        }));

        assert_eq!(result.quarantine.societies.len(), 1);
        let quarantined = &result.quarantine.societies[0];
        assert_eq!(quarantined.runtime_society_id, "soc-incomplete");
        assert_eq!(
            quarantined.reason_codes,
            vec!["missing_property_media".to_string()]
        );
        assert_eq!(
            quarantined.property_entity_ids,
            vec!["property:incomplete-2bhk".to_string()]
        );
    }

    #[test]
    fn corrected_source_fact_automatically_readmits_society() {
        let (entities, facts, metadata, edges) = two_society_records(true);
        let result = classify_and_prune(entities, facts, metadata, edges, "test-v2", &policy())
            .expect("eligibility classification should succeed");

        assert!(result.quarantine.societies.is_empty());
        assert!(result
            .entities
            .iter()
            .any(|entity| entity.entity_id == "society:incomplete"));
        assert!(result
            .entities
            .iter()
            .any(|entity| entity.entity_id == "property:incomplete-2bhk"));
    }

    #[test]
    fn one_incomplete_card_quarantines_the_whole_society() {
        let (mut entities, mut facts, _, mut edges) = two_society_records(true);
        entities.push(entity(
            "property:complete-3bhk",
            "property",
            "Complete 3 BHK",
        ));
        facts.extend([
            fact(
                "property:complete-3bhk",
                "price",
                FactValue::Numeric(12_000_000.0),
            ),
            fact("property:complete-3bhk", "bhk", FactValue::Numeric(3.0)),
            fact(
                "property:complete-3bhk",
                "carpet_area_sqft",
                FactValue::Numeric(1_300.0),
            ),
            fact(
                "property:complete-3bhk",
                "area",
                FactValue::Text("Shared Area".to_string()),
            ),
            fact(
                "property:complete-3bhk",
                "builder_name",
                FactValue::Text("Test Builder".to_string()),
            ),
        ]);
        let metadata = facts.iter().map(metadata).collect();
        edges.push(edge(
            "property:complete-3bhk",
            "in_society",
            "society:complete",
        ));

        let result = classify_and_prune(entities, facts, metadata, edges, "test-v3", &policy())
            .expect("eligibility classification should succeed");

        assert!(result
            .entities
            .iter()
            .all(|entity| entity.entity_id != "society:complete"
                && !entity.entity_id.starts_with("property:complete-")));
        let quarantined = result
            .quarantine
            .societies
            .iter()
            .find(|society| society.runtime_society_id == "soc-complete")
            .expect("complete society should be quarantined by its incomplete card");
        assert!(quarantined
            .reason_codes
            .contains(&"missing_property_media".to_string()));
    }

    #[test]
    fn missing_related_search_metadata_quarantines_every_affected_society() {
        let (entities, mut facts, mut metadata, edges) = two_society_records(true);
        let road_fact = fact("road:shared", "road_width", FactValue::Numeric(12.0));
        facts.push(road_fact.clone());
        metadata.retain(|row| {
            row.entity_id != road_fact.entity_id || row.fact_key != road_fact.fact_key
        });

        let result = classify_and_prune(entities, facts, metadata, edges, "test-v4", &policy())
            .expect("eligibility classification should succeed");

        assert_eq!(result.quarantine.societies.len(), 2);
        assert!(result.quarantine.societies.iter().all(|society| society
            .reason_codes
            .contains(&"missing_search_metadata".to_string())));
        assert!(result
            .entities
            .iter()
            .all(|entity| entity.entity_id != "road:shared"));
        assert!(result
            .facts
            .iter()
            .all(|fact| fact.entity_id != "road:shared"));
    }

    #[test]
    fn property_entity_without_price_remains_projectable() {
        let (entities, mut facts, _, edges) = two_society_records(true);
        facts.retain(|fact| {
            fact.entity_id != "property:incomplete-2bhk" || fact.fact_key != "price"
        });
        let metadata = facts.iter().map(metadata).collect();

        let result = classify_and_prune(entities, facts, metadata, edges, "test-v5", &policy())
            .expect("eligibility classification should succeed");

        assert!(result.quarantine.societies.is_empty());
        assert!(result
            .entities
            .iter()
            .any(|entity| entity.entity_id == "property:incomplete-2bhk"));
    }

    #[test]
    fn launch_policy_admits_image_backed_discovery_with_optional_gaps() {
        let entities = vec![
            entity("society:promising", "society", "Promising"),
            entity("property:promising-3bhk", "property", "Promising 3 BHK"),
        ];
        let facts = vec![
            fact("property:promising-3bhk", "bhk", FactValue::Numeric(3.0)),
            fact(
                "property:promising-3bhk",
                "hero_image",
                FactValue::Text("/media/promising.webp".to_string()),
            ),
            fact(
                "society:promising",
                "google_rating",
                FactValue::Numeric(4.4),
            ),
        ];
        let metadata = facts.iter().map(metadata).collect();
        let edges = vec![edge(
            "property:promising-3bhk",
            "in_society",
            "society:promising",
        )];
        let policy =
            crate::dag_config::load_serving_eligibility().expect("launch eligibility policy");

        let result =
            classify_and_prune(entities, facts, metadata, edges, "test-lenient-v1", &policy)
                .expect("classification succeeds");

        assert!(result.quarantine.societies.is_empty());
        assert!(result
            .entities
            .iter()
            .any(|entity| entity.entity_id == "property:promising-3bhk"));
    }
}
