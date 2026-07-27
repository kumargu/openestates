use crate::knowledge::FactValue;
use crate::models::Property;
use crate::serving::{ServingEntityFactRows, ServingFactIndex};

pub fn related_society_entity_ids(property: &Property, facts: &ServingFactIndex) -> Vec<String> {
    let target_names = related_society_match_names(property);
    if target_names.is_empty() {
        return Vec::new();
    }

    let primary_candidates = society_entity_id_candidates(&property.society_id);
    let mut entity_ids = facts
        .rows()
        .filter(|(entity_id, rows)| {
            entity_id.starts_with("society:")
                && !primary_candidates
                    .iter()
                    .any(|candidate| candidate == entity_id)
                && serving_society_rows_match_names(rows, &target_names)
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
    use crate::serving::{ServingFactIndex, ServingFactRecord};
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
}
