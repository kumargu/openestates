use std::collections::HashSet;

use crate::serving::{ServingEntityRecord, ServingFactIndex};

use super::intent::PreferenceSignal;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CapabilityExclusion {
    pub entity_id: String,
    pub fact_key: String,
    pub preference: String,
    pub reason: String,
}

/// Search dimensions that are both configured and present in the promoted bundle.
///
/// The index is built once at bundle load. Request-time validation only performs
/// set lookups; it never scans Parquet rows or invents support from parser terms.
#[derive(Debug, Clone, Default)]
pub struct SearchCapabilityIndex {
    fact_keys: HashSet<String>,
    preference_labels: HashSet<String>,
    entity_types: HashSet<String>,
    excluded: Vec<CapabilityExclusion>,
}

impl SearchCapabilityIndex {
    pub fn from_bundle(entities: &[ServingEntityRecord], facts: &ServingFactIndex) -> Self {
        let mut index = Self::default();
        for entity in entities {
            index
                .entity_types
                .insert(entity.entity_type.trim().to_ascii_lowercase());
        }
        for (entity_id, rows) in facts.rows() {
            for metadata in &rows.search_metadata {
                for preference in &metadata.answers_preferences {
                    if super::text::preference_capability_supported(
                        facts,
                        entity_id,
                        &metadata.fact_key,
                        preference,
                    ) {
                        index
                            .fact_keys
                            .insert(metadata.fact_key.trim().to_ascii_lowercase());
                        index
                            .preference_labels
                            .insert(preference.trim().to_ascii_lowercase());
                    } else {
                        index.excluded.push(CapabilityExclusion {
                            entity_id: entity_id.to_string(),
                            fact_key: metadata.fact_key.clone(),
                            preference: preference.clone(),
                            reason: "no_eligible_evaluable_evidence".to_string(),
                        });
                    }
                }
            }
        }
        index.excluded.sort_by(|a, b| {
            (&a.entity_id, &a.fact_key, &a.preference).cmp(&(
                &b.entity_id,
                &b.fact_key,
                &b.preference,
            ))
        });
        index.excluded.dedup();
        index
    }

    pub fn excluded_bindings(&self) -> &[CapabilityExclusion] {
        &self.excluded
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
                observation: Some(
                    crate::serving::SourceObservation::new(
                        "Computed",
                        "noise-observation",
                        "society:one",
                        Utc::now(),
                        None,
                        vec!["fixture/v1".to_string()],
                    )
                    .unwrap(),
                ),
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
        let rows = facts.entity("society:one").unwrap();
        for (confidence, value) in [
            (0.1, FactValue::Numeric(0.8)),
            (0.8, FactValue::Text("not a number".to_string())),
        ] {
            let mut fact = rows.facts[0].clone();
            fact.confidence = confidence;
            fact.value = value;
            let rejected = ServingFactIndex::from_records(vec![fact], rows.search_metadata.clone());
            assert!(
                !SearchCapabilityIndex::from_bundle(&[], &rejected)
                    .supports_fact_key("noise_score"),
                "capability must bind eligible evidence and an executable value type"
            );
        }
    }
}
