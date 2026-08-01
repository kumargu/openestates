//! Shared enrichment functions used by all routes that return property/society/area data.
//! Single source of truth: every route that returns these types calls these functions.

use std::collections::{BTreeMap, HashMap};

use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::knowledge::edge::Relation;
use crate::knowledge::{FactValue, KnowledgeGraph, SourcedFact, google_reviews_url_from_facts};
use crate::models::{AreaProfile, KgEntityRefs, Property, PropertyCard, Society};
use crate::serving::{ServingFactIndex, SocietyFactProjection};

// ---------------------------------------------------------------------------
// RERA and Area Intelligence response structs
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Debug, Default)]
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
    pub units_per_acre: Option<f64>,
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

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct ReraDecisionAction {
    pub kind: String,
    pub label: String,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
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

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
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

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
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

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ReraDocumentGroupSummary {
    pub group: String,
    pub count: i32,
}

#[derive(Serialize, Clone, Debug, Default)]
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

fn parse_rera_json<T: serde::de::DeserializeOwned>(value: &str) -> Option<T> {
    serde_json::from_str(value).ok()
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
        info.units_per_acre
            .map(|value| format!("{} homes/acre", compact_number(value))),
        info.open_area_pct
            .map(|value| format!("{}% open area", compact_number(value))),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    if !scale_parts.is_empty() {
        cards.push(ReraDecisionCard {
            id: "project_scale".to_string(),
            title: info
                .units_per_acre
                .map(|value| format!("{} homes/acre", compact_number(value)))
                .unwrap_or_else(|| "Project scale available".to_string()),
            detail: scale_parts.join(" · "),
            tone: "neutral".to_string(),
            source: "RERA".to_string(),
            labels: vec!["open-space".to_string()],
            facts: serde_json::json!({
                "total_land_area_acres": info.total_land_area_acres,
                "total_units": info.total_units,
                "units_per_acre": info.units_per_acre,
                "open_area_pct": info.open_area_pct,
            }),
            actions: Vec::new(),
            confidence: 0.84,
            validation_notes: Vec::new(),
        });
    }

    cards
}

fn get_bool_fact(facts: &[SourcedFact], key: &str) -> Option<bool> {
    facts
        .iter()
        .filter(|f| f.key == key)
        .max_by_key(|f| f.version)
        .and_then(|f| match &f.value {
            FactValue::Bool(b) => Some(*b),
            _ => None,
        })
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

pub(crate) fn units_per_acre(total_units: Option<i32>, acres: Option<f64>) -> Option<f64> {
    let units = f64::from(total_units?);
    let acres = acres?;
    (units.is_finite() && acres.is_finite() && units > 0.0 && acres > 0.0).then_some(units / acres)
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
    let total_units = projection
        .latest_numeric("rera_total_units")
        .and_then(|fact| projected_i32(fact.value));
    card.units_per_acre = units_per_acre(total_units, card.society_land_acres);
}

fn projected_i32(value: f64) -> Option<i32> {
    (value.is_finite() && value >= i32::MIN as f64 && value <= i32::MAX as f64)
        .then(|| value.round() as i32)
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

/// Extract RERA information from the knowledge graph for a given society.
/// Returns None if no "rera_registered" fact exists (meaning no RERA data has been ingested).
pub fn extract_rera_info(graph: &KnowledgeGraph, society_id: &str) -> Option<ReraInfo> {
    let node_id = society_node_id(society_id);
    let node = graph.get_node(&node_id)?;
    let facts = &node.facts;

    // Only return ReraInfo if we have RERA data
    let has_rera = facts.iter().any(|f| f.key == "rera_registered");
    if !has_rera {
        return None;
    }

    let registered = get_bool_fact(facts, "rera_registered").unwrap_or(false);

    let total_units = get_numeric_fact(facts, "rera_total_units").map(|n| n as i32);

    // Get last_verified from the rera_registered fact's learned_at timestamp
    let last_verified = get_fact_timestamp(facts, "rera_registered");

    // Map fact keys while deliberately avoiding RERA-entered cost fields.
    // Those costs are not reliable enough for buyer-facing price evidence.
    let registration_number = get_text_fact(facts, "rera_number")
        .or_else(|| get_text_fact(facts, "rera_registration_number"));
    let builder_projects = get_numeric_fact(facts, "rera_builder_projects_count")
        .or_else(|| get_numeric_fact(facts, "rera_builder_total_projects"))
        .map(|n| n as i32);

    // builder_states might be stored as Text (comma-separated) instead of Tags
    let builder_states = {
        let tags = get_tags_fact(facts, "rera_builder_states");
        if tags.is_empty() {
            // Try text version (comma-separated)
            get_text_fact(facts, "rera_builder_states")
                .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
                .unwrap_or_default()
        } else {
            tags
        }
    };

    let total_land_area_sqm = get_numeric_fact(facts, "rera_total_land_area_sqm");
    let total_land_area_acres = total_land_area_sqm.map(|sqm| sqm / 4_046.856_422_4);
    let units_per_acre = units_per_acre(total_units, total_land_area_acres);
    let document_manifest = get_text_fact(facts, "rera_document_manifest")
        .and_then(|value| parse_rera_json::<Vec<ReraDocumentManifestItem>>(&value))
        .unwrap_or_default();
    let complaint_summaries = get_text_fact(facts, "rera_complaint_summary_manifest")
        .and_then(|value| parse_rera_json::<Vec<ReraComplaintScopeSummary>>(&value))
        .unwrap_or_default();

    let mut info = ReraInfo {
        registered,
        registration_number,
        status: get_text_fact(facts, "rera_status"),
        start_date: get_text_fact(facts, "rera_start_date")
            .or_else(|| get_text_fact(facts, "project_start_date")),
        completion_date: get_text_fact(facts, "rera_completion_date"),
        original_completion_date: get_text_fact(facts, "rera_original_completion_date"),
        delay_months: get_numeric_fact(facts, "rera_delay_months").map(|n| n as i32),
        total_units,
        total_land_area_sqm,
        total_land_area_acres,
        open_area_pct: get_numeric_fact(facts, "project_open_area_pct")
            .or_else(|| get_numeric_fact(facts, "rera_open_area_pct")),
        units_per_acre,
        total_project_cost_inr: None,
        land_cost_inr: None,
        construction_cost_inr: None,
        cost_per_unit_inr: None,
        complaints_count: get_numeric_fact(facts, "rera_complaints_count").map(|n| n as i32),
        complaints_resolved_pct: get_numeric_fact(facts, "rera_complaints_resolved_pct"),
        project_complaints_count: get_numeric_fact(facts, "rera_project_complaints_count")
            .map(|n| n as i32),
        project_complaints_open_count: get_numeric_fact(
            facts,
            "rera_project_complaints_open_count",
        )
        .map(|n| n as i32),
        project_complaints_disposed_count: get_numeric_fact(
            facts,
            "rera_project_complaints_disposed_count",
        )
        .map(|n| n as i32),
        promoter_complaints_count: get_numeric_fact(facts, "rera_promoter_complaints_count")
            .map(|n| n as i32),
        promoter_complaints_open_count: get_numeric_fact(
            facts,
            "rera_promoter_complaints_open_count",
        )
        .map(|n| n as i32),
        promoter_complaints_disposed_count: get_numeric_fact(
            facts,
            "rera_promoter_complaints_disposed_count",
        )
        .map(|n| n as i32),
        complaint_summaries,
        document_groups: rera_document_groups(&document_manifest),
        affidavit_only_visible: rera_affidavit_only_visible(&document_manifest),
        document_manifest,
        builder_total_projects: builder_projects,
        builder_revocations: get_numeric_fact(facts, "rera_builder_revocations").map(|n| n as i32),
        builder_states,
        land_litigation: get_bool_fact(facts, "rera_land_litigation"),
        escrow_bank: get_text_fact(facts, "rera_escrow_bank"),
        has_borrowing: get_bool_fact(facts, "rera_has_borrowing"),
        has_mortgage: get_bool_fact(facts, "rera_has_mortgage"),
        rera_portal_url: get_text_fact(facts, "rera_portal_url"),
        last_verified,
        decision_cards: Vec::new(),
    };
    info.decision_cards = rera_decision_cards(&info);
    Some(info)
}

// ---------------------------------------------------------------------------
// Area Intelligence extraction — reads area intelligence facts from KG
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

#[derive(Serialize, Clone, Debug, Default)]
pub struct BuilderTrust {
    pub delivery_rate: Option<f64>,
    pub project_count: Option<u32>,
    pub delivery_display: Option<String>,
    pub zero_revocations: Option<bool>,
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

    let zero_revocations = get_text_fact(facts, "builder_zero_revocations").map(|v| v == "true");

    Some(BuilderTrust {
        delivery_rate,
        project_count,
        delivery_display,
        zero_revocations,
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

// ---------------------------------------------------------------------------
// Data freshness extraction — how recent and rich the data is
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Debug, Default)]
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

/// Extract data freshness information from a society's KG node.
/// Returns None if the society has no KG node.
pub fn extract_data_freshness(graph: &KnowledgeGraph, society_id: &str) -> Option<DataFreshness> {
    let node_id = society_node_id(society_id);
    let node = graph.get_node(&node_id)?;

    // Use the most recent learned_at from any fact for freshness, falling back to node updated_at
    let most_recent_fact_ts = node.facts.iter().map(|f| f.learned_at).max();
    let effective_ts = most_recent_fact_ts.unwrap_or(node.updated_at);
    let last_enriched = effective_ts.to_rfc3339();
    let days_ago = (Utc::now() - effective_ts).num_days().max(0) as u32;

    let freshness_label = if days_ago < 7 {
        "Fresh".to_string()
    } else if days_ago < 30 {
        "Recent".to_string()
    } else if days_ago < 90 {
        "Stale".to_string()
    } else {
        "Very stale".to_string()
    };

    let fact_count = node.facts.len() as u32;

    let mut source_breakdown: HashMap<String, u32> = HashMap::new();
    for fact in &node.facts {
        let source_name = format!("{:?}", fact.source.source_type);
        *source_breakdown.entry(source_name).or_insert(0) += 1;
    }

    Some(DataFreshness {
        last_enriched,
        days_ago,
        freshness_label,
        fact_count,
        source_breakdown,
    })
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

pub fn kg_tags(graph: &KnowledgeGraph, node_id: &str, key: &str) -> Option<Vec<String>> {
    let node = graph.get_node(node_id)?;
    node.facts
        .iter()
        .find(|f| f.key == key)
        .and_then(|f| match &f.value {
            FactValue::Tags(tags) => Some(tags.clone()),
            _ => None,
        })
}

fn is_placeholder(s: &str) -> bool {
    s.is_empty()
        || s.starts_with("Not yet enriched")
        || s.contains("Needs enrichment")
        || s.starts_with("Area discovered")
        || s.starts_with("Society discovered")
}

// ---------------------------------------------------------------------------
// Property card enrichment — used by /properties, /search, /properties/:id
// ---------------------------------------------------------------------------

/// Enrich a Property into a PropertyCard with KG data.
/// This is THE function — every route that returns a PropertyCard must use it.
pub fn enrich_property_card(
    p: &Property,
    societies: &[Society],
    graph: &KnowledgeGraph,
) -> PropertyCard {
    let society_name = societies
        .iter()
        .find(|s| to_slug(&s.id) == to_slug(&p.society_id))
        .map(|s| s.name.clone())
        .unwrap_or_default();

    let node_id = society_node_id(&p.society_id);

    let google_rating = kg_numeric(graph, &node_id, "google_rating");
    let google_review_count = kg_numeric(graph, &node_id, "google_review_count").map(|n| n as u32);
    let google_reviews_url = graph
        .get_node(&node_id)
        .and_then(|node| google_reviews_url_from_facts(&node.facts, &node.name));

    // Use photo_url from KG if property has no hero_image
    let hero_image = if p.hero_image.is_empty() {
        kg_text(graph, &node_id, "photo_url").unwrap_or_default()
    } else {
        p.hero_image.clone()
    };

    // Extract root_source and project_status from the society KG node
    let (root_source, project_status, project_status_display) =
        if let Some(node) = graph.get_node(&node_id) {
            let rs = node.root_source.map(|r| r.as_str().to_string());
            let ps = get_text_fact(&node.facts, "project_status");
            let ps_display = get_fact_display_template(&node.facts, "project_status").map(|tmpl| {
                // Replace {value} placeholder with the actual value
                let val = get_text_fact(&node.facts, "project_status").unwrap_or_default();
                if tmpl.contains("{value}") {
                    tmpl.replace("{value}", &val)
                } else {
                    tmpl
                }
            });
            (rs, ps, ps_display)
        } else {
            (None, None, None)
        };

    // Extract builder delivery display from builder node via BuiltBy edge
    let builder_delivery_display =
        extract_builder_trust(graph, &p.society_id).and_then(|bt| bt.delivery_display);

    // Extract data freshness from the society KG node
    let data_freshness = extract_data_freshness(graph, &p.society_id);

    PropertyCard {
        id: p.id.clone(),
        kg_entity_refs: kg_entity_refs_for_property(p, graph),
        title: p.title.clone(),
        area: p.area.clone(),
        price: p.price,
        price_per_sqft: p.price_per_sqft,
        bhk: p.bhk,
        sqft: p.carpet_area_sqft,
        carpet_area_sqft: p.carpet_area_sqft,
        super_builtup_sqft: p.super_builtup_sqft,
        society_name,
        builder_name: p.builder_name.clone(),
        hero_image,
        transparency_tags: compact_transparency_tags(&p.transparency_tags),
        description_summary: p.description_summary.clone(),
        possession_status: p.possession_status.clone(),
        metro_distance_mins: p.metro_distance_mins,
        floor: p.floor,
        total_floors: p.total_floors,
        facing: p.facing.clone(),
        google_rating,
        google_review_count,
        google_reviews_url,
        society_land_acres: None,
        open_space_pct: None,
        units_per_acre: None,
        root_source,
        project_status,
        project_status_display,
        home_state_display: None,
        builder_delivery_display,
        data_freshness,
        floor_plan_preview_url: None,
        plan_carpet_area_sqft: None,
        plan_sale_area_sqft: None,
        plan_configuration_type: None,
        decision_labels: Vec::new(),
        decision_check_summary: None,
    }
}

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

// ---------------------------------------------------------------------------
// Society enrichment — overlays KG facts onto in-memory data
// ---------------------------------------------------------------------------

/// Enrich a Society with knowledge graph facts. Mutates in place.
pub fn enrich_society(society: &mut Society, graph: &KnowledgeGraph) {
    let node_id = society_node_id(&society.id);

    if graph.get_node(&node_id).is_none() {
        return;
    }

    if let Some(node) = graph.get_node(&node_id) {
        society.google_reviews_url = google_reviews_url_from_facts(&node.facts, &node.name);
        if society.future_google_place_id.is_none() {
            society.future_google_place_id = kg_text(graph, &node_id, "google_place_id");
        }
    }

    if is_placeholder(&society.review_summary) {
        if let Some(val) = kg_text(graph, &node_id, "google_sentiment") {
            society.review_summary = val;
        }
    }

    if is_placeholder(&society.maintenance_sentiment) {
        if let Some(val) = kg_text(graph, &node_id, "google_sentiment") {
            society.maintenance_sentiment = if val.to_lowercase().contains("maintenance") {
                val
            } else {
                "See reviews".to_string()
            };
        }
    }

    if is_placeholder(&society.livability_sentiment) {
        if let Some(val) = kg_text(graph, &node_id, "google_sentiment") {
            society.livability_sentiment = val.chars().take(120).collect();
        }
    }

    if society.common_positives.is_empty() {
        if let Some(tags) = kg_tags(graph, &node_id, "google_top_positives") {
            society.common_positives = tags;
        }
    }

    if society.common_complaints.is_empty() {
        if let Some(tags) = kg_tags(graph, &node_id, "google_top_negatives") {
            society.common_complaints = tags;
        }
    }
}

// ---------------------------------------------------------------------------
// Area enrichment — overlays KG facts onto in-memory data
// ---------------------------------------------------------------------------

/// Enrich an AreaProfile with knowledge graph facts. Mutates in place.
pub fn enrich_area(area: &mut AreaProfile, graph: &KnowledgeGraph) {
    let node_id = area_node_id(&area.name);

    if graph.get_node(&node_id).is_none() {
        return;
    }

    let set_if_placeholder = |field: &mut String, key: &str| {
        if is_placeholder(field) {
            if let Some(val) = kg_text(graph, &node_id, key) {
                *field = val;
            }
        }
    };

    set_if_placeholder(&mut area.metro_access_summary, "metro_details");
    set_if_placeholder(&mut area.traffic_summary, "traffic_reality");
    set_if_placeholder(&mut area.waterlogging_summary, "waterlogging_detail");
    set_if_placeholder(&mut area.livability_summary, "livability_summary");
    set_if_placeholder(&mut area.community_notes, "area_vibe");

    // Trend
    if is_placeholder(&area.trend_summary) {
        if let Some(trend) = kg_text(graph, &node_id, "price_trend") {
            area.trend_summary = format!("Price trend: {}", trend);
            area.trend_direction = trend;
        }
    }

    // Tags
    if area.externality_tags.is_empty() {
        let mut tags = Vec::new();
        if let Some(wl) = kg_text(graph, &node_id, "waterlogging_risk") {
            tags.push(format!("Waterlogging: {}", wl));
        }
        if let Some(m) = kg_text(graph, &node_id, "metro_access") {
            tags.push(format!("Metro: {}", m));
        }
        if let Some(p) = kg_text(graph, &node_id, "price_trend") {
            tags.push(format!("Price: {}", p));
        }
        if !tags.is_empty() {
            area.externality_tags = tags;
        }
    }

    if area.infrastructure_tags.is_empty() {
        let mut tags = Vec::new();
        if let Some(infra) = kg_text(graph, &node_id, "upcoming_infra") {
            let truncated: String = infra.chars().take(80).collect();
            tags.push(truncated);
        }
        if let Some(schools) = kg_tags(graph, &node_id, "school_quality") {
            for s in schools.into_iter().take(3) {
                tags.push(format!("School: {}", s));
            }
        }
        if !tags.is_empty() {
            area.infrastructure_tags = tags;
        }
    }
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
