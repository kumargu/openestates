use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::knowledge::FactValue;
use crate::models::Property;
use crate::routes::enrichment::society_node_id;
use crate::search::{HashSemanticEmbedder, SemanticEmbedder};
use crate::serving::{LoadedServingBundle, ServingFactRecord};

const SUMMARY_JOB_TTL: Duration = Duration::from_secs(15 * 60);
const DEFAULT_SUMMARY_STYLE: &str = "buyer_brief";
const DEFAULT_MODEL_ID: &str = "openestates-local-summary-lm";
const MAX_EVIDENCE_REFS: usize = 12;
const GOOGLE_NEARBY_PLACES_SKILL_ID: &str = "fetch_google_nearby_places";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePropertySummaryJobRequest {
    #[serde(default)]
    pub summary_style: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PropertySummaryJobStatus {
    Pending,
    Ready,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertySummaryJobResponse {
    pub job_id: String,
    pub property_id: String,
    pub status: PropertySummaryJobStatus,
    pub summary_style: String,
    pub summary_paragraph: Option<String>,
    pub evidence_refs: Vec<SummaryEvidenceRef>,
    pub model: String,
    pub bundle_version: String,
    pub generated_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SummaryEvidenceRef {
    pub entity_id: String,
    pub label: String,
    pub source_type: String,
    pub source_url: Option<String>,
    pub learned_at: DateTime<Utc>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct SummaryJobKey {
    bundle_version: String,
    property_id: String,
    summary_style: String,
}

#[derive(Debug, Clone)]
struct SummaryJobRecord {
    key: SummaryJobKey,
    response: PropertySummaryJobResponse,
    expires_at: Instant,
}

#[derive(Debug, Default)]
pub struct PropertySummaryJobStore {
    jobs_by_id: HashMap<String, SummaryJobRecord>,
    jobs_by_key: HashMap<SummaryJobKey, String>,
}

impl PropertySummaryJobStore {
    pub fn create_or_get(
        &mut self,
        property_id: &str,
        bundle_version: &str,
        summary_style: Option<&str>,
    ) -> (PropertySummaryJobResponse, bool) {
        self.expire_old_jobs(Instant::now());
        let style = normalized_summary_style(summary_style);
        let key = SummaryJobKey {
            bundle_version: bundle_version.to_string(),
            property_id: property_id.to_string(),
            summary_style: style,
        };
        if let Some(job_id) = self.jobs_by_key.get(&key) {
            if let Some(record) = self.jobs_by_id.get(job_id) {
                return (record.response.clone(), false);
            }
        }

        let job_id = Uuid::new_v4().to_string();
        let response = PropertySummaryJobResponse {
            job_id: job_id.clone(),
            property_id: property_id.to_string(),
            status: PropertySummaryJobStatus::Pending,
            summary_style: key.summary_style.clone(),
            summary_paragraph: None,
            evidence_refs: Vec::new(),
            model: DEFAULT_MODEL_ID.to_string(),
            bundle_version: bundle_version.to_string(),
            generated_at: None,
            error_message: None,
        };
        let record = SummaryJobRecord {
            key: key.clone(),
            response: response.clone(),
            expires_at: Instant::now() + SUMMARY_JOB_TTL,
        };
        self.jobs_by_key.insert(key, job_id.clone());
        self.jobs_by_id.insert(job_id, record);
        (response, true)
    }

    pub fn get(&mut self, job_id: &str) -> Option<PropertySummaryJobResponse> {
        self.expire_old_jobs(Instant::now());
        self.jobs_by_id
            .get(job_id)
            .map(|record| record.response.clone())
    }

    pub fn complete_ready(
        &mut self,
        job_id: &str,
        summary_paragraph: String,
        evidence_refs: Vec<SummaryEvidenceRef>,
        model: String,
    ) {
        if let Some(record) = self.jobs_by_id.get_mut(job_id) {
            if !matches!(record.response.status, PropertySummaryJobStatus::Pending) {
                return;
            }
            record.response.status = PropertySummaryJobStatus::Ready;
            record.response.summary_paragraph = Some(summary_paragraph);
            record.response.evidence_refs = evidence_refs;
            record.response.model = model;
            record.response.generated_at = Some(Utc::now());
            record.response.error_message = None;
            record.expires_at = Instant::now() + SUMMARY_JOB_TTL;
        }
    }

    pub fn complete_error(&mut self, job_id: &str, message: impl Into<String>) {
        if let Some(record) = self.jobs_by_id.get_mut(job_id) {
            if !matches!(record.response.status, PropertySummaryJobStatus::Pending) {
                return;
            }
            record.response.status = PropertySummaryJobStatus::Error;
            record.response.error_message = Some(message.into());
            record.response.generated_at = Some(Utc::now());
            record.expires_at = Instant::now() + SUMMARY_JOB_TTL;
        }
    }

    #[cfg(test)]
    fn expire_all_for_tests(&mut self) {
        self.expire_old_jobs(Instant::now() + SUMMARY_JOB_TTL + Duration::from_secs(1));
    }

    fn expire_old_jobs(&mut self, now: Instant) {
        let expired = self
            .jobs_by_id
            .iter()
            .filter_map(|(job_id, record)| (record.expires_at <= now).then_some(job_id.clone()))
            .collect::<Vec<_>>();
        for job_id in expired {
            if let Some(record) = self.jobs_by_id.remove(&job_id) {
                self.jobs_by_key.remove(&record.key);
            }
        }
    }
}

#[async_trait]
pub trait SummaryModel: Send + Sync {
    fn model_id(&self) -> &'static str;
    async fn summarize(&self, context: &SummaryContext) -> Result<String, String>;
}

#[derive(Debug, Default)]
pub struct SummaryLmUnavailableModel;

#[async_trait]
impl SummaryModel for SummaryLmUnavailableModel {
    fn model_id(&self) -> &'static str {
        DEFAULT_MODEL_ID
    }

    async fn summarize(&self, _context: &SummaryContext) -> Result<String, String> {
        Err(
            "property summary generation is disabled until a local summary model is selected"
                .to_string(),
        )
    }
}

pub fn default_summary_model() -> Arc<dyn SummaryModel> {
    static MODEL: OnceLock<Arc<dyn SummaryModel>> = OnceLock::new();
    MODEL
        .get_or_init(|| Arc::new(SummaryLmUnavailableModel))
        .clone()
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SummaryContext {
    property_id: String,
    property_title: Option<String>,
    evidence: Vec<RankedSummaryEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SummaryEvidenceCategory {
    Core,
    Nearby,
    Market,
    Community,
}

#[derive(Debug, Clone)]
struct RankedSummaryEvidence {
    category: SummaryEvidenceCategory,
    label: String,
    source_type: String,
    source_url: Option<String>,
    entity_id: String,
    learned_at: DateTime<Utc>,
    confidence: f32,
    rank: u32,
}

pub async fn build_property_summary(
    property_id: &str,
    properties: &[Property],
    bundle: &LoadedServingBundle,
    model: &dyn SummaryModel,
) -> Result<(String, Vec<SummaryEvidenceRef>, String), String> {
    let embedder = summary_embedder().await?;
    eprintln!("Loaded summary embedder {}", embedder.model_id());
    let context = build_summary_context(property_id, properties, bundle, embedder.as_ref())?;
    eprintln!(
        "Built summary context for {property_id} with {} evidence items",
        context.evidence.len()
    );
    let paragraph = model.summarize(&context).await?;
    let receipts = context
        .evidence
        .iter()
        .take(MAX_EVIDENCE_REFS)
        .map(summary_evidence_ref)
        .collect::<Vec<_>>();
    Ok((paragraph, receipts, model.model_id().to_string()))
}

async fn summary_embedder() -> Result<Arc<dyn SemanticEmbedder>, String> {
    tokio::task::spawn_blocking(default_summary_embedder)
        .await
        .map_err(|err| format!("summary embedder task failed: {err}"))?
}

fn default_summary_embedder() -> Result<Arc<dyn SemanticEmbedder>, String> {
    static EMBEDDER: OnceLock<Result<Arc<dyn SemanticEmbedder>, String>> = OnceLock::new();
    EMBEDDER
        .get_or_init(|| Ok(Arc::new(HashSemanticEmbedder::default()) as Arc<dyn SemanticEmbedder>))
        .clone()
}

pub fn property_summary_anchor(
    property_id: &str,
    properties: &[Property],
    bundle: &LoadedServingBundle,
) -> String {
    properties
        .iter()
        .find(|property| property.id == property_id)
        .map(|property| society_node_id(&property.society_id))
        .or_else(|| society_anchor_for_property_slug(property_id, bundle))
        .unwrap_or_else(|| {
            if property_id.starts_with("property:") {
                property_id.to_string()
            } else {
                format!("property:{property_id}")
            }
        })
}

fn build_summary_context(
    property_id: &str,
    properties: &[Property],
    bundle: &LoadedServingBundle,
    embedder: &dyn SemanticEmbedder,
) -> Result<SummaryContext, String> {
    let property_title = properties
        .iter()
        .find(|property| property.id == property_id)
        .map(|property| property.title.clone());
    let anchor = property_summary_anchor(property_id, properties, bundle);
    let mut evidence = Vec::new();

    for entity_id in summary_entity_candidates(&anchor, bundle) {
        let Some(rows) = bundle.fact_index.entity(&entity_id) else {
            continue;
        };
        for fact in &rows.facts {
            if let Some(evidence_item) = summarize_fact(fact) {
                push_ranked_evidence(&mut evidence, evidence_item);
            }
        }
    }

    evidence = select_summary_evidence(evidence, embedder, property_title.as_deref());

    if evidence.is_empty() {
        return Err("No summary-ready facts found in the serving bundle.".to_string());
    }

    Ok(SummaryContext {
        property_id: property_id.to_string(),
        property_title,
        evidence,
    })
}

fn summary_entity_candidates(anchor_entity_id: &str, bundle: &LoadedServingBundle) -> Vec<String> {
    let mut candidates = Vec::new();
    push_unique_entity(&mut candidates, anchor_entity_id.to_string());
    if anchor_entity_id.starts_with("property:") {
        if let Some(society_id) = linked_society_anchor(anchor_entity_id, bundle) {
            push_unique_entity(&mut candidates, society_id);
        }
    }
    for edge in bundle.graph_index.walk_out(
        anchor_entity_id,
        &["near_place", "nearby_place", "has_nearby_place"],
        1,
    ) {
        push_unique_entity(&mut candidates, edge.to_entity_id.clone());
    }
    candidates
}

fn linked_society_anchor(property_anchor: &str, bundle: &LoadedServingBundle) -> Option<String> {
    bundle
        .graph_index
        .walk_out(property_anchor, &["in_society"], 1)
        .first()
        .map(|step| step.to_entity_id.clone())
}

fn society_anchor_for_property_slug(
    property_slug: &str,
    bundle: &LoadedServingBundle,
) -> Option<String> {
    let property_anchor = if property_slug.starts_with("property:") {
        property_slug.to_string()
    } else {
        format!("property:{property_slug}")
    };
    linked_society_anchor(&property_anchor, bundle).or_else(|| {
        let society_guess = society_node_id(property_slug.trim_start_matches("discovered-"));
        entity_exists_or_has_facts(bundle, &society_guess).then_some(society_guess)
    })
}

fn entity_exists_or_has_facts(bundle: &LoadedServingBundle, entity_id: &str) -> bool {
    bundle
        .entities
        .iter()
        .any(|entity| entity.entity_id == entity_id)
        || bundle.fact_index.entity(entity_id).is_some()
}

fn summarize_fact(fact: &ServingFactRecord) -> Option<RankedSummaryEvidence> {
    if is_google_review_metric(&fact.fact_key) && is_nearby_place_google_fact(fact) {
        return None;
    }
    let (category, rank) = summary_category_and_rank(&fact.fact_key)?;
    let value = fact_value_text(&fact.value)?;
    let label = fact_label(&fact.fact_key, &value)?;
    Some(RankedSummaryEvidence {
        category,
        label,
        source_type: fact.source_type.clone(),
        source_url: fact.source_url.clone(),
        entity_id: fact.entity_id.clone(),
        learned_at: fact.learned_at,
        confidence: fact.confidence,
        rank,
    })
}

fn is_google_review_metric(fact_key: &str) -> bool {
    matches!(fact_key, "google_rating" | "google_review_count")
}

fn is_nearby_place_google_fact(fact: &ServingFactRecord) -> bool {
    fact.skill_id.as_deref() == Some(GOOGLE_NEARBY_PLACES_SKILL_ID)
}

fn select_summary_evidence(
    mut evidence: Vec<RankedSummaryEvidence>,
    embedder: &dyn SemanticEmbedder,
    property_title: Option<&str>,
) -> Vec<RankedSummaryEvidence> {
    rerank_summary_evidence(&mut evidence, embedder, property_title);
    evidence.sort_by(compare_summary_evidence);
    let mut selected = Vec::new();

    push_category_evidence(&mut selected, &evidence, SummaryEvidenceCategory::Core, 4);
    push_nearby_evidence(&mut selected, &evidence, 5);
    push_category_evidence(&mut selected, &evidence, SummaryEvidenceCategory::Market, 3);
    push_category_evidence(
        &mut selected,
        &evidence,
        SummaryEvidenceCategory::Community,
        2,
    );

    for item in evidence {
        if selected.len() >= MAX_EVIDENCE_REFS {
            break;
        }
        push_ranked_evidence(&mut selected, item);
    }
    selected.truncate(MAX_EVIDENCE_REFS);
    selected
}

fn rerank_summary_evidence(
    evidence: &mut [RankedSummaryEvidence],
    embedder: &dyn SemanticEmbedder,
    property_title: Option<&str>,
) {
    if evidence.is_empty() {
        return;
    }
    let query = format!(
        "buyer property summary for {} covering location, nearby schools, metro, hospitals, pricing, resident review evidence, and cautions",
        property_title.unwrap_or("this home")
    );
    let mut texts = Vec::with_capacity(evidence.len() + 1);
    texts.push(query);
    texts.extend(evidence.iter().map(summary_embedding_text));
    let vectors = embedder.embed_batch(&texts);
    if vectors.len() != evidence.len() + 1 {
        return;
    }
    let query_vector = &vectors[0];
    for (item, vector) in evidence.iter_mut().zip(vectors.iter().skip(1)) {
        let similarity = cosine_similarity(query_vector, vector).max(0.0);
        let boost = (similarity * 10.0).round() as u32;
        item.rank = item.rank.saturating_sub(boost.min(item.rank));
    }
}

fn summary_embedding_text(evidence: &RankedSummaryEvidence) -> String {
    format!(
        "{} {:?} {} confidence {:.2}",
        evidence.label, evidence.category, evidence.source_type, evidence.confidence
    )
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f64 {
    let mut dot = 0.0f64;
    let mut left_norm = 0.0f64;
    let mut right_norm = 0.0f64;
    for (left, right) in left.iter().zip(right.iter()) {
        let left = f64::from(*left);
        let right = f64::from(*right);
        dot += left * right;
        left_norm += left * left;
        right_norm += right * right;
    }
    if left_norm <= f64::EPSILON || right_norm <= f64::EPSILON {
        return 0.0;
    }
    dot / (left_norm.sqrt() * right_norm.sqrt())
}

fn push_category_evidence(
    selected: &mut Vec<RankedSummaryEvidence>,
    evidence: &[RankedSummaryEvidence],
    category: SummaryEvidenceCategory,
    limit: usize,
) {
    let mut added = 0usize;
    for item in evidence
        .iter()
        .filter(|item| item.category == category)
        .cloned()
    {
        if added >= limit || selected.len() >= MAX_EVIDENCE_REFS {
            break;
        }
        let before = selected.len();
        push_ranked_evidence(selected, item);
        if selected.len() > before {
            added += 1;
        }
    }
}

fn push_nearby_evidence(
    selected: &mut Vec<RankedSummaryEvidence>,
    evidence: &[RankedSummaryEvidence],
    limit: usize,
) {
    let nearby = evidence
        .iter()
        .filter(|item| item.category == SummaryEvidenceCategory::Nearby)
        .cloned()
        .collect::<Vec<_>>();
    for group in [
        NearbyEvidenceGroup::School,
        NearbyEvidenceGroup::Metro,
        NearbyEvidenceGroup::Hospital,
        NearbyEvidenceGroup::TechPark,
        NearbyEvidenceGroup::Other,
    ] {
        if selected
            .iter()
            .filter(|item| item.category == SummaryEvidenceCategory::Nearby)
            .count()
            >= limit
            || selected.len() >= MAX_EVIDENCE_REFS
        {
            break;
        }
        if let Some(item) = nearby
            .iter()
            .filter(|item| nearby_group(&item.label) == group)
            .min_by(|left, right| compare_nearby_evidence(left, right))
            .cloned()
        {
            push_ranked_evidence(selected, item);
        }
    }
}

fn compare_summary_evidence(
    left: &RankedSummaryEvidence,
    right: &RankedSummaryEvidence,
) -> std::cmp::Ordering {
    left.rank
        .cmp(&right.rank)
        .then_with(|| {
            nearby_group(&left.label)
                .rank()
                .cmp(&nearby_group(&right.label).rank())
        })
        .then_with(|| compare_optional_f64(distance_km(&left.label), distance_km(&right.label)))
        .then_with(|| right.confidence.total_cmp(&left.confidence))
        .then_with(|| right.learned_at.cmp(&left.learned_at))
}

fn compare_nearby_evidence(
    left: &RankedSummaryEvidence,
    right: &RankedSummaryEvidence,
) -> std::cmp::Ordering {
    compare_optional_f64(distance_km(&left.label), distance_km(&right.label))
        .then_with(|| right.confidence.total_cmp(&left.confidence))
        .then_with(|| right.learned_at.cmp(&left.learned_at))
}

fn compare_optional_f64(left: Option<f64>, right: Option<f64>) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.total_cmp(&right),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NearbyEvidenceGroup {
    School,
    Metro,
    Hospital,
    TechPark,
    Other,
}

impl NearbyEvidenceGroup {
    fn rank(self) -> u8 {
        match self {
            Self::School => 0,
            Self::Metro => 1,
            Self::Hospital => 2,
            Self::TechPark => 3,
            Self::Other => 4,
        }
    }
}

fn nearby_group(label: &str) -> NearbyEvidenceGroup {
    let lower = label.to_ascii_lowercase();
    if lower.starts_with("schools nearby") {
        NearbyEvidenceGroup::School
    } else if lower.starts_with("metro nearby") {
        NearbyEvidenceGroup::Metro
    } else if lower.starts_with("hospitals nearby") {
        NearbyEvidenceGroup::Hospital
    } else if lower.starts_with("tech parks nearby") {
        NearbyEvidenceGroup::TechPark
    } else {
        NearbyEvidenceGroup::Other
    }
}

fn distance_km(label: &str) -> Option<f64> {
    let marker = " km";
    let marker_index = label.find(marker)?;
    let before = &label[..marker_index];
    let token = before
        .rsplit(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .find(|part| !part.is_empty())?;
    token.parse::<f64>().ok().filter(|value| value.is_finite())
}

fn summary_category_and_rank(fact_key: &str) -> Option<(SummaryEvidenceCategory, u32)> {
    match fact_key {
        "title"
        | "area"
        | "city"
        | "builder_name"
        | "project_status"
        | "possession_status"
        | "rera_registration_number"
        | "rera_total_land_area_acres"
        | "rera_total_land_area_sqm"
        | "google_rating"
        | "google_review_count" => Some((SummaryEvidenceCategory::Core, 10)),
        "nearby_schools"
        | "nearby_hospitals"
        | "nearby_metro_stations"
        | "nearby_tech_parks"
        | "nearest_metro_distance_km"
        | "metro_distance_km" => Some((SummaryEvidenceCategory::Nearby, 20)),
        "pricing_2bhk"
        | "pricing_3bhk"
        | "pricing_4bhk"
        | "price_per_sqft"
        | "external_listing_count"
        | "listing_inventory_status" => Some((SummaryEvidenceCategory::Market, 30)),
        "community_review_summary" | "reddit_positive_themes" | "reddit_concern_themes" => {
            Some((SummaryEvidenceCategory::Community, 40))
        }
        _ => None,
    }
}

fn fact_label(fact_key: &str, value: &str) -> Option<String> {
    let clean = value.trim();
    if clean.is_empty() || looks_internal(clean) {
        return None;
    }
    let label = match fact_key {
        "title" => clean.to_string(),
        "area" => format!("area: {clean}"),
        "city" => format!("city: {clean}"),
        "builder_name" => format!("builder: {clean}"),
        "project_status" | "possession_status" => format!("status: {clean}"),
        "rera_registration_number" => "RERA registration".to_string(),
        "rera_total_land_area_acres" => format!("RERA land area: {clean} acres"),
        "rera_total_land_area_sqm" => format!("RERA land area: {clean} sqm"),
        "google_rating" => format!("Google rating {clean}"),
        "google_review_count" => format!("{clean} Google reviews"),
        "nearby_schools" => format!("schools nearby: {clean}"),
        "nearby_hospitals" => format!("hospitals nearby: {clean}"),
        "nearby_metro_stations" => format!("metro nearby: {clean}"),
        "nearby_tech_parks" => format!("tech parks nearby: {clean}"),
        "nearest_metro_distance_km" | "metro_distance_km" => format!("metro distance: {clean} km"),
        "pricing_2bhk" => format!("2 BHK pricing evidence: {clean}"),
        "pricing_3bhk" => format!("3 BHK pricing evidence: {clean}"),
        "pricing_4bhk" => format!("4 BHK pricing evidence: {clean}"),
        "price_per_sqft" => format!("price per sqft: {clean}"),
        "external_listing_count" => format!("{clean} external listings"),
        "listing_inventory_status" => format!("inventory status: {clean}"),
        "community_review_summary" => clean.to_string(),
        "reddit_positive_themes" => format!("resident positives: {clean}"),
        "reddit_concern_themes" => format!("resident concerns: {clean}"),
        _ => return None,
    };
    Some(label)
}

fn fact_value_text(value: &FactValue) -> Option<String> {
    match value {
        FactValue::Text(text) => Some(text.trim().to_string()),
        FactValue::Numeric(value) if value.is_finite() => Some(format_numeric(*value)),
        FactValue::Bool(value) => Some(value.to_string()),
        FactValue::Tags(values) => {
            let clean = values
                .iter()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .take(4)
                .collect::<Vec<_>>();
            (!clean.is_empty()).then(|| clean.join(", "))
        }
        FactValue::Score { value, explanation } if value.is_finite() => {
            let explanation = explanation.trim();
            if explanation.is_empty() {
                Some(format_numeric(*value))
            } else {
                Some(format!("{} ({})", format_numeric(*value), explanation))
            }
        }
        _ => None,
    }
}

fn format_numeric(value: f64) -> String {
    if (value.fract()).abs() < f64::EPSILON {
        format!("{}", value as i64)
    } else {
        format!("{value:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn summary_evidence_ref(evidence: &RankedSummaryEvidence) -> SummaryEvidenceRef {
    SummaryEvidenceRef {
        entity_id: evidence.entity_id.clone(),
        label: evidence.label.clone(),
        source_type: evidence.source_type.clone(),
        source_url: evidence.source_url.clone(),
        learned_at: evidence.learned_at,
        confidence: evidence.confidence,
    }
}

fn push_ranked_evidence(evidence: &mut Vec<RankedSummaryEvidence>, item: RankedSummaryEvidence) {
    if evidence
        .iter()
        .any(|existing| existing.entity_id == item.entity_id && existing.label == item.label)
    {
        return;
    }
    evidence.push(item);
}

fn push_unique_entity(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn normalized_summary_style(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_SUMMARY_STYLE)
        .to_ascii_lowercase()
}

fn looks_internal(value: &str) -> bool {
    value.contains("generated_context_summary") || value.contains("data/lake")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_jobs_dedupe_by_bundle_property_and_style() {
        let mut store = PropertySummaryJobStore::default();
        let (first, first_created) = store.create_or_get("p1", "bundle-a", Some("buyer_brief"));
        let (second, second_created) = store.create_or_get("p1", "bundle-a", Some("buyer_brief"));
        let (third, third_created) = store.create_or_get("p1", "bundle-a", Some("compare"));

        assert_eq!(first.job_id, second.job_id);
        assert_ne!(first.job_id, third.job_id);
        assert!(first_created);
        assert!(!second_created);
        assert!(third_created);
    }

    #[test]
    fn pending_ready_error_and_ttl_states_are_tracked() {
        let mut store = PropertySummaryJobStore::default();
        let (pending, _) = store.create_or_get("p1", "bundle-a", None);
        assert_eq!(pending.status, PropertySummaryJobStatus::Pending);

        store.complete_ready(
            &pending.job_id,
            "RERA facts are available.".to_string(),
            Vec::new(),
            "test-model".to_string(),
        );
        let ready = store.get(&pending.job_id).unwrap();
        assert_eq!(ready.status, PropertySummaryJobStatus::Ready);
        assert_eq!(
            ready.summary_paragraph.as_deref(),
            Some("RERA facts are available.")
        );

        let (error, _) = store.create_or_get("p2", "bundle-a", None);
        store.complete_error(&error.job_id, "thin evidence");
        let failed = store.get(&error.job_id).unwrap();
        assert_eq!(failed.status, PropertySummaryJobStatus::Error);
        assert_eq!(failed.error_message.as_deref(), Some("thin evidence"));

        store.expire_all_for_tests();
        assert!(store.get(&pending.job_id).is_none());
        assert!(store.get(&error.job_id).is_none());
    }

    #[test]
    fn summary_labels_do_not_expose_raw_generated_summary_keys() {
        assert!(fact_label("generated_context_summary", "hello").is_none());
        assert!(fact_label("area", "Whitefield")
            .unwrap()
            .contains("Whitefield"));
    }

    #[test]
    fn summary_ignores_nearby_place_google_review_metrics() {
        let nearby_rating = summary_fact(
            "google_rating",
            FactValue::Numeric(4.9),
            Some(GOOGLE_NEARBY_PLACES_SKILL_ID),
        );
        let society_rating = summary_fact(
            "google_rating",
            FactValue::Numeric(3.6),
            Some("fetch_google_review_links"),
        );

        assert!(summarize_fact(&nearby_rating).is_none());
        assert_eq!(
            summarize_fact(&society_rating).map(|evidence| evidence.label),
            Some("Google rating 3.6".to_string())
        );
    }

    #[test]
    fn summary_evidence_balances_nearby_categories_by_distance() {
        let embedder = crate::search::HashSemanticEmbedder::default();
        let learned_at = Utc::now();
        let evidence = vec![
            nearby("hospitals nearby: Aster Hospital (2.9 km)", learned_at),
            nearby("hospitals nearby: Manipal Hospital (3.3 km)", learned_at),
            nearby("schools nearby: ORCHIDS (0.9 km)", learned_at),
            nearby(
                "metro nearby: Garudacharapalya Metro station (5.1 km)",
                learned_at,
            ),
            nearby("metro nearby: Whitefield (Kadugodi) (2.2 km)", learned_at),
            nearby("tech parks nearby: ITPB (3.2 km)", learned_at),
        ];

        let selected = select_summary_evidence(evidence, &embedder, Some("Godrej Splendour"));
        let labels = selected
            .iter()
            .filter(|item| item.category == SummaryEvidenceCategory::Nearby)
            .take(4)
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>();

        assert!(labels.contains(&"schools nearby: ORCHIDS (0.9 km)"));
        assert!(labels.contains(&"metro nearby: Whitefield (Kadugodi) (2.2 km)"));
        assert!(labels.contains(&"hospitals nearby: Aster Hospital (2.9 km)"));
        assert!(labels.contains(&"tech parks nearby: ITPB (3.2 km)"));
        assert!(!labels.contains(&"metro nearby: Garudacharapalya Metro station (5.1 km)"));
    }

    fn nearby(label: &str, learned_at: DateTime<Utc>) -> RankedSummaryEvidence {
        RankedSummaryEvidence {
            category: SummaryEvidenceCategory::Nearby,
            label: label.to_string(),
            source_type: "Google".to_string(),
            source_url: None,
            entity_id: "society:godrej-splendour".to_string(),
            learned_at,
            confidence: 0.82,
            rank: 20,
        }
    }

    fn summary_fact(fact_key: &str, value: FactValue, skill_id: Option<&str>) -> ServingFactRecord {
        ServingFactRecord {
            entity_id: "society:godrej-splendour".to_string(),
            fact_key: fact_key.to_string(),
            value_type: "test".to_string(),
            value_text: None,
            value,
            confidence: 0.82,
            source_type: "Google".to_string(),
            source_url: None,
            model: None,
            skill_id: skill_id.map(str::to_string),
            learned_at: Utc::now(),
        }
    }
}
