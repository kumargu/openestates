//! Deterministic livability diligence brief composed from DAG facts and mined themes.
//!
//! Reddit evidence is structurally supported but disabled until the isolated fetcher
//! container is deployed. Set `REDDIT_EVIDENCE_ENABLED` when lake artifacts exist.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::community::CommunityEvidenceRecord;
use crate::dag_config::{dag_root, load_json};

/// Reddit facts are accepted in the pipeline but excluded from brief composition for now.
pub const REDDIT_EVIDENCE_ENABLED: bool = false;

const LIVABILITY_BLOCK_MAX_WORDS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LivabilityLens {
    Operating,
    Risk,
    Positive,
    Lifecycle,
    Judgment,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LivabilityBriefBlock {
    pub lens: String,
    pub title: String,
    pub paragraph: String,
    pub themes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fact_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LivabilityBrief {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_paragraph: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<LivabilityBriefBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle_flag: Option<String>,
    #[serde(skip_serializing)]
    pub confidence_label: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_urls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ConcernTaxonomyFile {
    #[serde(default)]
    defaults: ConcernDefaults,
    #[serde(default)]
    buckets: Vec<ConcernBucket>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
struct ConcernDefaults {
    #[serde(default)]
    source_types: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ConcernBucket {
    #[serde(default)]
    leaves: Vec<ConcernLeaf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ConcernLeaf {
    fact_key: String,
    label: String,
    lens: String,
    #[serde(default)]
    polarity: String,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default)]
    terms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct LivabilityThemeDefinition {
    key: String,
    label: String,
    lens: String,
    polarity: String,
    scopes: Vec<String>,
    terms: Vec<String>,
    source_types: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct LivabilityThemeRegistry {
    themes: Vec<LivabilityThemeDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredFactSignal {
    pub fact_key: String,
    pub label: String,
    pub lens: LivabilityLens,
}

#[derive(Debug)]
pub struct LivabilityBriefInput<'a> {
    pub society_name: &'a str,
    pub area_name: &'a str,
    pub home_state: Option<&'a str>,
    pub home_age_bucket: Option<&'a str>,
    pub home_timeline_state: Option<&'a str>,
    pub evidence_records: &'a [CommunityEvidenceRecord],
    pub structured_facts: &'a [StructuredFactSignal],
    pub community_positives: &'a [String],
    pub community_concerns: &'a [String],
    pub source_urls: &'a [String],
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ThemeHit {
    key: String,
    label: String,
    lens: LivabilityLens,
}

pub fn compose_livability_brief(input: &LivabilityBriefInput<'_>) -> Option<LivabilityBrief> {
    let mined = mine_theme_hits(input.evidence_records);
    let mut lens_themes = group_hits_by_lens(&mined, input.structured_facts);

    merge_community_themes(
        &mut lens_themes,
        input.community_positives,
        LivabilityLens::Positive,
    );
    merge_community_themes(
        &mut lens_themes,
        input.community_concerns,
        LivabilityLens::Risk,
    );

    let lifecycle_flag = derive_lifecycle_flag(input);
    let has_signal = !lens_themes.values().any(|themes| !themes.is_empty())
        || input.home_state.is_some()
        || input.home_age_bucket.is_some();

    if !has_signal {
        return None;
    }

    let mut blocks = Vec::new();
    if let Some(block) = compose_operating_block(input, lens_themes.get(&LivabilityLens::Operating))
    {
        blocks.push(block);
    }
    if let Some(block) = compose_risk_block(input, lens_themes.get(&LivabilityLens::Risk)) {
        blocks.push(block);
    }
    if let Some(block) = compose_positive_block(input, lens_themes.get(&LivabilityLens::Positive)) {
        blocks.push(block);
    }
    if let Some(block) = compose_judgment_block(input, &lifecycle_flag) {
        blocks.push(block);
    }

    if blocks.is_empty() {
        return None;
    }

    let confidence_label = if input.evidence_records.is_empty() {
        "Directional".to_string()
    } else {
        "Strong proof".to_string()
    };

    Some(LivabilityBrief {
        summary_paragraph: None,
        blocks,
        lifecycle_flag,
        confidence_label,
        source_urls: input.source_urls.to_vec(),
    })
}

fn livability_theme_registry() -> &'static LivabilityThemeRegistry {
    static REGISTRY: OnceLock<LivabilityThemeRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let path = dag_root().join("concern_taxonomy.json");
        let taxonomy = load_json::<ConcernTaxonomyFile>(&path)
            .expect("app/config/dag/concern_taxonomy.json is required for livability brief");
        let source_types = if taxonomy.defaults.source_types.is_empty() {
            vec!["Google".to_string(), "Reddit".to_string()]
        } else {
            taxonomy
                .defaults
                .source_types
                .iter()
                .map(|value| {
                    if value == "RedditTheme" {
                        "Reddit".to_string()
                    } else {
                        value.clone()
                    }
                })
                .collect()
        };
        let themes = taxonomy
            .buckets
            .into_iter()
            .flat_map(|bucket| bucket.leaves)
            .map(|leaf| LivabilityThemeDefinition {
                key: leaf.fact_key,
                label: leaf.label,
                lens: leaf.lens,
                polarity: leaf.polarity,
                scopes: leaf.scopes,
                terms: leaf.terms,
                source_types: source_types.clone(),
            })
            .collect();
        LivabilityThemeRegistry { themes }
    })
}

fn mine_theme_hits(records: &[CommunityEvidenceRecord]) -> Vec<ThemeHit> {
    let mut hits = BTreeMap::<String, ThemeHit>::new();
    for record in records {
        if !REDDIT_EVIDENCE_ENABLED && record.source_type == "Reddit" {
            continue;
        }
        let haystack = evidence_haystack(record);
        if haystack.trim().is_empty() {
            continue;
        }
        for theme in &livability_theme_registry().themes {
            if !theme
                .source_types
                .iter()
                .any(|source| source == &record.source_type)
            {
                continue;
            }
            if theme
                .terms
                .iter()
                .any(|term| contains_term(&haystack, term))
            {
                hits.entry(theme.key.clone()).or_insert_with(|| ThemeHit {
                    key: theme.key.clone(),
                    label: theme.label.clone(),
                    lens: parse_lens(&theme.lens),
                });
            }
        }
    }
    hits.into_values().collect()
}

fn group_hits_by_lens(
    hits: &[ThemeHit],
    structured_facts: &[StructuredFactSignal],
) -> BTreeMap<LivabilityLens, Vec<String>> {
    let mut grouped = BTreeMap::<LivabilityLens, BTreeSet<String>>::new();
    for hit in hits {
        grouped
            .entry(hit.lens)
            .or_default()
            .insert(hit.label.clone());
    }
    for fact in structured_facts {
        grouped
            .entry(fact.lens)
            .or_default()
            .insert(fact.label.clone());
    }
    grouped
        .into_iter()
        .map(|(lens, labels)| {
            let mut themes = labels.into_iter().collect::<Vec<_>>();
            themes.sort();
            (lens, themes)
        })
        .collect()
}

fn merge_community_themes(
    lens_themes: &mut BTreeMap<LivabilityLens, Vec<String>>,
    themes: &[String],
    lens: LivabilityLens,
) {
    if themes.is_empty() {
        return;
    }
    let bucket = lens_themes.entry(lens).or_default();
    for theme in themes {
        if !bucket.iter().any(|existing| existing == theme) {
            bucket.push(theme.clone());
        }
    }
    bucket.sort();
}

fn derive_lifecycle_flag(input: &LivabilityBriefInput<'_>) -> Option<String> {
    let state = input.home_state?.to_ascii_lowercase();
    if state.contains("under construction") || state.contains("upcoming") {
        return Some("understand-before-you-buy".to_string());
    }
    if state.contains("delivered") || state.contains("ready") {
        if input
            .home_age_bucket
            .is_some_and(|age| age.contains('+') || age.contains("year"))
        {
            return Some("livability-first".to_string());
        }
        return Some("ready-to-move".to_string());
    }
    None
}

fn cap_block_themes(mut themes: Vec<String>) -> Vec<String> {
    themes.truncate(3);
    themes
}

fn compose_operating_block(
    input: &LivabilityBriefInput<'_>,
    themes: Option<&Vec<String>>,
) -> Option<LivabilityBriefBlock> {
    let themes = themes.cloned().unwrap_or_default();
    if themes.is_empty() && input.home_state.is_none() {
        return None;
    }

    let lifecycle_flag = derive_lifecycle_flag(input);
    let opener = match lifecycle_flag.as_deref() {
        Some("livability-first") => format!(
            "{} is best understood as a livability-first gated community rather than just a price-per-sqft asset.",
            input.society_name
        ),
        _ => format!(
            "Buyers evaluating {} should look closely at daily operating quality.",
            input.society_name
        ),
    };

    let headline_themes = cap_block_themes(themes.clone());
    let body = if headline_themes.is_empty() {
        "Verify maintenance charges, lift uptime, water source, tanker dependence, STP handling, waste management, parking pressure, and how well the association responds to complaints on your visit.".to_string()
    } else {
        format!(
            "Recurring resident signals point to {}.",
            join_natural_list(&headline_themes)
        )
    };

    Some(LivabilityBriefBlock {
        lens: "operating".to_string(),
        title: "Operating quality".to_string(),
        paragraph: clamp_block_words(format!("{opener} {body}")),
        themes: cap_block_themes(themes),
        fact_keys: vec!["home_state".to_string()],
    })
}

fn compose_risk_block(
    input: &LivabilityBriefInput<'_>,
    themes: Option<&Vec<String>>,
) -> Option<LivabilityBriefBlock> {
    let mut themes = themes.cloned().unwrap_or_default();
    if themes.is_empty() {
        return None;
    }

    if input.area_name.trim().is_empty() {
        themes.retain(|theme| !theme.contains("rajakaluve"));
    }

    let paragraph = format!(
        "Around {} and the approach roads, the biggest risk signals to verify are {}. These may not show up in brochure material but can affect resale value, tenant demand, and day-to-day comfort.",
        if input.area_name.trim().is_empty() {
            "the project"
        } else {
            input.area_name
        },
        join_natural_list(&themes)
    );

    Some(LivabilityBriefBlock {
        lens: "risk".to_string(),
        title: "Risk signals".to_string(),
        paragraph: clamp_block_words(paragraph),
        themes: cap_block_themes(themes),
        fact_keys: vec![
            "approach_road_condition".to_string(),
            "waterlogging_detail".to_string(),
            "stp_concern".to_string(),
            "high_tension_wire_concern".to_string(),
        ],
    })
}

fn compose_positive_block(
    input: &LivabilityBriefInput<'_>,
    themes: Option<&Vec<String>>,
) -> Option<LivabilityBriefBlock> {
    let themes = themes.cloned().unwrap_or_default();
    if themes.is_empty() {
        return None;
    }

    let headline_themes = cap_block_themes(themes.clone());
    let paragraph = format!(
        "Residents repeatedly praise {} at {}. Mature communities with predictable monthly costs tend to feel safer for end-use and rental investors.",
        join_natural_list(&headline_themes),
        input.society_name
    );

    Some(LivabilityBriefBlock {
        lens: "positive".to_string(),
        title: "Positive signals".to_string(),
        paragraph: clamp_block_words(paragraph),
        themes: cap_block_themes(themes),
        fact_keys: vec![
            "nearby_schools".to_string(),
            "nearby_metro_stations".to_string(),
            "google_review_snippets".to_string(),
        ],
    })
}

fn compose_judgment_block(
    input: &LivabilityBriefInput<'_>,
    lifecycle_flag: &Option<String>,
) -> Option<LivabilityBriefBlock> {
    let mut checklist = vec![
        "recent resident feedback".to_string(),
        "maintenance cost trend".to_string(),
        "water source".to_string(),
        "flooding history".to_string(),
        "legal approvals".to_string(),
        "OC/CC/RERA status".to_string(),
    ];
    if input
        .structured_facts
        .iter()
        .any(|fact| fact.fact_key == "high_tension_wire_concern")
    {
        checklist.push("high-tension wire buffers".to_string());
    }

    let lifecycle_note = match lifecycle_flag.as_deref() {
        Some("understand-before-you-buy") => {
            "Treat this as an understand-before-you-buy project, not a ready lifestyle bet."
        }
        Some("livability-first") => {
            "This reads more like a livability-first end-use society than a pure price-per-sqft play."
        }
        Some("ready-to-move") => "This is positioned as a ready-to-move society.",
        _ => "Judge this society on verified livability, not only brand name or quoted price.",
    };

    let paragraph = format!(
        "{lifecycle_note} Before shortlisting, confirm {} and any visible environmental or infrastructure risks around the project.",
        join_natural_list(&checklist)
    );

    Some(LivabilityBriefBlock {
        lens: "judgment".to_string(),
        title: "How to judge".to_string(),
        paragraph: clamp_block_words(paragraph),
        themes: checklist,
        fact_keys: vec![
            "home_state".to_string(),
            "home_timeline_state".to_string(),
            "rera_status".to_string(),
        ],
    })
}

fn parse_lens(value: &str) -> LivabilityLens {
    match value {
        "operating" => LivabilityLens::Operating,
        "risk" => LivabilityLens::Risk,
        "positive" => LivabilityLens::Positive,
        "lifecycle" => LivabilityLens::Lifecycle,
        _ => LivabilityLens::Judgment,
    }
}

fn evidence_haystack(record: &CommunityEvidenceRecord) -> String {
    let mut parts = vec![record.fact_key.as_str()];
    if let Some(text) = record.text.as_deref() {
        parts.push(text);
    }
    for tag in &record.tags {
        parts.push(tag.as_str());
    }
    parts.join(" ").to_ascii_lowercase()
}

fn contains_term(haystack: &str, term: &str) -> bool {
    let normalized = term.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }
    if normalized.contains(' ') {
        return haystack.contains(&normalized);
    }
    haystack
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|token| token == normalized)
}

fn join_natural_list(values: &[String]) -> String {
    match values.len() {
        0 => String::new(),
        1 => values[0].clone(),
        2 => format!("{} and {}", values[0], values[1]),
        _ => format!(
            "{}, and {}",
            values[..values.len() - 1].join(", "),
            values[values.len() - 1]
        ),
    }
}

fn clamp_block_words(mut text: String) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= LIVABILITY_BLOCK_MAX_WORDS {
        return text;
    }
    text = words
        .into_iter()
        .take(LIVABILITY_BLOCK_MAX_WORDS)
        .collect::<Vec<_>>()
        .join(" ");
    if !text.ends_with('.') {
        text.push('.');
    }
    text
}

pub fn filter_reddit_evidence(
    records: Vec<CommunityEvidenceRecord>,
) -> Vec<CommunityEvidenceRecord> {
    if REDDIT_EVIDENCE_ENABLED {
        return records;
    }
    records
        .into_iter()
        .filter(|record| record.source_type != "Reddit")
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_record(text: &str) -> CommunityEvidenceRecord {
        CommunityEvidenceRecord {
            entity_id: "society:example".to_string(),
            source_type: "Google".to_string(),
            source_url: Some("https://maps.google.com/example".to_string()),
            fact_key: "google_review_snippets".to_string(),
            text: None,
            numeric_value: None,
            tags: vec![text.to_string()],
            confidence: 0.8,
            learned_at: chrono::Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap(),
        }
    }

    #[test]
    fn brief_composes_from_mined_themes_without_reddit() {
        let records = vec![
            sample_record("Greenery, clubhouse, and pool are well maintained."),
            sample_record("Traffic near the approach road and STP smell are concerns."),
        ];
        let input = LivabilityBriefInput {
            society_name: "Godrej Park Retreat",
            area_name: "Sarjapur",
            home_state: Some("Delivered"),
            home_age_bucket: Some("7+ years"),
            home_timeline_state: None,
            evidence_records: &records,
            structured_facts: &[],
            community_positives: &[],
            community_concerns: &[],
            source_urls: &[],
        };

        let brief = compose_livability_brief(&input).expect("brief should exist");
        assert!(brief.blocks.len() >= 3);
        assert!(brief.blocks.iter().any(|block| block.lens == "operating"));
        assert!(brief.blocks.iter().any(|block| block.lens == "risk"));
        assert!(brief.blocks.iter().any(|block| block.lens == "positive"));
        assert!(brief
            .lifecycle_flag
            .as_deref()
            .is_some_and(|flag| flag.contains("livability")));
        assert!(!brief
            .blocks
            .iter()
            .any(|block| block.paragraph.contains('"')));
    }

    #[test]
    fn reddit_evidence_is_filtered_when_disabled() {
        let records = vec![CommunityEvidenceRecord {
            entity_id: "society:example".to_string(),
            source_type: "Reddit".to_string(),
            source_url: None,
            fact_key: "resident_discussion".to_string(),
            text: Some("Tanker dependence is a real issue.".to_string()),
            numeric_value: None,
            tags: Vec::new(),
            confidence: 0.7,
            learned_at: chrono::Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap(),
        }];
        let filtered = filter_reddit_evidence(records);
        assert!(filtered.is_empty());
    }
}
