//! Shared enrichment functions used by all routes that return property/society/area data.
//! Single source of truth: every route that returns these types calls these functions.

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::models::PropertyCard;
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
    if society_id.starts_with("society:") {
        return society_id.to_string();
    }
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

// ---------------------------------------------------------------------------
// KG fact extraction helpers
// ---------------------------------------------------------------------------

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
