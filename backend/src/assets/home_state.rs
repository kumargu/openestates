use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::knowledge::FactValue;
use crate::lake::LakeStore;

use super::{
    read_skill_fact_artifact_rows, MaterializationId, MaterializationRecord,
    ProjectEnrichmentAssetError, SkillFactAnnotationRecord, SkillFactRecord, SkillFactsInput,
    SourceWatermark,
};

pub const HOME_STATE_SIGNALS_ASSET_ID: &str = "home_state_signals";

#[derive(Debug, Clone, Default)]
struct HomeStateSourceFacts {
    delay_months: Option<SourceNumericFact>,
    status: Option<SourceTextFact>,
}

#[derive(Debug, Clone)]
struct SourceTextFact {
    value: String,
    source_url: Option<String>,
    selection_key: String,
}

#[derive(Debug, Clone)]
struct SourceNumericFact {
    value: f64,
    source_url: Option<String>,
    selection_key: String,
}

pub async fn home_state_signals_input(
    lake: &LakeStore,
    source_records: &[MaterializationRecord],
    run_id: &MaterializationId,
    as_of: DateTime<Utc>,
) -> Result<SkillFactsInput, ProjectEnrichmentAssetError> {
    let rows = read_skill_fact_artifact_rows(lake, source_records).await?;
    let mut by_entity = BTreeMap::<String, HomeStateSourceFacts>::new();

    for fact in rows.facts {
        let entry = by_entity.entry(fact.entity_id.clone()).or_default();
        match fact.fact_key.as_str() {
            "rera_delay_months" => update_numeric(&mut entry.delay_months, &fact)?,
            "rera_status" => update_text(&mut entry.status, &fact)?,
            _ => {}
        }
    }

    let mut facts = Vec::new();
    let mut annotations = Vec::new();
    for (entity_id, source) in by_entity {
        append_home_state_signals(
            &entity_id,
            &source,
            run_id,
            as_of,
            &mut facts,
            &mut annotations,
        )?;
    }

    Ok(SkillFactsInput {
        source: "home_state".to_string(),
        snapshot_date: source_records
            .iter()
            .map(|record| record.version.as_str())
            .max()
            .unwrap_or("unknown")
            .to_string(),
        facts,
        fact_annotations: annotations,
        source_watermarks: source_records
            .iter()
            .flat_map(|record| record.source_watermarks.clone())
            .chain(std::iter::once(SourceWatermark {
                source: "home_state_signals".to_string(),
                high_watermark: as_of.to_rfc3339(),
            }))
            .collect(),
    })
}

fn append_home_state_signals(
    entity_id: &str,
    source: &HomeStateSourceFacts,
    run_id: &MaterializationId,
    as_of: DateTime<Utc>,
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
) -> Result<(), ProjectEnrichmentAssetError> {
    let evidence_source = stable_evidence_source(source);
    let explicit_delay = source
        .delay_months
        .as_ref()
        .is_some_and(|fact| fact.value > 0.0);

    if let Some(state) = source
        .status
        .as_ref()
        .and_then(|fact| explicit_home_state(&fact.value, explicit_delay))
    {
        append_fact(
            entity_id,
            "home_state",
            FactValue::Text(state.to_string()),
            0.9,
            evidence_source.source_url.clone(),
            as_of,
            run_id,
            "Home state: {value}",
            &home_state_preferences(state),
            facts,
            annotations,
        )?;
        append_fact(
            entity_id,
            "project_delivery_state",
            FactValue::Text(project_delivery_state(state, source.delay_months.as_ref())),
            0.9,
            evidence_source.source_url.clone(),
            as_of,
            run_id,
            "Delivery state: {value}",
            &[
                "new property",
                "old society",
                "upcoming",
                "under construction",
                "delivered",
            ],
            facts,
            annotations,
        )?;
    }

    if let Some(delay) = source.delay_months.as_ref().filter(|fact| fact.value > 0.0) {
        append_fact(
            entity_id,
            "home_timeline_state",
            FactValue::Text("delayed".to_string()),
            0.9,
            delay.source_url.clone(),
            as_of,
            run_id,
            "Timeline state: {value}",
            &["delayed", "avoid delayed", "possession delay"],
            facts,
            annotations,
        )?;
        append_fact(
            entity_id,
            "project_timeline_state",
            FactValue::Text("delayed".to_string()),
            0.9,
            delay.source_url.clone(),
            as_of,
            run_id,
            "Project timeline: {value}",
            &["delayed", "avoid delayed", "possession delay"],
            facts,
            annotations,
        )?;
    }

    Ok(())
}

fn update_text(
    target: &mut Option<SourceTextFact>,
    fact: &SkillFactRecord,
) -> Result<(), ProjectEnrichmentAssetError> {
    let value = fact_text_value(fact)?;
    if value.trim().is_empty() {
        return Ok(());
    }
    let selection_key = source_fact_selection_key(fact);
    if target
        .as_ref()
        .is_none_or(|current| selection_key < current.selection_key)
    {
        *target = Some(SourceTextFact {
            value,
            source_url: fact.source_url.clone(),
            selection_key,
        });
    }
    Ok(())
}

fn update_numeric(
    target: &mut Option<SourceNumericFact>,
    fact: &SkillFactRecord,
) -> Result<(), ProjectEnrichmentAssetError> {
    let value = fact_numeric_value(fact)?;
    if !value.is_finite() {
        return Ok(());
    }
    let selection_key = source_fact_selection_key(fact);
    if target
        .as_ref()
        .is_none_or(|current| selection_key < current.selection_key)
    {
        *target = Some(SourceNumericFact {
            value,
            source_url: fact.source_url.clone(),
            selection_key,
        });
    }
    Ok(())
}

fn source_fact_selection_key(fact: &SkillFactRecord) -> String {
    let payload = serde_json::to_vec(&(
        &fact.fact_key,
        &fact.value_json,
        &fact.source_type,
        &fact.source_url,
        &fact.input_hash,
        &fact.observation_provider,
        &fact.provider_observation_id,
        &fact.asset_lineage,
    ))
    .expect("source fact identity fields are serializable");
    sha256_hex(&payload)
}

fn stable_evidence_source(source: &HomeStateSourceFacts) -> SourceTextFact {
    let mut candidates = Vec::new();
    if let Some(fact) = &source.status {
        candidates.push(fact.clone());
    }
    if let Some(fact) = &source.delay_months {
        candidates.push(SourceTextFact {
            value: fact.value.to_string(),
            source_url: fact.source_url.clone(),
            selection_key: fact.selection_key.clone(),
        });
    }
    candidates
        .into_iter()
        .min_by(|left, right| left.selection_key.cmp(&right.selection_key))
        .unwrap_or_else(|| SourceTextFact {
            value: String::new(),
            source_url: None,
            selection_key: String::new(),
        })
}

fn fact_text_value(fact: &SkillFactRecord) -> Result<String, ProjectEnrichmentAssetError> {
    let value: FactValue = serde_json::from_str(&fact.value_json)?;
    match value {
        FactValue::Text(value) => Ok(value),
        _ => Ok(String::new()),
    }
}

fn fact_numeric_value(fact: &SkillFactRecord) -> Result<f64, ProjectEnrichmentAssetError> {
    let value: FactValue = serde_json::from_str(&fact.value_json)?;
    match value {
        FactValue::Numeric(value) => Ok(value),
        _ => Ok(f64::NAN),
    }
}

#[allow(clippy::too_many_arguments)]
fn append_fact(
    entity_id: &str,
    fact_key: &str,
    value: FactValue,
    confidence: f32,
    source_url: Option<String>,
    learned_at: DateTime<Utc>,
    run_id: &MaterializationId,
    display_template: &str,
    answers_preferences: &[&str],
    facts: &mut Vec<SkillFactRecord>,
    annotations: &mut Vec<SkillFactAnnotationRecord>,
) -> Result<(), ProjectEnrichmentAssetError> {
    let value_type = match value {
        FactValue::Numeric(_) => "numeric",
        FactValue::Text(_) => "text",
        FactValue::Bool(_) => "bool",
        FactValue::Tags(_) => "tags",
        FactValue::Score { .. } => "score",
    };
    let value_json = serde_json::to_string(&value)?;
    facts.push(SkillFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        value_type: value_type.to_string(),
        value_json: value_json.clone(),
        confidence,
        source_type: "Computed".to_string(),
        source_url,
        model: None,
        skill_id: Some("home_state_signals".to_string()),
        triggered_by: Some("asset_dag".to_string()),
        learned_at,
        run_id: run_id.to_string(),
        input_hash: sha256_hex(format!("{entity_id}:{fact_key}:{value_json}").as_bytes()),
        observation_provider: None,
        provider_observation_id: None,
        asset_lineage: Vec::new(),
    });
    annotations.push(SkillFactAnnotationRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        display_template: Some(display_template.to_string()),
        answers_preferences_json: serde_json::to_string(answers_preferences)?,
        scoring_direction: None,
        scoring_weight: None,
        scoring_thresholds_json: "[]".to_string(),
    });
    Ok(())
}

fn home_state_preferences(state: &str) -> Vec<&'static str> {
    match state {
        "delivered" => vec![
            "delivered society",
            "ready to move",
            "ready resale",
            "completed project",
        ],
        "delayed" => vec!["delayed", "avoid delayed", "possession delay"],
        "under_construction" => vec!["under construction", "new launch", "new property"],
        _ => Vec::new(),
    }
}

fn project_delivery_state(state: &str, delay: Option<&SourceNumericFact>) -> String {
    let delayed = delay.is_some_and(|fact| fact.value > 0.0);
    match (state, delayed) {
        ("delivered", true) => "delayed_delivered",
        ("delivered", false) => "delivered",
        ("under_construction", true) | ("delayed", true) => "delayed_under_construction",
        ("delayed", false) => "delayed_under_construction",
        _ => state,
    }
    .to_string()
}

fn explicit_home_state(value: &str, delayed: bool) -> Option<&'static str> {
    let normalized = value.to_ascii_lowercase();
    if normalized.contains("complete")
        || normalized.contains("delivered")
        || normalized.contains("ready")
    {
        Some("delivered")
    } else if normalized.contains("approved")
        || normalized.contains("ongoing")
        || normalized.contains("under")
        || normalized.contains("construction")
    {
        Some(if delayed {
            "delayed"
        } else {
            "under_construction"
        })
    } else {
        None
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_status_does_not_create_time_derived_age() {
        let run_id = MaterializationId::new();
        let mut facts = Vec::new();
        let mut annotations = Vec::new();
        let source = HomeStateSourceFacts {
            status: Some(SourceTextFact {
                value: "Completed".to_string(),
                source_url: Some("https://rera.example/project".to_string()),
                selection_key: "status:completed".to_string(),
            }),
            ..Default::default()
        };

        append_home_state_signals(
            "society:rera-sample",
            &source,
            &run_id,
            DateTime::parse_from_rfc3339("2026-07-17T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            &mut facts,
            &mut annotations,
        )
        .unwrap();

        assert!(facts
            .iter()
            .any(|fact| fact.fact_key == "home_state" && fact.value_json.contains("delivered")));
        assert!(!facts.iter().any(|fact| fact.fact_key.contains("age")));
        assert!(facts
            .iter()
            .any(|fact| fact.fact_key == "project_delivery_state"
                && fact.value_json.contains("delivered")));
        assert!(!facts
            .iter()
            .any(|fact| fact.fact_key == "home_delay_months"));
        assert!(annotations
            .iter()
            .all(|annotation| annotation.scoring_direction.is_none()
                && annotation.scoring_weight.is_none()
                && annotation.scoring_thresholds_json == "[]"));
    }

    #[test]
    fn explicit_delay_and_status_get_buyer_state() {
        let run_id = MaterializationId::new();
        let mut facts = Vec::new();
        let mut annotations = Vec::new();
        let source = HomeStateSourceFacts {
            status: Some(SourceTextFact {
                value: "Ongoing".to_string(),
                source_url: None,
                selection_key: "status:ongoing".to_string(),
            }),
            delay_months: Some(SourceNumericFact {
                value: 8.0,
                source_url: None,
                selection_key: "delay:8".to_string(),
            }),
        };

        append_home_state_signals(
            "society:rera-delayed",
            &source,
            &run_id,
            DateTime::parse_from_rfc3339("2026-07-17T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            &mut facts,
            &mut annotations,
        )
        .unwrap();

        assert!(facts
            .iter()
            .any(|fact| fact.fact_key == "home_state" && fact.value_json.contains("delayed")));
        assert!(facts
            .iter()
            .any(|fact| fact.fact_key == "home_timeline_state"));
        assert!(facts
            .iter()
            .any(|fact| fact.fact_key == "project_delivery_state"
                && fact.value_json.contains("delayed_under_construction")));
        assert!(facts
            .iter()
            .any(|fact| fact.fact_key == "project_timeline_state"));
        assert!(!facts.iter().any(|fact| fact.fact_key == "home_age_years"));
    }
}
