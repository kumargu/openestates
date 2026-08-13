use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::dag_config::{
    rera_decision_labels_config, ReraDecisionLabelCondition, ReraDecisionLabelDefinition,
    ReraDecisionLabelSource,
};
use crate::knowledge::FactValue;
use crate::serving::{ServingFactIndex, SocietyFactProjection};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionLabel {
    pub key: String,
    pub label: String,
    pub severity: String,
    pub scope: String,
    pub visual_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surfaces: Vec<String>,
    pub priority: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_fact_keys: Vec<String>,
    pub confidence: f32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notebook_labels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compare_group: Option<String>,
    pub group_id: String,
    pub placement: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionCheckSummary {
    pub tile_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tile_caption: Option<String>,
    pub tone: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_number_compact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry_url: Option<String>,
    pub primary_count: usize,
    pub total_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub primary_labels: Vec<DecisionLabel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<DecisionLabelGroup>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionLabelGroup {
    pub id: String,
    pub title: String,
    pub labels: Vec<DecisionLabel>,
}

#[derive(Debug, Clone, PartialEq)]
struct LabelValue {
    numeric: Option<f64>,
    bool_value: Option<bool>,
    text: Option<String>,
    confidence: f32,
    source_fact_keys: Vec<String>,
}

pub fn rera_decision_labels_for_society(
    serving_facts: &ServingFactIndex,
    society_id: &str,
) -> Vec<DecisionLabel> {
    let Ok(config) = rera_decision_labels_config() else {
        return Vec::new();
    };
    let projection = SocietyFactProjection::from_index(serving_facts, society_id);
    let mut labels = config
        .labels
        .iter()
        .filter_map(|definition| label_for_definition(definition, &projection))
        .collect::<Vec<_>>();
    labels.sort_by(label_order);
    labels
}

pub fn rera_decision_check_summary_for_society(
    serving_facts: &ServingFactIndex,
    society_id: &str,
) -> Option<DecisionCheckSummary> {
    let Ok(config) = rera_decision_labels_config() else {
        return None;
    };
    let projection = SocietyFactProjection::from_index(serving_facts, society_id);
    let labels = rera_decision_labels_for_society(serving_facts, society_id);
    let registration_number = registration_number(&projection);

    if labels.is_empty() && registration_number.is_none() {
        return None;
    }

    let attention_count = labels
        .iter()
        .filter(|label| matches!(label.severity.as_str(), "risk" | "caution"))
        .count();
    let has_attention = attention_count > 0;
    let primary_labels = labels
        .iter()
        .filter(|label| {
            label.placement == "primary" || (!has_attention && label.severity == "positive")
        })
        .filter(|label| !has_attention || label.severity != "positive")
        .take(config.summary.primary_limit)
        .cloned()
        .collect::<Vec<_>>();
    let groups = config
        .summary
        .groups
        .iter()
        .filter_map(|group| {
            let group_labels = labels
                .iter()
                .filter(|label| label.group_id == group.id)
                .cloned()
                .collect::<Vec<_>>();
            (!group_labels.is_empty()).then_some(DecisionLabelGroup {
                id: group.id.clone(),
                title: group.title.clone(),
                labels: group_labels,
            })
        })
        .collect::<Vec<_>>();

    let registration_number_compact = registration_number.as_deref().map(compact_rera_number);
    Some(DecisionCheckSummary {
        tile_label: config.summary.tile_label.clone(),
        tile_caption: None,
        tone: summary_tone(&labels),
        registration_number,
        registration_number_compact,
        registry_url: registry_url(&projection),
        primary_count: primary_labels.len(),
        total_count: labels.len(),
        primary_labels,
        groups,
    })
}

fn label_for_definition(
    definition: &ReraDecisionLabelDefinition,
    projection: &SocietyFactProjection<'_>,
) -> Option<DecisionLabel> {
    let value = value_for_source(&definition.source, projection)?;
    if !condition_matches(&definition.condition, &value) {
        return None;
    }

    let formatted_value = formatted_label_value(&value, definition.value_precision);
    let label = render_label_template(&definition.label_template, formatted_value.as_deref());
    Some(DecisionLabel {
        key: definition.key.clone(),
        label,
        severity: definition.severity.clone(),
        scope: definition.scope.clone(),
        visual_id: definition
            .visual_id
            .clone()
            .unwrap_or_else(|| definition.key.clone()),
        value: value.numeric,
        value_text: formatted_value.or(value.text),
        unit: definition.unit.clone(),
        surfaces: definition.surfaces.clone(),
        priority: definition.priority,
        source_fact_keys: value.source_fact_keys,
        confidence: value.confidence,
        notebook_labels: if definition.notebook_labels.is_empty() {
            vec![definition.key.clone()]
        } else {
            definition.notebook_labels.clone()
        },
        compare_group: definition.compare_group.clone(),
        group_id: definition.group_id.clone(),
        placement: definition.placement.clone(),
    })
}

fn value_for_source(
    source: &ReraDecisionLabelSource,
    projection: &SocietyFactProjection<'_>,
) -> Option<LabelValue> {
    match source {
        ReraDecisionLabelSource::Fact { fact_key } => value_for_fact(fact_key, projection),
        ReraDecisionLabelSource::FactAny { fact_keys } => fact_keys
            .iter()
            .find_map(|fact_key| value_for_fact(fact_key, projection)),
        ReraDecisionLabelSource::Ratio {
            numerator_fact_key,
            denominator_fact_key,
        } => ratio_value(numerator_fact_key, denominator_fact_key, projection),
    }
}

fn value_for_fact(fact_key: &str, projection: &SocietyFactProjection<'_>) -> Option<LabelValue> {
    let fact = projection.latest_record(fact_key)?;
    match &fact.value {
        FactValue::Numeric(value) if value.is_finite() => Some(LabelValue {
            numeric: Some(*value),
            bool_value: None,
            text: Some(format_numeric(*value, None)),
            confidence: fact.confidence,
            source_fact_keys: vec![fact.fact_key.clone()],
        }),
        FactValue::Bool(value) => Some(LabelValue {
            numeric: None,
            bool_value: Some(*value),
            text: Some(if *value { "yes" } else { "no" }.to_string()),
            confidence: fact.confidence,
            source_fact_keys: vec![fact.fact_key.clone()],
        }),
        FactValue::Text(value) if !value.trim().is_empty() => Some(LabelValue {
            numeric: None,
            bool_value: None,
            text: Some(value.trim().to_string()),
            confidence: fact.confidence,
            source_fact_keys: vec![fact.fact_key.clone()],
        }),
        FactValue::Score { value, .. } if value.is_finite() => Some(LabelValue {
            numeric: Some(*value),
            bool_value: None,
            text: Some(format_numeric(*value, None)),
            confidence: fact.confidence,
            source_fact_keys: vec![fact.fact_key.clone()],
        }),
        _ => None,
    }
}

fn ratio_value(
    numerator_fact_key: &str,
    denominator_fact_key: &str,
    projection: &SocietyFactProjection<'_>,
) -> Option<LabelValue> {
    let numerator = projection.latest_numeric(numerator_fact_key)?;
    let denominator = projection.latest_numeric(denominator_fact_key)?;
    if denominator.value <= 0.0 {
        return None;
    }
    let ratio = numerator.value / denominator.value;
    if !ratio.is_finite() {
        return None;
    }
    Some(LabelValue {
        numeric: Some(ratio),
        bool_value: None,
        text: Some(format_numeric(ratio, Some(2))),
        confidence: numerator.confidence.min(denominator.confidence),
        source_fact_keys: vec![
            numerator_fact_key.to_string(),
            denominator_fact_key.to_string(),
        ],
    })
}

fn condition_matches(condition: &ReraDecisionLabelCondition, value: &LabelValue) -> bool {
    if condition.present == Some(false) {
        return false;
    }
    if let Some(expected) = condition.eq_bool {
        return value.bool_value == Some(expected);
    }

    let has_numeric_condition = condition.gt.is_some()
        || condition.gte.is_some()
        || condition.lt.is_some()
        || condition.lte.is_some();
    if !has_numeric_condition {
        return condition.present.unwrap_or(true);
    }

    let Some(numeric) = value.numeric else {
        return false;
    };
    if condition.gt.is_some_and(|threshold| numeric <= threshold) {
        return false;
    }
    if condition.gte.is_some_and(|threshold| numeric < threshold) {
        return false;
    }
    if condition.lt.is_some_and(|threshold| numeric >= threshold) {
        return false;
    }
    if condition.lte.is_some_and(|threshold| numeric > threshold) {
        return false;
    }
    true
}

fn render_label_template(template: &str, value: Option<&str>) -> String {
    let rendered = match value {
        Some(value) => template.replace("{value}", value),
        None => template.replace("{value}", "").replace("  ", " "),
    };
    rendered.trim().to_string()
}

fn formatted_label_value(value: &LabelValue, precision: Option<u8>) -> Option<String> {
    value
        .numeric
        .map(|numeric| format_numeric(numeric, precision))
        .or_else(|| value.text.clone())
}

fn format_numeric(value: f64, precision: Option<u8>) -> String {
    let precision = precision.unwrap_or_else(|| {
        if value.fract().abs() < f64::EPSILON {
            0
        } else {
            1
        }
    });
    let formatted = format!("{value:.precision$}", precision = precision as usize);
    if precision == 0 {
        formatted
    } else {
        formatted
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn label_order(left: &DecisionLabel, right: &DecisionLabel) -> Ordering {
    right
        .priority
        .cmp(&left.priority)
        .then_with(|| severity_rank(&left.severity).cmp(&severity_rank(&right.severity)))
        .then_with(|| left.key.cmp(&right.key))
}

fn severity_rank(severity: &str) -> u8 {
    match severity {
        "risk" => 0,
        "caution" => 1,
        "positive" => 2,
        "info" => 3,
        _ => 4,
    }
}

fn registration_number(projection: &SocietyFactProjection<'_>) -> Option<String> {
    projection
        .latest_text("rera_number")
        .or_else(|| projection.latest_text("rera_registration_number"))
        .map(|fact| fact.value)
}

fn registry_url(projection: &SocietyFactProjection<'_>) -> Option<String> {
    projection
        .latest_text("rera_portal_url")
        .map(|fact| fact.value)
        .or_else(|| {
            projection
                .latest_record("rera_number")
                .and_then(|fact| fact.source_url.clone())
        })
        .or_else(|| {
            projection
                .latest_record("rera_registered")
                .and_then(|fact| fact.source_url.clone())
        })
}

fn compact_rera_number(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() <= 24 {
        return trimmed.to_string();
    }
    let parts = trimmed.split('/').collect::<Vec<_>>();
    if parts.len() >= 4 {
        return format!("{}/{}/.../{}", parts[0], parts[1], parts[parts.len() - 1]);
    }
    let prefix = trimmed.chars().take(12).collect::<String>();
    let suffix = trimmed
        .chars()
        .rev()
        .take(6)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("{prefix}...{suffix}")
}

fn summary_tone(labels: &[DecisionLabel]) -> String {
    if labels.iter().any(|label| label.severity == "risk") {
        "risk"
    } else if labels.iter().any(|label| label.severity == "caution") {
        "caution"
    } else if labels.iter().any(|label| label.severity == "positive") {
        "positive"
    } else {
        "neutral"
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::serving::{ServingFactRecord, ServingSearchMetadataRecord};

    #[test]
    fn derives_threshold_and_ratio_labels_from_serving_facts() {
        let index = index(vec![
            fact(
                "society:sample",
                "rera_delay_months",
                FactValue::Numeric(21.0),
            ),
            fact(
                "society:sample",
                "rera_land_litigation",
                FactValue::Bool(true),
            ),
            fact("society:sample", "rera_registered", FactValue::Bool(true)),
            fact(
                "society:sample",
                "rera_number",
                FactValue::Text("PRM/KA/RERA/1251/446/PR/200811/003528".to_string()),
            ),
            fact(
                "society:sample",
                "parking_total_car_count",
                FactValue::Numeric(63.0),
            ),
            fact(
                "society:sample",
                "project_unit_count",
                FactValue::Numeric(100.0),
            ),
        ]);

        let labels = rera_decision_labels_for_society(&index, "sample");
        let keys = labels
            .iter()
            .map(|label| label.key.as_str())
            .collect::<Vec<_>>();

        assert!(keys.contains(&"rera_land_litigation"));
        assert!(keys.contains(&"project_major_delay"));
        assert!(keys.contains(&"low_parking_coverage"));
        assert!(!keys.contains(&"rera_registration_available"));
        assert!(!keys.contains(&"project_delay"));
        assert_eq!(
            labels
                .iter()
                .find(|label| label.key == "low_parking_coverage")
                .map(|label| label.label.as_str()),
            Some("0.63 parking/home")
        );

        let summary = rera_decision_check_summary_for_society(&index, "sample")
            .expect("summary should be present");
        assert_eq!(summary.tile_label, "RERA");
        assert_eq!(summary.tone, "risk");
        assert_eq!(summary.primary_count, 2);
        assert_eq!(
            summary.registration_number_compact.as_deref(),
            Some("PRM/KA/.../003528")
        );
        assert!(summary.tile_caption.is_none());
        assert!(summary.groups.iter().any(|group| group.id == "attention"));
        assert!(summary
            .groups
            .iter()
            .any(|group| group.id == "project_facts"));
    }

    #[test]
    fn hides_low_signal_complaint_counts() {
        let index = index(vec![fact(
            "society:sample",
            "rera_project_complaints_count",
            FactValue::Numeric(1.0),
        )]);

        let labels = rera_decision_labels_for_society(&index, "sample");

        assert!(labels.is_empty());
    }

    #[test]
    fn legacy_complaint_totals_support_the_configured_project_fallback() {
        let index = index(vec![fact(
            "society:sample",
            "rera_complaints_count",
            FactValue::Numeric(8.0),
        )]);

        let labels = rera_decision_labels_for_society(&index, "sample");

        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].key, "project_high_complaints");
        assert_eq!(labels[0].label, "8 project complaints");
    }

    fn index(facts: Vec<ServingFactRecord>) -> ServingFactIndex {
        ServingFactIndex::from_records(facts, Vec::<ServingSearchMetadataRecord>::new())
    }

    fn fact(entity_id: &str, fact_key: &str, value: FactValue) -> ServingFactRecord {
        ServingFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            value_type: "test".to_string(),
            value_text: None,
            value,
            confidence: 0.9,
            source_type: "Rera".to_string(),
            source_url: Some("https://example.com/rera".to_string()),
            model: None,
            skill_id: Some("fetch_rera".to_string()),
            learned_at: chrono::Utc.timestamp_opt(1, 0).single().unwrap(),
        }
    }
}
