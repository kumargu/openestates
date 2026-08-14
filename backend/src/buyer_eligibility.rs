use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::dag_config::{buyer_eligibility_config, BuyerEligibilityFile};
use crate::models::Property;

pub const DISCOVERY_SURFACE: &str = "discovery";
pub const SEARCH_SURFACE: &str = "search";
pub const RECOMMENDATIONS_SURFACE: &str = "recommendations";
pub const DETAIL_SURFACE: &str = "detail";
pub const COMPARE_SURFACE: &str = "compare";
pub const PLAN_SURFACE: &str = "plan";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuyerEligibility {
    pub policy_version: u32,
    pub surfaces: BTreeMap<String, BuyerEligibilityDecision>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observed_reasons: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuyerEligibilityDecision {
    pub eligible: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reason_codes: Vec<String>,
}

impl BuyerEligibility {
    pub fn decision(&self, surface: &str) -> Option<&BuyerEligibilityDecision> {
        self.surfaces.get(surface)
    }

    pub fn eligible_for(&self, surface: &str) -> bool {
        self.decision(surface)
            .is_some_and(|decision| decision.eligible)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BuyerEligibilitySignals {
    pub identity: bool,
    pub area: bool,
    pub price: bool,
    pub configuration: bool,
    pub lifecycle: bool,
    pub trusted_media: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RegulatoryStatus(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LifecycleStatus(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PossessionStatus(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PossessionTiming(pub String);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusValidationState {
    #[default]
    Missing,
    Supported,
    Conflict,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regulatory: Option<RegulatoryStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<LifecycleStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub possession: Option<PossessionStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub possession_timing: Option<PossessionTiming>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub age_display: Option<String>,
    pub validation_state: StatusValidationState,
}

pub fn evaluate_property(property: &Property) -> BuyerEligibility {
    evaluate_signals(property_signals(property))
}

pub fn evaluate_signals(signals: BuyerEligibilitySignals) -> BuyerEligibility {
    let policy = buyer_eligibility_config().expect("buyer eligibility config must be valid");
    evaluate_signals_with_policy(signals, policy)
}

pub fn evaluate_signals_with_policy(
    signals: BuyerEligibilitySignals,
    policy: &BuyerEligibilityFile,
) -> BuyerEligibility {
    let signals = signals.as_map();
    let surfaces = policy
        .surfaces
        .iter()
        .map(|(surface, surface_policy)| {
            let reason_codes = surface_policy
                .required
                .iter()
                .filter(|field| !signals.get(field.as_str()).copied().unwrap_or(false))
                .filter_map(|field| policy.requirements.get(field))
                .map(|requirement| requirement.reason_code.clone())
                .collect::<Vec<_>>();
            (
                surface.clone(),
                BuyerEligibilityDecision {
                    eligible: reason_codes.is_empty(),
                    reason_codes,
                },
            )
        })
        .collect();
    let observed_reasons = policy
        .observed
        .iter()
        .filter(|field| !signals.get(field.as_str()).copied().unwrap_or(false))
        .filter_map(|field| policy.requirements.get(field))
        .map(|requirement| requirement.reason_code.clone())
        .collect();

    BuyerEligibility {
        policy_version: policy.version,
        surfaces,
        observed_reasons,
    }
}

fn property_signals(property: &Property) -> BuyerEligibilitySignals {
    BuyerEligibilitySignals {
        identity: !property.title.trim().is_empty(),
        area: !property.area.trim().is_empty(),
        price: property.price > 0,
        configuration: property.bhk > 0,
        lifecycle: property.status.lifecycle.is_some(),
        trusted_media: property.media.iter().any(|asset| asset.hero_eligible),
    }
}

impl BuyerEligibilitySignals {
    pub const fn complete_without_media() -> Self {
        Self {
            identity: true,
            area: true,
            price: true,
            configuration: true,
            lifecycle: true,
            trusted_media: false,
        }
    }

    fn as_map(self) -> BTreeMap<&'static str, bool> {
        BTreeMap::from([
            ("identity", self.identity),
            ("area", self.area),
            ("price", self.price),
            ("configuration", self.configuration),
            ("lifecycle", self.lifecycle),
            ("trusted_media", self.trusted_media),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluator_uses_typed_required_fields_and_keeps_observed_gaps_non_blocking() {
        let mut property = complete_property();
        property.status.lifecycle = None;

        let eligibility = evaluate_property(&property);
        assert!(eligibility.eligible_for(SEARCH_SURFACE));
        assert_eq!(
            eligibility.observed_reasons,
            ["missing_lifecycle", "missing_trusted_media"]
        );

        property.area.clear();
        property.price = 0;
        let eligibility = evaluate_property(&property);
        let search = eligibility
            .decision(SEARCH_SURFACE)
            .expect("search decision");
        assert!(!search.eligible);
        assert_eq!(search.reason_codes, ["missing_area", "missing_price"]);
    }

    fn complete_property() -> Property {
        Property {
            id: "eligible-3bhk".to_string(),
            title: "3 BHK in Eligible Home".to_string(),
            area: "Whitefield".to_string(),
            area_id: "area-whitefield".to_string(),
            city: "Bengaluru".to_string(),
            society_id: "eligible-home".to_string(),
            builder_name: String::new(),
            property_type: "Apartment".to_string(),
            listing_type: "Resale".to_string(),
            bhk: 3,
            price: 15_000_000,
            price_per_sqft: 10_000,
            carpet_area_sqft: 1_200,
            super_builtup_sqft: 1_500,
            floor: 0,
            total_floors: 0,
            facing: String::new(),
            possession_status: String::new(),
            status: PropertyStatus {
                lifecycle: Some(LifecycleStatus("ready_to_move".to_string())),
                ..PropertyStatus::default()
            },
            buyer_eligibility: BuyerEligibility::default(),
            metro_distance_mins: 0,
            maintenance_cost_monthly: 0,
            society_quality_score: None,
            builder_quality_score: None,
            document_completeness_score: None,
            litigation_risk: None,
            noise_score: None,
            sunlight_score: None,
            airport_noise_score: None,
            waterlogging_risk_score: None,
            traffic_score: None,
            days_on_market: 0,
            greenery_score: None,
            open_space_score: None,
            resale_strength_score: None,
            interest_level: None,
            saves_last_7d: None,
            offers_last_7d: None,
            images: Vec::new(),
            hero_image: String::new(),
            media: Vec::new(),
            description_summary: String::new(),
            transparency_tags: Vec::new(),
            source_reference: String::new(),
        }
    }
}
