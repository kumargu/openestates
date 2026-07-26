use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, NaiveDate, Utc};

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
    start_date: Option<SourceTextFact>,
    completion_date: Option<SourceTextFact>,
    original_completion_date: Option<SourceTextFact>,
    delay_months: Option<SourceNumericFact>,
    status: Option<SourceTextFact>,
}

#[derive(Debug, Clone)]
struct SourceTextFact {
    value: String,
    learned_at: DateTime<Utc>,
    source_url: Option<String>,
}

#[derive(Debug, Clone)]
struct SourceNumericFact {
    value: f64,
    learned_at: DateTime<Utc>,
    source_url: Option<String>,
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
            "rera_start_date" | "project_start_date" => update_text(&mut entry.start_date, &fact)?,
            "rera_completion_date" | "project_revised_completion_date" => {
                update_text(&mut entry.completion_date, &fact)?
            }
            "rera_original_completion_date" | "project_original_completion_date" => {
                update_text(&mut entry.original_completion_date, &fact)?
            }
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
    let completion = source
        .completion_date
        .as_ref()
        .and_then(|fact| parse_rera_date(&fact.value));
    let original_completion = source
        .original_completion_date
        .as_ref()
        .and_then(|fact| parse_rera_date(&fact.value));
    let latest_fact = latest_source_fact(source);

    if let Some(completion) = completion {
        let state = if completion <= as_of.date_naive() {
            "delivered"
        } else if source
            .delay_months
            .as_ref()
            .is_some_and(|fact| fact.value > 0.0)
        {
            "delayed"
        } else {
            "under_construction"
        };
        append_fact(
            entity_id,
            "home_state",
            FactValue::Text(state.to_string()),
            0.9,
            latest_fact.source_url.clone(),
            latest_fact.learned_at.max(as_of),
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
            latest_fact.source_url.clone(),
            latest_fact.learned_at.max(as_of),
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

        if completion <= as_of.date_naive() {
            let years = completed_years(completion, as_of.date_naive());
            let age_years = completed_years_precise(completion, as_of.date_naive());
            append_fact(
                entity_id,
                "home_age_years",
                FactValue::Numeric(age_years),
                0.85,
                source
                    .completion_date
                    .as_ref()
                    .and_then(|fact| fact.source_url.clone()),
                latest_fact.learned_at.max(as_of),
                run_id,
                "Estimated home age: {value} years",
                &[
                    "new property",
                    "newly delivered",
                    "old society",
                    "property age",
                ],
                facts,
                annotations,
            )?;
            append_fact(
                entity_id,
                "project_age_years",
                FactValue::Numeric(age_years),
                0.85,
                source
                    .completion_date
                    .as_ref()
                    .and_then(|fact| fact.source_url.clone()),
                latest_fact.learned_at.max(as_of),
                run_id,
                "Project age: {value} years",
                &["1 year old", "new property", "old society", "property age"],
                facts,
                annotations,
            )?;
            let bucket = age_bucket(years);
            append_fact(
                entity_id,
                "home_age_bucket",
                FactValue::Text(bucket.to_string()),
                0.85,
                source
                    .completion_date
                    .as_ref()
                    .and_then(|fact| fact.source_url.clone()),
                latest_fact.learned_at.max(as_of),
                run_id,
                "Home age: {value}",
                &age_bucket_preferences(bucket),
                facts,
                annotations,
            )?;
            append_fact(
                entity_id,
                "project_age_bucket",
                FactValue::Text(project_age_bucket(years).to_string()),
                0.85,
                source
                    .completion_date
                    .as_ref()
                    .and_then(|fact| fact.source_url.clone()),
                latest_fact.learned_at.max(as_of),
                run_id,
                "Project age: {value}",
                &age_bucket_preferences(project_age_bucket(years)),
                facts,
                annotations,
            )?;
        }
    } else if source
        .status
        .as_ref()
        .is_some_and(|fact| status_suggests_under_construction(&fact.value))
    {
        append_fact(
            entity_id,
            "home_state",
            FactValue::Text("under_construction".to_string()),
            0.75,
            latest_fact.source_url.clone(),
            latest_fact.learned_at.max(as_of),
            run_id,
            "Home state: {value}",
            &home_state_preferences("under_construction"),
            facts,
            annotations,
        )?;
        append_fact(
            entity_id,
            "project_delivery_state",
            FactValue::Text("under_construction".to_string()),
            0.75,
            latest_fact.source_url.clone(),
            latest_fact.learned_at.max(as_of),
            run_id,
            "Delivery state: {value}",
            &["under construction", "upcoming"],
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
            delay.learned_at.max(as_of),
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
            delay.learned_at.max(as_of),
            run_id,
            "Project timeline: {value}",
            &["delayed", "avoid delayed", "possession delay"],
            facts,
            annotations,
        )?;
    } else if let (Some(original), Some(completion), Some(fact)) = (
        original_completion,
        completion,
        source.original_completion_date.as_ref(),
    ) {
        if completion <= original {
            append_fact(
                entity_id,
                "home_timeline_state",
                FactValue::Text("on_track".to_string()),
                0.8,
                fact.source_url.clone(),
                latest_fact.learned_at.max(as_of),
                run_id,
                "Timeline state: {value}",
                &["on time", "not delayed", "avoid delayed"],
                facts,
                annotations,
            )?;
            append_fact(
                entity_id,
                "project_timeline_state",
                FactValue::Text("on_track".to_string()),
                0.8,
                fact.source_url.clone(),
                latest_fact.learned_at.max(as_of),
                run_id,
                "Project timeline: {value}",
                &["on time", "not delayed", "avoid delayed"],
                facts,
                annotations,
            )?;
        }
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
    if target
        .as_ref()
        .is_none_or(|current| fact.learned_at > current.learned_at)
    {
        *target = Some(SourceTextFact {
            value,
            learned_at: fact.learned_at,
            source_url: fact.source_url.clone(),
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
    if target
        .as_ref()
        .is_none_or(|current| fact.learned_at > current.learned_at)
    {
        *target = Some(SourceNumericFact {
            value,
            learned_at: fact.learned_at,
            source_url: fact.source_url.clone(),
        });
    }
    Ok(())
}

fn latest_source_fact(source: &HomeStateSourceFacts) -> SourceTextFact {
    let mut candidates = Vec::new();
    if let Some(fact) = &source.start_date {
        candidates.push(fact.clone());
    }
    if let Some(fact) = &source.completion_date {
        candidates.push(fact.clone());
    }
    if let Some(fact) = &source.original_completion_date {
        candidates.push(fact.clone());
    }
    if let Some(fact) = &source.status {
        candidates.push(fact.clone());
    }
    if let Some(fact) = &source.delay_months {
        candidates.push(SourceTextFact {
            value: fact.value.to_string(),
            learned_at: fact.learned_at,
            source_url: fact.source_url.clone(),
        });
    }
    candidates
        .into_iter()
        .max_by_key(|fact| fact.learned_at)
        .unwrap_or_else(|| SourceTextFact {
            value: String::new(),
            learned_at: as_epoch(),
            source_url: None,
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

fn parse_rera_date(value: &str) -> Option<NaiveDate> {
    let trimmed = value.trim();
    for format in ["%d/%m/%Y", "%d-%m-%Y", "%Y-%m-%d", "%d.%m.%Y"] {
        if let Ok(date) = NaiveDate::parse_from_str(trimmed, format) {
            return Some(date);
        }
    }
    None
}

fn completed_years(completion: NaiveDate, as_of: NaiveDate) -> i32 {
    let mut years = as_of.year() - completion.year();
    if (as_of.month(), as_of.day()) < (completion.month(), completion.day()) {
        years -= 1;
    }
    years.max(0)
}

fn completed_years_precise(completion: NaiveDate, as_of: NaiveDate) -> f64 {
    let days = as_of.signed_duration_since(completion).num_days().max(0) as f64;
    (days / 365.2425 * 10.0).round() / 10.0
}

fn age_bucket(years: i32) -> &'static str {
    match years {
        0 => "newly delivered",
        1..=4 => "1-5 yrs old",
        5..=9 => "5-10 yrs old",
        _ => "10+ yrs old",
    }
}

fn project_age_bucket(years: i32) -> &'static str {
    match years {
        0..=1 => "new_0_1y",
        2..=5 => "young_1_5y",
        6..=10 => "mature_5_10y",
        _ => "old_10y_plus",
    }
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

fn age_bucket_preferences(bucket: &str) -> Vec<&'static str> {
    match bucket {
        "newly delivered" | "new_0_1y" => {
            vec![
                "new property",
                "newly delivered",
                "new society",
                "1 year old",
            ]
        }
        "1-5 yrs old" | "young_1_5y" => {
            vec!["recently delivered", "newer society", "property age"]
        }
        "5-10 yrs old" | "mature_5_10y" => {
            vec!["established society", "old society", "property age"]
        }
        "10+ yrs old" | "old_10y_plus" => {
            vec!["old society", "mature society", "established society"]
        }
        _ => vec!["property age"],
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

fn status_suggests_under_construction(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    normalized.contains("approved")
        || normalized.contains("ongoing")
        || normalized.contains("under")
        || normalized.contains("construction")
}

fn as_epoch() -> DateTime<Utc> {
    DateTime::from_timestamp(0, 0).unwrap_or_else(Utc::now)
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
    fn derives_delivered_age_without_duplicate_delay_value() {
        let run_id = MaterializationId::new();
        let mut facts = Vec::new();
        let mut annotations = Vec::new();
        let source = HomeStateSourceFacts {
            completion_date: Some(SourceTextFact {
                value: "31/10/2019".to_string(),
                learned_at: Utc::now(),
                source_url: Some("https://rera.example/project".to_string()),
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
        assert!(facts.iter().any(|fact| fact.fact_key == "home_age_years"));
        assert!(facts
            .iter()
            .any(|fact| fact.fact_key == "home_age_bucket"
                && fact.value_json.contains("5-10 yrs old")));
        assert!(facts
            .iter()
            .any(|fact| fact.fact_key == "project_age_years"));
        assert!(facts
            .iter()
            .any(|fact| fact.fact_key == "project_age_bucket"
                && fact.value_json.contains("mature_5_10y")));
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
    fn delayed_future_completion_gets_buyer_state() {
        let run_id = MaterializationId::new();
        let mut facts = Vec::new();
        let mut annotations = Vec::new();
        let source = HomeStateSourceFacts {
            completion_date: Some(SourceTextFact {
                value: "31/10/2027".to_string(),
                learned_at: Utc::now(),
                source_url: None,
            }),
            delay_months: Some(SourceNumericFact {
                value: 8.0,
                learned_at: Utc::now(),
                source_url: None,
            }),
            ..Default::default()
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
