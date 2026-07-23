use backend::assets::{
    community_review_summary_facts_from_records, MaterializationId, SkillFactRecord,
};
use backend::knowledge::FactValue;
use chrono::{TimeZone, Utc};

#[test]
fn google_rating_metadata_creates_summary_without_fake_themes() {
    let run_id = MaterializationId::new();
    let learned_at = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();
    let facts = vec![
        fact(
            "society:prestige-lavender-fields",
            "google_rating",
            FactValue::Numeric(3.9),
            "Google",
            Some("https://maps.google.com/?cid=123"),
            learned_at,
        ),
        fact(
            "society:prestige-lavender-fields",
            "google_review_count",
            FactValue::Numeric(392.0),
            "Google",
            Some("https://maps.google.com/?cid=123"),
            learned_at,
        ),
    ];

    let input = community_review_summary_facts_from_records(&facts, &run_id, "2026-07-14").unwrap();

    assert_eq!(input.source, "community");
    assert!(input.facts.iter().any(|fact| {
        fact.fact_key == "community_review_summary"
            && fact
                .value_json
                .contains("written review themes are still limited")
    }));
    assert!(!input
        .facts
        .iter()
        .any(|fact| fact.fact_key == "community_sentiment_score"));
    assert!(!input
        .facts
        .iter()
        .any(|fact| fact.fact_key == "community_positive_themes"));
    assert!(!input
        .facts
        .iter()
        .any(|fact| fact.fact_key == "community_concern_themes"));
}

#[test]
fn reddit_text_evidence_creates_dynamic_theme_facts() {
    let run_id = MaterializationId::new();
    let learned_at = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();
    let facts = vec![fact(
        "society:example-green",
        "resident_discussion",
        FactValue::Text(
            "Residents mention many trees, a useful clubhouse and pool, but traffic is bad."
                .to_string(),
        ),
        "Reddit",
        Some("https://www.reddit.com/r/BangaloreRealEstates/comments/example"),
        learned_at,
    )];

    let input = community_review_summary_facts_from_records(&facts, &run_id, "2026-07-14").unwrap();

    assert!(input.facts.iter().any(|fact| {
        fact.fact_key == "community_positive_themes"
            && fact.value_json.contains("greenery")
            && fact.value_json.contains("amenities")
    }));
    assert!(input.facts.iter().any(|fact| {
        fact.fact_key == "community_concern_themes" && fact.value_json.contains("traffic")
    }));
    assert!(input.fact_annotations.iter().any(|annotation| {
        annotation.fact_key == "community_positive_themes"
            && annotation.answers_preferences_json.contains("greenery")
            && annotation.answers_preferences_json.contains("clubhouse")
    }));
}

#[test]
fn google_review_snippets_create_dynamic_theme_facts() {
    let run_id = MaterializationId::new();
    let learned_at = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();
    let facts = vec![
        fact(
            "society:example-green",
            "google_rating",
            FactValue::Numeric(4.1),
            "Google",
            Some("https://maps.google.com/?cid=123"),
            learned_at,
        ),
        fact(
            "society:example-green",
            "google_review_snippets",
            FactValue::Tags(vec![
                "Well maintained society with greenery and useful clubhouse.".to_string(),
                "Traffic near the approach road gets heavy.".to_string(),
            ]),
            "Google",
            Some("https://maps.google.com/?cid=123"),
            learned_at,
        ),
    ];

    let input = community_review_summary_facts_from_records(&facts, &run_id, "2026-07-14").unwrap();

    assert!(input.facts.iter().any(|fact| {
        fact.fact_key == "community_positive_themes"
            && fact.value_json.contains("greenery")
            && fact.value_json.contains("maintenance")
            && fact.value_json.contains("amenities")
    }));
    assert!(input.facts.iter().any(|fact| {
        fact.fact_key == "community_concern_themes" && fact.value_json.contains("traffic")
    }));
    assert!(input.facts.iter().any(|fact| {
        fact.fact_key == "community_review_summary"
            && !fact.value_json.contains("Review text is not ingested yet")
            && !fact.value_json.contains("/5")
    }));
    assert!(input.facts.iter().any(|fact| {
        fact.fact_key == "community_review_highlights"
            && fact.value_json.contains("Well maintained society")
            && fact.value_json.contains("Traffic near the approach")
    }));
}

fn fact(
    entity_id: &str,
    fact_key: &str,
    value: FactValue,
    source_type: &str,
    source_url: Option<&str>,
    learned_at: chrono::DateTime<Utc>,
) -> SkillFactRecord {
    let value_type = match &value {
        FactValue::Numeric(_) => "numeric",
        FactValue::Text(_) => "text",
        FactValue::Bool(_) => "bool",
        FactValue::Tags(_) => "tags",
        FactValue::Score { .. } => "score",
    };
    SkillFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        value_type: value_type.to_string(),
        value_json: serde_json::to_string(&value).unwrap(),
        confidence: 0.85,
        source_type: source_type.to_string(),
        source_url: source_url.map(str::to_string),
        model: None,
        skill_id: Some("fixture".to_string()),
        triggered_by: Some("test".to_string()),
        learned_at,
        run_id: "fixture-run".to_string(),
        input_hash: "sha256:fixture".to_string(),
    }
}
