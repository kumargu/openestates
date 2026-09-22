use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::assets::{
    ReraAssertionMode, ReraClaimDerivation, ReraClaimEffectiveTime, ReraClaimSubject, ReraClaimV1,
    ReraClaimValue, ReraSourceTrust,
};
use crate::decision_labels::{DecisionCheckSummary, DecisionLabel};
use crate::models::{KgEntityRefs, PropertyCard};
use crate::recommendations::{
    build_recommendation_branches, RecommendationBranch, RecommendationBranchInputs,
    RecommendationEnvelope, RecommendationResponse, RecommendationStatus,
    RECOMMENDATION_ENGINE_VERSION,
};
use crate::scoring::scoring_policy;
use crate::serving::{
    GoogleReviewEvidence, LoadedServingBundle, ReraEvidenceEntity, ReraEvidenceEvent,
    ReraEvidenceSeries, ReraEvidenceSource, ReraRegulatoryCoverage, ServingFactIndex,
    ServingFactRecord, ServingReraEvidenceRecord, SocietyFactProjection,
    RERA_EVIDENCE_SCHEMA_VERSION,
};
use crate::state::AppState;

use crate::community::{
    community_evidence_from_fact_value, community_pulse_from_summary,
    deterministic_community_summarizer, CommunityPulse,
};
use crate::dag_config::{
    evidence_sections_config, fact_registry_index_config, rera_report_surface_config,
    ContextFactDefinition, EvidenceSectionDefinition, EvidenceSectionPresentation,
    FactRegistryIndex, ReraReportSurfaceFile,
};
use crate::knowledge::FactValue;
use crate::livability_brief::{
    compose_livability_brief, filter_reddit_evidence, LivabilityBrief, LivabilityBriefInput,
    LivabilityLens, StructuredFactSignal,
};

use super::enrichment::{
    overlay_project_scale_facts, rera_affidavit_only_visible, rera_decision_cards,
    rera_document_groups, society_node_id, ReraComplaintScopeSummary, ReraDocumentManifestItem,
    ReraInfo, ReraScheduleSection,
};

pub(crate) fn property_card(
    property: &crate::models::Property,
    societies: &[crate::models::Society],
) -> PropertyCard {
    let name = societies
        .iter()
        .find(|society| society_node_id(&society.id) == society_node_id(&property.society_id))
        .map(|society| society.name.as_str())
        .unwrap_or("");
    property.to_card(name)
}

/// GET /api/properties — returns UI-ready property cards.
pub async fn list_properties(State(state): State<Arc<AppState>>) -> Result<Response, StatusCode> {
    let runtime = state.search_runtime.load_full();
    let version = format!(
        "{}:{}:{}",
        runtime.version_key.serving_bundle_version,
        runtime.version_key.scoring_policy_version,
        runtime.version_key.search_engine_version
    );
    let mut cache = state.property_catalog_cache.lock().await;
    if let Some((cached_version, bytes)) = cache.as_ref() {
        if cached_version == &version {
            return Ok(serialized_catalog_response(bytes.clone()));
        }
    }

    let bytes = state
        .execution
        .run_customer_compute(move || {
            let serving_facts = &runtime.bundle.fact_index;
            let cards: Vec<PropertyCard> = runtime
                .properties
                .iter()
                .filter(|property| property.is_listable())
                .map(|property| {
                    let card = property_card(property, &runtime.societies);
                    overlay_serving_google_reviews(card, &property.society_id, Some(serving_facts))
                })
                .collect();
            serde_json::to_vec(&cards)
        })
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .map(Bytes::from)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    *cache = Some((version, bytes.clone()));
    Ok(serialized_catalog_response(bytes))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertySummaryRequest {
    pub property_ids: Vec<String>,
    pub snapshot_identity: Option<String>,
}

#[derive(schemars::JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertySummaries {
    pub contract_version: u32,
    pub snapshot_identity: String,
    pub items: Vec<PropertyCard>,
    pub missing_ids: Vec<String>,
}

pub async fn property_summaries(
    State(state): State<Arc<AppState>>,
    Json(request): Json<PropertySummaryRequest>,
) -> Result<Json<PropertySummaries>, (StatusCode, Json<ErrorResponse>)> {
    let runtime = state.search_runtime.load_full();
    let limits = &crate::security::security_tuning().context_requests;
    let error = |status, code: &str| {
        (
            status,
            Json(ErrorResponse {
                error: code.to_string(),
            }),
        )
    };
    if request.property_ids.len() > limits.batch_property_limit
        || request
            .property_ids
            .iter()
            .any(|id| id.is_empty() || id.len() > limits.max_property_id_bytes)
    {
        return Err(error(StatusCode::BAD_REQUEST, "invalid_property_ids"));
    }
    let snapshot_identity = runtime.bundle.manifest.proof_snapshot_identity();
    if request
        .snapshot_identity
        .as_deref()
        .is_some_and(|expected| expected != snapshot_identity)
    {
        return Err(error(StatusCode::CONFLICT, "stale_snapshot"));
    }
    let mut items = Vec::new();
    let mut missing_ids = Vec::new();
    let mut seen = HashSet::new();
    for id in request.property_ids {
        if !seen.insert(id.clone()) {
            continue;
        }
        if let Some(property) = runtime
            .property_by_id
            .get(&id)
            .and_then(|index| runtime.properties.get(*index))
        {
            items.push(overlay_serving_google_reviews(
                property_card(property, &runtime.societies),
                &property.society_id,
                Some(&runtime.bundle.fact_index),
            ));
        } else {
            missing_ids.push(id);
        }
    }
    Ok(Json(PropertySummaries {
        contract_version: 1,
        snapshot_identity: snapshot_identity.to_string(),
        items,
        missing_ids,
    }))
}

fn serialized_catalog_response(bytes: Bytes) -> Response {
    let mut response = Body::from(bytes).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=60"),
    );
    response
}

#[derive(schemars::JsonSchema, Serialize)]
pub struct PropertyDetail {
    pub availability: crate::models::property::InventoryAvailability,
    pub contract_version: u32,
    pub snapshot_identity: String,
    pub property: crate::public_contract::PropertyAttributes,
    /// Stable graph IDs the UI can dereference to render dynamic KG-backed sections.
    pub entity_refs: KgEntityRefs,
    /// Canonical UI read model for dynamic proof-backed cards on the property page.
    ///
    /// New UI should render optional property-page sections from this field
    /// instead of hardcoding cards or calling legacy KG endpoints directly.
    /// `source_panels` below is retained as a compatibility field for the
    /// current frontend while it migrates.
    pub evidence: PropertyEvidenceResponse,
    pub society: Option<crate::public_contract::SocietySummary>,
    /// Counterfactual branches — why you might consider an alternative instead.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recommendation_branches: Vec<RecommendationBranch>,
    /// Async recommendation status. The detail page should fetch the branch cards
    /// from `/api/properties/{id}/recommendations` instead of doing this work inline.
    pub recommendations: RecommendationEnvelope,
    /// RERA regulatory data from the knowledge graph (None if not yet enriched).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rera: Option<ReraInfo>,
    /// Compact link to the dedicated RERA evidence report.
    pub rera_report_ref: ReraReportRef,
    /// Config-derived labels intended for notes and compare surfaces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decision_labels: Vec<DecisionLabel>,
    /// Grouped project-check read model for the buyer-facing detail page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_check_summary: Option<DecisionCheckSummary>,
    /// Lowest price_per_sqft among properties in the same area.
    pub area_price_range_low: Option<u64>,
    /// Highest price_per_sqft among properties in the same area.
    pub area_price_range_high: Option<u64>,
    /// Number of buyers who have expressed interest.
    pub interest_count: usize,
    /// Where the society data originally came from: "rera", "seller", "discovered", "legacy"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_source: Option<String>,
    /// Human-readable project status from skill's display_template
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_status_display: Option<String>,
    /// Compact buyer-facing state signal for first-scan UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home_state_display: Option<String>,
    /// Machine-readable project status: "ready_to_move", "under_construction", etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_status: Option<String>,
    /// Other locally tracked projects tied to the same normalized legal promoter name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builder_portfolio: Option<BuilderPortfolio>,
    /// Current external review evidence projected from the Parquet serving bundle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_reviews: Option<ExternalReviews>,
    /// Buyer-facing positive themes from external reviews and resident feedback.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detail_signals: Vec<DetailSignal>,
    /// Receipt-backed livability diligence brief composed from DAG facts and mined themes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub livability_brief: Option<LivabilityBrief>,
    pub context: crate::property_context::PropertyContext,
    /// Buyer-facing site overview + floor plans (RERA brochure promotions).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plans: Option<crate::plans::ProjectPlansView>,
}

#[derive(schemars::JsonSchema, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReraEvidenceAvailability {
    Available,
    Partial,
    Unavailable,
}

#[derive(schemars::JsonSchema, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct ReraReportRef {
    pub registration_ids: Vec<String>,
    pub href: String,
    pub availability: ReraEvidenceAvailability,
}

#[derive(Serialize, Clone, Debug)]
pub struct ReraEvidenceReportResponse {
    pub availability: ReraEvidenceAvailability,
    pub evidence: ReraEvidenceProjectionResponse,
    pub surface: ReraReportSurfaceResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buyer_report: Option<ReraBuyerReport>,
}

/// Buyer-oriented read model layered over the canonical evidence projection.
/// These rows come from the same promoted serving bundle and remain separate
/// from immutable claims so older broad facts are never misrepresented as L2.
#[derive(Serialize, Clone, Debug)]
pub struct ReraBuyerReport {
    pub fact_sections: Vec<ReraBuyerFactSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builder_portfolio: Option<BuilderPortfolio>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub complaints: Vec<ReraBuyerComplaintSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schedules: Vec<ReraScheduleSection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documents: Vec<ReraBuyerDocument>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry_url: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ReraBuyerFactSection {
    pub id: String,
    pub title: String,
    pub facts: Vec<ReraBuyerFact>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ReraBuyerFact {
    pub key: String,
    pub label: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    pub learned_at: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct ReraBuyerComplaintSummary {
    pub scope: String,
    pub total: i32,
    pub open: i32,
    pub disposed: i32,
    pub rows_parsed: i32,
    pub status_counts_complete: bool,
    pub theme_counts: BTreeMap<String, i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sample_subjects: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ReraBuyerDocument {
    pub id: String,
    pub label: String,
    pub group: String,
    pub group_label: String,
    pub url: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct ReraEvidenceProjectionResponse {
    pub schema_version: String,
    pub property_id: String,
    pub bundle_id: String,
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub registration_ids: Vec<String>,
    pub entities: Vec<ReraEvidenceEntity>,
    pub claims: Vec<ReraPublicClaim>,
    pub events: Vec<ReraEvidenceEvent>,
    pub series: Vec<ReraEvidenceSeries>,
    pub discrepancies: Vec<crate::assets::ReraInventoryReconciliationV1>,
    pub regulatory_coverage: Vec<ReraRegulatoryCoverage>,
    pub source_index: Vec<ReraEvidenceSource>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ReraPublicClaim {
    pub claim_id: String,
    pub subject: ReraClaimSubject,
    pub predicate: String,
    pub value: ReraClaimValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_time: Option<ReraClaimEffectiveTime>,
    pub assertion_mode: ReraAssertionMode,
    pub source_trust: ReraSourceTrust,
    pub evidence: Vec<ReraPublicClaimEvidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derivation: Option<ReraClaimDerivation>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ReraPublicClaimEvidence {
    pub source_record_id: String,
    pub receipt_id: String,
    pub capture_id: String,
    pub locator: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supporting_quote: Option<String>,
}

impl From<&ReraClaimV1> for ReraPublicClaim {
    fn from(claim: &ReraClaimV1) -> Self {
        Self {
            claim_id: claim.claim_id.clone(),
            subject: claim.subject.clone(),
            predicate: claim.predicate.clone(),
            value: claim.value.clone(),
            unit: claim.unit.clone(),
            effective_time: claim.effective_time.clone(),
            assertion_mode: claim.assertion_mode,
            source_trust: claim.source_trust,
            evidence: claim
                .evidence
                .iter()
                .map(|evidence| ReraPublicClaimEvidence {
                    source_record_id: evidence.source_record_id.clone(),
                    receipt_id: evidence.receipt_id.clone(),
                    capture_id: evidence.capture_id.clone(),
                    locator: evidence.locator.clone(),
                    page: evidence.page,
                    supporting_quote: evidence.supporting_quote.clone(),
                })
                .collect(),
            derivation: claim.derivation.clone(),
        }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct ReraReportSurfaceResponse {
    pub version: u32,
    pub coverage_note: String,
    pub regulatory_event_order: Vec<String>,
    pub sections: Vec<ReraReportSurfaceSectionResponse>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ReraReportSurfaceSectionResponse {
    pub id: String,
    pub title: String,
    pub renderer: String,
    pub selectors: Vec<crate::dag_config::ReraReportSelectorRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_per_page: Option<usize>,
    pub preview_kinds: Vec<String>,
    pub empty_behavior: String,
}

#[derive(schemars::JsonSchema, Serialize, Clone, Debug, PartialEq)]
pub struct ExternalReviews {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub google_rating: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u32")]
    pub google_review_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub google_reviews_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviews: Vec<ExternalReviewCard>,
}

#[derive(schemars::JsonSchema, Serialize, Clone, Debug, PartialEq)]
pub struct ExternalReviewCard {
    pub id: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "f64")]
    pub rating: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub date_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u32")]
    pub helpful_count: Option<u32>,
    pub text: String,
    pub tone: ReviewTone,
}

#[derive(schemars::JsonSchema, Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewTone {
    Positive,
    Concern,
    Neutral,
}

#[derive(Deserialize)]
struct StructuredGoogleReview {
    text: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    rating: Option<f64>,
    #[serde(default)]
    date_label: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    helpful_count: Option<u32>,
}

struct RankedReview {
    card: ExternalReviewCard,
    helpful_count: u32,
    recency_score: i64,
    source_order: usize,
}

#[derive(schemars::JsonSchema, Serialize, Clone, Debug, PartialEq)]
pub struct DetailSignal {
    pub key: String,
    pub label: String,
    pub icon: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u32")]
    pub count: Option<u32>,
}

#[derive(schemars::JsonSchema, Serialize, Clone, Debug)]
pub struct BuilderPortfolio {
    pub builder_name: String,
    pub tracked_projects: usize,
    pub rera_registered_projects: usize,
    pub delayed_projects: usize,
    pub complaint_projects: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "i32")]
    pub revocations: Option<i32>,
    pub projects: Vec<BuilderProjectRecord>,
}

#[derive(schemars::JsonSchema, Serialize, Clone, Debug)]
pub struct BuilderProjectRecord {
    pub property_id: String,
    pub project_name: String,
    pub area: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub rera_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub rera_portal_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub rera_status: Option<String>,
    pub rera_registered: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub start_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub completion_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "i32")]
    pub delay_months: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "i32")]
    pub complaints_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub project_status_display: Option<String>,
    pub current: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct SourcePanel {
    pub kind: String,
    pub title: String,
    pub subtitle: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationship: Option<String>,
    pub priority: u32,
    pub constellation: String,
    pub presentation: EvidencePresentation,
    pub items: Vec<SourceItem>,
    pub missing: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<EvidenceMediaStrip>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community_pulse: Option<CommunityPulse>,
}

#[derive(schemars::JsonSchema, Serialize, Clone, Debug)]
pub struct SourceItem {
    pub evidence: Vec<crate::serving::EvidenceRef>,
    pub entity_id: String,
    pub key: String,
    pub label: String,
    pub value: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub relationship: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    pub source_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributions: Vec<SourceAttribution>,
    #[serde(skip_serializing)]
    pub confidence_pct: u8,
    pub learned_at: String,
}

#[derive(schemars::JsonSchema, Serialize, Clone, Debug, PartialEq)]
pub struct SourceAttribution {
    pub evidence: crate::serving::EvidenceRef,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub source_url: Option<String>,
    pub source_type: String,
    #[serde(skip_serializing)]
    pub confidence_pct: u8,
    pub learned_at: String,
}

const APPROACH_ROAD_MEDIA_FRAME_LIMIT: usize = 6;

#[derive(schemars::JsonSchema, Serialize, Clone, Debug, PartialEq)]
pub struct EvidenceMediaStrip {
    pub kind: String,
    pub provider: String,
    pub title: String,
    pub caption: String,
    pub capture_date_label: String,
    pub coverage_quality: String,
    pub frames: Vec<EvidenceMediaFrame>,
}

#[derive(schemars::JsonSchema, Serialize, Clone, Debug, PartialEq)]
pub struct EvidenceMediaFrame {
    pub label: String,
    pub distance_from_gate_m: u32,
    pub image_url: String,
    pub heading: f64,
    pub pitch: f64,
    pub fov: f64,
    pub capture_date: String,
    pub source_url: String,
}

#[derive(Deserialize)]
struct ApproachRoadVisualRecord {
    provider: String,
    coverage_quality: String,
    frames: Vec<ApproachRoadVisualFrameRecord>,
}

#[derive(Deserialize)]
struct ApproachRoadVisualFrameRecord {
    label: String,
    distance_from_gate_m: u32,
    #[serde(default)]
    pano_id: Option<String>,
    #[serde(default)]
    latitude: Option<f64>,
    #[serde(default)]
    longitude: Option<f64>,
    #[serde(default)]
    location_query: Option<String>,
    #[serde(default)]
    radius_m: Option<u32>,
    heading: f64,
    pitch: f64,
    fov: f64,
    capture_date: String,
    #[serde(default)]
    image_url: Option<String>,
}

#[derive(schemars::JsonSchema, Serialize, Clone, Debug)]
pub struct PropertyEvidenceResponse {
    pub property_id: String,
    pub entity_refs: KgEntityRefs,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub serving_bundle_version: Option<String>,
    pub sections: Vec<EvidenceSection>,
}

#[derive(schemars::JsonSchema, Serialize, Clone, Debug)]
pub struct EvidenceSection {
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub subtitle: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub relationship: Option<String>,
    pub priority: u32,
    pub constellation: String,
    pub header_meta: String,
    pub confidence_pct: u8,
    pub source_types: Vec<String>,
    pub entity_ids: Vec<String>,
    pub presentation: EvidencePresentation,
    pub items: Vec<SourceItem>,
    pub missing: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<EvidenceMediaStrip>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "CommunityPulse")]
    pub community_pulse: Option<CommunityPulse>,
}

#[derive(schemars::JsonSchema, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EvidencePresentation {
    pub variant: String,
    pub density: String,
    pub max_preview_items: usize,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

fn recommendation_cache_key(property_id: &str, serving_bundle_version: Option<&str>) -> String {
    format!(
        "property:{property_id}:bundle:{}:policy:{}:engine:{}",
        serving_bundle_version.unwrap_or("missing"),
        scoring_policy().version,
        RECOMMENDATION_ENGINE_VERSION
    )
}

fn recommendation_envelope_for(
    property_id: &str,
    serving_bundle_version: Option<String>,
) -> RecommendationEnvelope {
    RecommendationEnvelope {
        status: RecommendationStatus::Pending,
        cache_key: recommendation_cache_key(property_id, serving_bundle_version.as_deref()),
        engine_version: RECOMMENDATION_ENGINE_VERSION.to_string(),
        scoring_policy_version: scoring_policy().version,
        serving_bundle_version,
    }
}

fn area_median_ppsf_for(
    property: &crate::models::Property,
    properties: &[crate::models::Property],
) -> Option<u64> {
    let measurement = property.area_measurement.as_ref()?;
    let mut values = properties
        .iter()
        .filter(|candidate| {
            candidate.area_id == property.area_id
                && candidate.area_measurement.as_ref().is_some_and(|other| {
                    other.basis == measurement.basis && other.unit == measurement.unit
                })
        })
        .filter_map(|candidate| (candidate.price_per_sqft > 0).then_some(candidate.price_per_sqft))
        .collect::<Vec<_>>();
    values.sort_unstable();
    values.get(values.len() / 2).copied()
}

fn find_property_by_request_id<'a>(
    properties: &'a [crate::models::Property],
    id: &str,
) -> Option<&'a crate::models::Property> {
    let canonical_id = id;
    properties.iter().find(|p| p.id == canonical_id)
}

fn project_name_for(property: &crate::models::Property) -> String {
    let in_prefix = format!("{} BHK in ", property.bhk);
    let at_prefix = format!("{} BHK at ", property.bhk);
    property
        .title
        .strip_prefix(&in_prefix)
        .or_else(|| property.title.strip_prefix(&at_prefix))
        .unwrap_or(&property.title)
        .to_string()
}

fn project_status_display_for(
    facts: Option<&ServingFactIndex>,
    society_id: &str,
) -> Option<String> {
    SocietyFactProjection::from_index(facts?, society_id)
        .project_status(None, None)
        .display
}

fn fact_value_display(value: &FactValue) -> String {
    match value {
        FactValue::Numeric(n) => {
            if n.fract().abs() < f64::EPSILON {
                format!("{}", *n as i64)
            } else {
                format!("{n:.1}")
            }
        }
        FactValue::Text(text) => text.clone(),
        FactValue::Bool(value) => {
            if *value {
                "Yes".to_string()
            } else {
                "No".to_string()
            }
        }
        FactValue::Tags(tags) => tags.join(", "),
        FactValue::Score { value, explanation } => format!("{value:.1}: {explanation}"),
    }
}

fn is_low_signal_source_value(key: &str, value: &str) -> bool {
    let normalized = value.trim().to_lowercase();
    if normalized.is_empty() {
        return true;
    }

    match key {
        "best_quote" | "sentiment_summary" | "resident_sentiment" | "google_sentiment" => {
            normalized.contains("not available")
                || normalized.contains("not captured")
                || normalized.contains("not provided")
                || normalized.contains("no specific resident sentiment")
                || normalized.contains("no reddit discussion content")
                || normalized.contains("no verbatim quotes")
                || normalized.contains("direct resident sentiment")
                || normalized.contains("actual content of the reddit discussions was not provided")
                || normalized.contains("quote can be extracted")
        }
        _ => false,
    }
}

fn serving_source_item(
    snapshot_identity: &str,
    projection: &SocietyFactProjection<'_>,
    key: &str,
    label: &str,
) -> Option<SourceItem> {
    serving_source_item_with_display_key(snapshot_identity, projection, key, key, label)
}

fn serving_source_item_with_display_key(
    snapshot_identity: &str,
    projection: &SocietyFactProjection<'_>,
    fact_key: &str,
    display_key: &str,
    label: &str,
) -> Option<SourceItem> {
    let fact = projection
        .records(fact_key)
        .into_iter()
        .find(|fact| eligible_source_observation(fact).is_some())?;
    let display_template = projection
        .search_metadata(fact_key)
        .and_then(|metadata| metadata.display_template.as_deref());
    source_item_from_serving_fact(
        snapshot_identity,
        fact,
        fact_key,
        display_key,
        label,
        display_template,
    )
}

fn serving_entity_source_item(
    snapshot_identity: &str,
    facts: &ServingFactIndex,
    entity_id: &str,
    fact_key: &str,
    display_key: &str,
    label: &str,
) -> Option<SourceItem> {
    let rows = facts.entity(entity_id)?;
    let fact = rows
        .facts
        .iter()
        .filter(|fact| fact.fact_key == fact_key && eligible_source_observation(fact).is_some())
        .max_by(|left, right| {
            left.confidence.total_cmp(&right.confidence).then_with(|| {
                left.stable_selection_key()
                    .cmp(&right.stable_selection_key())
            })
        })?;
    let display_template = rows
        .search_metadata_for_fact_key(fact_key)
        .next()
        .and_then(|metadata| metadata.display_template.as_deref());
    source_item_from_serving_fact(
        snapshot_identity,
        fact,
        fact_key,
        display_key,
        label,
        display_template,
    )
}

fn eligible_source_observation(
    fact: &crate::serving::ServingFactRecord,
) -> Option<&crate::serving::SourceObservation> {
    fact.observation.as_ref().filter(|observation| {
        observation.subject_entity_id == fact.entity_id && observation.validate().is_ok()
    })
}

fn source_item_from_serving_fact(
    snapshot_identity: &str,
    fact: &crate::serving::ServingFactRecord,
    fact_key: &str,
    display_key: &str,
    label: &str,
    display_template: Option<&str>,
) -> Option<SourceItem> {
    let observation = eligible_source_observation(fact)?;
    let evidence = crate::serving::EvidenceRef::for_observation(snapshot_identity, observation);
    let values = match &fact.value {
        FactValue::Tags(tags) => tags.clone(),
        _ => Vec::new(),
    };
    let raw_value = fact_value_display(&fact.value);
    let value = if fact_key == "google_review_snippets" && !values.is_empty() {
        format!("{} Google review highlights", values.len())
    } else {
        display_template
            .map(|template| template.replace("{value}", &raw_value))
            .unwrap_or(raw_value)
    };
    if is_low_signal_source_value(display_key, &value) {
        return None;
    }
    Some(SourceItem {
        evidence: vec![evidence],
        entity_id: fact.entity_id.clone(),
        key: display_key.to_string(),
        label: label.to_string(),
        value,
        scope: entity_scope(&fact.entity_id).to_string(),
        relationship: None,
        values,
        source_type: fact.source_type.clone(),
        source_url: fact.source_url.clone(),
        attributions: Vec::new(),
        confidence_pct: (fact.confidence * 100.0).round().clamp(0.0, 100.0) as u8,
        learned_at: fact.learned_at.to_rfc3339(),
    })
}

#[derive(Clone)]
struct SourceValue {
    evidence: crate::serving::EvidenceRef,
    value: String,
    source_url: Option<String>,
    source_type: String,
    confidence_pct: u8,
    learned_at: chrono::DateTime<chrono::Utc>,
}

fn serving_multi_source_item(
    snapshot_identity: &str,
    projection: &SocietyFactProjection<'_>,
    fact_key: &str,
    label: &str,
    max_values: usize,
) -> Option<SourceItem> {
    let mut values = Vec::<SourceValue>::new();
    for fact in projection.records(fact_key) {
        let Some(observation) = eligible_source_observation(fact) else {
            continue;
        };
        let evidence = crate::serving::EvidenceRef::for_observation(snapshot_identity, observation);
        match &fact.value {
            FactValue::Text(value) if !value.trim().is_empty() => values.push(SourceValue {
                evidence: evidence.clone(),
                value: value.trim().to_string(),
                source_url: fact.source_url.clone(),
                source_type: fact.source_type.clone(),
                confidence_pct: (fact.confidence * 100.0).round().clamp(0.0, 100.0) as u8,
                learned_at: fact.learned_at,
            }),
            FactValue::Tags(tags) => {
                for value in tags
                    .iter()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                {
                    values.push(SourceValue {
                        evidence: evidence.clone(),
                        value: value.to_string(),
                        source_url: fact.source_url.clone(),
                        source_type: fact.source_type.clone(),
                        confidence_pct: (fact.confidence * 100.0).round().clamp(0.0, 100.0) as u8,
                        learned_at: fact.learned_at,
                    });
                }
            }
            _ => {}
        }
    }
    values.sort_by(|left, right| {
        nearby_distance_key(&left.value)
            .partial_cmp(&nearby_distance_key(&right.value))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.confidence_pct.cmp(&left.confidence_pct))
            .then_with(|| left.value.cmp(&right.value))
    });
    values.dedup_by(|left, right| left.value == right.value && left.source_url == right.source_url);
    values.truncate(max_values);
    if values.is_empty() {
        return None;
    }

    let first = values[0].clone();
    let learned_at = values
        .iter()
        .map(|value| value.learned_at)
        .max()
        .unwrap_or(first.learned_at);
    let confidence_pct = values
        .iter()
        .map(|value| value.confidence_pct)
        .max()
        .unwrap_or(0);
    let attributions = values
        .iter()
        .map(|value| SourceAttribution {
            evidence: value.evidence.clone(),
            value: value.value.clone(),
            source_url: value.source_url.clone(),
            source_type: value.source_type.clone(),
            confidence_pct: value.confidence_pct,
            learned_at: value.learned_at.to_rfc3339(),
        })
        .collect::<Vec<_>>();
    let item_values = values
        .iter()
        .map(|value| value.value.clone())
        .collect::<Vec<_>>();
    let value = if item_values.len() == 1 {
        item_values[0].clone()
    } else {
        format!("{} map-backed places", item_values.len())
    };

    Some(SourceItem {
        evidence: values.iter().map(|value| value.evidence.clone()).collect(),
        entity_id: projection
            .records(fact_key)
            .first()
            .map(|fact| fact.entity_id.clone())
            .unwrap_or_default(),
        key: fact_key.to_string(),
        label: label.to_string(),
        value,
        scope: "society".to_string(),
        relationship: None,
        values: item_values,
        source_type: first.source_type,
        source_url: first.source_url,
        attributions,
        confidence_pct,
        learned_at: learned_at.to_rfc3339(),
    })
}

fn nearby_distance_key(value: &str) -> f64 {
    let Some(open_index) = value.find('(') else {
        return f64::INFINITY;
    };
    let tail = &value[open_index + 1..];
    let Some(km_index) = tail.find(" km") else {
        return f64::INFINITY;
    };
    tail[..km_index]
        .trim()
        .parse::<f64>()
        .unwrap_or(f64::INFINITY)
}

fn collect_community_evidence_records(
    projection: Option<&SocietyFactProjection<'_>>,
) -> Vec<crate::community::CommunityEvidenceRecord> {
    const KEYS: &[&str] = &[
        "google_rating",
        "google_review_count",
        "google_review_snippets",
        "google_reviews_url",
        "resident_discussion",
    ];
    filter_reddit_evidence(collect_community_records_for_keys(projection, KEYS))
}

fn collect_structured_livability_facts(
    property: &crate::models::Property,
    projection: Option<&SocietyFactProjection<'_>>,
    serving_facts: Option<&ServingFactIndex>,
    graph_index: Option<&crate::graph::GraphIndex>,
) -> Vec<StructuredFactSignal> {
    let society_id = society_node_id(&property.society_id);
    let area_id = property.area_id.clone();

    let mut signals = Vec::new();
    for fact in buyer_context_definitions()
        .iter()
        .flat_map(|section| &section.facts)
        .filter(|fact| fact.livability_lens.is_some())
    {
        let lens = fact
            .livability_lens
            .as_deref()
            .map(livability_lens_from_config)
            .unwrap_or(LivabilityLens::Judgment);
        let entity_id = match fact.scope.as_str() {
            "area" => area_id.as_str(),
            _ => society_id.as_str(),
        };
        let has_fact = if fact.scope == "road_segment" {
            serving_facts.is_some_and(|facts| {
                road_segment_entity_ids(property, facts, graph_index)
                    .iter()
                    .any(|entity_id| {
                        facts.entity(entity_id).is_some_and(|rows| {
                            rows.facts
                                .iter()
                                .any(|serving_fact| serving_fact.fact_key == fact.key)
                        })
                    })
            })
        } else {
            projection
                .and_then(|projection| projection.latest_record(&fact.key))
                .is_some()
                || serving_facts
                    .and_then(|facts| facts.entity(entity_id))
                    .is_some_and(|rows| rows.facts.iter().any(|row| row.fact_key == fact.key))
        };
        if has_fact {
            signals.push(StructuredFactSignal {
                fact_key: fact.key.clone(),
                label: fact
                    .livability_label
                    .as_deref()
                    .unwrap_or(&fact.label)
                    .to_string(),
                lens,
            });
        }
    }
    signals
}

fn livability_lens_from_config(value: &str) -> LivabilityLens {
    match value {
        "operating" => LivabilityLens::Operating,
        "risk" => LivabilityLens::Risk,
        "positive" => LivabilityLens::Positive,
        "lifecycle" => LivabilityLens::Lifecycle,
        _ => LivabilityLens::Judgment,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_livability_brief(
    property: &crate::models::Property,
    society_name: &str,
    projection: Option<&SocietyFactProjection<'_>>,
    serving_facts: Option<&ServingFactIndex>,
    graph_index: Option<&crate::graph::GraphIndex>,
    community_records: &[crate::community::CommunityEvidenceRecord],
    community_pulse: Option<&CommunityPulse>,
) -> Option<LivabilityBrief> {
    let home_state_evidence = projection.map(|projection| projection.project_home_state());
    let home_state = home_state_evidence
        .as_ref()
        .and_then(|evidence| evidence.state.as_deref());
    let home_age_bucket = home_state_evidence
        .as_ref()
        .and_then(|evidence| evidence.age_bucket.as_deref());
    let home_timeline_state = projection
        .and_then(|projection| projection.latest_text("home_timeline_state"))
        .map(|fact| fact.value);
    let home_timeline_ref = home_timeline_state.as_deref();
    let structured_facts =
        collect_structured_livability_facts(property, projection, serving_facts, graph_index);
    let (community_positives, community_concerns, source_urls) =
        if let Some(pulse) = community_pulse {
            // Brief owns synthesized themes; pulse keeps review receipts only.
            (&[][..], &[][..], pulse.source_urls.as_slice())
        } else {
            (&[][..], &[][..], &[][..])
        };

    compose_livability_brief(&LivabilityBriefInput {
        society_name,
        area_name: &property.area,
        home_state,
        home_age_bucket,
        home_timeline_state: home_timeline_ref,
        evidence_records: community_records,
        structured_facts: &structured_facts,
        community_positives,
        community_concerns,
        source_urls,
    })
}

fn collect_community_records_for_keys(
    projection: Option<&SocietyFactProjection<'_>>,
    keys: &[&str],
) -> Vec<crate::community::CommunityEvidenceRecord> {
    keys.iter()
        .filter_map(|key| {
            let fact = projection?.latest_record(key)?;
            eligible_source_observation(fact)?;
            community_evidence_from_fact_value(
                &fact.entity_id,
                &fact.source_type,
                fact.source_url.clone(),
                key,
                &fact.value,
                fact.confidence,
                fact.learned_at,
            )
        })
        .collect()
}

fn entity_scope(entity_id: &str) -> &'static str {
    if entity_id.starts_with("property:") {
        "property"
    } else if entity_id.starts_with("society:") || entity_id.starts_with("soc-") {
        "society"
    } else if entity_id.starts_with("area:") || entity_id.starts_with("area-") {
        "area"
    } else if entity_id.starts_with("builder:") {
        "builder"
    } else if entity_id.starts_with("road_segment:") {
        "road_segment"
    } else if entity_id.starts_with("road:") {
        "road"
    } else if entity_id.starts_with("poi:") {
        "poi"
    } else if entity_id.starts_with("waterbody:") {
        "waterbody"
    } else {
        "entity"
    }
}

fn approach_road_media_for(
    property: &crate::models::Property,
    serving_facts: Option<&ServingFactIndex>,
    graph_index: Option<&crate::graph::GraphIndex>,
) -> Option<EvidenceMediaStrip> {
    let api_key = crate::street_view::google_maps_api_key();
    let facts = serving_facts?;
    let road_entity_id = resolve_road_segment_entity_id(property, facts, graph_index)?;
    let fact = facts
        .entity(&road_entity_id)?
        .facts
        .iter()
        .find(|fact| fact.fact_key == "media.approach_road_frames")?;
    let payload = match &fact.value {
        FactValue::Text(text) => text.as_str(),
        _ => return None,
    };
    let record: ApproachRoadVisualRecord = serde_json::from_str(payload).ok()?;
    if record.coverage_quality == "missing" || record.frames.is_empty() {
        return None;
    }

    let frames = record
        .frames
        .into_iter()
        .take(APPROACH_ROAD_MEDIA_FRAME_LIMIT)
        .filter_map(|frame| approach_road_media_frame(frame, api_key.as_deref()))
        .collect::<Vec<_>>();
    if frames.is_empty() {
        return None;
    }

    let capture_date_label = capture_date_label(&frames);
    let caption = format!("Last-lane context · {capture_date_label}");
    Some(EvidenceMediaStrip {
        kind: "street_view_strip".to_string(),
        provider: record.provider,
        title: "Approach road".to_string(),
        caption,
        capture_date_label,
        coverage_quality: record.coverage_quality,
        frames,
    })
}

fn resolve_road_segment_entity_id(
    property: &crate::models::Property,
    serving_facts: &ServingFactIndex,
    graph_index: Option<&crate::graph::GraphIndex>,
) -> Option<String> {
    road_segment_entity_ids(property, serving_facts, graph_index)
        .into_iter()
        .find(|entity_id| has_approach_road_frames(serving_facts, entity_id))
}

fn road_segment_entity_ids(
    property: &crate::models::Property,
    serving_facts: &ServingFactIndex,
    graph_index: Option<&crate::graph::GraphIndex>,
) -> Vec<String> {
    let society_id = crate::routes::enrichment::society_node_id(&property.society_id);
    let mut entity_ids = Vec::new();
    let mut seen = HashSet::new();

    if let Some(index) = graph_index {
        let steps = index.walk_out(&society_id, &["served_by_road"], 1);
        for step in steps {
            if !step.to_entity_id.starts_with("road_segment:") {
                continue;
            }
            if serving_facts.entity(&step.to_entity_id).is_none() {
                continue;
            }
            if seen.insert(step.to_entity_id.clone()) {
                entity_ids.push(step.to_entity_id);
            }
        }
    }

    let slug = society_id
        .strip_prefix("society:")
        .unwrap_or(society_id.as_str());
    let fallback = format!("road_segment:{slug}-approach");
    if serving_facts.entity(&fallback).is_some() && seen.insert(fallback.clone()) {
        entity_ids.push(fallback);
    }

    entity_ids
}

fn has_approach_road_frames(serving_facts: &ServingFactIndex, entity_id: &str) -> bool {
    serving_facts.entity(entity_id).is_some_and(|rows| {
        rows.facts
            .iter()
            .any(|fact| fact.fact_key == "media.approach_road_frames")
    })
}

fn approach_road_media_frame(
    frame: ApproachRoadVisualFrameRecord,
    api_key: Option<&str>,
) -> Option<EvidenceMediaFrame> {
    let street_input = crate::street_view::StreetViewFrameInput {
        pano_id: frame.pano_id.clone(),
        location: match (frame.latitude, frame.longitude) {
            (Some(latitude), Some(longitude)) => Some(crate::street_view::StreetViewLocation {
                latitude,
                longitude,
            }),
            _ => None,
        },
        location_query: frame.location_query.clone(),
        radius_m: frame.radius_m,
        heading: frame.heading,
        pitch: frame.pitch,
        fov: frame.fov,
    };
    let image_url = frame
        .image_url
        .filter(|url| !url.trim().is_empty())
        .or_else(|| {
            api_key.and_then(|api_key| {
                crate::street_view::street_view_static_url(&street_input, api_key)
            })
        })?;
    let source_url = crate::street_view::street_view_pano_url(&street_input)?;

    Some(EvidenceMediaFrame {
        label: frame.label,
        distance_from_gate_m: frame.distance_from_gate_m,
        image_url,
        heading: frame.heading,
        pitch: frame.pitch,
        fov: frame.fov,
        capture_date: frame.capture_date,
        source_url,
    })
}

fn capture_date_label(frames: &[EvidenceMediaFrame]) -> String {
    frames
        .iter()
        .map(|frame| frame.capture_date.as_str())
        .max()
        .map(|date| format!("Street View {date}"))
        .unwrap_or_else(|| "Street View".to_string())
}

fn evidence_section_definition(kind: &str) -> Option<&'static EvidenceSectionDefinition> {
    buyer_context_definitions()
        .iter()
        .find(|definition| definition.kind == kind)
}

fn default_evidence_presentation() -> EvidencePresentation {
    EvidencePresentation {
        variant: "fact_list".to_string(),
        density: "standard".to_string(),
        max_preview_items: 4,
    }
}

fn buyer_context_definitions() -> &'static [EvidenceSectionDefinition] {
    evidence_sections_config().expect("app/config/dag/evidence_sections.json should be valid")
}

fn build_configured_evidence_panels(
    snapshot_identity: &str,
    property: &crate::models::Property,
    projection: Option<&SocietyFactProjection<'_>>,
    serving_facts: Option<&ServingFactIndex>,
    graph_index: Option<&crate::graph::GraphIndex>,
) -> Vec<SourcePanel> {
    build_configured_evidence_panels_from_definitions(
        snapshot_identity,
        buyer_context_definitions(),
        property,
        projection,
        serving_facts,
        graph_index,
    )
}

fn build_configured_evidence_panels_from_definitions(
    snapshot_identity: &str,
    definitions: &[EvidenceSectionDefinition],
    property: &crate::models::Property,
    projection: Option<&SocietyFactProjection<'_>>,
    serving_facts: Option<&ServingFactIndex>,
    graph_index: Option<&crate::graph::GraphIndex>,
) -> Vec<SourcePanel> {
    definitions
        .iter()
        .filter_map(|definition| {
            if definition.derived.is_some() {
                return build_derived_evidence_panel(property, projection, definition);
            }
            let items = collect_buyer_context_items(
                snapshot_identity,
                property,
                projection,
                serving_facts,
                graph_index,
                &definition.facts,
            );
            let media = definition
                .media
                .iter()
                .filter_map(|media_kind| {
                    context_media_for(media_kind, property, serving_facts, graph_index)
                })
                .collect::<Vec<_>>();
            (!items.is_empty() || !media.is_empty()).then(|| SourcePanel {
                kind: definition.kind.clone(),
                title: definition.title.clone(),
                subtitle: definition.subtitle.clone(),
                scope: definition.scope.clone(),
                relationship: Some(definition.relationship.clone()),
                priority: definition.priority,
                constellation: section_constellation_from_definition(definition),
                presentation: evidence_presentation_from_definition(definition),
                items,
                missing: definition.missing.clone(),
                media,
                community_pulse: None,
            })
        })
        .collect()
}

fn build_derived_evidence_panel(
    property: &crate::models::Property,
    projection: Option<&SocietyFactProjection<'_>>,
    definition: &EvidenceSectionDefinition,
) -> Option<SourcePanel> {
    match definition.derived.as_deref()? {
        "community_pulse" => build_community_pulse_panel(property, projection, definition),
        _ => None,
    }
}

fn build_community_pulse_panel(
    _property: &crate::models::Property,
    projection: Option<&SocietyFactProjection<'_>>,
    definition: &EvidenceSectionDefinition,
) -> Option<SourcePanel> {
    let community_records = collect_community_evidence_records(projection);
    if community_records.is_empty() {
        return None;
    }

    let pulse = deterministic_community_summarizer()
        .summarize(&community_records)
        .into_iter()
        .next()
        .map(|summary| community_pulse_from_summary(&summary))?;

    let mut missing = definition.missing.clone();
    if pulse.positives.is_empty() && pulse.concerns.is_empty() {
        missing.push("Review themes are limited.".to_string());
    }
    if pulse.quotes.is_empty() {
        missing.push("Review snippets are limited.".to_string());
    }

    Some(SourcePanel {
        kind: definition.kind.clone(),
        title: definition.title.clone(),
        subtitle: format!("{} · {}", pulse.source_label, pulse.sentiment_band),
        scope: definition.scope.clone(),
        relationship: Some(definition.relationship.clone()),
        priority: definition.priority,
        constellation: section_constellation_from_definition(definition),
        presentation: evidence_presentation_from_definition(definition),
        items: Vec::new(),
        missing,
        media: vec![],
        community_pulse: Some(pulse),
    })
}

fn collect_buyer_context_items(
    snapshot_identity: &str,
    property: &crate::models::Property,
    projection: Option<&SocietyFactProjection<'_>>,
    serving_facts: Option<&ServingFactIndex>,
    graph_index: Option<&crate::graph::GraphIndex>,
    facts: &[ContextFactDefinition],
) -> Vec<SourceItem> {
    let area_id = property.area_id.clone();
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    for fact in facts {
        let scoped = match fact.scope.as_str() {
            "area" | "waterbody" | "poi" => serving_facts
                .and_then(|facts| {
                    serving_entity_source_item(
                        snapshot_identity,
                        facts,
                        &area_id,
                        &fact.key,
                        &fact.key,
                        &fact.label,
                    )
                })
                .or_else(|| {
                    projection.and_then(|projection| {
                        serving_source_item(snapshot_identity, projection, &fact.key, &fact.label)
                    })
                }),
            "road_segment" => serving_road_segment_source_item(
                snapshot_identity,
                property,
                serving_facts,
                graph_index,
                &fact.key,
                &fact.label,
            ),
            _ => fact
                .max_values
                .and_then(|max_values| {
                    projection.and_then(|projection| {
                        serving_multi_source_item(
                            snapshot_identity,
                            projection,
                            &fact.key,
                            &fact.label,
                            max_values,
                        )
                    })
                })
                .or_else(|| {
                    projection.and_then(|projection| {
                        serving_source_item(snapshot_identity, projection, &fact.key, &fact.label)
                    })
                }),
        };
        if let Some(item) = scoped.map(|item| with_context_scope(item, fact)) {
            let key = format!("{}:{}:{}", item.entity_id, item.key, item.value);
            if seen.insert(key) {
                items.push(item);
            }
        }
    }

    items
}

fn serving_road_segment_source_item(
    snapshot_identity: &str,
    property: &crate::models::Property,
    serving_facts: Option<&ServingFactIndex>,
    graph_index: Option<&crate::graph::GraphIndex>,
    fact_key: &str,
    label: &str,
) -> Option<SourceItem> {
    let facts = serving_facts?;
    road_segment_entity_ids(property, facts, graph_index)
        .iter()
        .find_map(|entity_id| {
            serving_entity_source_item(
                snapshot_identity,
                facts,
                entity_id,
                fact_key,
                fact_key,
                label,
            )
        })
}

fn with_context_scope(mut item: SourceItem, fact: &ContextFactDefinition) -> SourceItem {
    item.scope = fact.scope.clone();
    item.relationship = Some(fact.relationship.clone());
    item
}

fn context_media_for(
    media_kind: &str,
    property: &crate::models::Property,
    serving_facts: Option<&ServingFactIndex>,
    graph_index: Option<&crate::graph::GraphIndex>,
) -> Option<EvidenceMediaStrip> {
    match media_kind {
        "approach_road_visuals" => approach_road_media_for(property, serving_facts, graph_index),
        _ => None,
    }
}

pub(crate) fn build_source_panels(
    snapshot_identity: &str,
    property: &crate::models::Property,
    serving_facts: Option<&ServingFactIndex>,
    graph_index: Option<&crate::graph::GraphIndex>,
) -> Vec<SourcePanel> {
    let projection =
        serving_facts.map(|facts| SocietyFactProjection::from_index(facts, &property.society_id));
    let panels = build_configured_evidence_panels(
        snapshot_identity,
        property,
        projection.as_ref(),
        serving_facts,
        graph_index,
    );

    panels
        .into_iter()
        .filter(|panel| {
            !panel.items.is_empty() || !panel.media.is_empty() || panel.community_pulse.is_some()
        })
        .collect()
}

fn build_property_evidence_response(
    entity_refs: KgEntityRefs,
    property: &crate::models::Property,
    serving_bundle: Option<&LoadedServingBundle>,
) -> PropertyEvidenceResponse {
    let snapshot_identity = serving_bundle
        .map(|bundle| bundle.manifest.proof_snapshot_identity())
        .unwrap_or("");
    let source_panels = build_source_panels(
        snapshot_identity,
        property,
        serving_bundle.map(|bundle| &bundle.fact_index),
        serving_bundle.map(|bundle| &bundle.graph_index),
    );
    build_property_evidence_response_from_panels(
        property.id.clone(),
        entity_refs,
        serving_bundle,
        source_panels,
    )
}

fn build_property_evidence_response_from_panels(
    property_id: String,
    entity_refs: KgEntityRefs,
    serving_bundle: Option<&LoadedServingBundle>,
    source_panels: Vec<SourcePanel>,
) -> PropertyEvidenceResponse {
    let mut sections = source_panels
        .into_iter()
        .map(|panel| evidence_section_from_panel(panel, &entity_refs))
        .collect::<Vec<_>>();
    sections.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| left.title.cmp(&right.title))
    });

    PropertyEvidenceResponse {
        property_id,
        entity_refs,
        serving_bundle_version: serving_bundle.map(|bundle| bundle.manifest.bundle_version.clone()),
        sections,
    }
}

pub(crate) fn evidence_section_from_panel(
    panel: SourcePanel,
    entity_refs: &KgEntityRefs,
) -> EvidenceSection {
    let source_types = unique_sorted(
        panel
            .items
            .iter()
            .map(|item| item.source_type.clone())
            .collect(),
    );
    let mut entity_ids = unique_sorted(
        panel
            .items
            .iter()
            .map(|item| item.entity_id.clone())
            .collect(),
    );
    if entity_ids.is_empty() {
        entity_ids = entity_ids_for_section_scope(&panel.scope, entity_refs);
    }
    let confidence_pct = panel
        .community_pulse
        .as_ref()
        .map(|pulse| pulse.confidence_pct)
        .filter(|_| panel.kind == "community")
        .unwrap_or_else(|| section_confidence_pct(&panel.items));
    let summary = panel
        .community_pulse
        .as_ref()
        .map(|pulse| pulse.paragraph.clone())
        .unwrap_or_else(|| section_summary(&panel));
    let presentation = panel.presentation.clone();
    let constellation = panel.constellation.clone();
    let header_meta = section_header_meta(&panel, &source_types);

    EvidenceSection {
        priority: panel.priority,
        kind: panel.kind,
        title: panel.title,
        summary,
        subtitle: panel.subtitle,
        scope: panel.scope,
        relationship: panel.relationship,
        constellation,
        header_meta,
        confidence_pct,
        source_types,
        entity_ids,
        items: panel.items,
        presentation,
        missing: panel.missing,
        media: panel.media,
        community_pulse: panel.community_pulse,
    }
}

fn section_header_meta(panel: &SourcePanel, source_types: &[String]) -> String {
    let fact_count = if let Some(pulse) = panel.community_pulse.as_ref() {
        pulse.quotes.len()
    } else {
        panel
            .items
            .iter()
            .filter(|item| source_item_has_display_value(item))
            .count()
    };
    if source_types.is_empty() {
        format!("{fact_count} facts")
    } else {
        format!("{fact_count} facts · {}", source_types.join(", "))
    }
}

fn source_item_has_display_value(item: &SourceItem) -> bool {
    item.values.iter().any(|value| !value.trim().is_empty()) || !item.value.trim().is_empty()
}

fn dedup_community_pulse_against_brief(panels: &mut [SourcePanel], brief: &LivabilityBrief) {
    let mut brief_themes = std::collections::BTreeSet::new();
    for block in &brief.blocks {
        for theme in &block.themes {
            brief_themes.insert(theme.to_ascii_lowercase());
        }
    }
    if brief_themes.is_empty() {
        return;
    }

    for panel in panels.iter_mut() {
        if panel.kind != "community" {
            continue;
        }
        let Some(pulse) = panel.community_pulse.as_mut() else {
            continue;
        };
        pulse
            .positives
            .retain(|theme| !brief_themes.contains(&theme.to_ascii_lowercase()));
        pulse
            .concerns
            .retain(|theme| !brief_themes.contains(&theme.to_ascii_lowercase()));
    }
}

fn section_constellation_from_definition(definition: &EvidenceSectionDefinition) -> String {
    if definition.constellation.trim().is_empty() {
        "trust".to_string()
    } else {
        definition.constellation.clone()
    }
}

fn evidence_presentation_from_definition(
    definition: &EvidenceSectionDefinition,
) -> EvidencePresentation {
    definition
        .presentation
        .as_ref()
        .map(evidence_presentation_from_config)
        .unwrap_or_else(default_evidence_presentation)
}

fn evidence_presentation_from_config(
    presentation: &EvidenceSectionPresentation,
) -> EvidencePresentation {
    EvidencePresentation {
        variant: presentation.variant.clone(),
        density: presentation.density.clone(),
        max_preview_items: presentation.max_preview_items,
    }
}

fn section_confidence_pct(items: &[SourceItem]) -> u8 {
    if items.is_empty() {
        return 0;
    }
    let total = items
        .iter()
        .map(|item| u32::from(item.confidence_pct))
        .sum::<u32>();
    ((total / items.len() as u32).min(100)) as u8
}

fn section_summary(panel: &SourcePanel) -> String {
    if let Some(item) = primary_section_item(panel) {
        if panel.kind == "water_context" && item.key == "environment.groundwater_potential_class" {
            return groundwater_potential_summary(item);
        }
        return source_item_summary(item);
    }
    if let Some(media) = panel.media.first() {
        return media.caption.clone();
    }
    if panel.subtitle.trim().is_empty() {
        "Details not ready yet.".to_string()
    } else {
        truncate_summary(&panel.subtitle, 96)
    }
}

fn groundwater_potential_summary(item: &SourceItem) -> String {
    let class = item
        .value
        .rsplit_once(':')
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| item.value.trim());
    if class.is_empty() {
        return "Groundwater potential context is available for this society.".to_string();
    }
    format!("{class} groundwater potential zone near the society.")
}

fn primary_section_item(panel: &SourcePanel) -> Option<&SourceItem> {
    if let Some(definition) = evidence_section_definition(&panel.kind) {
        for fact in &definition.facts {
            if let Some(item) = panel.items.iter().find(|item| item.key == fact.key) {
                return Some(item);
            }
        }
    }
    panel.items.first()
}

fn entity_ids_for_section_scope(scope: &str, entity_refs: &KgEntityRefs) -> Vec<String> {
    match scope {
        "area" | "waterbody" | "poi" => vec![entity_refs.area_entity_id.clone()],
        "society" => vec![entity_refs.society_entity_id.clone()],
        "road_segment" => vec![
            entity_refs.society_entity_id.clone(),
            "road_segment:approach-road".to_string(),
        ],
        _ => vec![entity_refs.property_entity_id.clone()],
    }
}

fn source_item_summary(item: &SourceItem) -> String {
    if let Some(first_value) = item.values.first() {
        let suffix = if item.values.len() > 1 {
            format!(" +{} more", item.values.len() - 1)
        } else {
            String::new()
        };
        return format!(
            "{}: {}{}",
            item.label,
            truncate_summary(first_value, 80),
            suffix
        );
    }
    let value = truncate_summary(&item.value, 96);
    if source_value_is_self_labeled(&item.value, &item.label) {
        value
    } else {
        format!("{}: {value}", item.label)
    }
}

fn source_value_is_self_labeled(value: &str, label: &str) -> bool {
    let value = value.trim();
    let label = label.trim();
    if value.is_empty() || label.is_empty() {
        return false;
    }

    let normalized_value = value.to_lowercase();
    let normalized_label = label.to_lowercase();
    if normalized_value.starts_with(&format!("{normalized_label}:")) {
        return true;
    }

    let Some(colon_index) = value.find(':') else {
        return false;
    };
    if colon_index > 48 || value.starts_with("http://") || value.starts_with("https://") {
        return false;
    }

    let prefix = value[..colon_index].trim();
    prefix
        .chars()
        .any(|character| character.is_ascii_alphabetic())
}

fn truncate_summary(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut out = trimmed.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}

fn unique_sorted(mut values: Vec<String>) -> Vec<String> {
    values.retain(|value| !value.trim().is_empty());
    values.sort();
    values.dedup();
    values
}

fn build_builder_portfolio(
    properties: &[crate::models::Property],
    current: &crate::models::Property,
    serving_facts: Option<&ServingFactIndex>,
    graph_index: &crate::graph::GraphIndex,
) -> Option<BuilderPortfolio> {
    let builders = graph_index.targets_out(&society_node_id(&current.society_id), &["built_by"]);
    if builders.is_empty() {
        return None;
    }

    let mut seen_societies = HashSet::new();
    let mut projects = Vec::new();
    let mut rera_registered_projects = 0;
    let mut delayed_projects = 0;
    let mut complaint_projects = 0;
    let mut revocations: Option<i32> = None;

    for property in properties {
        if !graph_index
            .targets_out(&society_node_id(&property.society_id), &["built_by"])
            .iter()
            .any(|id| builders.contains(id))
            || !seen_societies.insert(property.society_id.clone())
        {
            continue;
        }

        let rera = rera_info_for(&property.society_id, serving_facts);
        if rera.as_ref().is_some_and(|record| record.registered) {
            rera_registered_projects += 1;
        }
        if rera
            .as_ref()
            .and_then(|record| record.delay_months)
            .is_some_and(|months| months > 0)
        {
            delayed_projects += 1;
        }
        if rera
            .as_ref()
            .and_then(|record| record.complaints_count)
            .is_some_and(|count| count > 0)
        {
            complaint_projects += 1;
        }
        if let Some(builder_revocations) =
            rera.as_ref().and_then(|record| record.builder_revocations)
        {
            revocations = Some(revocations.unwrap_or(0).max(builder_revocations));
        }

        projects.push(BuilderProjectRecord {
            property_id: property.id.clone(),
            project_name: project_name_for(property),
            area: property.area.clone(),
            rera_number: rera
                .as_ref()
                .and_then(|record| record.registration_number.clone()),
            rera_portal_url: rera
                .as_ref()
                .and_then(|record| record.rera_portal_url.clone()),
            rera_status: rera.as_ref().and_then(|record| record.status.clone()),
            rera_registered: rera.as_ref().is_some_and(|record| record.registered),
            start_date: rera.as_ref().and_then(|record| record.start_date.clone()),
            completion_date: rera
                .as_ref()
                .and_then(|record| record.completion_date.clone()),
            delay_months: rera.as_ref().and_then(|record| record.delay_months),
            complaints_count: rera.as_ref().and_then(|record| record.complaints_count),
            project_status_display: project_status_display_for(serving_facts, &property.society_id),
            current: property.society_id == current.society_id,
        });
    }

    if projects.len() <= 1 && rera_registered_projects == 0 {
        return None;
    }

    projects.sort_by(|left, right| {
        right
            .current
            .cmp(&left.current)
            .then_with(|| left.project_name.cmp(&right.project_name))
    });

    Some(BuilderPortfolio {
        builder_name: current.builder_name.clone(),
        tracked_projects: projects.len(),
        rera_registered_projects,
        delayed_projects,
        complaint_projects,
        revocations,
        projects,
    })
}

/// GET /api/properties/:id/evidence — returns backend-shaped dynamic evidence
/// sections for the UI. React should render these sections, not interpret raw KG
/// facts itself.
pub async fn get_property_evidence(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<PropertySnapshotQuery>,
) -> Result<Json<PropertyEvidenceResponse>, (StatusCode, Json<ErrorResponse>)> {
    let runtime = state.search_runtime.load_full();
    check_property_snapshot(&runtime, query.snapshot_identity.as_deref())?;
    let property = find_property_by_request_id(&runtime.properties, &id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "property_not_found".to_string(),
            }),
        )
    })?;

    Ok(Json(build_property_evidence_response(
        runtime.entity_refs_for_property(property),
        property,
        Some(runtime.bundle.as_ref()),
    )))
}

/// GET /api/properties/:id/recommendations — computes serving-native branches
/// from recall channels and the shared scoring policy. This intentionally runs
/// outside the detail endpoint so the first paint is not gated on recommendation
/// work.
pub async fn get_property_recommendations(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<PropertySnapshotQuery>,
) -> Result<Json<RecommendationResponse>, (StatusCode, Json<ErrorResponse>)> {
    let runtime = state.search_runtime.load_full();
    check_property_snapshot(&runtime, query.snapshot_identity.as_deref())?;
    let property = find_property_by_request_id(&runtime.properties, &id)
        .cloned()
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "property_not_found".to_string(),
                }),
            )
        })?;
    if !property.is_listable() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "property_not_found".to_string(),
            }),
        ));
    }

    let bundle_version = Some(runtime.bundle.manifest.bundle_version.clone());
    let cache_key = recommendation_cache_key(&property.id, bundle_version.as_deref());
    if let Some(cached) = state
        .recommendation_cache
        .read()
        .await
        .get(&cache_key)
        .cloned()
    {
        return Ok(Json(cached));
    }

    let evidence = build_property_evidence_response(
        runtime.entity_refs_for_property(&property),
        &property,
        Some(runtime.bundle.as_ref()),
    );
    let area_median_ppsf = area_median_ppsf_for(&property, &runtime.properties);
    let items = build_recommendation_branches(RecommendationBranchInputs {
        current: &property,
        current_evidence: &evidence,
        properties: &runtime.properties,
        societies: &runtime.societies,
        serving_bundle: Some(runtime.bundle.as_ref()),
        area_median_ppsf,
    });
    let response = RecommendationResponse {
        status: RecommendationStatus::Ready,
        engine_version: RECOMMENDATION_ENGINE_VERSION.to_string(),
        scoring_policy_version: scoring_policy().version,
        serving_bundle_version: bundle_version,
        items,
    };

    state
        .recommendation_cache
        .write()
        .await
        .insert(cache_key, response.clone());
    Ok(Json(response))
}

/// GET /api/properties/:id — returns joined property + society + area,
/// enriched from the knowledge graph.
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertySnapshotQuery {
    pub snapshot_identity: Option<String>,
}

fn check_property_snapshot(
    runtime: &crate::state::SearchRuntimeSnapshot,
    expected: Option<&str>,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if expected
        .is_some_and(|expected| expected != runtime.bundle.manifest.proof_snapshot_identity())
    {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "stale_snapshot".to_string(),
            }),
        ));
    }
    Ok(())
}

pub async fn get_property(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<PropertySnapshotQuery>,
) -> Result<Json<PropertyDetail>, (StatusCode, Json<ErrorResponse>)> {
    // Use the immutable serving snapshot so no async lock guard survives the
    // interest-file read later in this handler.
    let runtime = state.search_runtime.load_full();
    check_property_snapshot(&runtime, query.snapshot_identity.as_deref())?;
    let serving_bundle = Some(runtime.bundle.clone());
    let properties = &runtime.properties;
    let property = find_property_by_request_id(properties, &id)
        .cloned()
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "property_not_found".to_string(),
                }),
            )
        })?;
    if !property.is_listable() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "property_not_found".to_string(),
            }),
        ));
    }

    let societies = &runtime.societies;

    // Enrich society from KG
    let society_key = super::enrichment::to_slug(&property.society_id);
    let society = societies
        .iter()
        .find(|s| super::enrichment::to_slug(&s.id) == society_key)
        .cloned();

    let external_reviews = external_reviews_for(
        &property.society_id,
        serving_bundle.as_ref().map(|bundle| &bundle.fact_index),
    );

    // Extract RERA info from the society's KG node
    let rera = rera_info_for(
        &property.society_id,
        serving_bundle.as_ref().map(|bundle| &bundle.fact_index),
    );
    let rera_report_ref = rera_report_ref_for_property(&property, serving_bundle.as_deref());
    let (decision_labels, decision_check_summary) =
        if let Some(serving_bundle) = serving_bundle.as_ref() {
            (
                crate::decision_labels::rera_decision_labels_for_society(
                    &serving_bundle.fact_index,
                    &property.society_id,
                ),
                crate::decision_labels::rera_decision_check_summary_for_society(
                    &serving_bundle.fact_index,
                    &property.society_id,
                ),
            )
        } else {
            (Vec::new(), None)
        };

    // Compute area price range from all properties in the same area
    let (area_price_range_low, area_price_range_high) = {
        let area_props: Vec<u64> = properties
            .iter()
            .filter(|p| p.area_id == property.area_id && p.price_per_sqft > 0)
            .map(|p| p.price_per_sqft)
            .collect();

        if area_props.len() >= 2 {
            let low = area_props.iter().copied().min();
            let high = area_props.iter().copied().max();
            (low, high)
        } else {
            (None, None)
        }
    };

    // Read interest count from JSONL file.
    let interest_count = {
        let file_path = state
            .project_root
            .join("data")
            .join("interests")
            .join(format!("{}.jsonl", property.id));
        count_interest_lines(&file_path).await
    };

    let root_source = runtime
        .search_index
        .society_entity_id_for_property(&property.id)
        .and_then(|id| runtime.search_index.entity_root_source(id))
        .map(str::to_string);
    let status =
        SocietyFactProjection::from_index(&runtime.bundle.fact_index, &property.society_id)
            .project_status(None, None);
    let (project_status, project_status_display) = (status.status, status.display);
    let home_state_display = serving_bundle.as_ref().and_then(|bundle| {
        SocietyFactProjection::from_index(&bundle.fact_index, &property.society_id)
            .project_home_state()
            .display
    });

    let builder_portfolio = build_builder_portfolio(
        properties,
        &property,
        serving_bundle.as_ref().map(|bundle| &bundle.fact_index),
        &runtime.bundle.graph_index,
    );
    let entity_refs = runtime.entity_refs_for_property(&property);
    let snapshot_identity = runtime.bundle.manifest.proof_snapshot_identity();
    let mut source_panels = build_source_panels(
        snapshot_identity,
        &property,
        serving_bundle.as_ref().map(|bundle| &bundle.fact_index),
        serving_bundle.as_ref().map(|bundle| &bundle.graph_index),
    );
    let community_pulse = source_panels
        .iter()
        .find(|panel| panel.kind == "community")
        .and_then(|panel| panel.community_pulse.as_ref());
    let society_projection = serving_bundle
        .as_ref()
        .map(|bundle| SocietyFactProjection::from_index(&bundle.fact_index, &property.society_id));
    let community_records = collect_community_evidence_records(society_projection.as_ref());
    let society_display_name = society
        .as_ref()
        .map(|society| society.name.as_str())
        .unwrap_or(property.society_id.as_str());
    let livability_brief = build_livability_brief(
        &property,
        society_display_name,
        society_projection.as_ref(),
        serving_bundle.as_ref().map(|bundle| &bundle.fact_index),
        serving_bundle.as_ref().map(|bundle| &bundle.graph_index),
        &community_records,
        community_pulse,
    );
    if let Some(brief) = livability_brief.as_ref() {
        dedup_community_pulse_against_brief(&mut source_panels, brief);
    }
    let evidence = build_property_evidence_response_from_panels(
        property.id.clone(),
        entity_refs.clone(),
        serving_bundle.as_deref(),
        source_panels.clone(),
    );

    let recommendations = recommendation_envelope_for(
        &property.id,
        serving_bundle
            .as_ref()
            .map(|bundle| bundle.manifest.bundle_version.clone()),
    );

    let context = crate::property_context::build_property_context(
        &property,
        entity_refs.clone(),
        &runtime.bundle,
        &runtime.context_lookup,
        None,
    );
    let detail_signals = detail_signals_for(external_reviews.as_ref(), society.as_ref());
    let plans = crate::plans::project_plans_for_society(
        &entity_refs.society_entity_id,
        &property.society_id,
        serving_bundle.as_ref().map(|bundle| &bundle.fact_index),
    );
    Ok(Json(PropertyDetail {
        availability: property.inventory_availability(),
        contract_version: 1,
        snapshot_identity: runtime
            .bundle
            .manifest
            .proof_snapshot_identity()
            .to_string(),
        entity_refs,
        evidence,
        property: (&property).into(),
        society: society.as_ref().map(Into::into),
        recommendation_branches: Vec::new(),
        recommendations,
        rera,
        rera_report_ref,
        decision_labels,
        decision_check_summary,
        area_price_range_low,
        area_price_range_high,
        interest_count,
        root_source,
        project_status_display,
        home_state_display,
        project_status,
        builder_portfolio,
        external_reviews,
        detail_signals,
        livability_brief,
        context,
        plans,
    }))
}

fn google_review_cards_for(
    society_id: &str,
    serving_facts: Option<&ServingFactIndex>,
) -> Vec<ExternalReviewCard> {
    let Some(serving_facts) = serving_facts else {
        return Vec::new();
    };
    let projection = SocietyFactProjection::from_index(serving_facts, society_id);
    let mut ranked = Vec::new();

    for fact in projection.records("google_review_cards") {
        let values = match &fact.value {
            FactValue::Tags(values) => values.as_slice(),
            _ => continue,
        };
        for (index, value) in values.iter().enumerate() {
            let Ok(review) = serde_json::from_str::<StructuredGoogleReview>(value) else {
                continue;
            };
            let text = clean_review_text(&review.text);
            if text.is_empty() {
                continue;
            }
            let helpful_count = review.helpful_count.unwrap_or(0);
            let recency_score = review
                .published_at
                .as_deref()
                .and_then(review_date_score)
                .unwrap_or_else(|| fact.learned_at.timestamp());
            ranked.push(RankedReview {
                card: ExternalReviewCard {
                    id: review_card_id(&text, ranked.len()),
                    source: "Google".to_string(),
                    author: review.author.and_then(clean_optional_review_text),
                    rating: review.rating.filter(|rating| (0.0..=5.0).contains(rating)),
                    date_label: review
                        .date_label
                        .and_then(clean_optional_review_text)
                        .or(review.published_at),
                    helpful_count: review.helpful_count,
                    tone: review_tone(review.rating, &text),
                    text,
                },
                helpful_count,
                recency_score,
                source_order: index,
            });
        }
    }

    if ranked.is_empty() {
        for fact in projection.records("google_review_snippets") {
            let values = match &fact.value {
                FactValue::Tags(values) => values.as_slice(),
                _ => continue,
            };
            for (index, value) in values.iter().enumerate() {
                let text = clean_review_text(value);
                if text.is_empty() {
                    continue;
                }
                ranked.push(RankedReview {
                    card: ExternalReviewCard {
                        id: review_card_id(&text, ranked.len()),
                        source: "Google".to_string(),
                        author: Some("Google reviewer".to_string()),
                        rating: None,
                        date_label: None,
                        helpful_count: None,
                        tone: review_tone(None, &text),
                        text,
                    },
                    helpful_count: 0,
                    recency_score: fact.learned_at.timestamp(),
                    source_order: index,
                });
            }
        }
    }

    ranked.sort_by(|left, right| {
        right
            .helpful_count
            .cmp(&left.helpful_count)
            .then_with(|| right.recency_score.cmp(&left.recency_score))
            .then_with(|| left.source_order.cmp(&right.source_order))
            .then_with(|| left.card.text.cmp(&right.card.text))
    });
    balanced_review_cards(ranked, 12)
}

fn balanced_review_cards(ranked: Vec<RankedReview>, limit: usize) -> Vec<ExternalReviewCard> {
    let mut selected = ranked
        .iter()
        .take(limit)
        .map(|item| item.card.clone())
        .collect::<Vec<_>>();
    let has_positive = selected
        .iter()
        .any(|review| review.tone == ReviewTone::Positive);
    let has_concern = selected
        .iter()
        .any(|review| review.tone == ReviewTone::Concern);
    if has_positive && !has_concern {
        if let Some(concern) = ranked
            .iter()
            .find(|item| item.card.tone == ReviewTone::Concern)
            .map(|item| item.card.clone())
        {
            if selected.len() >= limit {
                selected.pop();
            }
            selected.push(concern);
        }
    }
    selected
}

fn clean_review_text(value: &str) -> String {
    value
        .replace('\n', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn clean_optional_review_text(value: String) -> Option<String> {
    let cleaned = clean_review_text(&value);
    (!cleaned.is_empty()).then_some(cleaned)
}

fn review_date_score(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|date| date.timestamp())
        .ok()
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .ok()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .map(|date| date.and_utc().timestamp())
        })
}

fn review_tone(rating: Option<f64>, text: &str) -> ReviewTone {
    let normalized = text.to_ascii_lowercase();
    if [
        "traffic", "noise", "water", "smell", "seepage", "parking", "delay", "bad", "poor",
        "issue", "problem",
    ]
    .iter()
    .any(|term| normalized.contains(term))
    {
        return ReviewTone::Concern;
    }
    if let Some(rating) = rating {
        if rating < 4.0 {
            return ReviewTone::Concern;
        }
        if rating >= 4.0 {
            return ReviewTone::Positive;
        }
    }
    if [
        "good",
        "great",
        "excellent",
        "well maintained",
        "clean",
        "spacious",
        "greenery",
        "amenities",
        "peaceful",
    ]
    .iter()
    .any(|term| normalized.contains(term))
    {
        ReviewTone::Positive
    } else {
        ReviewTone::Neutral
    }
}

fn review_card_id(text: &str, index: usize) -> String {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    format!("google-review-{index}-{:x}", hasher.finish())
}

fn detail_signals_for(
    external_reviews: Option<&ExternalReviews>,
    society: Option<&crate::models::Society>,
) -> Vec<DetailSignal> {
    let mut signals = Vec::new();
    let review_texts = positive_review_signal_texts(external_reviews, society);
    push_theme_signal(
        &mut signals,
        &review_texts,
        "amenities",
        "Amenities",
        "amenities",
        &["amenit", "clubhouse", "pool", "gym", "play area"],
    );
    push_theme_signal(
        &mut signals,
        &review_texts,
        "cleanliness",
        "Cleanliness",
        "cleanliness",
        &["clean", "hygien"],
    );
    push_theme_signal(
        &mut signals,
        &review_texts,
        "location",
        "Location",
        "location",
        &["location", "nearby", "close to", "connectivity"],
    );
    push_theme_signal(
        &mut signals,
        &review_texts,
        "greenery",
        "Greenery",
        "greenery",
        &["green", "open space", "garden", "trees"],
    );
    push_theme_signal(
        &mut signals,
        &review_texts,
        "maintenance",
        "Maintenance",
        "maintenance",
        &["maintenance", "maintained", "managed", "upkeep"],
    );

    signals.truncate(8);
    signals
}

fn positive_review_signal_texts(
    external_reviews: Option<&ExternalReviews>,
    society: Option<&crate::models::Society>,
) -> Vec<String> {
    let mut texts = external_reviews
        .map(|reviews| {
            reviews
                .reviews
                .iter()
                .filter(|review| review.tone == ReviewTone::Positive)
                .map(|review| review.text.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if let Some(society) = society {
        texts.extend(society.common_positives.iter().cloned());
    }
    texts
}

fn push_theme_signal(
    signals: &mut Vec<DetailSignal>,
    texts: &[String],
    key: &str,
    label: &str,
    icon: &str,
    terms: &[&str],
) {
    let count = texts
        .iter()
        .filter(|text| {
            let normalized = text.to_ascii_lowercase();
            terms.iter().any(|term| normalized.contains(term))
        })
        .count();
    if count > 0 {
        signals.push(DetailSignal {
            key: key.to_string(),
            label: label.to_string(),
            icon: icon.to_string(),
            count: None,
        });
    }
}

fn external_reviews_for(
    society_id: &str,
    serving_facts: Option<&ServingFactIndex>,
) -> Option<ExternalReviews> {
    let facts = serving_facts?;
    let evidence = SocietyFactProjection::from_index(facts, society_id)
        .project_google_reviews(GoogleReviewEvidence::default());
    let reviews = google_review_cards_for(society_id, Some(facts));
    (!evidence.is_empty() || !reviews.is_empty()).then_some(ExternalReviews {
        google_rating: evidence.rating,
        google_review_count: evidence.review_count,
        google_reviews_url: evidence.reviews_url,
        reviews,
    })
}

pub(crate) fn overlay_serving_google_reviews(
    mut card: PropertyCard,
    society_id: &str,
    serving_facts: Option<&ServingFactIndex>,
) -> PropertyCard {
    if let Some(serving_facts) = serving_facts {
        let fallback = GoogleReviewEvidence {
            rating: card.google_rating,
            review_count: card.google_review_count,
            reviews_url: card.google_reviews_url.clone(),
        };
        let evidence = SocietyFactProjection::from_index(serving_facts, society_id)
            .project_google_reviews(fallback);
        card.google_rating = evidence.rating;
        card.google_review_count = evidence.review_count;
        card.google_reviews_url = evidence.reviews_url;
        card.home_state_display = SocietyFactProjection::from_index(serving_facts, society_id)
            .project_home_state()
            .display;
        overlay_project_scale_facts(&mut card, serving_facts, society_id);
        card.decision_labels =
            crate::decision_labels::rera_decision_labels_for_society(serving_facts, society_id);
        card.decision_check_summary =
            crate::decision_labels::rera_decision_check_summary_for_society(
                serving_facts,
                society_id,
            );
    }
    crate::plans::overlay_project_plans_on_card(&mut card, society_id, serving_facts);
    card
}

fn rera_manifest_group(item: &ReraDocumentManifestItem) -> String {
    let group = item.document_group.trim();
    if !group.is_empty() {
        return group.to_string();
    }

    let haystack = format!(
        "{} {} {}",
        item.kind,
        item.label,
        item.source_field_label.as_deref().unwrap_or("")
    )
    .to_ascii_lowercase();
    if haystack.contains("approval") || haystack.contains("noc") {
        "approvals_nocs".to_string()
    } else if haystack.contains("khata")
        || haystack.contains("land")
        || haystack.contains("encumbrance")
    {
        "legal_land".to_string()
    } else if haystack.contains("affidavit") {
        "affidavits".to_string()
    } else if haystack.contains("plan") {
        "plans".to_string()
    } else if haystack.contains("brochure") {
        "brochure".to_string()
    } else {
        "other".to_string()
    }
}

fn parse_rera_document_manifest_value(value: &str) -> Vec<ReraDocumentManifestItem> {
    parse_rera_projection_json::<Vec<ReraDocumentManifestItem>>(value)
        .unwrap_or_default()
        .into_iter()
        .map(|mut item| {
            if item.document_group.trim().is_empty() {
                item.document_group = rera_manifest_group(&item);
            }
            item
        })
        .collect()
}

fn merge_rera_document_manifest(
    existing: &mut Vec<ReraDocumentManifestItem>,
    incoming: Vec<ReraDocumentManifestItem>,
) {
    for item in incoming {
        let duplicate = existing.iter().any(|candidate| {
            (!item.artifact_id.is_empty() && candidate.artifact_id == item.artifact_id)
                || (item.source_url.is_some() && candidate.source_url == item.source_url)
        });
        if !duplicate {
            existing.push(item);
        }
    }
}

fn refresh_rera_document_summary(info: &mut ReraInfo) {
    info.document_groups = rera_document_groups(&info.document_manifest);
    info.affidavit_only_visible = rera_affidavit_only_visible(&info.document_manifest);
}

fn rera_evidence_availability(record: &ServingReraEvidenceRecord) -> ReraEvidenceAvailability {
    let checked_sources = record
        .regulatory_coverage
        .iter()
        .filter(|coverage| coverage.status == "checked")
        .map(|coverage| coverage.source.as_str())
        .collect::<HashSet<_>>();
    if checked_sources.contains("K-RERA") {
        ReraEvidenceAvailability::Available
    } else {
        ReraEvidenceAvailability::Partial
    }
}

fn rera_report_ref_for_property(
    property: &crate::models::Property,
    bundle: Option<&LoadedServingBundle>,
) -> ReraReportRef {
    let record = bundle.and_then(|bundle| rera_evidence_for_property(bundle, property));
    let has_buyer_facts = bundle
        .and_then(|bundle| {
            let config = rera_report_surface_config().ok()?;
            Some(
                !rera_buyer_fact_sections_for_society(
                    &property.society_id,
                    Some(&bundle.fact_index),
                    config,
                )
                .is_empty(),
            )
        })
        .unwrap_or(false);
    ReraReportRef {
        registration_ids: record
            .map(|record| record.registration_ids.clone())
            .unwrap_or_default(),
        href: format!("/property/{}/rera", property.id),
        availability: record
            .map(rera_evidence_availability)
            .or_else(|| has_buyer_facts.then_some(ReraEvidenceAvailability::Partial))
            .unwrap_or(ReraEvidenceAvailability::Unavailable),
    }
}

fn rera_evidence_for_property<'a>(
    bundle: &'a LoadedServingBundle,
    property: &crate::models::Property,
) -> Option<&'a ServingReraEvidenceRecord> {
    let canonical_id = society_node_id(&property.society_id);
    bundle
        .rera_evidence_index
        .society(&canonical_id)
        .or_else(|| bundle.rera_evidence_index.society(&property.society_id))
}

fn rera_buyer_fact_sections_for_society(
    society_id: &str,
    serving_facts: Option<&ServingFactIndex>,
    config: &ReraReportSurfaceFile,
) -> Vec<ReraBuyerFactSection> {
    let Some(serving_facts) = serving_facts else {
        return Vec::new();
    };
    let registry = fact_registry_index_config().ok();
    let rows = rera_society_entity_id_candidates(society_id)
        .into_iter()
        .filter_map(|entity_id| serving_facts.entity(&entity_id))
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return Vec::new();
    }

    let mut selected = BTreeMap::<(String, String), (&ServingFactRecord, String)>::new();
    for fact in rows
        .iter()
        .flat_map(|row| row.facts.iter())
        .filter(|fact| rera_buyer_fact_candidate(config, fact))
    {
        let Some(value) = rera_buyer_fact_value(config, fact) else {
            continue;
        };
        selected
            .entry((fact.fact_key.clone(), value.clone()))
            .and_modify(|current| {
                if fact.confidence > current.0.confidence
                    || (fact.confidence == current.0.confidence
                        && fact.stable_selection_key() < current.0.stable_selection_key())
                {
                    *current = (fact, value.clone());
                }
            })
            .or_insert((fact, value));
    }

    let mut grouped = BTreeMap::<String, (String, u32, Vec<ReraBuyerFact>)>::new();
    for (fact, value) in selected.into_values() {
        let Some((section_id, section_title, rank)) =
            rera_buyer_section_for_key(config, &fact.fact_key)
        else {
            continue;
        };
        grouped
            .entry(section_id.to_string())
            .or_insert_with(|| (section_title.to_string(), rank, Vec::new()))
            .2
            .push(ReraBuyerFact {
                key: fact.fact_key.clone(),
                label: rera_buyer_fact_label(config, registry, &fact.fact_key),
                value,
                source_url: fact.source_url.clone(),
                learned_at: fact.learned_at.to_rfc3339(),
            });
    }

    let mut sections = grouped
        .into_iter()
        .map(|(id, (title, rank, mut facts))| {
            facts.sort_by(|left, right| {
                left.label
                    .cmp(&right.label)
                    .then_with(|| left.key.cmp(&right.key))
            });
            facts.dedup_by(|left, right| {
                left.label.eq_ignore_ascii_case(&right.label)
                    && left.value.eq_ignore_ascii_case(&right.value)
            });
            (rank, ReraBuyerFactSection { id, title, facts })
        })
        .collect::<Vec<_>>();
    sections.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.title.cmp(&right.1.title))
    });
    sections.into_iter().map(|(_, section)| section).collect()
}

fn rera_buyer_complaints(
    info: Option<&ReraInfo>,
    evidence: Option<&ServingReraEvidenceRecord>,
) -> Vec<ReraBuyerComplaintSummary> {
    let mut by_scope = info
        .into_iter()
        .flat_map(|record| &record.complaint_summaries)
        .filter_map(|summary| {
            let scope = summary.scope.trim().to_ascii_lowercase();
            if scope.is_empty() {
                return None;
            }
            let total = summary
                .total_count_from_tab_label
                .unwrap_or(summary.row_count_parsed);
            Some((
                scope.clone(),
                ReraBuyerComplaintSummary {
                    scope,
                    total,
                    open: summary.open_count,
                    disposed: summary.disposed_count,
                    rows_parsed: summary.row_count_parsed,
                    status_counts_complete: total == summary.row_count_parsed,
                    theme_counts: summary
                        .theme_counts
                        .iter()
                        .filter(|(_, count)| **count > 0)
                        .map(|(theme, count)| (theme.clone(), *count))
                        .collect(),
                    sample_subjects: summary
                        .sample_subjects
                        .iter()
                        .map(|subject| subject.trim())
                        .filter(|subject| !subject.is_empty())
                        .take(3)
                        .map(str::to_string)
                        .collect(),
                },
            ))
        })
        .collect::<BTreeMap<_, _>>();

    if let Some(evidence) = evidence {
        for scope in ["project", "promoter"] {
            let Some(total) = complaint_claim_i32(
                &evidence.claims,
                &format!("complaint_{scope}_recorded_total"),
            ) else {
                continue;
            };
            let rows_parsed =
                complaint_claim_i32(&evidence.claims, &format!("complaint_{scope}_rows_parsed"))
                    .unwrap_or(0);
            let entry =
                by_scope
                    .entry(scope.to_string())
                    .or_insert_with(|| ReraBuyerComplaintSummary {
                        scope: scope.to_string(),
                        total,
                        open: 0,
                        disposed: 0,
                        rows_parsed,
                        status_counts_complete: false,
                        theme_counts: BTreeMap::new(),
                        sample_subjects: Vec::new(),
                    });
            entry.total = total;
            entry.rows_parsed = rows_parsed;
            entry.open = complaint_claim_i32(
                &evidence.claims,
                &format!("complaint_{scope}_open_count_in_parsed_rows"),
            )
            .unwrap_or(0);
            entry.disposed = complaint_claim_i32(
                &evidence.claims,
                &format!("complaint_{scope}_disposed_count_in_parsed_rows"),
            )
            .unwrap_or(0);
            entry.status_counts_complete = complaint_claim_bool(
                &evidence.claims,
                &format!("complaint_{scope}_status_counts_complete"),
            )
            .unwrap_or(false);
        }
    }

    by_scope
        .into_values()
        .filter(|summary| {
            summary.total > 0
                || summary.rows_parsed > 0
                || summary.open > 0
                || summary.disposed > 0
                || !summary.theme_counts.is_empty()
        })
        .collect()
}

fn complaint_claim_i32(claims: &[ReraClaimV1], predicate: &str) -> Option<i32> {
    claims.iter().find_map(|claim| {
        (claim.predicate == predicate)
            .then_some(&claim.value)
            .and_then(|value| match value {
                ReraClaimValue::Number(value)
                    if value.is_finite()
                        && value.fract() == 0.0
                        && *value >= 0.0
                        && *value <= i32::MAX as f64 =>
                {
                    Some(*value as i32)
                }
                _ => None,
            })
    })
}

fn complaint_claim_bool(claims: &[ReraClaimV1], predicate: &str) -> Option<bool> {
    claims.iter().find_map(|claim| {
        (claim.predicate == predicate)
            .then_some(&claim.value)
            .and_then(|value| match value {
                ReraClaimValue::Boolean(value) => Some(*value),
                _ => None,
            })
    })
}

fn rera_buyer_documents(info: &ReraInfo, config: &ReraReportSurfaceFile) -> Vec<ReraBuyerDocument> {
    let mut documents = info
        .document_manifest
        .iter()
        .filter_map(|item| {
            let visibility = item
                .buyer_visibility
                .as_deref()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            let preview = item
                .preview_policy
                .as_deref()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if matches!(
                visibility.as_str(),
                "hidden" | "private" | "private_or_sensitive"
            ) || matches!(
                preview.as_str(),
                "hidden" | "private" | "private_or_sensitive"
            ) {
                return None;
            }
            let url = item.source_url.as_deref()?.trim();
            if !url.starts_with("https://") && !url.starts_with("http://") {
                return None;
            }
            let label = [
                item.label.as_str(),
                item.source_field_label.as_deref().unwrap_or(""),
                item.kind.as_str(),
            ]
            .into_iter()
            .map(str::trim)
            .find(|value| !value.is_empty())
            .unwrap_or("Document")
            .to_string();
            let group = rera_manifest_group(item);
            let group_label = config
                .document_group_labels
                .get(&group)
                .cloned()
                .unwrap_or_else(|| group.replace(['_', '-'], " "));
            Some(ReraBuyerDocument {
                id: if item.artifact_id.trim().is_empty() {
                    url.to_string()
                } else {
                    item.artifact_id.clone()
                },
                label,
                group,
                group_label,
                url: url.to_string(),
            })
        })
        .collect::<Vec<_>>();
    documents.sort_by(|left, right| {
        left.group
            .cmp(&right.group)
            .then_with(|| left.label.cmp(&right.label))
    });
    documents.dedup_by(|left, right| left.id == right.id || left.url == right.url);
    documents
}

fn rera_buyer_fact_candidate(config: &ReraReportSurfaceFile, fact: &ServingFactRecord) -> bool {
    let key = fact.fact_key.to_ascii_lowercase();
    if config
        .candidate_rules
        .exclude_key_contains
        .iter()
        .any(|value| key.contains(&value.trim().to_ascii_lowercase()))
        || config
            .candidate_rules
            .exclude_key_suffixes
            .iter()
            .any(|value| key.ends_with(&value.trim().to_ascii_lowercase()))
    {
        return false;
    }
    config
        .candidate_rules
        .include_source_types
        .iter()
        .any(|value| value.eq_ignore_ascii_case(&fact.source_type))
        || config
            .candidate_rules
            .include_fact_keys
            .iter()
            .any(|value| key == value.trim().to_ascii_lowercase())
        || config
            .candidate_rules
            .include_key_prefixes
            .iter()
            .any(|value| key.starts_with(&value.trim().to_ascii_lowercase()))
}

fn rera_buyer_section_for_key<'a>(
    config: &'a ReraReportSurfaceFile,
    key: &str,
) -> Option<(&'a str, &'a str, u32)> {
    let key = key.to_ascii_lowercase();
    config
        .sections
        .iter()
        .find(|section| {
            rera_key_matches(
                &key,
                &section.fact_keys,
                &section.key_prefixes,
                &section.key_contains,
                &section.key_suffixes,
            )
        })
        .map(|section| (section.id.as_str(), section.title.as_str(), section.rank))
}

fn rera_buyer_fact_label(
    config: &ReraReportSurfaceFile,
    registry: Option<&FactRegistryIndex>,
    key: &str,
) -> String {
    let normalized = key.to_ascii_lowercase();
    if let Some(rule) = config
        .display_rules
        .iter()
        .find(|rule| rera_key_matches(&normalized, &rule.fact_keys, &[], &rule.key_contains, &[]))
    {
        return rule.label.clone();
    }
    if let Some(label) = registry
        .and_then(|index| index.lookup(key))
        .and_then(|entry| entry.label.as_deref())
        .map(str::trim)
        .filter(|label| !label.is_empty())
    {
        return label.to_string();
    }
    key.replace(['_', '-'], " ")
}

fn rera_buyer_fact_value(
    config: &ReraReportSurfaceFile,
    fact: &ServingFactRecord,
) -> Option<String> {
    let key = fact.fact_key.to_ascii_lowercase();
    let value = match &fact.value {
        FactValue::Numeric(value) if value.is_finite() => {
            let number = if value.fract().abs() < 0.05 {
                format!("{}", value.round() as i64)
            } else {
                format!("{value:.1}")
            };
            let suffix = config
                .value_rules
                .numeric_units
                .iter()
                .find(|rule| {
                    rera_key_matches(&key, &[], &[], &rule.key_contains, &rule.key_suffixes)
                })
                .map(|rule| rule.suffix.as_str())
                .unwrap_or("");
            format!("{number}{suffix}")
        }
        FactValue::Numeric(_) => return None,
        FactValue::Text(value) => {
            if config.value_rules.skip_json_containers
                && serde_json::from_str::<serde_json::Value>(value)
                    .ok()
                    .is_some_and(|parsed| parsed.is_array() || parsed.is_object())
            {
                return None;
            }
            value.clone()
        }
        FactValue::Bool(value) => if *value { "Yes" } else { "No" }.to_string(),
        FactValue::Tags(values) => values
            .iter()
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        FactValue::Score { value, explanation } => {
            let score = format!("{value:.1}");
            if explanation.trim().is_empty() {
                score
            } else {
                format!("{score} · {}", explanation.trim())
            }
        }
    };
    let value = value.trim();
    if value.is_empty()
        || config
            .value_rules
            .skip_text_values
            .iter()
            .any(|skip| value.eq_ignore_ascii_case(skip.trim()))
    {
        None
    } else {
        Some(value.to_string())
    }
}

fn rera_key_matches(
    key: &str,
    fact_keys: &[String],
    key_prefixes: &[String],
    key_contains: &[String],
    key_suffixes: &[String],
) -> bool {
    fact_keys
        .iter()
        .any(|value| key == value.trim().to_ascii_lowercase())
        || key_prefixes
            .iter()
            .any(|value| key.starts_with(&value.trim().to_ascii_lowercase()))
        || key_contains
            .iter()
            .any(|value| key.contains(&value.trim().to_ascii_lowercase()))
        || key_suffixes
            .iter()
            .any(|value| key.ends_with(&value.trim().to_ascii_lowercase()))
}

fn rera_society_entity_id_candidates(society_id: &str) -> Vec<String> {
    let raw = society_id.trim().to_lowercase().replace(['_', ' '], "-");
    let slug = raw
        .strip_prefix("society:")
        .or_else(|| raw.strip_prefix("soc-"))
        .unwrap_or(&raw);
    let canonical = format!("society:{slug}");
    if raw == canonical {
        vec![canonical]
    } else {
        vec![canonical, raw]
    }
}

/// GET /api/properties/:id/rera — serves the accepted public RERA evidence
/// projection already loaded from the promoted serving bundle.
pub async fn get_property_rera(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<PropertySnapshotQuery>,
) -> Result<Json<ReraEvidenceReportResponse>, (StatusCode, Json<ErrorResponse>)> {
    let runtime = state.search_runtime.load_full();
    check_property_snapshot(&runtime, query.snapshot_identity.as_deref())?;
    let property = find_property_by_request_id(&runtime.properties, &id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "property_not_found".to_string(),
            }),
        )
    })?;
    let config = rera_report_surface_config().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "rera_surface_config_invalid".to_string(),
            }),
        )
    })?;
    let surface = rera_surface_response(config);
    let project_record = rera_info_for(&property.society_id, Some(&runtime.bundle.fact_index));
    let evidence_record = rera_evidence_for_property(&runtime.bundle, property);
    let buyer_report = ReraBuyerReport {
        fact_sections: rera_buyer_fact_sections_for_society(
            &property.society_id,
            Some(&runtime.bundle.fact_index),
            config,
        ),
        builder_portfolio: build_builder_portfolio(
            &runtime.properties,
            property,
            Some(&runtime.bundle.fact_index),
            &runtime.bundle.graph_index,
        ),
        complaints: rera_buyer_complaints(project_record.as_ref(), evidence_record),
        schedules: project_record
            .as_ref()
            .map(|record| record.schedule_sections.clone())
            .unwrap_or_default(),
        documents: project_record
            .as_ref()
            .map(|record| rera_buyer_documents(record, config))
            .unwrap_or_default(),
        registry_url: project_record.and_then(|record| record.rera_portal_url),
    };
    let mut response = rera_evidence_report_for_property(
        &property.id,
        &runtime.bundle.manifest.bundle_version,
        runtime.bundle.manifest.created_at,
        evidence_record,
        surface,
    );
    if response.availability == ReraEvidenceAvailability::Unavailable
        && !buyer_report.fact_sections.is_empty()
    {
        response.availability = ReraEvidenceAvailability::Partial;
    }
    response.buyer_report = Some(buyer_report);
    Ok(Json(response))
}

fn rera_evidence_report_for_property(
    property_id: &str,
    bundle_id: &str,
    generated_at: chrono::DateTime<chrono::Utc>,
    record: Option<&ServingReraEvidenceRecord>,
    surface: ReraReportSurfaceResponse,
) -> ReraEvidenceReportResponse {
    let Some(record) = record else {
        return ReraEvidenceReportResponse {
            availability: ReraEvidenceAvailability::Unavailable,
            evidence: empty_rera_evidence_projection(property_id, bundle_id, generated_at),
            surface,
            buyer_report: None,
        };
    };
    ReraEvidenceReportResponse {
        availability: rera_evidence_availability(record),
        evidence: ReraEvidenceProjectionResponse {
            schema_version: RERA_EVIDENCE_SCHEMA_VERSION.to_string(),
            property_id: property_id.to_string(),
            bundle_id: bundle_id.to_string(),
            generated_at,
            registration_ids: record.registration_ids.clone(),
            entities: record.entities.clone(),
            claims: record.claims.iter().map(ReraPublicClaim::from).collect(),
            events: record.events.clone(),
            series: record.series.clone(),
            discrepancies: record.discrepancies.clone(),
            regulatory_coverage: record.regulatory_coverage.clone(),
            source_index: record.source_index.clone(),
        },
        surface,
        buyer_report: None,
    }
}

fn rera_surface_response(config: &ReraReportSurfaceFile) -> ReraReportSurfaceResponse {
    let mut sections = config.sections.clone();
    sections.sort_by_key(|section| section.rank);
    ReraReportSurfaceResponse {
        version: config.version,
        coverage_note: config.coverage_note.clone(),
        regulatory_event_order: config.regulatory_event_order.clone(),
        sections: sections
            .into_iter()
            .map(|section| ReraReportSurfaceSectionResponse {
                id: section.id,
                title: section.title,
                renderer: section.renderer,
                selectors: section.selectors,
                items_per_page: section.items_per_page,
                preview_kinds: section.preview_kinds,
                empty_behavior: section.empty_behavior,
            })
            .collect(),
    }
}

fn empty_rera_evidence_projection(
    property_id: &str,
    bundle_id: &str,
    generated_at: chrono::DateTime<chrono::Utc>,
) -> ReraEvidenceProjectionResponse {
    ReraEvidenceProjectionResponse {
        schema_version: RERA_EVIDENCE_SCHEMA_VERSION.to_string(),
        property_id: property_id.to_string(),
        bundle_id: bundle_id.to_string(),
        generated_at,
        registration_ids: Vec::new(),
        entities: Vec::new(),
        claims: Vec::new(),
        events: Vec::new(),
        series: Vec::new(),
        discrepancies: Vec::new(),
        regulatory_coverage: Vec::new(),
        source_index: Vec::new(),
    }
}

fn rera_info_for(society_id: &str, serving_facts: Option<&ServingFactIndex>) -> Option<ReraInfo> {
    let serving_facts = serving_facts?;
    let projection = SocietyFactProjection::from_index(serving_facts, society_id);
    let has_serving_rera = projection.latest_bool("rera_registered").is_some()
        || projection.latest_text("rera_number").is_some()
        || projection
            .latest_text("rera_complaint_summary_manifest")
            .is_some()
        || projection.latest_text("rera_document_manifest").is_some()
        || projection
            .latest_text("rera_plan_artifact_manifest")
            .is_some();
    if !has_serving_rera {
        return None;
    }
    let mut info = ReraInfo::default();
    if let Some(fact) = projection.latest_bool("rera_registered") {
        info.registered = fact.value;
    }
    if let Some(fact) = projection.latest_text("rera_number") {
        info.registration_number = Some(fact.value);
    }
    if let Some(fact) = projection.latest_text("rera_status") {
        info.status = Some(fact.value);
    }
    if let Some(fact) = projection
        .latest_text("rera_start_date")
        .or_else(|| projection.latest_text("project_start_date"))
    {
        info.start_date = Some(fact.value);
    }
    if let Some(fact) = projection.latest_text("rera_completion_date") {
        info.completion_date = Some(fact.value);
    }
    if let Some(fact) = projection.latest_text("rera_original_completion_date") {
        info.original_completion_date = Some(fact.value);
    }
    if let Some(fact) = projection.latest_numeric("rera_delay_months") {
        info.delay_months = projected_i32(fact.value);
    }
    if let Some(fact) = projection.latest_numeric("rera_total_units") {
        info.total_units = projected_i32(fact.value);
    }
    if let Some(fact) = projection.latest_numeric("rera_total_land_area_sqm") {
        info.total_land_area_sqm = Some(fact.value);
        info.total_land_area_acres = Some(fact.value / 4_046.856_422_4);
    } else if let Some(fact) = projection.latest_numeric("project_land_area_acres") {
        info.total_land_area_acres = Some(fact.value);
    }
    if let Some(fact) = projection
        .latest_numeric("project_open_area_pct")
        .or_else(|| projection.latest_numeric("rera_open_area_pct"))
    {
        info.open_area_pct = Some(fact.value);
    }
    info.total_project_cost_inr = None;
    info.land_cost_inr = None;
    info.construction_cost_inr = None;
    info.cost_per_unit_inr = None;
    if let Some(fact) = projection.latest_numeric("rera_complaints_count") {
        info.complaints_count = projected_i32(fact.value);
    }
    if let Some(fact) = projection.latest_numeric("rera_complaints_resolved_pct") {
        info.complaints_resolved_pct = Some(fact.value);
    }
    if let Some(fact) = projection.latest_numeric("rera_project_complaints_count") {
        info.project_complaints_count = projected_i32(fact.value);
    }
    if let Some(fact) = projection.latest_numeric("rera_project_complaints_open_count") {
        info.project_complaints_open_count = projected_i32(fact.value);
    }
    if let Some(fact) = projection.latest_numeric("rera_project_complaints_disposed_count") {
        info.project_complaints_disposed_count = projected_i32(fact.value);
    }
    if let Some(fact) = projection.latest_numeric("rera_promoter_complaints_count") {
        info.promoter_complaints_count = projected_i32(fact.value);
    }
    if let Some(fact) = projection.latest_numeric("rera_promoter_complaints_open_count") {
        info.promoter_complaints_open_count = projected_i32(fact.value);
    }
    if let Some(fact) = projection.latest_numeric("rera_promoter_complaints_disposed_count") {
        info.promoter_complaints_disposed_count = projected_i32(fact.value);
    }
    if let Some(fact) = projection.latest_text("rera_complaint_summary_manifest") {
        info.complaint_summaries =
            parse_rera_projection_json::<Vec<ReraComplaintScopeSummary>>(&fact.value)
                .unwrap_or_default();
    }
    if let Some(fact) = projection.latest_text("rera_schedule_manifest") {
        info.schedule_sections =
            parse_rera_projection_json::<Vec<ReraScheduleSection>>(&fact.value).unwrap_or_default();
    }
    if let Some(fact) = projection.latest_text("rera_document_manifest") {
        info.document_manifest = parse_rera_document_manifest_value(&fact.value);
        refresh_rera_document_summary(&mut info);
    }
    if let Some(fact) = projection.latest_text("rera_plan_artifact_manifest") {
        merge_rera_document_manifest(
            &mut info.document_manifest,
            parse_rera_document_manifest_value(&fact.value),
        );
        refresh_rera_document_summary(&mut info);
    }
    if let Some(fact) = projection.latest_numeric("rera_builder_projects_count") {
        info.builder_total_projects = projected_i32(fact.value);
    }
    if let Some(fact) = projection.latest_numeric("rera_builder_revocations") {
        info.builder_revocations = projected_i32(fact.value);
    }
    if let Some(fact) = projection.latest_text("rera_builder_states") {
        info.builder_states = fact
            .value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect();
    }
    if let Some(fact) = projection.latest_bool("rera_land_litigation") {
        info.land_litigation = Some(fact.value);
    }
    if let Some(fact) = projection.latest_text("rera_escrow_bank") {
        info.escrow_bank = Some(fact.value);
    }
    if let Some(fact) = projection.latest_bool("rera_has_borrowing") {
        info.has_borrowing = Some(fact.value);
    }
    if let Some(fact) = projection.latest_bool("rera_has_mortgage") {
        info.has_mortgage = Some(fact.value);
    }
    if let Some(fact) = projection.latest_text("rera_portal_url") {
        info.rera_portal_url = Some(fact.value);
    }
    info.last_verified = projection
        .provenance_time_with_prefix("rera_")
        .map(|timestamp| timestamp.to_rfc3339())
        .or(info.last_verified);
    info.decision_cards = rera_decision_cards(&info);
    Some(info)
}

fn parse_rera_projection_json<T: serde::de::DeserializeOwned>(value: &str) -> Option<T> {
    serde_json::from_str(value).ok()
}

fn projected_i32(value: f64) -> Option<i32> {
    (value.is_finite() && value >= i32::MIN as f64 && value <= i32::MAX as f64)
        .then(|| value.round() as i32)
}

/// Count non-empty lines in a JSONL file (interest events).
async fn count_interest_lines(path: &std::path::Path) -> usize {
    match tokio::fs::read_to_string(path).await {
        Ok(contents) => contents
            .lines()
            .filter(|l: &&str| !l.trim().is_empty())
            .count(),
        Err(_) => 0,
    }
}

#[cfg(test)]
mod serving_state_tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::models::{Property, Society};
    use crate::serving::{ServingFactRecord, ServingSearchMetadataRecord};

    #[test]
    fn rera_evidence_report_distinguishes_available_partial_and_unavailable() {
        let captured_at = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let surface = ReraReportSurfaceResponse {
            version: 1,
            coverage_note: "High Court proceedings are outside this release.".to_string(),
            regulatory_event_order: vec!["historical".to_string()],
            sections: Vec::new(),
        };
        let available = rera_record_with_coverage(true, captured_at);
        let response = rera_evidence_report_for_property(
            "sample-3bhk",
            "bundle-1",
            captured_at,
            Some(&available),
            surface.clone(),
        );
        assert_eq!(response.availability, ReraEvidenceAvailability::Available);
        assert_eq!(response.evidence.registration_ids, vec!["reg-1"]);

        let partial = rera_record_with_coverage(false, captured_at);
        let response = rera_evidence_report_for_property(
            "sample-3bhk",
            "bundle-1",
            captured_at,
            Some(&partial),
            surface.clone(),
        );
        assert_eq!(response.availability, ReraEvidenceAvailability::Partial);

        let response = rera_evidence_report_for_property(
            "sample-3bhk",
            "bundle-1",
            captured_at,
            None,
            surface,
        );
        assert_eq!(response.availability, ReraEvidenceAvailability::Unavailable);
        assert!(response.evidence.registration_ids.is_empty());
        assert!(response.evidence.claims.is_empty());
    }

    #[test]
    fn property_detail_rera_reference_is_stable_without_evidence() {
        let reference = rera_report_ref_for_property(&property(), None);
        assert_eq!(reference.href, "/property/sample-3bhk/rera");
        assert_eq!(
            reference.availability,
            ReraEvidenceAvailability::Unavailable
        );
        assert!(reference.registration_ids.is_empty());
    }

    #[test]
    fn buyer_report_documents_keep_only_public_links_and_configured_group_labels() {
        let info = ReraInfo {
            document_manifest: vec![
                ReraDocumentManifestItem {
                    artifact_id: "public-noc".to_string(),
                    label: "Fire clearance".to_string(),
                    source_url: Some("https://example.com/fire-clearance.pdf".to_string()),
                    document_group: "approvals_nocs".to_string(),
                    buyer_visibility: Some("public".to_string()),
                    ..ReraDocumentManifestItem::default()
                },
                ReraDocumentManifestItem {
                    artifact_id: "private-finance".to_string(),
                    label: "Account statement".to_string(),
                    source_url: Some("https://example.com/private.pdf".to_string()),
                    document_group: "promoter_financials".to_string(),
                    buyer_visibility: Some("private_or_sensitive".to_string()),
                    ..ReraDocumentManifestItem::default()
                },
            ],
            ..ReraInfo::default()
        };

        let documents = rera_buyer_documents(
            &info,
            rera_report_surface_config().expect("RERA surface config"),
        );
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].id, "public-noc");
        assert_eq!(documents[0].group_label, "Approvals and NOCs");
    }

    #[test]
    fn buyer_report_omits_facts_without_an_explicit_section() {
        let config = rera_report_surface_config().expect("RERA surface config");

        let registration = rera_buyer_section_for_key(config, "rera_registration_number")
            .expect("configured registration fact should have a buyer section");
        assert_eq!(registration.0, "registration");
        assert!(rera_buyer_section_for_key(config, "rera_unmapped_internal_fact").is_none());
    }

    #[test]
    fn buyer_report_maps_legal_finance_and_complaints_to_explicit_sections() {
        let config = rera_report_surface_config().expect("RERA surface config");

        assert_eq!(
            rera_buyer_section_for_key(config, "rera_land_litigation").map(|section| section.0),
            Some("finance")
        );
        assert_eq!(
            rera_buyer_section_for_key(config, "rera_has_borrowing").map(|section| section.0),
            Some("finance")
        );
        assert_eq!(
            rera_buyer_section_for_key(config, "rera_project_complaints_count")
                .map(|section| section.0),
            Some("complaints")
        );
    }

    #[test]
    fn buyer_complaint_projection_retains_scope_coverage_themes_and_subjects() {
        let info = ReraInfo {
            complaint_summaries: vec![
                ReraComplaintScopeSummary {
                    scope: "project".to_string(),
                    total_count_from_tab_label: Some(62),
                    row_count_parsed: 10,
                    disposed_count: 7,
                    open_count: 3,
                    theme_counts: [("refund".to_string(), 6), ("delay".to_string(), 2)]
                        .into_iter()
                        .collect(),
                    sample_subjects: vec![
                        "Refund after cancellation".to_string(),
                        "Possession delay".to_string(),
                    ],
                    confidence: 0.88,
                    validation_notes: vec!["Status counts cover returned rows".to_string()],
                },
                ReraComplaintScopeSummary {
                    scope: "promoter".to_string(),
                    total_count_from_tab_label: Some(0),
                    ..ReraComplaintScopeSummary::default()
                },
            ],
            ..ReraInfo::default()
        };

        let complaints = rera_buyer_complaints(Some(&info), None);
        assert_eq!(complaints.len(), 1);
        assert_eq!(complaints[0].scope, "project");
        assert_eq!(complaints[0].total, 62);
        assert_eq!(complaints[0].rows_parsed, 10);
        assert!(!complaints[0].status_counts_complete);
        assert_eq!(complaints[0].theme_counts["refund"], 6);
        assert_eq!(
            complaints[0].sample_subjects,
            vec!["Refund after cancellation", "Possession delay"]
        );
    }

    #[test]
    fn builder_portfolio_is_limited_to_unique_projects_in_the_local_catalog() {
        let current = property();
        let mut sibling = property();
        sibling.id = "sibling-3bhk".to_string();
        sibling.title = "3 BHK in Sibling Society".to_string();
        sibling.society_id = "sibling".to_string();
        sibling.area = "Varthur".to_string();
        let mut duplicate_configuration = sibling.clone();
        duplicate_configuration.id = "sibling-2bhk".to_string();
        duplicate_configuration.bhk = 2;

        let graph_index = crate::graph::GraphIndex::from_serving_edges(
            &["society:sample", "society:sibling"].map(|id| crate::serving::ServingEdgeRecord {
                from_entity_id: id.to_string(),
                to_entity_id: "builder:shared".to_string(),
                edge_type: "built_by".to_string(),
                confidence: 1.0,
                source_type: "Rera".to_string(),
                derivation: None,
            }),
        );
        let portfolio = build_builder_portfolio(
            &[current.clone(), sibling, duplicate_configuration],
            &current,
            None,
            &graph_index,
        )
        .expect("two catalog projects from one builder should be projected");

        assert_eq!(portfolio.tracked_projects, 2);
        assert_eq!(
            portfolio
                .projects
                .iter()
                .filter(|project| project.current)
                .count(),
            1
        );
    }

    fn rera_record_with_coverage(
        complete: bool,
        latest_observed_at: chrono::DateTime<Utc>,
    ) -> ServingReraEvidenceRecord {
        ServingReraEvidenceRecord {
            society_id: "sample".to_string(),
            registration_ids: vec!["reg-1".to_string()],
            entities: Vec::new(),
            claims: Vec::new(),
            events: Vec::new(),
            series: Vec::new(),
            discrepancies: Vec::new(),
            regulatory_coverage: ["K-RERA"]
                .into_iter()
                .take(if complete { 1 } else { 0 })
                .map(|source| ReraRegulatoryCoverage {
                    source: source.to_string(),
                    checked_at: latest_observed_at,
                    status: "checked".to_string(),
                })
                .collect(),
            source_index: Vec::new(),
        }
    }

    #[test]
    fn search_card_and_property_detail_project_the_same_current_review_facts() {
        let property = property();
        let society = society();
        let serving = serving_index();

        let mut card = property_card(&property, std::slice::from_ref(&society));
        crate::search::text::enrich_card_from_serving_facts(
            &mut card,
            &serving,
            &property.society_id,
        );
        let detail = external_reviews_for(&property.society_id, Some(&serving))
            .expect("review evidence should be present");

        assert_eq!(detail.google_rating, card.google_rating);
        assert_eq!(detail.google_review_count, card.google_review_count);
        assert_eq!(detail.google_reviews_url, card.google_reviews_url);
        assert_eq!(detail.google_rating, Some(4.6));
        assert_eq!(detail.google_review_count, Some(431));
        assert_eq!(
            detail.google_reviews_url.as_deref(),
            Some("https://example.com/current")
        );
    }

    #[test]
    fn property_detail_withholds_reviews_without_serving_facts() {
        assert!(external_reviews_for("sample", None).is_none());
    }

    #[test]
    fn property_detail_review_cards_rank_helpful_recent_reviews_and_keep_concerns() {
        let serving = ServingFactIndex::from_records(
            vec![serving_fact(
                "google_review_cards",
                FactValue::Tags(vec![
                    serde_json::json!({
                        "author": "Newest Low Signal",
                        "rating": 5.0,
                        "published_at": "2026-07-20",
                        "date_label": "July 2026",
                        "helpful_count": 1,
                        "text": "Good greenery and clubhouse."
                    })
                    .to_string(),
                    serde_json::json!({
                        "author": "Most Helpful",
                        "rating": 5.0,
                        "published_at": "2026-06-20",
                        "date_label": "June 2026",
                        "helpful_count": 18,
                        "text": "Well maintained campus with clean common areas."
                    })
                    .to_string(),
                    serde_json::json!({
                        "author": "Concerned Resident",
                        "rating": 3.0,
                        "published_at": "2026-07-18",
                        "date_label": "July 2026",
                        "helpful_count": 2,
                        "text": "Traffic near the approach road is still a problem."
                    })
                    .to_string(),
                ]),
                20,
            )],
            Vec::<ServingSearchMetadataRecord>::new(),
        );

        let detail =
            external_reviews_for("sample", Some(&serving)).expect("review cards should be exposed");

        assert_eq!(detail.reviews.len(), 3);
        assert_eq!(detail.reviews[0].author.as_deref(), Some("Most Helpful"));
        assert!(detail
            .reviews
            .iter()
            .any(|review| review.tone == ReviewTone::Concern
                && review.author.as_deref() == Some("Concerned Resident")));
    }

    #[test]
    fn detail_signals_are_review_themes_without_counts() {
        let external_reviews = ExternalReviews {
            google_rating: Some(4.6),
            google_review_count: Some(120),
            google_reviews_url: None,
            reviews: vec![
                ExternalReviewCard {
                    id: "r1".to_string(),
                    source: "Google".to_string(),
                    author: None,
                    rating: Some(5.0),
                    date_label: None,
                    helpful_count: Some(8),
                    text: "Clean society with good amenities and greenery.".to_string(),
                    tone: ReviewTone::Positive,
                },
                ExternalReviewCard {
                    id: "r2".to_string(),
                    source: "Google".to_string(),
                    author: None,
                    rating: Some(5.0),
                    date_label: None,
                    helpful_count: Some(3),
                    text: "Great location with close connectivity.".to_string(),
                    tone: ReviewTone::Positive,
                },
            ],
        };

        let signals = detail_signals_for(Some(&external_reviews), None);
        let keys = signals
            .iter()
            .map(|signal| signal.key.as_str())
            .collect::<Vec<_>>();

        assert!(keys.contains(&"amenities"));
        assert!(keys.contains(&"cleanliness"));
        assert!(keys.contains(&"location"));
        assert!(keys.contains(&"greenery"));
        assert!(!keys.contains(&"schools"));
        assert!(!keys.contains(&"hospitals"));
        assert!(signals.iter().all(|signal| signal.count.is_none()));
    }

    #[test]
    fn detail_signals_do_not_promote_complaints_as_positive_themes() {
        let external_reviews = ExternalReviews {
            google_rating: Some(3.4),
            google_review_count: Some(28),
            google_reviews_url: None,
            reviews: vec![ExternalReviewCard {
                id: "r1".to_string(),
                source: "Google".to_string(),
                author: None,
                rating: Some(2.0),
                date_label: None,
                helpful_count: Some(5),
                text: "Bad location and poor maintenance near the approach road.".to_string(),
                tone: ReviewTone::Concern,
            }],
        };

        assert!(detail_signals_for(Some(&external_reviews), None).is_empty());
    }

    #[test]
    fn review_tone_treats_high_rated_complaint_text_as_concern() {
        assert_eq!(
            review_tone(
                Some(5.0),
                "Good society, but poor maintenance is a problem."
            ),
            ReviewTone::Concern
        );
    }

    #[test]
    fn detail_signals_can_promote_positive_maintenance_wording() {
        let text = "Excellent maintenance and well managed common areas.";
        let external_reviews = ExternalReviews {
            google_rating: Some(4.8),
            google_review_count: Some(64),
            google_reviews_url: None,
            reviews: vec![ExternalReviewCard {
                id: "r1".to_string(),
                source: "Google".to_string(),
                author: None,
                rating: Some(5.0),
                date_label: None,
                helpful_count: Some(6),
                text: text.to_string(),
                tone: review_tone(Some(5.0), text),
            }],
        };

        let signals = detail_signals_for(Some(&external_reviews), None);
        let keys = signals
            .iter()
            .map(|signal| signal.key.as_str())
            .collect::<Vec<_>>();

        assert!(keys.contains(&"maintenance"));
    }

    #[test]
    fn property_detail_projects_current_rera_facts_and_exposes_acreage() {
        let serving = ServingFactIndex::from_records(
            vec![
                serving_fact("rera_registered", FactValue::Bool(true), 10),
                serving_fact(
                    "rera_number",
                    FactValue::Text("PRM-CURRENT".to_string()),
                    10,
                ),
                serving_fact("rera_total_units", FactValue::Numeric(1_520.0), 10),
                serving_fact(
                    "rera_total_land_area_sqm",
                    FactValue::Numeric(112_652.0),
                    10,
                ),
                serving_fact("rera_delay_months", FactValue::Numeric(12.0), 10),
                serving_fact(
                    "rera_complaint_summary_manifest",
                    FactValue::Text(
                        serde_json::json!([
                            {
                                "scope": "project",
                                "total_count_from_tab_label": 15,
                                "row_count_parsed": 15,
                                "disposed_count": 9,
                                "open_count": 6,
                                "theme_counts": {
                                    "refund": 9,
                                    "cancellation": 3,
                                    "agreement_payment": 2
                                },
                                "sample_subjects": ["Refund after cancellation"],
                                "confidence": 0.88,
                                "validation_notes": []
                            }
                        ])
                        .to_string(),
                    ),
                    10,
                ),
                serving_fact(
                    "rera_document_manifest",
                    FactValue::Text(
                        serde_json::json!([
                            {
                                "artifact_id": "site-1",
                                "kind": "site_plan",
                                "label": "Site plan",
                                "document_group": "plans",
                                "buyer_visibility": "buyer_visible",
                                "preview_policy": "preview_allowed"
                            },
                            {
                                "artifact_id": "khata-1",
                                "kind": "khata",
                                "label": "Khata",
                                "document_group": "legal_land",
                                "buyer_visibility": "buyer_visible",
                                "preview_policy": "list_only"
                            }
                        ])
                        .to_string(),
                    ),
                    10,
                ),
                serving_fact(
                    "rera_plan_artifact_manifest",
                    FactValue::Text(
                        serde_json::json!([
                            {
                                "artifact_id": "plan-site",
                                "kind": "site_plan",
                                "label": "Site Plan.pdf",
                                "source_url": "https://rera.karnataka.gov.in/download_jc?DOC_ID=site"
                            },
                            {
                                "artifact_id": "plan-sanction",
                                "kind": "sanction_plan",
                                "label": "Sanction Plans.pdf",
                                "source_url": "https://rera.karnataka.gov.in/download_jc?DOC_ID=sanction"
                            }
                        ])
                        .to_string(),
                    ),
                    10,
                ),
                serving_fact("rera_has_mortgage", FactValue::Bool(true), 10),
            ],
            Vec::<ServingSearchMetadataRecord>::new(),
        );

        let detail = rera_info_for("sample", Some(&serving))
            .expect("serving RERA facts should create detail evidence");

        assert!(detail.registered);
        assert_eq!(detail.registration_number.as_deref(), Some("PRM-CURRENT"));
        assert_eq!(detail.total_units, Some(1_520));
        assert_eq!(detail.total_land_area_sqm, Some(112_652.0));
        assert!((detail.total_land_area_acres.unwrap() - 27.8369).abs() < 0.001);
        assert_eq!(
            detail.last_verified.as_deref(),
            Some("1970-01-01T00:00:10+00:00")
        );
        let card_titles = detail
            .decision_cards
            .iter()
            .map(|card| card.title.as_str())
            .collect::<Vec<_>>();
        assert!(card_titles.contains(&"Delivery moved by 12 months"));
        assert!(
            card_titles.contains(&"Mostly money back / refund complaints"),
            "expected rolled-up complaint title in {card_titles:?}"
        );
        assert!(card_titles.contains(&"Official files available"));
        assert!(card_titles.contains(&"Legal follow-up needed"));
        assert!(detail.document_manifest.iter().any(|item| {
            item.kind == "site_plan"
                && item.source_url.as_deref()
                    == Some("https://rera.karnataka.gov.in/download_jc?DOC_ID=site")
        }));
        let complaints = detail
            .decision_cards
            .iter()
            .find(|card| card.id == "complaints_project")
            .expect("complaint decision card should exist");
        assert_eq!(complaints.labels, vec!["legal", "risk"]);
        assert_eq!(complaints.facts["total"], 15);
        assert_eq!(complaints.facts["fine_theme_counts"]["refund"], 9);
    }

    #[test]
    fn source_panels_project_current_serving_land_area_and_review_link() {
        let property = property();
        let serving = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "rera_total_land_area_sqm",
                    FactValue::Numeric(112_652.0),
                    10,
                ),
                serving_fact(
                    "google_reviews_url",
                    FactValue::Text("https://example.com/current".to_string()),
                    10,
                ),
            ],
            Vec::<ServingSearchMetadataRecord>::new(),
        );

        let panels = build_source_panels("test-snapshot", &property, Some(&serving), None);
        let keys = panels
            .iter()
            .flat_map(|panel| panel.items.iter().map(|item| item.key.as_str()))
            .collect::<Vec<_>>();

        assert!(
            keys.contains(&"rera_total_land_area_sqm"),
            "RERA land area should be visible in detail source panels: {keys:?}"
        );
        let community = panels
            .iter()
            .find(|panel| panel.kind == "community")
            .expect("community pulse should exist when Google review facts are present");
        assert!(
            community.community_pulse.as_ref().is_some_and(|pulse| pulse
                .source_urls
                .iter()
                .any(|url| url == "https://example.com/current")),
            "Google review link should be visible in community pulse sources"
        );
    }

    #[test]
    fn source_panels_withhold_claims_without_observations() {
        let mut fact = serving_fact_with_url(
            "rera_total_land_area_sqm",
            FactValue::Numeric(112_652.0),
            "https://rera.example/project",
            10,
        );
        fact.observation = None;
        let serving = ServingFactIndex::from_records(vec![fact], Vec::new());
        let panels = build_source_panels("test-snapshot", &property(), Some(&serving), None);
        assert!(
            panels.iter().all(|panel| panel.items.is_empty()),
            "a source URL alone cannot establish a receipt"
        );
    }

    #[test]
    fn source_panels_include_dynamic_nearby_items_when_backed_by_facts() {
        let property = property();
        let serving = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "nearby_schools",
                    FactValue::Text("Far School (4.0 km, 4.5 rating)".to_string()),
                    10,
                ),
                serving_fact_with_url(
                    "nearby_schools",
                    FactValue::Text("Greenwood High (1.2 km, 4.3 rating)".to_string()),
                    "https://maps.google.com/greenwood",
                    10,
                ),
            ],
            Vec::<ServingSearchMetadataRecord>::new(),
        );

        let panels = build_source_panels("test-snapshot", &property, Some(&serving), None);
        let nearby = panels
            .iter()
            .find(|panel| panel.kind == "nearby")
            .expect("nearby panel should appear when map-backed nearby facts exist");

        assert_eq!(nearby.items[0].key, "nearby_schools");
        assert_eq!(
            nearby.items[0].values,
            vec![
                "Greenwood High (1.2 km, 4.3 rating)".to_string(),
                "Far School (4.0 km, 4.5 rating)".to_string(),
            ]
        );
        assert_eq!(
            nearby.items[0].attributions[0].source_url.as_deref(),
            Some("https://maps.google.com/greenwood")
        );
        assert!(nearby.missing.is_empty());
    }

    #[test]
    fn source_panels_merge_google_reviews_into_community_pulse() {
        let property = property();
        let serving = ServingFactIndex::from_records(
            vec![
                serving_fact("google_rating", FactValue::Numeric(3.9), 10),
                serving_fact("google_review_count", FactValue::Numeric(392.0), 10),
                serving_fact(
                    "google_review_snippets",
                    FactValue::Tags(vec![
                        "Amenities and greenery are repeatedly praised.".to_string(),
                        "Traffic is still called out as a concern.".to_string(),
                    ]),
                    10,
                ),
            ],
            Vec::<ServingSearchMetadataRecord>::new(),
        );

        let panels = build_source_panels("test-snapshot", &property, Some(&serving), None);
        let community = panels
            .iter()
            .find(|panel| panel.kind == "community")
            .expect("community pulse should be backed by current summary and Google facts");
        let pulse = community
            .community_pulse
            .as_ref()
            .expect("community pulse should expose structured read model");

        assert_eq!(pulse.source_label, "Google review");
        assert_eq!(pulse.sentiment_band, "Mixed-positive");
        assert!(pulse.paragraph.contains("greenery"));
        assert!(pulse.paragraph.contains("amenities"));
        assert!(pulse.paragraph.contains("traffic"));
        assert!(!pulse.paragraph.contains("3.9"));
        assert!(!pulse.paragraph.contains("/5"));
        assert_eq!(pulse.positives, vec!["amenities", "greenery"]);
        assert_eq!(pulse.concerns, vec!["traffic"]);
        assert_eq!(pulse.quotes.len(), 2);
        assert!(community.items.is_empty());
        assert!(
            panels.iter().all(|panel| panel.kind != "reviews"),
            "Google reviews should be a source inside Community pulse, not a separate panel"
        );
        assert!(!community
            .missing
            .iter()
            .any(|item| item.contains("Review text is not ingested")));
    }

    #[test]
    fn source_panels_do_not_derive_approach_road_signal_from_review_snippets() {
        let property = property();
        let serving = ServingFactIndex::from_records(
            vec![serving_fact(
                "google_review_snippets",
                FactValue::Tags(vec![
                    "Approach road is wide and clean near the main gate.".to_string(),
                    "Clubhouse and greenery are repeatedly praised.".to_string(),
                    "Access road has some road digging during peak hours.".to_string(),
                ]),
                10,
            )],
            Vec::<ServingSearchMetadataRecord>::new(),
        );

        let panels = build_source_panels("test-snapshot", &property, Some(&serving), None);
        assert!(panels
            .iter()
            .filter(|panel| panel.kind == "approach_road")
            .flat_map(|panel| panel.items.iter())
            .all(|item| item.key != "approach_road_condition"));
    }

    #[test]
    fn source_panels_include_approach_road_fact_from_served_road_segment() {
        let property = property();
        let serving = ServingFactIndex::from_records(
            vec![serving_fact_for_entity(
                "road_segment:sample-approach",
                "access_road_quality",
                FactValue::Text(
                    "Gate-side approach road identified; visual road-width verification is pending."
                        .to_string(),
                ),
                10,
            )],
            Vec::<ServingSearchMetadataRecord>::new(),
        );
        let graph_index =
            crate::graph::GraphIndex::from_serving_edges(&[crate::serving::ServingEdgeRecord {
                from_entity_id: "society:sample".to_string(),
                edge_type: "served_by_road".to_string(),
                to_entity_id: "road_segment:sample-approach".to_string(),
                confidence: 0.82,
                source_type: "approach_road".to_string(),
                derivation: None,
            }]);

        let panels = build_source_panels(
            "test-snapshot",
            &property,
            Some(&serving),
            Some(&graph_index),
        );
        let approach = panels
            .iter()
            .find(|panel| panel.kind == "approach_road")
            .expect("road-segment facts should produce an approach-road panel");
        let item = approach
            .items
            .iter()
            .find(|item| item.key == "access_road_quality")
            .expect("served road segment fact should be surfaced");

        assert_eq!(item.entity_id, "road_segment:sample-approach");
        assert_eq!(item.scope, "road_segment");
        assert_eq!(item.relationship.as_deref(), Some("approach road"));
        assert!(item.value.contains("Gate-side approach road"));
        assert!(
            !approach
                .items
                .iter()
                .any(|item| item.key == "approach_road_condition"),
            "road-segment facts should not require review-snippet phrase matching"
        );
    }

    #[test]
    fn source_panels_expose_six_approach_road_frames() {
        let property = property();
        let frames = (0..6)
            .map(|index| {
                serde_json::json!({
                    "label": format!("Frame {}", index + 1),
                    "distance_from_gate_m": if index < 4 { 0 } else { (index - 3) * 80 },
                    "latitude": 12.9819914,
                    "longitude": 77.7421819,
                    "location_query": null,
                    "pano_id": null,
                    "radius_m": 250,
                    "heading": (index as f64) * 45.0,
                    "pitch": 0.0,
                    "fov": 80.0,
                    "capture_date": "latest available",
                    "image_url": format!("https://example.com/frame-{index}.jpg")
                })
            })
            .collect::<Vec<_>>();
        let payload = serde_json::json!({
            "provider": "Google Street View",
            "coverage_quality": "usable",
            "frames": frames
        })
        .to_string();
        let serving = ServingFactIndex::from_records(
            vec![serving_fact_for_entity(
                "road_segment:sample-approach",
                "media.approach_road_frames",
                FactValue::Text(payload),
                10,
            )],
            Vec::<ServingSearchMetadataRecord>::new(),
        );
        let graph_index =
            crate::graph::GraphIndex::from_serving_edges(&[crate::serving::ServingEdgeRecord {
                from_entity_id: "society:sample".to_string(),
                edge_type: "served_by_road".to_string(),
                to_entity_id: "road_segment:sample-approach".to_string(),
                confidence: 0.82,
                source_type: "approach_road".to_string(),
                derivation: None,
            }]);

        let panels = build_source_panels(
            "test-snapshot",
            &property,
            Some(&serving),
            Some(&graph_index),
        );
        let approach = panels
            .iter()
            .find(|panel| panel.kind == "approach_road")
            .expect("served media frames should produce an approach-road panel");
        let strip = approach
            .media
            .first()
            .expect("approach-road panel should include a media strip");

        assert!(
            approach.items.is_empty(),
            "visual media must not synthesize buyer claims"
        );
        assert_eq!(strip.frames.len(), 6);
        assert_eq!(strip.frames[4].distance_from_gate_m, 80);
        assert_eq!(strip.frames[5].distance_from_gate_m, 160);
    }

    #[test]
    fn property_evidence_sections_are_backend_shaped_for_dynamic_ui() {
        let serving = ServingFactIndex::from_records(
            vec![
                serving_fact(
                    "rera_number",
                    FactValue::Text("PRM-UI-CONTRACT".to_string()),
                    1,
                ),
                serving_fact(
                    "nearby_schools",
                    FactValue::Tags(vec![
                        "Greenwood High (1.2 km, 4.3 rating)".to_string(),
                        "Inventure Academy (2.1 km, 4.1 rating)".to_string(),
                    ]),
                    1,
                ),
            ],
            vec![],
        );
        let property = property();
        let response = build_property_evidence_response_from_panels(
            property.id.clone(),
            property.to_card("").kg_entity_refs,
            None,
            build_source_panels("test-snapshot", &property, Some(&serving), None),
        );
        assert_eq!(response.property_id, "sample-3bhk");
        assert_eq!(response.entity_refs.society_entity_id, "society:sample");
        assert!(response.sections.len() >= 2);
        assert!(
            response
                .sections
                .windows(2)
                .all(|pair| pair[0].priority <= pair[1].priority),
            "evidence sections should be sorted by backend priority: {:?}",
            response
                .sections
                .iter()
                .map(|section| (&section.kind, section.priority))
                .collect::<Vec<_>>()
        );

        let rera = response
            .sections
            .iter()
            .find(|section| section.kind == "rera")
            .expect("RERA section should be produced when RERA facts exist");
        assert_eq!(rera.priority, 10);
        assert_eq!(rera.constellation, "trust");
        assert_eq!(rera.header_meta, "1 facts · Rera");
        assert_eq!(rera.summary, "Registration: PRM-UI-CONTRACT");
        assert_eq!(rera.source_types, vec!["Rera".to_string()]);
        assert_eq!(rera.entity_ids, vec!["society:sample".to_string()]);
        assert!(
            rera.items
                .iter()
                .all(|item| item.entity_id == "society:sample"),
            "items should carry graph drilldown IDs: {:?}",
            rera.items
        );

        let nearby = response
            .sections
            .iter()
            .find(|section| section.kind == "nearby")
            .expect("nearby section should be produced when map-backed facts exist");
        assert_eq!(
            nearby.summary,
            "Schools: Greenwood High (1.2 km, 4.3 rating) +1 more"
        );
        assert!(nearby.missing.is_empty());
    }

    #[test]
    fn property_evidence_adds_config_only_section_without_rust_branch() {
        let serving = ServingFactIndex::from_records(
            vec![serving_fact(
                "test_config_only_signal",
                FactValue::Text("Config-only proof".to_string()),
                1,
            )],
            vec![],
        );
        let projection = SocietyFactProjection::from_index(&serving, "sample");
        let property = property();
        let mut definitions = buyer_context_definitions().to_vec();
        definitions.push(EvidenceSectionDefinition {
            kind: "test_config_only".to_string(),
            priority: 33,
            constellation: "trust".to_string(),
            surfaces: Vec::new(),
            title: "Config-only section".to_string(),
            subtitle: "Test-only section from DAG metadata.".to_string(),
            scope: "society".to_string(),
            relationship: "config-only proof".to_string(),
            derived: None,
            presentation: Some(EvidenceSectionPresentation {
                variant: "fact_grid".to_string(),
                density: "compact".to_string(),
                max_preview_items: 2,
            }),
            media: Vec::new(),
            missing: Vec::new(),
            facts: vec![ContextFactDefinition {
                key: "test_config_only_signal".to_string(),
                label: "Config-only signal".to_string(),
                scope: "society".to_string(),
                relationship: "config-only proof".to_string(),
                livability_lens: None,
                livability_label: None,
                max_values: None,
            }],
        });

        let panels = build_configured_evidence_panels_from_definitions(
            "test-snapshot",
            &definitions,
            &property,
            Some(&projection),
            Some(&serving),
            None,
        );
        let response = build_property_evidence_response_from_panels(
            property.id.clone(),
            property.to_card("").kg_entity_refs,
            None,
            panels,
        );
        let section = response
            .sections
            .iter()
            .find(|section| section.kind == "test_config_only")
            .expect("config-only section should appear from supplied DAG metadata");

        assert_eq!(section.title, "Config-only section");
        assert_eq!(section.priority, 33);
        assert_eq!(section.presentation.variant, "fact_grid");
        assert_eq!(section.items[0].label, "Config-only signal");
        assert_eq!(section.summary, "Config-only signal: Config-only proof");
    }

    #[test]
    fn property_evidence_includes_groundwater_potential_water_context() {
        let property = property();
        let serving = ServingFactIndex::from_records(
            vec![typed_serving_fact(
                "environment.groundwater_potential_class",
                FactValue::Text("Moderate".to_string()),
                "OpenCity",
                Some("https://data.opencity.in/example-groundwater.kml"),
                10,
            )],
            Vec::<ServingSearchMetadataRecord>::new(),
        );

        let response = build_property_evidence_response_from_panels(
            property.id.clone(),
            property.to_card("").kg_entity_refs,
            None,
            build_source_panels("test-snapshot", &property, Some(&serving), None),
        );
        let section = response
            .sections
            .iter()
            .find(|section| section.kind == "water_context")
            .expect("groundwater fact should produce the water context section");

        assert_eq!(section.title, "Water context");
        assert_eq!(section.priority, 34);
        assert_eq!(
            section.summary,
            "Moderate groundwater potential zone near the society."
        );
        assert_eq!(section.source_types, vec!["OpenCity".to_string()]);
        assert_eq!(
            section.items[0].key,
            "environment.groundwater_potential_class"
        );
        assert_eq!(
            section.items[0].relationship.as_deref(),
            Some("surrounding water quality")
        );
    }

    #[test]
    fn property_evidence_finds_groundwater_by_canonical_society_identity() {
        let mut property = property();
        property.society_id = "society:rera-sample".to_string();
        let serving = ServingFactIndex::from_records(
            vec![
                serving_fact_for_entity(
                    "society:rera-sample",
                    "environment.groundwater_potential_class",
                    FactValue::Text("Moderate".to_string()),
                    10,
                ),
                serving_fact_for_entity(
                    "society:rera-sample",
                    "listing_society",
                    FactValue::Text("Sample Society".to_string()),
                    10,
                ),
            ],
            Vec::<ServingSearchMetadataRecord>::new(),
        );

        let response = build_property_evidence_response_from_panels(
            property.id.clone(),
            property.to_card("").kg_entity_refs,
            None,
            build_source_panels("test-snapshot", &property, Some(&serving), None),
        );
        let section = response
            .sections
            .iter()
            .find(|section| section.kind == "water_context")
            .expect("matching RERA society groundwater fact should produce water context");

        assert_eq!(
            section.summary,
            "Moderate groundwater potential zone near the society."
        );
        assert_eq!(section.items[0].entity_id, "society:rera-sample");
    }

    #[test]
    fn property_evidence_omits_missing_only_sections() {
        let response = build_property_evidence_response(
            property().to_card("").kg_entity_refs,
            &property(),
            None,
        );

        assert!(
            response.sections.iter().all(|section| {
                !section.items.is_empty()
                    || !section.media.is_empty()
                    || section.community_pulse.is_some()
            }),
            "missing-only sections should stay out of the user-facing evidence response"
        );
    }

    #[test]
    fn evidence_summaries_do_not_repeat_self_labeled_values() {
        let item = SourceItem {
            evidence: Vec::new(),
            entity_id: "society:sample".to_string(),
            key: "listing_source_name_3bhk".to_string(),
            label: "3BHK source name".to_string(),
            value: "3BHK source name: MagicBricks".to_string(),
            scope: "society".to_string(),
            relationship: None,
            values: Vec::new(),
            source_type: "ExternalListing".to_string(),
            source_url: None,
            attributions: Vec::new(),
            confidence_pct: 90,
            learned_at: "2026-07-15T00:00:00Z".to_string(),
        };

        assert_eq!(source_item_summary(&item), "3BHK source name: MagicBricks");
    }

    #[test]
    fn market_trail_uses_configured_external_listing_facts() {
        let property = property();
        let serving = ServingFactIndex::from_records(
            vec![
                typed_serving_fact(
                    "listing_source_name_3bhk",
                    FactValue::Text("MagicBricks".to_string()),
                    "ExternalListing",
                    Some("https://www.magicbricks.com/example-green"),
                    11,
                ),
                typed_serving_fact(
                    "listing_price_range_3bhk",
                    FactValue::Text("INR 2.5 Cr - 3.5 Cr".to_string()),
                    "ExternalListing",
                    Some("https://www.magicbricks.com/example-green"),
                    11,
                ),
                typed_serving_fact(
                    "rent_monthly_range_3bhk",
                    FactValue::Text("INR 90K - 1.4L".to_string()),
                    "ExternalListing",
                    Some("https://www.squareyards.com/example-green-rent"),
                    11,
                ),
            ],
            Vec::<ServingSearchMetadataRecord>::new(),
        );

        let market_panel = build_source_panels("test-snapshot", &property, Some(&serving), None)
            .into_iter()
            .find(|panel| panel.kind == "market")
            .expect("Market trail should render when builder or listing facts exist");
        let market =
            evidence_section_from_panel(market_panel, &property.to_card("").kg_entity_refs);

        assert_eq!(market.source_types, vec!["ExternalListing".to_string()]);
        assert!(market.items.iter().any(|item| {
            item.key == "listing_source_name_3bhk"
                && item.label == "3BHK source name"
                && item.value == "MagicBricks"
                && item.source_type == "ExternalListing"
        }));
        assert!(market.items.iter().any(|item| {
            item.key == "rent_monthly_range_3bhk"
                && item.label == "3BHK monthly rent"
                && item.value == "INR 90K - 1.4L"
                && item.source_type == "ExternalListing"
        }));
    }

    fn serving_index() -> ServingFactIndex {
        ServingFactIndex::from_records(
            vec![
                serving_fact("google_rating", FactValue::Numeric(4.6), 2),
                serving_fact("google_review_count", FactValue::Numeric(431.0), 2),
                serving_fact(
                    "google_reviews_url",
                    FactValue::Text("https://example.com/current".to_string()),
                    2,
                ),
                serving_fact(
                    "google_reviews_url",
                    FactValue::Text("invalid".to_string()),
                    3,
                ),
            ],
            Vec::<ServingSearchMetadataRecord>::new(),
        )
    }

    fn serving_fact(key: &str, value: FactValue, learned_at: i64) -> ServingFactRecord {
        serving_fact_with_optional_url(key, value, None, learned_at)
    }

    fn serving_fact_for_entity(
        entity_id: &str,
        key: &str,
        value: FactValue,
        learned_at: i64,
    ) -> ServingFactRecord {
        let mut fact = serving_fact(key, value, learned_at);
        fact.entity_id = entity_id.to_string();
        fact.observation = Some(test_observation(&fact));
        fact
    }

    fn serving_fact_with_url(
        key: &str,
        value: FactValue,
        source_url: &str,
        learned_at: i64,
    ) -> ServingFactRecord {
        serving_fact_with_optional_url(key, value, Some(source_url.to_string()), learned_at)
    }

    fn serving_fact_with_optional_url(
        key: &str,
        value: FactValue,
        source_url: Option<String>,
        learned_at: i64,
    ) -> ServingFactRecord {
        typed_serving_fact(
            key,
            value,
            if key.starts_with("rera_") {
                "Rera"
            } else {
                "Google"
            },
            source_url.as_deref(),
            learned_at,
        )
    }

    fn typed_serving_fact(
        key: &str,
        value: FactValue,
        source_type: &str,
        source_url: Option<&str>,
        learned_at: i64,
    ) -> ServingFactRecord {
        let mut fact = ServingFactRecord {
            entity_id: "society:sample".to_string(),
            fact_key: key.to_string(),
            value_type: "test".to_string(),
            value_text: None,
            value,
            confidence: 0.9,
            source_type: source_type.to_string(),
            source_url: source_url.map(str::to_string),
            model: None,
            skill_id: None,
            learned_at: Utc.timestamp_opt(learned_at, 0).unwrap(),
            observation: None,
        };
        fact.observation = Some(test_observation(&fact));
        fact
    }

    fn test_observation(fact: &ServingFactRecord) -> crate::serving::SourceObservation {
        crate::serving::SourceObservation::new(
            &fact.source_type,
            format!("fixture:{}:{}", fact.fact_key, fact.learned_at),
            &fact.entity_id,
            fact.learned_at,
            fact.source_url.clone(),
            vec!["fixture:property-evidence".into()],
        )
        .unwrap()
    }

    fn society() -> Society {
        Society {
            id: "sample".to_string(),
            name: "Sample Society".to_string(),
            area: "Whitefield".to_string(),
            city: "Bengaluru".to_string(),
            builder_name: "Sample Builder".to_string(),
            year_built: 2020,
            total_units: 100,
            summary: String::new(),
            maintenance_sentiment: String::new(),
            livability_sentiment: String::new(),
            common_positives: Vec::new(),
            common_complaints: Vec::new(),
            review_summary: String::new(),
            google_reviews_url: Some("https://example.com/legacy".to_string()),
            future_google_place_name: String::new(),
            future_google_place_id: None,
            future_review_enrichment_status: String::new(),
        }
    }

    fn property() -> Property {
        Property {
            id: "sample-3bhk".to_string(),
            title: "3 BHK in Sample Society".to_string(),
            area: "Whitefield".to_string(),
            area_id: "whitefield".to_string(),
            city: "Bengaluru".to_string(),
            society_id: "sample".to_string(),
            builder_name: "Sample Builder".to_string(),
            property_type: "Apartment".to_string(),
            listing_type: "Resale".to_string(),
            bhk: 3,
            price: 20_000_000,
            price_min: None,
            price_max: None,
            price_per_sqft: 10_000,
            carpet_area_sqft: 1_500,
            super_builtup_sqft: 2_000,
            area_measurement: None,
            floor: 5,
            total_floors: 20,
            facing: "East".to_string(),
            possession_status: "Ready to move".to_string(),
            metro_distance_mins: 10,
            maintenance_cost_monthly: 8_000,
            society_quality_score: Some(0.8),
            builder_quality_score: Some(0.8),
            document_completeness_score: Some(0.8),
            litigation_risk: Some(0.1),
            noise_score: Some(0.2),
            sunlight_score: Some(0.8),
            airport_noise_score: Some(0.1),
            waterlogging_risk_score: Some(0.1),
            traffic_score: Some(0.4),
            days_on_market: 10,
            greenery_score: None,
            open_space_score: None,
            resale_strength_score: None,
            interest_level: None,
            saves_last_7d: None,
            offers_last_7d: None,
            images: Vec::new(),
            hero_image: String::new(),
            description_summary: String::new(),

            source_reference: String::new(),
        }
    }
}
