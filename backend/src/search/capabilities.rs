use std::collections::HashSet;

use crate::serving::{ServingEntityRecord, ServingFactIndex};

use super::intent::PreferenceSignal;

/// Search dimensions that are both configured and present in the promoted bundle.
///
/// The index is built once at bundle load. Request-time validation only performs
/// set lookups; it never scans Parquet rows or invents support from parser terms.
#[derive(Debug, Clone, Default)]
pub struct SearchCapabilityIndex {
    fact_keys: HashSet<String>,
    preference_labels: HashSet<String>,
    entity_types: HashSet<String>,
}

impl SearchCapabilityIndex {
    pub fn from_bundle(entities: &[ServingEntityRecord], facts: &ServingFactIndex) -> Self {
        let mut index = Self::default();
        for entity in entities {
            index
                .entity_types
                .insert(entity.entity_type.trim().to_ascii_lowercase());
        }
        for (_, rows) in facts.rows() {
            for fact in &rows.facts {
                index
                    .fact_keys
                    .insert(fact.fact_key.trim().to_ascii_lowercase());
            }
            for metadata in &rows.search_metadata {
                if !index
                    .fact_keys
                    .contains(&metadata.fact_key.trim().to_ascii_lowercase())
                {
                    continue;
                }
                for preference in &metadata.answers_preferences {
                    index
                        .preference_labels
                        .insert(preference.trim().to_ascii_lowercase());
                }
            }
        }
        index
    }

    pub fn supports_preference(&self, preference: &PreferenceSignal) -> bool {
        preference
            .expanded_keys
            .iter()
            .any(|key| self.supports_fact_key(key))
            || self
                .preference_labels
                .contains(&preference.raw_text.trim().to_ascii_lowercase())
    }

    pub fn supports_fact_key(&self, fact_key: &str) -> bool {
        let requested = fact_key.trim().to_ascii_lowercase();
        self.fact_keys.contains(&requested)
            || self.fact_keys.iter().any(|available| {
                available.starts_with(&format!("{requested}_"))
                    || requested.starts_with(&format!("{available}_"))
            })
    }

    pub fn supports_entity_type(&self, entity_type: &str) -> bool {
        self.entity_types
            .contains(&entity_type.trim().to_ascii_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::knowledge::FactValue;
    use crate::serving::{ServingFactRecord, ServingSearchMetadataRecord};

    use super::*;
    use crate::search::intent::Polarity;

    #[test]
    fn capability_requires_a_promoted_fact_not_metadata_alone() {
        let facts = ServingFactIndex::from_records(
            vec![ServingFactRecord {
                entity_id: "society:one".to_string(),
                fact_key: "noise_score".to_string(),
                value_type: "numeric".to_string(),
                value_text: Some("0.8".to_string()),
                value: FactValue::Numeric(0.8),
                confidence: 0.8,
                source_type: "Computed".to_string(),
                source_url: None,
                model: None,
                skill_id: None,
                learned_at: Utc::now(),
            }],
            vec![ServingSearchMetadataRecord {
                entity_id: "society:one".to_string(),
                fact_key: "noise_score".to_string(),
                display_template: None,
                answers_preferences: vec!["quiet neighborhood".to_string()],
                scoring_direction: Some("HigherIsBetter".to_string()),
                scoring_weight: Some(1.0),
                scoring_thresholds: Vec::new(),
            }],
        );
        let index = SearchCapabilityIndex::from_bundle(&[], &facts);
        assert!(index.supports_preference(&PreferenceSignal {
            raw_text: "quiet neighborhood".to_string(),
            polarity: Polarity::Positive,
            expanded_keys: vec!["noise_score".to_string()],
            gap_keys: Vec::new(),
            weight: 1.0,
            required: false,
            missing_evidence_neutral: true,
        }));
        assert!(!index.supports_fact_key("cats"));
    }
}
