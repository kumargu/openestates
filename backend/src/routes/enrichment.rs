//! Shared enrichment functions used by all routes that return property/society/area data.
//! Single source of truth: every route that returns these types calls these functions.

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::knowledge::edge::Relation;
use crate::knowledge::{FactValue, KnowledgeGraph, SourcedFact};
use crate::models::{KgEntityRefs, Property, PropertyCard};
use crate::serving::{ServingFactIndex, SocietyFactProjection};

// ---------------------------------------------------------------------------
// RERA and Area Intelligence response structs
// ---------------------------------------------------------------------------

#[derive(schemars::JsonSchema, Serialize, Clone, Debug, Default)]
pub struct ReraInfo {
    pub registered: bool,
    pub registration_number: Option<String>,
    pub status: Option<String>,
    pub start_date: Option<String>,
    pub completion_date: Option<String>,
    pub original_completion_date: Option<String>,
    pub delay_months: Option<i32>,
    pub total_units: Option<i32>,
    pub total_land_area_sqm: Option<f64>,
    pub total_land_area_acres: Option<f64>,
    pub open_area_pct: Option<f64>,
    pub total_project_cost_inr: Option<f64>,
    pub land_cost_inr: Option<f64>,
    pub construction_cost_inr: Option<f64>,
    pub cost_per_unit_inr: Option<f64>,
    pub complaints_count: Option<i32>,
    pub complaints_resolved_pct: Option<f64>,
    pub project_complaints_count: Option<i32>,
    pub project_complaints_open_count: Option<i32>,
    pub project_complaints_disposed_count: Option<i32>,
    pub promoter_complaints_count: Option<i32>,
    pub promoter_complaints_open_count: Option<i32>,
    pub promoter_complaints_disposed_count: Option<i32>,
    pub complaint_summaries: Vec<ReraComplaintScopeSummary>,
    pub document_manifest: Vec<ReraDocumentManifestItem>,
    pub document_groups: Vec<ReraDocumentGroupSummary>,
    pub schedule_sections: Vec<ReraScheduleSection>,
    pub affidavit_only_visible: Option<bool>,
    pub builder_total_projects: Option<i32>,
    pub builder_revocations: Option<i32>,
    pub builder_states: Vec<String>,
    pub land_litigation: Option<bool>,
    pub escrow_bank: Option<String>,
    pub has_borrowing: Option<bool>,
    pub has_mortgage: Option<bool>,
    pub rera_portal_url: Option<String>,
    pub last_verified: Option<String>,
    pub decision_cards: Vec<ReraDecisionCard>,
}

#[derive(schemars::JsonSchema, Serialize, Clone, Debug, Default, PartialEq)]
pub struct ReraDecisionAction {
    pub kind: String,
    pub label: String,
}

#[derive(schemars::JsonSchema, Serialize, Clone, Debug, Default, PartialEq)]
pub struct ReraDecisionCard {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub tone: String,
    pub source: String,
    pub labels: Vec<String>,
    pub facts: serde_json::Value,
    pub actions: Vec<ReraDecisionAction>,
    pub confidence: f64,
    pub validation_notes: Vec<String>,
}

#[derive(schemars::JsonSchema, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ReraComplaintScopeSummary {
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub total_count_from_tab_label: Option<i32>,
    #[serde(default)]
    pub row_count_parsed: i32,
    #[serde(default)]
    pub disposed_count: i32,
    #[serde(default)]
    pub open_count: i32,
    #[serde(default)]
    pub theme_counts: HashMap<String, i32>,
    #[serde(default)]
    pub sample_subjects: Vec<String>,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub validation_notes: Vec<String>,
}

#[derive(schemars::JsonSchema, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ReraDocumentManifestItem {
    #[serde(default)]
    pub artifact_id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub source_tab: Option<String>,
    #[serde(default)]
    pub source_field_label: Option<String>,
    #[serde(default)]
    pub document_group: String,
    #[serde(default)]
    pub buyer_visibility: Option<String>,
    #[serde(default)]
    pub preview_policy: Option<String>,
    #[serde(default)]
    pub configuration_type: Option<String>,
    #[serde(default)]
    pub bedroom_count: Option<f64>,
    #[serde(default)]
    pub confidence: Option<f64>,
}

#[derive(schemars::JsonSchema, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ReraDocumentGroupSummary {
    pub group: String,
    pub count: i32,
}

#[derive(schemars::JsonSchema, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ReraScheduleSection {
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub rows: Vec<ReraScheduleRow>,
}

#[derive(schemars::JsonSchema, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ReraScheduleRow {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub available: Option<bool>,
    #[serde(default)]
    pub area_sqm: Option<f64>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub confidence: Option<f64>,
}

#[derive(schemars::JsonSchema, Serialize, Clone, Debug, Default)]
pub struct AreaIntelligence {
    pub safety: Option<String>,
    pub commute_reality: Option<String>,
    pub water_supply: Option<String>,
    pub noise_level: Option<String>,
    pub green_cover: Option<String>,
    pub community_vibe: Option<String>,
    pub walkability: Option<String>,
    pub school_quality: Option<String>,
    pub grocery_shopping: Option<String>,
    pub healthcare_access: Option<String>,
    pub recurring_complaints: Vec<String>,
    pub hidden_gems: Vec<String>,
    pub deal_breakers: Vec<String>,
    pub overall_sentiment: Option<String>,
    pub source_count: Option<i32>,
    pub last_updated: Option<String>,
}

// ---------------------------------------------------------------------------
// Fact extraction helpers — work on a node's facts slice
// ---------------------------------------------------------------------------

fn get_text_fact(facts: &[SourcedFact], key: &str) -> Option<String> {
    facts
        .iter()
        .filter(|f| f.key == key)
        .max_by_key(|f| f.version)
        .and_then(|f| match &f.value {
            FactValue::Text(s) => Some(s.clone()),
            _ => None,
        })
}

fn get_numeric_fact(facts: &[SourcedFact], key: &str) -> Option<f64> {
    facts
        .iter()
        .filter(|f| f.key == key)
        .max_by_key(|f| f.version)
        .and_then(|f| match &f.value {
            FactValue::Numeric(n) => Some(*n),
            _ => None,
        })
}

pub fn rera_document_groups(
    manifest: &[ReraDocumentManifestItem],
) -> Vec<ReraDocumentGroupSummary> {
    let mut counts = BTreeMap::<String, i32>::new();
    for item in manifest {
        let group = item.document_group.trim();
        if group.is_empty() {
            continue;
        }
        *counts.entry(group.to_string()).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|(group, count)| ReraDocumentGroupSummary { group, count })
        .collect()
}

pub fn rera_affidavit_only_visible(manifest: &[ReraDocumentManifestItem]) -> Option<bool> {
    if manifest.is_empty() {
        return None;
    }
    Some(manifest.iter().all(|item| item.kind == "affidavit"))
}

fn compact_number(value: f64) -> String {
    if (value.fract()).abs() < 0.05 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.1}")
    }
}

fn compact_i32(value: i32) -> String {
    value.to_string()
}

fn format_rera_month(value: Option<&String>) -> Option<String> {
    let raw = value?.trim();
    if raw.is_empty() {
        return None;
    }
    for format in ["%Y-%m-%d", "%d-%m-%Y", "%d/%m/%Y"] {
        if let Ok(date) = NaiveDate::parse_from_str(raw, format) {
            return Some(date.format("%b %Y").to_string());
        }
    }
    Some(raw.to_string())
}

fn card_action(kind: &str, label: &str) -> ReraDecisionAction {
    ReraDecisionAction {
        kind: kind.to_string(),
        label: label.to_string(),
    }
}

fn document_group_label(group: &str) -> String {
    let normalized = group.trim().to_ascii_lowercase().replace(['_', '-'], " ");
    if normalized.contains("site") {
        "site plan".to_string()
    } else if normalized.contains("floor") {
        "floor plan".to_string()
    } else if normalized.contains("khata") {
        "khata".to_string()
    } else if normalized.contains("approval") || normalized.contains("noc") {
        "approvals/NOCs".to_string()
    } else if normalized.contains("legal") || normalized.contains("land") {
        "land files".to_string()
    } else if normalized.contains("affidavit") {
        "affidavit".to_string()
    } else if normalized.is_empty() {
        "other files".to_string()
    } else {
        normalized
    }
}

fn rolled_complaint_theme(theme: &str) -> &'static str {
    match theme {
        "refund" | "cancellation" => "Money back / refund",
        "delay" | "possession" | "compensation" => "Delay / possession",
        "agreement_payment" | "interest_demand" => "Payment dispute",
        "title_land" | "khata" | "approval_oc_cc" | "registration_document" => {
            "Legal/title documents"
        }
        "quality" => "Construction quality",
        "amenities" | "parking" | "maintenance" => "Amenities / upkeep",
        "builder_conduct" => "Builder conduct",
        _ => "Other",
    }
}

fn top_theme_labels(theme_counts: &HashMap<String, i32>, limit: usize) -> Vec<String> {
    let mut rolled = BTreeMap::<String, i32>::new();
    for (theme, count) in theme_counts {
        if *count <= 0 {
            continue;
        }
        *rolled
            .entry(rolled_complaint_theme(theme).to_string())
            .or_insert(0) += *count;
    }
    let mut rows: Vec<(String, i32)> = rolled.into_iter().collect();
    rows.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    rows.into_iter()
        .take(limit)
        .map(|(label, _)| label)
        .collect()
}

fn rera_complaint_card(summary: &ReraComplaintScopeSummary) -> Option<ReraDecisionCard> {
    let total = summary
        .total_count_from_tab_label
        .unwrap_or(summary.row_count_parsed);
    if total <= 0 && summary.open_count <= 0 && summary.disposed_count <= 0 {
        return None;
    }
    let scope_label = if summary.scope.to_ascii_lowercase().contains("promoter") {
        "promoter"
    } else {
        "project"
    };
    let top_themes = top_theme_labels(&summary.theme_counts, 2);
    let title = if let Some(first) = top_themes.first() {
        format!("Mostly {} complaints", first.to_ascii_lowercase())
    } else {
        format!("{} {} complaints", compact_i32(total), scope_label)
    };
    let detail = [
        Some(format!("{} {} complaints", compact_i32(total), scope_label)),
        (summary.open_count > 0).then(|| format!("{} open", compact_i32(summary.open_count))),
        (summary.disposed_count > 0)
            .then(|| format!("{} disposed", compact_i32(summary.disposed_count))),
        (!top_themes.is_empty()).then(|| format!("themes: {}", top_themes.join(", "))),
        (!summary.validation_notes.is_empty()).then(|| "parsed with caveats".to_string()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    Some(ReraDecisionCard {
        id: format!("complaints_{scope_label}"),
        title,
        detail,
        tone: if summary.open_count > 0 || total > 0 {
            "watch".to_string()
        } else {
            "positive".to_string()
        },
        source: "RERA complaints".to_string(),
        labels: vec!["legal".to_string(), "risk".to_string()],
        facts: serde_json::json!({
            "scope": summary.scope.clone(),
            "total": total,
            "open": summary.open_count,
            "disposed": summary.disposed_count,
            "fine_theme_counts": summary.theme_counts.clone(),
            "rolled_up_themes": top_themes,
            "sample_subjects": summary.sample_subjects.clone(),
        }),
        actions: vec![card_action("open_source", "Open complaints")],
        confidence: summary.confidence,
        validation_notes: summary.validation_notes.clone(),
    })
}

pub fn rera_decision_cards(info: &ReraInfo) -> Vec<ReraDecisionCard> {
    let mut cards = Vec::new();

    if info.registered || info.registration_number.is_some() {
        cards.push(ReraDecisionCard {
            id: "registration".to_string(),
            title: if info.registered {
                "RERA registered".to_string()
            } else {
                "RERA number available".to_string()
            },
            detail: [info.registration_number.clone(), info.status.clone()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · "),
            tone: if info.registered {
                "positive"
            } else {
                "neutral"
            }
            .to_string(),
            source: "RERA".to_string(),
            labels: vec!["legal".to_string()],
            facts: serde_json::json!({
                "registered": info.registered,
                "registration_number": info.registration_number.clone(),
                "status": info.status.clone(),
            }),
            actions: vec![card_action("open_source", "Open source")],
            confidence: 0.9,
            validation_notes: Vec::new(),
        });
    }

    let original_target = format_rera_month(info.original_completion_date.as_ref());
    let current_target = format_rera_month(info.completion_date.as_ref());
    if let Some(delay) = info.delay_months.filter(|value| *value > 0) {
        cards.push(ReraDecisionCard {
            id: "delivery_movement".to_string(),
            title: format!("Delivery moved by {} months", compact_i32(delay)),
            detail: [
                original_target.map(|value| format!("original {value}")),
                current_target.map(|value| format!("current {value}")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · "),
            tone: "watch".to_string(),
            source: "RERA schedule".to_string(),
            labels: vec!["legal".to_string(), "risk".to_string()],
            facts: serde_json::json!({
                "delay_months": delay,
                "original_completion_date": info.original_completion_date.clone(),
                "completion_date": info.completion_date.clone(),
            }),
            actions: vec![card_action("save_note", "Remember")],
            confidence: 0.86,
            validation_notes: Vec::new(),
        });
    }

    for summary in &info.complaint_summaries {
        if let Some(card) = rera_complaint_card(summary) {
            cards.push(card);
        }
    }

    if !info.document_groups.is_empty() {
        let labels: Vec<String> = info
            .document_groups
            .iter()
            .filter(|group| group.count > 0)
            .map(|group| document_group_label(&group.group))
            .collect();
        let has_plan = labels.iter().any(|label| label.contains("plan"));
        cards.push(ReraDecisionCard {
            id: "official_files".to_string(),
            title: "Official files available".to_string(),
            detail: labels
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
            tone: "neutral".to_string(),
            source: "RERA documents".to_string(),
            labels: if has_plan {
                vec!["legal".to_string(), "layout".to_string()]
            } else {
                vec!["legal".to_string()]
            },
            facts: serde_json::json!({
                "document_groups": info.document_groups.clone(),
                "manifest_count": info.document_manifest.len(),
                "affidavit_only_visible": info.affidavit_only_visible,
            }),
            actions: vec![card_action("request_file", "Request file")],
            confidence: 0.82,
            validation_notes: Vec::new(),
        });
    }

    let mut land_signals = Vec::new();
    if info.land_litigation == Some(true) {
        land_signals.push("land litigation recorded");
    }
    if info.has_mortgage == Some(true) {
        land_signals.push("mortgage reported");
    }
    if info.has_borrowing == Some(true) {
        land_signals.push("borrowing reported");
    }
    if !land_signals.is_empty() {
        cards.push(ReraDecisionCard {
            id: "legal_follow_up".to_string(),
            title: "Legal follow-up needed".to_string(),
            detail: land_signals.join(" · "),
            tone: "watch".to_string(),
            source: "RERA".to_string(),
            labels: vec!["legal".to_string(), "risk".to_string()],
            facts: serde_json::json!({
                "land_litigation": info.land_litigation,
                "has_mortgage": info.has_mortgage,
                "has_borrowing": info.has_borrowing,
            }),
            actions: vec![card_action("ask_lawyer", "Ask lawyer")],
            confidence: 0.78,
            validation_notes: vec!["Not a legal opinion; verify source documents.".to_string()],
        });
    }

    let scale_parts = [
        info.total_land_area_acres
            .map(|value| format!("{} acres", compact_number(value))),
        info.total_units
            .map(|value| format!("{} homes", compact_i32(value))),
        info.open_area_pct
            .map(|value| format!("{}% open area", compact_number(value))),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    if !scale_parts.is_empty() {
        cards.push(ReraDecisionCard {
            id: "project_scale".to_string(),
            title: "Project scale".to_string(),
            detail: scale_parts.join(" · "),
            tone: "neutral".to_string(),
            source: "RERA".to_string(),
            labels: vec!["open-space".to_string()],
            facts: serde_json::json!({
                "total_land_area_acres": info.total_land_area_acres,
                "total_units": info.total_units,
                "open_area_pct": info.open_area_pct,
            }),
            actions: Vec::new(),
            confidence: 0.84,
            validation_notes: Vec::new(),
        });
    }

    cards
}

fn get_fact_display_template(facts: &[SourcedFact], key: &str) -> Option<String> {
    facts
        .iter()
        .filter(|f| f.key == key)
        .max_by_key(|f| f.version)
        .and_then(|f| f.display_template.clone())
}

fn get_tags_fact(facts: &[SourcedFact], key: &str) -> Vec<String> {
    facts
        .iter()
        .filter(|f| f.key == key)
        .max_by_key(|f| f.version)
        .and_then(|f| match &f.value {
            FactValue::Tags(tags) => Some(tags.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

pub(crate) fn overlay_project_scale_facts(
    card: &mut PropertyCard,
    serving_facts: &ServingFactIndex,
    society_id: &str,
) {
    let projection = SocietyFactProjection::from_index(serving_facts, society_id);
    if let Some(fact) = projection.latest_numeric("rera_total_land_area_sqm") {
        card.society_land_acres = Some(fact.value / 4_046.856_422_4);
    } else if let Some(fact) = projection.latest_numeric("project_land_area_acres") {
        card.society_land_acres = Some(fact.value);
    }
    if let Some(fact) = projection
        .latest_numeric("project_open_area_pct")
        .or_else(|| projection.latest_numeric("rera_open_area_pct"))
    {
        card.open_space_pct = Some(fact.value);
    }
}

/// Get the learned_at timestamp from any fact matching the key, formatted as ISO string.
fn get_fact_timestamp(facts: &[SourcedFact], key: &str) -> Option<String> {
    facts
        .iter()
        .filter(|f| f.key == key)
        .max_by_key(|f| f.version)
        .map(|f| f.learned_at.to_rfc3339())
}

// ---------------------------------------------------------------------------
// RERA extraction — reads rera_* facts from a society KG node
// ---------------------------------------------------------------------------

/// Extract area intelligence from the knowledge graph for a given area.
/// Returns None if no Reddit-sourced area intelligence facts exist.
pub fn extract_area_intelligence(
    graph: &KnowledgeGraph,
    area_id: &str,
) -> Option<AreaIntelligence> {
    let node_id = area_node_id(area_id);
    let node = graph.get_node(&node_id)?;
    let facts = &node.facts;

    // Check if we have any area intelligence facts (Reddit-sourced or LLM-sourced)
    let intelligence_keys = [
        "safety",
        "commute_reality",
        "water_supply",
        "noise_level",
        "green_cover",
        "community_vibe",
        "walkability",
        "school_quality",
        "grocery_shopping",
        "healthcare_access",
        "recurring_complaints",
        "hidden_gems",
        "deal_breakers",
        "overall_sentiment",
    ];
    let has_intelligence = facts
        .iter()
        .any(|f| intelligence_keys.contains(&f.key.as_str()));
    if !has_intelligence {
        return None;
    }

    // Count source threads (look for source_count fact or count Reddit-sourced facts)
    let source_count = get_numeric_fact(facts, "source_count").map(|n| n as i32);
    let last_updated = get_fact_timestamp(facts, "safety")
        .or_else(|| get_fact_timestamp(facts, "overall_sentiment"));

    Some(AreaIntelligence {
        safety: get_text_fact(facts, "safety"),
        commute_reality: get_text_fact(facts, "commute_reality"),
        water_supply: get_text_fact(facts, "water_supply"),
        noise_level: get_text_fact(facts, "noise_level"),
        green_cover: get_text_fact(facts, "green_cover"),
        community_vibe: get_text_fact(facts, "community_vibe"),
        walkability: get_text_fact(facts, "walkability"),
        school_quality: get_text_fact(facts, "school_quality"),
        grocery_shopping: get_text_fact(facts, "grocery_shopping"),
        healthcare_access: get_text_fact(facts, "healthcare_access"),
        recurring_complaints: get_tags_fact(facts, "recurring_complaints"),
        hidden_gems: get_tags_fact(facts, "hidden_gems"),
        deal_breakers: get_tags_fact(facts, "deal_breakers"),
        overall_sentiment: get_text_fact(facts, "overall_sentiment"),
        source_count,
        last_updated,
    })
}

// ---------------------------------------------------------------------------
// Builder trust extraction — reads builder facts via BuiltBy edges
// ---------------------------------------------------------------------------

#[derive(schemars::JsonSchema, Serialize, Clone, Debug, Default)]
pub struct BuilderTrust {
    pub delivery_rate: Option<f64>,
    pub project_count: Option<u32>,
    pub delivery_display: Option<String>,
}

/// Extract builder trust from a facts slice — shared logic between direct and canonical builder.
fn builder_trust_from_facts(facts: &[SourcedFact]) -> Option<BuilderTrust> {
    let delivery_rate = get_numeric_fact(facts, "builder_delivery_rate");
    let project_count = get_numeric_fact(facts, "builder_project_count").map(|n| n as u32);

    // Only return BuilderTrust if we have delivery data
    if delivery_rate.is_none() && project_count.is_none() {
        return None;
    }

    let delivery_display =
        get_fact_display_template(facts, "builder_delivery_rate").and_then(|tmpl| {
            if tmpl.contains("{value}") {
                delivery_rate.map(|r| {
                    let pct = (r * 100.0) as u32;
                    tmpl.replace("{value}", &pct.to_string())
                })
            } else {
                Some(tmpl)
            }
        });

    Some(BuilderTrust {
        delivery_rate,
        project_count,
        delivery_display,
    })
}

/// Extract builder trust data by traversing BuiltBy edges from society to builder node.
/// If the builder has a `canonical_builder` fact (orphan resolution), follows the
/// reference to the canonical builder node and reads delivery data from there.
/// Returns None if no builder node found or no delivery data.
pub fn extract_builder_trust(graph: &KnowledgeGraph, society_id: &str) -> Option<BuilderTrust> {
    let soc_node_id = society_node_id(society_id);

    // Find builder nodes connected via BuiltBy edge (society -> builder)
    let builder_nodes = graph.neighbors(&soc_node_id, Some(Relation::BuiltBy));
    let builder = builder_nodes.first()?;

    // Check for canonical_builder fact — if present, follow to canonical builder node
    // and read delivery data from there instead of the orphan.
    if let Some(canonical_id) = get_text_fact(&builder.facts, "canonical_builder") {
        if let Some(canonical_node) = graph.get_node(&canonical_id) {
            if let Some(trust) = builder_trust_from_facts(&canonical_node.facts) {
                return Some(trust);
            }
        }
    }

    // Fall back to direct builder facts
    builder_trust_from_facts(&builder.facts)
}

/// Legacy optional API shape. Search and detail responses do not calculate
/// freshness or age from timestamps.
#[derive(schemars::JsonSchema, Serialize, Clone, Debug, Default)]
pub struct DataFreshness {
    /// ISO timestamp of last enrichment
    pub last_enriched: String,
    /// How many days ago the node was last updated
    pub days_ago: u32,
    /// Human-readable label: "Fresh", "Recent", "Stale", "Very stale"
    pub freshness_label: String,
    /// Total number of facts on the node
    pub fact_count: u32,
    /// Breakdown of facts by source type, e.g. {"Rera": 5, "Reddit": 3}
    pub source_breakdown: HashMap<String, u32>,
}

// ---------------------------------------------------------------------------
// Slug normalization — single canonical implementation
// ---------------------------------------------------------------------------

/// Canonical slug: lowercase, hyphens, no "soc-" prefix.
pub fn to_slug(id: &str) -> String {
    let s = id.to_lowercase().replace(['_', ' '], "-");
    s.strip_prefix("soc-").unwrap_or(&s).to_string()
}

/// Build a society node ID for KG lookup.
pub fn society_node_id(society_id: &str) -> String {
    let normalized = society_id.trim().to_lowercase().replace(['_', ' '], "-");
    if normalized.starts_with("society:") {
        normalized
    } else {
        format!("society:{}", to_slug(&normalized))
    }
}

/// Build an area node ID for KG lookup.
pub fn area_node_id(area_name: &str) -> String {
    format!("area:{}", to_slug(area_name))
}

pub fn property_node_id(property_id: &str) -> String {
    let normalized = property_id.trim().to_lowercase().replace(['_', ' '], "-");
    if normalized.starts_with("property:") {
        normalized
    } else {
        format!("property:{normalized}")
    }
}

pub fn kg_entity_refs_for_property(p: &Property, graph: &KnowledgeGraph) -> KgEntityRefs {
    let property_entity_id = property_node_id(&p.id);
    let society_entity_id = society_node_id(&p.society_id);
    let area_entity_id = area_node_id(&p.area);
    let builder_entity_id = graph
        .edges_from(&society_entity_id)
        .iter()
        .find(|edge| edge.relation == Relation::BuiltBy && graph.get_node(&edge.to).is_some())
        .map(|edge| edge.to.clone());

    let mut source_entity_ids = vec![
        property_entity_id.clone(),
        society_entity_id.clone(),
        area_entity_id.clone(),
    ];
    if let Some(builder_entity_id) = &builder_entity_id {
        source_entity_ids.push(builder_entity_id.clone());
    }
    source_entity_ids.retain(|id| graph.get_node(id).is_some());
    source_entity_ids.sort();
    source_entity_ids.dedup();

    KgEntityRefs {
        property_entity_id,
        society_entity_id,
        area_entity_id,
        builder_entity_id,
        source_entity_ids,
    }
}

// ---------------------------------------------------------------------------
// KG fact extraction helpers
// ---------------------------------------------------------------------------

pub fn kg_numeric(graph: &KnowledgeGraph, node_id: &str, key: &str) -> Option<f64> {
    let node = graph.get_node(node_id)?;
    node.facts
        .iter()
        .find(|f| f.key == key)
        .and_then(|f| match &f.value {
            FactValue::Numeric(n) => Some(*n),
            _ => None,
        })
}

pub fn kg_text(graph: &KnowledgeGraph, node_id: &str, key: &str) -> Option<String> {
    let node = graph.get_node(node_id)?;
    node.facts
        .iter()
        .find(|f| f.key == key)
        .and_then(|f| match &f.value {
            FactValue::Text(s) => Some(s.clone()),
            _ => None,
        })
}

// ---------------------------------------------------------------------------
// Property card enrichment — used by /properties, /search, /properties/:id
// ---------------------------------------------------------------------------

pub fn compact_transparency_tags(tags: &[String]) -> Vec<String> {
    let mut compact = tags.iter().take(3).cloned().collect::<Vec<_>>();
    if tags
        .iter()
        .any(|tag| tag.eq_ignore_ascii_case("Price unavailable"))
        && !compact
            .iter()
            .any(|tag| tag.eq_ignore_ascii_case("Price unavailable"))
    {
        compact.push("Price unavailable".to_string());
    }
    compact
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::edge::Edge;
    use crate::knowledge::fact::{FactSource, FactValue, SourceType, SourcedFact};
    use crate::knowledge::graph::KnowledgeGraph;
    use crate::knowledge::node::{Node, NodeType};

    fn make_text_fact(key: &str, value: &str) -> SourcedFact {
        SourcedFact {
            key: key.into(),
            value: FactValue::Text(value.into()),
            confidence: 0.9,
            source: FactSource {
                source_type: SourceType::Manual,
                url: None,
                model: None,
                skill_id: None,
                triggered_by: None,
            },
            learned_at: chrono::Utc::now(),
            version: 1,
            display_template: None,
            answers_preferences: Vec::new(),
            scoring_hint: None,
        }
    }

    fn make_numeric_fact(key: &str, value: f64) -> SourcedFact {
        SourcedFact {
            key: key.into(),
            value: FactValue::Numeric(value),
            confidence: 0.9,
            source: FactSource {
                source_type: SourceType::Manual,
                url: None,
                model: None,
                skill_id: None,
                triggered_by: None,
            },
            learned_at: chrono::Utc::now(),
            version: 1,
            display_template: Some("Delivery rate: {value}%".into()),
            answers_preferences: Vec::new(),
            scoring_hint: None,
        }
    }

    #[test]
    fn test_canonical_builder_resolution() {
        let mut g = KnowledgeGraph::new();

        // Create society node
        let soc_id = "society:test-society";
        g.add_node(Node::new(soc_id, NodeType::Society, "Test Society"));

        // Create orphan builder node with canonical_builder pointing to canonical
        let orphan_id = "builder:orphan-builder";
        let mut orphan = Node::new(orphan_id, NodeType::Builder, "Orphan Builder");
        orphan.add_fact(make_text_fact(
            "canonical_builder",
            "builder:canonical-builder",
        ));
        g.add_node(orphan);

        // Create canonical builder node with actual delivery data
        let canonical_id = "builder:canonical-builder";
        let mut canonical = Node::new(canonical_id, NodeType::Builder, "Canonical Builder");
        canonical.add_fact(make_numeric_fact("builder_delivery_rate", 0.85));
        canonical.add_fact(make_numeric_fact("builder_project_count", 12.0));
        g.add_node(canonical);

        // Add BuiltBy edge from society to orphan builder
        g.add_edge(Edge::new(
            soc_id.to_string(),
            orphan_id.to_string(),
            Relation::BuiltBy,
        ));

        // Extract builder trust — should follow canonical_builder to canonical node
        let trust = extract_builder_trust(&g, "test-society").unwrap();
        assert!(
            (trust.delivery_rate.unwrap() - 0.85).abs() < 0.001,
            "Should read delivery_rate from canonical builder, got {:?}",
            trust.delivery_rate
        );
        assert_eq!(trust.project_count, Some(12));
    }

    #[test]
    fn test_builder_trust_direct_when_no_canonical() {
        let mut g = KnowledgeGraph::new();

        // Create society node
        let soc_id = "society:direct-society";
        g.add_node(Node::new(soc_id, NodeType::Society, "Direct Society"));

        // Create builder node with delivery data but NO canonical_builder fact
        let builder_id = "builder:direct-builder";
        let mut builder = Node::new(builder_id, NodeType::Builder, "Direct Builder");
        builder.add_fact(make_numeric_fact("builder_delivery_rate", 0.90));
        g.add_node(builder);

        // Add BuiltBy edge
        g.add_edge(Edge::new(
            soc_id.to_string(),
            builder_id.to_string(),
            Relation::BuiltBy,
        ));

        let trust = extract_builder_trust(&g, "direct-society").unwrap();
        assert!(
            (trust.delivery_rate.unwrap() - 0.90).abs() < 0.001,
            "Should read delivery_rate directly from builder"
        );
    }
}
