//! Buyer-facing project plan media (site overview + floor plans).
//!
//! Offline promotion writes:
//! - preview images under `data/lake/media/previews/rera_plans/{slug}/`
//! - structured payload at `data/lake/media/rera_plans/{slug}/project_plan_frames.json`
//!
//! Serving facts may also carry the same JSON under `media.project_plan_frames`.
//! Request handlers never download or OCR plan PDFs.

use serde::{Deserialize, Serialize};

use crate::knowledge::FactValue;
use crate::serving::ServingFactIndex;

pub const PROJECT_PLAN_FRAMES_FACT_KEY: &str = "media.project_plan_frames";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectPlansView {
    pub provider: String,
    pub coverage_quality: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registration_number: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site_overview: Option<SiteOverviewPlan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub floor_plans: Vec<FloorPlanVariant>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SiteOverviewPlan {
    pub artifact_id: String,
    pub label: String,
    pub preview_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FloorPlanVariant {
    pub id: String,
    pub artifact_id: String,
    pub configuration_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_type_label: Option<String>,
    pub bedroom_count: u32,
    pub tab_label: String,
    pub title: String,
    pub preview_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carpet_area_sqft: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carpet_area_sqm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sale_area_sqft: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sale_area_sqm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usable_area_ratio: Option<f64>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct ProjectPlanFramesRecord {
    provider: String,
    coverage_quality: String,
    #[serde(default)]
    source_url: Option<String>,
    #[serde(default)]
    registration_number: Option<String>,
    #[serde(default)]
    society_entity_id: Option<String>,
    #[serde(default)]
    site_overview: Option<SiteOverviewPlan>,
    #[serde(default)]
    floor_plans: Vec<FloorPlanVariant>,
}

/// Resolve buyer plan media for a society from the promoted serving bundle.
pub fn project_plans_for_society(
    society_entity_id: &str,
    _society_id: &str,
    serving_facts: Option<&ServingFactIndex>,
) -> Option<ProjectPlansView> {
    serving_plans(society_entity_id, serving_facts)
}

/// Pick the representative floor plan for a listing BHK on compare cards.
pub fn matched_floor_plan_for_bhk(plans: &ProjectPlansView, bhk: u32) -> Option<&FloorPlanVariant> {
    matched_floor_plan_for_listing(plans, bhk, None)
}

/// Prefer the plan whose carpet is closest to the listing when multiple BHK matches exist.
pub fn matched_floor_plan_for_listing(
    plans: &ProjectPlansView,
    bhk: u32,
    listing_carpet_sqft: Option<u32>,
) -> Option<&FloorPlanVariant> {
    let mut matches: Vec<&FloorPlanVariant> = plans
        .floor_plans
        .iter()
        .filter(|plan| plan.bedroom_count == bhk)
        .collect();
    if matches.is_empty() {
        return None;
    }
    matches.sort_by(|left, right| {
        carpet_distance(left, listing_carpet_sqft)
            .cmp(&carpet_distance(right, listing_carpet_sqft))
            .then_with(|| {
                right
                    .confidence
                    .partial_cmp(&left.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.id.cmp(&right.id))
    });
    matches.first().copied()
}

fn carpet_distance(plan: &FloorPlanVariant, listing_carpet_sqft: Option<u32>) -> u32 {
    let Some(target) = listing_carpet_sqft.filter(|value| *value > 0) else {
        return u32::MAX / 4;
    };
    let Some(plan_carpet) = plan.carpet_area_sqft else {
        return u32::MAX / 2;
    };
    target.abs_diff(plan_carpet)
}

/// Attach compare-ready plan fields to a property card when available.
pub fn overlay_project_plans_on_card(
    card: &mut crate::models::PropertyCard,
    society_id: &str,
    serving_facts: Option<&ServingFactIndex>,
) {
    let society_entity_id = normalize_society_entity_id(society_id);
    let Some(plans) = project_plans_for_society(&society_entity_id, society_id, serving_facts)
    else {
        return;
    };
    let Some(matched) =
        matched_floor_plan_for_listing(&plans, card.bhk, Some(card.carpet_area_sqft))
    else {
        return;
    };
    card.floor_plan_preview_url = Some(matched.preview_url.clone());
    card.plan_carpet_area_sqft = matched.carpet_area_sqft;
    card.plan_sale_area_sqft = matched.sale_area_sqft;
    card.plan_configuration_type = Some(matched.configuration_type.clone());
}

fn normalize_society_entity_id(society_id: &str) -> String {
    let normalized = society_id
        .trim()
        .to_ascii_lowercase()
        .replace(['_', ' '], "-");
    if normalized.starts_with("society:") {
        normalized
    } else {
        let slug = normalized
            .strip_prefix("soc-")
            .unwrap_or(normalized.as_str());
        format!("society:{}", society_slug(slug, slug))
    }
}

fn society_slug(society_entity_id: &str, society_id: &str) -> String {
    let raw = society_entity_id
        .strip_prefix("society:")
        .unwrap_or(society_id);
    let raw = raw.strip_prefix("soc-").unwrap_or(raw);
    raw.trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn serving_plans(
    society_entity_id: &str,
    serving_facts: Option<&ServingFactIndex>,
) -> Option<ProjectPlansView> {
    let facts = serving_facts?;
    let entity = facts.entity(society_entity_id)?;
    let fact = entity
        .facts
        .iter()
        .find(|fact| fact.fact_key == PROJECT_PLAN_FRAMES_FACT_KEY)?;
    if !fact.source_type.eq_ignore_ascii_case("rera") {
        return None;
    }
    let payload = match &fact.value {
        FactValue::Text(text) => text.as_str(),
        _ => return None,
    };
    parse_plans_payload(payload)
}

fn parse_plans_payload(payload: &str) -> Option<ProjectPlansView> {
    let record: ProjectPlanFramesRecord = serde_json::from_str(payload).ok()?;
    if !record.provider.eq_ignore_ascii_case("rera") {
        return None;
    }
    if record.site_overview.is_none() && record.floor_plans.is_empty() {
        return None;
    }
    Some(record_to_view(record))
}

fn record_to_view(record: ProjectPlanFramesRecord) -> ProjectPlansView {
    ProjectPlansView {
        provider: record.provider,
        coverage_quality: record.coverage_quality,
        source_url: record.source_url,
        registration_number: record.registration_number,
        site_overview: record.site_overview,
        floor_plans: record.floor_plans,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    use crate::serving::{ServingFactIndex, ServingFactRecord};

    fn minimal_payload(provider: &str) -> String {
        serde_json::json!({
            "provider": provider,
            "coverage_quality": "usable",
            "source_url": "https://rera.test/source",
            "registration_number": "PRM-1",
            "society_entity_id": "society:test",
            "floor_plans": [
                {
                    "id": "type-a",
                    "artifact_id": "test:floor_plan:a",
                    "configuration_type": "3BHK",
                    "bedroom_count": 3,
                    "tab_label": "3 BHK",
                    "title": "Type A 3 BHK",
                    "preview_url": "/media/previews/rera_plans/test/type-a.png",
                    "source_url": "https://rera.test/source",
                    "carpet_area_sqft": 1200,
                    "sale_area_sqft": 1800,
                    "usable_area_ratio": 0.667,
                    "confidence": 0.86
                }
            ]
        })
        .to_string()
    }

    fn serving_index(payload: String, source_type: &str) -> ServingFactIndex {
        ServingFactIndex::from_records(
            vec![ServingFactRecord {
                entity_id: "society:test".to_string(),
                fact_key: PROJECT_PLAN_FRAMES_FACT_KEY.to_string(),
                value_type: "Text".to_string(),
                value_text: Some(payload.clone()),
                value: FactValue::Text(payload),
                confidence: 0.86,
                source_type: source_type.to_string(),
                source_url: Some("https://rera.test/source".to_string()),
                model: None,
                skill_id: Some("promote_rera_project_plans".to_string()),
                learned_at: Utc::now(),
            }],
            Vec::new(),
        )
    }

    #[test]
    fn matches_highest_confidence_plan_for_bhk() {
        let plans = ProjectPlansView {
            provider: "RERA".to_string(),
            coverage_quality: "usable".to_string(),
            source_url: None,
            registration_number: None,
            site_overview: None,
            floor_plans: vec![
                FloorPlanVariant {
                    id: "compact".to_string(),
                    artifact_id: "a".to_string(),
                    configuration_type: "3BHK".to_string(),
                    unit_type_label: Some("B1".to_string()),
                    bedroom_count: 3,
                    tab_label: "3 bed compact".to_string(),
                    title: "B1".to_string(),
                    preview_url: "/media/a.png".to_string(),
                    thumbnail_url: None,
                    source_url: None,
                    page: Some(14),
                    carpet_area_sqft: Some(1197),
                    carpet_area_sqm: None,
                    sale_area_sqft: Some(1775),
                    sale_area_sqm: None,
                    usable_area_ratio: Some(0.675),
                    confidence: 0.8,
                },
                FloorPlanVariant {
                    id: "large".to_string(),
                    artifact_id: "b".to_string(),
                    configuration_type: "3BHK".to_string(),
                    unit_type_label: Some("C2".to_string()),
                    bedroom_count: 3,
                    tab_label: "3 bed large".to_string(),
                    title: "C2".to_string(),
                    preview_url: "/media/b.png".to_string(),
                    thumbnail_url: None,
                    source_url: None,
                    page: Some(22),
                    carpet_area_sqft: Some(1382),
                    carpet_area_sqm: None,
                    sale_area_sqft: Some(2027),
                    sale_area_sqm: None,
                    usable_area_ratio: Some(0.682),
                    confidence: 0.9,
                },
            ],
        };

        let matched = matched_floor_plan_for_bhk(&plans, 3).expect("match");
        assert_eq!(matched.id, "large");
    }

    #[test]
    fn parses_serving_project_plan_frames_fact() {
        let serving = serving_index(minimal_payload("RERA"), "Rera");

        let view = project_plans_for_society("society:test", "test", Some(&serving))
            .expect("serving fact should parse");

        assert_eq!(view.provider, "RERA");
        assert_eq!(view.floor_plans.len(), 1);
        assert_eq!(
            view.floor_plans[0].preview_url,
            "/media/previews/rera_plans/test/type-a.png"
        );
    }

    #[test]
    fn rejects_non_rera_project_plan_payloads() {
        assert!(parse_plans_payload(&minimal_payload("manual")).is_none());

        let serving = serving_index(minimal_payload("RERA"), "Manual");
        assert!(project_plans_for_society("society:test", "test", Some(&serving)).is_none());
    }

    #[test]
    fn ignores_empty_rera_project_plan_payloads() {
        let payload = serde_json::json!({
            "provider": "RERA",
            "coverage_quality": "missing",
            "source_url": "https://rera.test/source",
            "registration_number": "PRM-1",
            "society_entity_id": "society:test",
            "floor_plans": []
        })
        .to_string();

        assert!(parse_plans_payload(&payload).is_none());
    }
}
