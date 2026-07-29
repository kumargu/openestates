use crate::knowledge::FactValue;
use crate::models::Property;
use crate::serving::{ServingEntityFactRows, ServingEntityRecord, ServingFactIndex};

pub fn related_society_entity_ids(property: &Property, facts: &ServingFactIndex) -> Vec<String> {
    related_society_entity_ids_with_entities(property, facts, &[])
}

pub fn related_society_entity_ids_with_entities(
    property: &Property,
    facts: &ServingFactIndex,
    entities: &[ServingEntityRecord],
) -> Vec<String> {
    let target_names = related_society_match_names(property);
    if target_names.is_empty() {
        return Vec::new();
    }

    let primary_candidates = society_entity_id_candidates(&property.society_id);
    let society_entity_names = entities
        .iter()
        .filter(|entity| entity.entity_type.eq_ignore_ascii_case("society"))
        .filter_map(|entity| {
            normalized_project_name(&entity.name).map(|name| (entity.entity_id.as_str(), name))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let mut entity_ids = facts
        .rows()
        .filter(|(entity_id, rows)| {
            entity_id.starts_with("society:")
                && !primary_candidates
                    .iter()
                    .any(|candidate| candidate == entity_id)
                && (serving_society_rows_match_names(rows, &target_names)
                    || society_entity_names
                        .get(entity_id)
                        .is_some_and(|name| entity_name_matches_targets(name, &target_names)))
        })
        .map(|(entity_id, _)| entity_id.to_string())
        .collect::<Vec<_>>();
    entity_ids.sort();
    entity_ids.dedup();
    entity_ids
}

pub fn related_society_match_names(property: &Property) -> Vec<String> {
    let mut names = vec![project_name_for(property), property.society_id.clone()];
    if let Some(society_slug) = property
        .society_id
        .strip_prefix("soc-")
        .or_else(|| property.society_id.strip_prefix("society:"))
    {
        names.push(society_slug.replace('-', " "));
    }
    names.sort();
    names.dedup();
    names
        .into_iter()
        .filter_map(|name| normalized_project_name(&name))
        .collect()
}

pub fn serving_society_rows_match_names(
    rows: &ServingEntityFactRows,
    target_names: &[String],
) -> bool {
    const NAME_FACT_KEYS: &[&str] = &["listing_society", "title", "rera_project_name"];
    rows.facts.iter().any(|fact| {
        NAME_FACT_KEYS.contains(&fact.fact_key.as_str())
            && match &fact.value {
                FactValue::Text(value) => normalized_project_name(value).is_some_and(|name| {
                    target_names
                        .iter()
                        .any(|target| names_compatible(target, &name))
                }),
                _ => false,
            }
    })
}

fn entity_name_matches_targets(name: &str, target_names: &[String]) -> bool {
    target_names
        .iter()
        .any(|target| names_compatible(target, name))
}

pub fn names_compatible(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    let left_tokens = left.split_whitespace().collect::<Vec<_>>();
    let right_tokens = right.split_whitespace().collect::<Vec<_>>();
    if left_tokens.is_empty() || right_tokens.is_empty() {
        return false;
    }
    let (smaller, larger) = if left_tokens.len() <= right_tokens.len() {
        (&left_tokens, &right_tokens)
    } else {
        (&right_tokens, &left_tokens)
    };
    let mut start = 0usize;
    for token in smaller {
        match larger[start..]
            .iter()
            .position(|candidate| candidate == token)
        {
            Some(index) => start += index + 1,
            None => return false,
        }
    }
    true
}

pub fn normalized_project_name(value: &str) -> Option<String> {
    let normalized = value
        .to_lowercase()
        .replace('&', " and ")
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| {
            !token.is_empty()
                && !matches!(
                    *token,
                    "soc" | "society" | "rera" | "project" | "phase" | "the"
                )
        })
        .collect::<Vec<_>>()
        .join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

pub fn project_name_for(property: &Property) -> String {
    let prefix = format!("{} BHK in ", property.bhk);
    property
        .title
        .strip_prefix(&prefix)
        .unwrap_or(&property.title)
        .to_string()
}

fn society_entity_id_candidates(society_id: &str) -> Vec<String> {
    let raw = society_id.trim().to_lowercase().replace(['_', ' '], "-");
    let slug = raw
        .strip_prefix("society:")
        .or_else(|| raw.strip_prefix("soc-"))
        .unwrap_or(&raw);
    let canonical = format!("society:{slug}");
    if raw == canonical {
        vec![canonical]
    } else {
        vec![canonical, raw]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serving::{ServingEntityRecord, ServingFactIndex, ServingFactRecord};
    use chrono::{TimeZone, Utc};

    #[test]
    fn related_society_matching_allows_phase_suffixes() {
        let index = ServingFactIndex::from_records(
            vec![ServingFactRecord {
                entity_id: "society:rera-assetz".to_string(),
                fact_key: "rera_project_name".to_string(),
                value_type: "text".to_string(),
                value_text: None,
                value: FactValue::Text("Assetz Marq Phase 3A".to_string()),
                confidence: 0.9,
                source_type: "Rera".to_string(),
                source_url: None,
                model: None,
                skill_id: None,
                learned_at: Utc.timestamp_opt(10, 0).unwrap(),
            }],
            Vec::new(),
        );
        let rows = index.entity("society:rera-assetz").unwrap();
        assert!(serving_society_rows_match_names(
            rows,
            &["assetz marq".to_string()]
        ));
    }

    #[test]
    fn related_society_matching_uses_serving_entity_name_when_name_fact_is_missing() {
        let property = Property {
            id: "property-one".to_string(),
            title: "3 BHK in Godrej Air".to_string(),
            area: "Whitefield".to_string(),
            area_id: "whitefield".to_string(),
            city: "Bengaluru".to_string(),
            society_id: "godrej-air".to_string(),
            builder_name: "Godrej".to_string(),
            property_type: "Apartment".to_string(),
            listing_type: "Resale".to_string(),
            bhk: 3,
            price: 20_000_000,
            price_per_sqft: 10_000,
            carpet_area_sqft: 1_500,
            super_builtup_sqft: 2_000,
            floor: 5,
            total_floors: 20,
            facing: "East".to_string(),
            possession_status: "Ready to move".to_string(),
            metro_distance_mins: 10,
            maintenance_cost_monthly: 8_000,
            society_quality_score: None,
            builder_quality_score: None,
            document_completeness_score: None,
            litigation_risk: None,
            noise_score: None,
            sunlight_score: None,
            airport_noise_score: None,
            waterlogging_risk_score: None,
            traffic_score: None,
            days_on_market: 10,
            greenery_score: None,
            open_space_score: None,
            resale_strength_score: None,
            interest_level: None,
            saves_last_7d: None,
            offers_last_7d: None,
            images: Vec::new(),
            hero_image: String::new(),
            description_summary: String::new(),
            transparency_tags: Vec::new(),
            source_reference: String::new(),
        };
        let entities = vec![
            ServingEntityRecord {
                entity_id: "society:godrej-air".to_string(),
                entity_type: "society".to_string(),
                name: "Godrej Air".to_string(),
                root_source: None,
                searchable_text: "Godrej Air".to_string(),
            },
            ServingEntityRecord {
                entity_id: "society:rera-godrej-air".to_string(),
                entity_type: "society".to_string(),
                name: "Godrej Air".to_string(),
                root_source: None,
                searchable_text: "Godrej Air high voltage transmission line nearby".to_string(),
            },
        ];
        let index = ServingFactIndex::from_records(
            vec![
                ServingFactRecord {
                    entity_id: "society:godrej-air".to_string(),
                    fact_key: "listing_society".to_string(),
                    value_type: "text".to_string(),
                    value_text: None,
                    value: FactValue::Text("Godrej Air".to_string()),
                    confidence: 0.9,
                    source_type: "Listing".to_string(),
                    source_url: None,
                    model: None,
                    skill_id: None,
                    learned_at: Utc.timestamp_opt(10, 0).unwrap(),
                },
                ServingFactRecord {
                    entity_id: "society:rera-godrej-air".to_string(),
                    fact_key: "high_voltage_transmission_line_nearby".to_string(),
                    value_type: "text".to_string(),
                    value_text: None,
                    value: FactValue::Text("Transmission line (91 m, severity: high)".to_string()),
                    confidence: 0.9,
                    source_type: "OpenStreetMap".to_string(),
                    source_url: None,
                    model: None,
                    skill_id: None,
                    learned_at: Utc.timestamp_opt(10, 0).unwrap(),
                },
            ],
            Vec::new(),
        );

        assert_eq!(
            related_society_entity_ids_with_entities(&property, &index, &entities),
            vec!["society:rera-godrej-air".to_string()]
        );
    }
}
