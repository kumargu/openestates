use serde::{Deserialize, Serialize};

use crate::knowledge::FactValue;
use crate::models::Property;
use crate::serving::{DerivedEvidence, EvidenceRef, ServingFactIndex};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryOption {
    pub confidence: f32,
    pub property_id: String,
    pub society_id: String,
    pub bhk: Option<u32>,
    pub price_min: Option<u64>,
    pub price_max: Option<u64>,
    pub size_sqft: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area_measurement: Option<crate::models::Measurement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_reference: Option<EvidenceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_fact_key: Option<String>,
}

impl InventoryOption {
    pub fn from_serving_observation(
        property: &Property,
        society_entity_id: &str,
        serving_facts: &ServingFactIndex,
        snapshot_identity: &str,
    ) -> Option<Self> {
        let rows = serving_facts.entity(society_entity_id)?;
        let policies = crate::serving::admission::policies();
        let mut candidates = rows
            .facts
            .iter()
            .filter_map(|fact| {
                let value = InventoryObservationValue::from_fact(fact).ok()?;
                let observation = fact.observation.as_ref()?;
                if fact.entity_id != society_entity_id {
                    return None;
                }
                if value
                    .property_id
                    .as_deref()
                    .is_some_and(|id| id != property.id)
                    || (serving_facts
                        .entity(&format!("property:{}", property.id))
                        .is_some()
                        && value.property_id.as_deref() != Some(property.id.as_str()))
                {
                    return None;
                }
                let area = value.area_sqft;
                let basis = value.area_type.clone();
                let minimum = value.area_sqft_min;
                let maximum = value.area_sqft_max;
                let mut option = value.into_option(property, society_entity_id)?;
                option.confidence = fact.confidence;
                if let Some(value) = area.filter(|v| *v > 0) {
                    option.area_measurement = Some(crate::models::Measurement {
                        value: value as f64,
                        minimum,
                        maximum,
                        unit: "sqft".to_string(),
                        basis: basis.unwrap_or_else(|| "unspecified".to_string()),
                        entity_id: property.id.clone(),
                        evidence: EvidenceRef::for_observation(snapshot_identity, observation),
                    });
                }
                Some((fact, observation, fact.fact_key.as_str(), option))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            let better = |a: &crate::serving::ServingFactRecord,
                          b: &crate::serving::ServingFactRecord| {
                crate::dag_config::better_source_type_for_fact(
                    Some(&a.fact_key),
                    &a.source_type,
                    &b.source_type,
                    a.confidence,
                    b.confidence,
                    policies,
                )
            };
            if better(left.0, right.0) {
                std::cmp::Ordering::Less
            } else if better(right.0, left.0) {
                std::cmp::Ordering::Greater
            } else {
                left.1
                    .observation_id
                    .as_str()
                    .cmp(right.1.observation_id.as_str())
                    .then_with(|| {
                        left.0
                            .stable_selection_key()
                            .cmp(&right.0.stable_selection_key())
                    })
            }
        });
        let (_, observation, fact_key, mut option) = candidates.into_iter().next()?;
        let evidence_reference = EvidenceRef::for_observation(snapshot_identity, observation);
        evidence_reference
            .validate_for(society_entity_id, snapshot_identity)
            .ok()?;
        if let Some(area) = option.area_measurement.as_mut() {
            area.evidence = evidence_reference.clone();
        }
        option.evidence_reference = Some(evidence_reference);
        option.evidence_fact_key = Some(fact_key.to_string());
        Some(option)
    }

    pub fn evaluate_bhk(
        &self,
        property_id: &str,
        society_entity_id: &str,
        expected: u32,
        snapshot_identity: &str,
    ) -> BooleanEvaluation {
        let Some(actual) = self.bhk else {
            return BooleanEvaluation::unknown();
        };
        let Some(verified) = self.verified_match(
            property_id,
            society_entity_id,
            format!("{expected} BHK"),
            "equals",
            "inventory_option_bhk",
            actual as f64,
            "bhk",
            snapshot_identity,
        ) else {
            return BooleanEvaluation::unknown();
        };
        if actual == expected {
            BooleanEvaluation::satisfied(vec![verified])
        } else {
            BooleanEvaluation::unsatisfied_with(vec![verified])
        }
    }

    pub fn evaluate_budget(
        &self,
        property_id: &str,
        society_entity_id: &str,
        min: Option<u64>,
        max: Option<u64>,
        snapshot_identity: &str,
    ) -> BooleanEvaluation {
        let (Some(actual_min), Some(actual_max)) = (self.price_min, self.price_max) else {
            return BooleanEvaluation::unknown();
        };
        let mut matches = Vec::new();
        let mut satisfied = true;
        if let Some(min) = min {
            let Some(verified) = self.verified_match(
                property_id,
                society_entity_id,
                format!("price at least {min}"),
                "at_least",
                "inventory_option_price_max",
                actual_max as f64,
                "INR",
                snapshot_identity,
            ) else {
                return BooleanEvaluation::unknown();
            };
            satisfied &= actual_max >= min;
            matches.push(verified);
        }
        if let Some(max) = max {
            let Some(verified) = self.verified_match(
                property_id,
                society_entity_id,
                format!("price at most {max}"),
                "at_most",
                "inventory_option_price_min",
                actual_min as f64,
                "INR",
                snapshot_identity,
            ) else {
                return BooleanEvaluation::unknown();
            };
            satisfied &= actual_min <= max;
            matches.push(verified);
        }
        if matches.is_empty() {
            return BooleanEvaluation::unknown();
        }
        if satisfied {
            BooleanEvaluation::satisfied(matches)
        } else {
            BooleanEvaluation::unsatisfied_with(matches)
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn verified_match(
        &self,
        property_id: &str,
        society_entity_id: &str,
        predicate: String,
        relation: &str,
        metric: &str,
        value: f64,
        unit: &str,
        snapshot_identity: &str,
    ) -> Option<VerifiedMatch> {
        if self.property_id != property_id || self.society_id != society_entity_id {
            return None;
        }
        let evidence_reference = self.evidence_reference.as_ref()?;
        evidence_reference
            .validate_for(society_entity_id, snapshot_identity)
            .ok()?;
        Some(VerifiedMatch {
            constraint: None,
            subject_entity_id: society_entity_id.to_string(),
            target_entity_id: Some(property_id.to_string()),
            predicate,
            relation: relation.to_string(),
            metric: metric.to_string(),
            value: Some(value),
            unit: Some(unit.to_string()),
            observation_ids: Vec::new(),
            evidence_refs: vec![evidence_reference.clone()],
            fact_key: self.evidence_fact_key.clone(),
            derived_evidence: None,
            algorithm_version: "inventory-option-evaluator-v2".to_string(),
            confidence: Some(self.confidence),
            snapshot_identity: snapshot_identity.to_string(),
        })
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct InventoryObservationValue {
    #[serde(default)]
    property_id: Option<String>,
    #[serde(default)]
    listing_type: Option<String>,
    bhk: f64,
    #[serde(default)]
    price: Option<u64>,
    #[serde(default)]
    price_min: Option<u64>,
    #[serde(default)]
    price_max: Option<u64>,
    #[serde(default)]
    area_sqft: Option<u32>,
    #[serde(default)]
    area_type: Option<String>,
    #[serde(default)]
    area_sqft_min: Option<f64>,
    #[serde(default)]
    area_sqft_max: Option<f64>,
}

impl InventoryObservationValue {
    pub(crate) fn from_fact(
        fact: &crate::serving::ServingFactRecord,
    ) -> Result<Self, &'static str> {
        crate::serving::admission::admit_inventory_fact(fact)?;
        let FactValue::Text(encoded) = &fact.value else {
            return Err("invalid_inventory_record");
        };
        let value: Self = serde_json::from_str(encoded).map_err(|_| "invalid_inventory_record")?;
        value.validate()?;
        if !value.is_individual() {
            return Err("requires_consistent_individual_listing");
        }
        Ok(value)
    }
    fn into_option(self, property: &Property, society_entity_id: &str) -> Option<InventoryOption> {
        if !self.bhk.is_finite() || self.bhk.fract().abs() > f64::EPSILON {
            return None;
        }
        let bhk = self.bhk as u32;
        let exact_price = self.price.filter(|value| *value > 0);
        let price_min = self.price_min.or(exact_price);
        let price_max = self.price_max.or(exact_price);
        let option = InventoryOption {
            confidence: 0.0,
            property_id: property.id.clone(),
            society_id: society_entity_id.to_string(),
            bhk: Some(bhk),
            price_min,
            price_max,
            size_sqft: self.area_sqft.filter(|value| *value > 0),
            area_measurement: None,
            evidence_reference: None,
            evidence_fact_key: None,
        };
        (option.bhk == (property.bhk > 0).then_some(property.bhk)).then_some(option)
    }

    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !self.bhk.is_finite() || self.bhk <= 0.0 || self.bhk.fract() != 0.0 {
            return Err("inventory bedrooms must be a positive integer");
        }
        for (value, min, max) in [
            (
                self.price.map(|v| v as f64),
                self.price_min.map(|v| v as f64),
                self.price_max.map(|v| v as f64),
            ),
            (
                self.area_sqft.map(|v| v as f64),
                self.area_sqft_min,
                self.area_sqft_max,
            ),
        ] {
            if [value, min, max]
                .into_iter()
                .flatten()
                .any(|v| !v.is_finite() || v <= 0.0)
                || min.zip(max).is_some_and(|(lo, hi)| lo > hi)
                || value.zip(min).is_some_and(|(v, lo)| v < lo)
                || value.zip(max).is_some_and(|(v, hi)| v > hi)
            {
                return Err("inventory value is outside its observed range");
            }
        }
        Ok(())
    }

    pub(crate) fn is_individual(&self) -> bool {
        // A project/configuration range cannot bind attributes to the same listing observation.
        static ELIGIBILITY: std::sync::OnceLock<crate::dag_config::ServingEligibilityFile> =
            std::sync::OnceLock::new();
        let eligibility = ELIGIBILITY.get_or_init(|| {
            crate::dag_config::load_serving_eligibility()
                .expect("serving eligibility must validate before inventory admission")
        });
        self.listing_type.as_ref().is_some_and(|kind| {
            eligibility
                .inventory_listing_types
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(kind))
        }) && self.price.is_some()
            && self.price_min.is_none_or(|v| Some(v) == self.price)
            && self.price_max.is_none_or(|v| Some(v) == self.price)
            && self
                .area_sqft_min
                .is_none_or(|v| Some(v) == self.area_sqft.map(f64::from))
            && self
                .area_sqft_max
                .is_none_or(|v| Some(v) == self.area_sqft.map(f64::from))
    }
}

/// Evidence produced by exact predicate evaluation after candidate recall.
/// Ranking and proof projections must use this record rather than re-deriving
/// a claim from query text or recall membership.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedMatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraint: Option<super::intent::HardConstraint>,
    pub subject_entity_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_entity_id: Option<String>,
    pub predicate: String,
    pub relation: String,
    pub metric: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observation_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_refs: Vec<EvidenceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derived_evidence: Option<DerivedEvidence>,
    pub algorithm_version: String,
    pub confidence: Option<f32>,
    pub snapshot_identity: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationEvidence {
    pub predicate: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceGap {
    pub predicate: String,
    pub reason: String,
}

/// Four-state predicate result. `Unknown` and `Unsupported` are deliberately
/// not booleans: negation must preserve both states instead of turning missing
/// evidence into a verified match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", content = "detail", rename_all = "camelCase")]
#[allow(clippy::large_enum_variant)]
pub enum PredicateEvaluation {
    Satisfied(VerifiedMatch),
    Unsatisfied(EvaluationEvidence),
    Unknown(EvidenceGap),
    Unsupported(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BooleanEvaluation {
    pub state: EvaluationState,
    pub verified_matches: Vec<VerifiedMatch>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvaluationState {
    Satisfied,
    Unsatisfied,
    Unknown,
    Unsupported,
}

impl BooleanEvaluation {
    pub fn satisfied(matches: Vec<VerifiedMatch>) -> Self {
        Self {
            state: EvaluationState::Satisfied,
            verified_matches: matches,
        }
    }

    pub fn from_predicate(evaluation: PredicateEvaluation) -> Self {
        match evaluation {
            PredicateEvaluation::Satisfied(verified) => Self::satisfied(vec![verified]),
            PredicateEvaluation::Unsatisfied(_) => Self {
                state: EvaluationState::Unsatisfied,
                verified_matches: Vec::new(),
            },
            PredicateEvaluation::Unknown(_) => Self {
                state: EvaluationState::Unknown,
                verified_matches: Vec::new(),
            },
            PredicateEvaluation::Unsupported(_) => Self {
                state: EvaluationState::Unsupported,
                verified_matches: Vec::new(),
            },
        }
    }

    pub fn unsatisfied() -> Self {
        Self::from_state(EvaluationState::Unsatisfied)
    }

    pub fn unsatisfied_with(matches: Vec<VerifiedMatch>) -> Self {
        Self {
            state: EvaluationState::Unsatisfied,
            verified_matches: matches,
        }
    }

    pub fn unknown() -> Self {
        Self::from_state(EvaluationState::Unknown)
    }

    pub fn unsupported() -> Self {
        Self::from_state(EvaluationState::Unsupported)
    }

    fn from_state(state: EvaluationState) -> Self {
        Self {
            state,
            verified_matches: Vec::new(),
        }
    }

    pub fn is_satisfied(&self) -> bool {
        self.state == EvaluationState::Satisfied
    }

    pub fn negated(mut self) -> Self {
        self.state = match self.state {
            EvaluationState::Satisfied => EvaluationState::Unsatisfied,
            EvaluationState::Unsatisfied => EvaluationState::Satisfied,
            EvaluationState::Unknown => EvaluationState::Unknown,
            EvaluationState::Unsupported => EvaluationState::Unsupported,
        };
        if self.state != EvaluationState::Satisfied {
            self.verified_matches.clear();
        }
        self
    }

    pub fn all(evaluations: impl IntoIterator<Item = Self>) -> Self {
        combine(evaluations, true)
    }

    pub fn any(evaluations: impl IntoIterator<Item = Self>) -> Self {
        combine(evaluations, false)
    }
}

fn combine(
    evaluations: impl IntoIterator<Item = BooleanEvaluation>,
    all: bool,
) -> BooleanEvaluation {
    let evaluations = evaluations.into_iter().collect::<Vec<_>>();
    if evaluations.is_empty() {
        return BooleanEvaluation {
            state: if all {
                EvaluationState::Satisfied
            } else {
                EvaluationState::Unsatisfied
            },
            verified_matches: Vec::new(),
        };
    }

    let decisive = if all {
        evaluations
            .iter()
            .find(|evaluation| evaluation.state == EvaluationState::Unsatisfied)
            .map(|_| EvaluationState::Unsatisfied)
    } else {
        evaluations
            .iter()
            .find(|evaluation| evaluation.state == EvaluationState::Satisfied)
            .map(|_| EvaluationState::Satisfied)
    };
    let state = decisive.unwrap_or_else(|| {
        if evaluations
            .iter()
            .any(|evaluation| evaluation.state == EvaluationState::Unknown)
        {
            EvaluationState::Unknown
        } else if evaluations
            .iter()
            .any(|evaluation| evaluation.state == EvaluationState::Unsupported)
        {
            EvaluationState::Unsupported
        } else if all {
            EvaluationState::Satisfied
        } else {
            EvaluationState::Unsatisfied
        }
    });
    let verified_matches = if state == EvaluationState::Satisfied {
        evaluations
            .into_iter()
            .filter(|evaluation| all || evaluation.state == EvaluationState::Satisfied)
            .flat_map(|evaluation| evaluation.verified_matches)
            .collect()
    } else {
        Vec::new()
    };
    BooleanEvaluation {
        state,
        verified_matches,
    }
}

#[cfg(test)]
mod inventory_tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::serving::{ServingFactRecord, SourceObservation};

    fn property() -> Property {
        Property {
            id: "property:one".to_string(),
            title: "One".to_string(),
            area: "Test Area".to_string(),
            area_id: "area:test".to_string(),
            city: "Bengaluru".to_string(),
            society_id: "society:one".to_string(),
            builder_name: "Builder".to_string(),
            property_type: "Apartment".to_string(),
            listing_type: "Resale".to_string(),
            bhk: 3,
            price: 10_000_000,
            price_min: None,
            price_max: None,
            price_per_sqft: 10_000,
            carpet_area_sqft: 900,
            super_builtup_sqft: 1_000,
            area_measurement: None,
            floor: 1,
            total_floors: 10,
            facing: "East".to_string(),
            possession_status: "Ready".to_string(),
            metro_distance_mins: 10,
            maintenance_cost_monthly: 5_000,
            society_quality_score: None,
            builder_quality_score: None,
            document_completeness_score: None,
            litigation_risk: None,
            noise_score: None,
            sunlight_score: None,
            airport_noise_score: None,
            waterlogging_risk_score: None,
            traffic_score: None,
            days_on_market: 1,
            greenery_score: None,
            open_space_score: None,
            resale_strength_score: None,
            interest_level: None,
            saves_last_7d: None,
            offers_last_7d: None,
            images: Vec::new(),
            hero_image: String::new(),
            description_summary: String::new(),

            source_reference: "test".to_string(),
        }
    }

    fn observation(subject: &str, provider_id: &str) -> SourceObservation {
        SourceObservation::new(
            "FixtureProvider",
            provider_id,
            subject,
            Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            Some(format!("https://example.test/{provider_id}")),
            vec!["asset:external-listings/v1".to_string()],
        )
        .unwrap()
    }

    fn inventory_fact(
        entity_id: &str,
        payload: &str,
        observation: Option<SourceObservation>,
    ) -> ServingFactRecord {
        ServingFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: "listing_3bhk".to_string(),
            value_type: "text".to_string(),
            value_text: Some(payload.to_string()),
            value: FactValue::Text(payload.to_string()),
            confidence: 0.9,
            source_type: "ExternalListing".to_string(),
            source_url: Some("https://example.test/listing".to_string()),
            model: None,
            skill_id: Some("listing_3bhk".to_string()),
            learned_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            observation,
        }
    }

    #[test]
    fn inventory_selection_is_row_order_invariant_and_snapshot_qualified() {
        let property = property();
        let subject = "society:one";
        let payload = r#"{"listing_type":"sale","bhk":3,"price":10000000,"area_sqft":1000}"#;
        let first = inventory_fact(subject, payload, Some(observation(subject, "listing-a")));
        let second = inventory_fact(subject, payload, Some(observation(subject, "listing-b")));
        let forward =
            ServingFactIndex::from_records(vec![first.clone(), second.clone()], Vec::new());
        let reverse = ServingFactIndex::from_records(vec![second, first], Vec::new());

        let forward_option =
            InventoryOption::from_serving_observation(&property, subject, &forward, "bundle:v9")
                .unwrap();
        let reverse_option =
            InventoryOption::from_serving_observation(&property, subject, &reverse, "bundle:v9")
                .unwrap();

        assert_eq!(forward_option, reverse_option);
        assert_eq!(
            forward_option.evidence_fact_key.as_deref(),
            Some("listing_3bhk")
        );
        let evaluation = forward_option.evaluate_bhk("property:one", subject, 3, "bundle:v9");
        assert_eq!(
            evaluation.verified_matches[0].fact_key.as_deref(),
            Some("listing_3bhk")
        );
        let evidence = forward_option.evidence_reference.unwrap();
        assert!(evidence.validate_for(subject, "bundle:v9").is_ok());
    }

    #[test]
    fn inconsistent_measurements_and_project_ranges_are_not_inventory() {
        let subject = "society:one";
        for payload in [
            r#"{"listing_type":"sale","bhk":3,"price":10000000,"area_sqft":1112,"area_sqft_min":742,"area_sqft_max":764,"area_type":"carpet"}"#,
            r#"{"listing_type":"sale","bhk":3,"price":10000000,"price_min":10000000,"price_max":30000000,"area_sqft":1500,"area_sqft_min":1000,"area_sqft_max":2000}"#,
        ] {
            let mut property = property();
            let value: serde_json::Value = serde_json::from_str(payload).unwrap();
            property.price_min = value["price_min"].as_u64();
            property.price_max = value["price_max"].as_u64();
            let facts = ServingFactIndex::from_records(
                vec![inventory_fact(
                    subject,
                    payload,
                    Some(observation(subject, "invalid-listing")),
                )],
                Vec::new(),
            );
            assert!(
                InventoryOption::from_serving_observation(&property, subject, &facts, "bundle:v9")
                    .is_none(),
                "{payload}"
            );
        }
    }

    #[test]
    fn legacy_and_cross_subject_inventory_facts_cannot_verify() {
        let property = property();
        let subject = "society:one";
        let payload = r#"{"listing_type":"sale","bhk":3,"price":10000000}"#;
        let facts = ServingFactIndex::from_records(
            vec![
                inventory_fact(subject, payload, None),
                inventory_fact(
                    subject,
                    payload,
                    Some(observation("society:other", "listing-cross-subject")),
                ),
            ],
            Vec::new(),
        );

        assert!(
            InventoryOption::from_serving_observation(&property, subject, &facts, "bundle:v9")
                .is_none()
        );
    }

    #[test]
    fn bhk_and_price_must_coexist_on_the_same_observation() {
        let property = property();
        let subject = "society:one";
        let facts = ServingFactIndex::from_records(
            vec![
                inventory_fact(
                    subject,
                    r#"{"listing_type":"sale","bhk":3,"price":11000000}"#,
                    Some(observation(subject, "listing-right-bhk")),
                ),
                inventory_fact(
                    subject,
                    r#"{"listing_type":"sale","bhk":2,"price":10000000}"#,
                    Some(observation(subject, "listing-right-price")),
                ),
            ],
            Vec::new(),
        );

        let option =
            InventoryOption::from_serving_observation(&property, subject, &facts, "bundle:v9")
                .unwrap();
        assert_eq!(option.bhk, Some(3));
        assert_eq!(option.price_min, Some(11_000_000));
        assert_eq!(
            option
                .evaluate_budget(&property.id, subject, None, Some(10_000_000), "bundle:v9")
                .state,
            EvaluationState::Unsatisfied
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(state: EvaluationState) -> BooleanEvaluation {
        BooleanEvaluation {
            state,
            verified_matches: Vec::new(),
        }
    }

    #[test]
    fn negation_preserves_unknown_and_unsupported() {
        assert_eq!(
            state(EvaluationState::Unknown).negated().state,
            EvaluationState::Unknown
        );
        assert_eq!(
            state(EvaluationState::Unsupported).negated().state,
            EvaluationState::Unsupported
        );
    }

    #[test]
    fn boolean_composition_is_fail_closed() {
        assert_eq!(
            BooleanEvaluation::all([
                state(EvaluationState::Satisfied),
                state(EvaluationState::Unknown),
            ])
            .state,
            EvaluationState::Unknown
        );
        assert_eq!(
            BooleanEvaluation::any([
                state(EvaluationState::Unsatisfied),
                state(EvaluationState::Unknown),
            ])
            .state,
            EvaluationState::Unknown
        );
    }
}
