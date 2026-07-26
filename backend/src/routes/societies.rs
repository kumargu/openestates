use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::knowledge::node::RootSource;
use crate::knowledge::KnowledgeGraph;
use crate::models::{AreaProfile, Society};
use crate::routes::enrichment::society_node_id;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Serialize, Clone)]
pub struct SocietySearchResult {
    pub slug: String,
    pub name: String,
    pub builder: String,
    pub year_built: Option<u32>,
    pub total_units: Option<u32>,
    pub unit_types: Option<String>,
    pub price_range: Option<String>,
    pub summary: String,
    pub overall_score: u32,
    pub rank: u32,
    pub best_for_label: String,
    pub life_fit_reason: String,
    pub dimension_scores: HashMap<String, u32>,
    pub confidence: String,
    pub evidence: SocietyEvidence,
    pub photos: Vec<String>,
    pub signals: Vec<String>,
    pub cautions: Vec<String>,
    pub resident_quote: Option<String>,
    pub why_above_next: String,
}

#[derive(Serialize, Clone)]
pub struct SocietyEvidence {
    pub reddit_threads: u32,
    pub society_threads: u32,
    pub area_threads: u32,
    pub has_seed_data: bool,
    pub reddit_confidence: String,
}

#[derive(Serialize)]
pub struct QueryInterpreted {
    pub original: String,
    pub area: String,
    pub city: String,
    pub intent: String,
    pub weights_applied: HashMap<String, f32>,
}

#[derive(Serialize)]
pub struct SocietySearchResponse {
    pub query_interpreted: QueryInterpreted,
    pub results: Vec<SocietySearchResult>,
    pub result_count: usize,
    pub area_context: Option<SocietyAreaContext>,
    pub enrichment_status: EnrichmentStatus,
}

#[derive(Serialize)]
pub struct SocietyAreaContext {
    pub name: String,
    pub city: String,
    pub median_price_per_sqft: u64,
    pub trend_direction: String,
    pub trend_summary: String,
    pub metro_access_summary: String,
    pub traffic_summary: String,
    pub livability_summary: String,
    pub infrastructure_tags: Vec<String>,
    pub externality_tags: Vec<String>,
    pub community_notes: String,
}

#[derive(Serialize)]
pub struct EnrichmentStatus {
    pub societies_discovered: usize,
    pub societies_scored: usize,
    pub reddit_enriched: usize,
    pub seed_matched: usize,
    pub photos_available: usize,
    pub scored_at: String,
}

/// GET /api/societies/search?q=... — local ranked society search over AppState.
pub async fn search_societies(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchQuery>,
) -> Json<SocietySearchResponse> {
    let query = params.q.unwrap_or_default();
    let graph = state.knowledge.read().await;
    let societies = state.societies.read().await;
    let areas = state.areas.read().await;
    let results = rank_societies(&societies, &areas, &query, &graph);
    let interpreted_area = infer_area(&query, &areas)
        .or_else(|| results.first().map(|r| r_area(r, &societies)))
        .unwrap_or_else(|| "Bengaluru".to_string());
    let area_context = area_context_for(&interpreted_area, &areas);
    let enrichment_status = enrichment_status(&societies, &results, &graph);

    Json(SocietySearchResponse {
        query_interpreted: QueryInterpreted {
            original: query.clone(),
            area: interpreted_area,
            city: "Bengaluru".to_string(),
            intent: if query.trim().is_empty() {
                "rank transparent societies".to_string()
            } else {
                query
            },
            weights_applied: default_weights(),
        },
        result_count: results.len(),
        results,
        area_context,
        enrichment_status,
    })
}

/// GET /api/societies/:slug — local society detail using the same card contract.
pub async fn get_society(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> Result<Json<SocietySearchResult>, (StatusCode, Json<ErrorResponse>)> {
    let graph = state.knowledge.read().await;
    let societies = state.societies.read().await;
    let areas = state.areas.read().await;
    let result = rank_societies(&societies, &areas, "", &graph)
        .into_iter()
        .find(|item| item.slug == slug)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "society_not_found".to_string(),
                }),
            )
        })?;

    Ok(Json(result))
}

fn rank_societies(
    societies: &[Society],
    areas: &[AreaProfile],
    query: &str,
    graph: &KnowledgeGraph,
) -> Vec<SocietySearchResult> {
    let query_terms = normalized_terms(query);
    let area = infer_area(query, areas);

    let mut scored: Vec<(f64, SocietySearchResult)> = societies
        .iter()
        .filter_map(|society| {
            let haystack = format!(
                "{} {} {} {} {} {} {}",
                society.name,
                society.area,
                society.builder_name,
                society.summary,
                society.review_summary,
                society.common_positives.join(" "),
                society.common_complaints.join(" ")
            )
            .to_lowercase();

            if let Some(ref area_filter) = area {
                if !society.area.eq_ignore_ascii_case(area_filter)
                    && !society
                        .area
                        .to_lowercase()
                        .contains(&area_filter.to_lowercase())
                    && !area_filter
                        .to_lowercase()
                        .contains(&society.area.to_lowercase())
                {
                    return None;
                }
            }

            let mut score = 0.0;
            for term in &query_terms {
                if haystack.contains(term) {
                    score += 5.0;
                }
            }

            let node_id = society_node_id(&society.id);
            let root_source = graph
                .get_node(&node_id)
                .and_then(|node| node.root_source)
                .unwrap_or(RootSource::Legacy);
            let rera_boost = if root_source == RootSource::Rera {
                25.0
            } else {
                0.0
            };
            let enrichment_boost =
                (society.common_positives.len() + society.common_complaints.len()).min(8) as f64
                    * 2.0;
            let unit_boost = if society.total_units > 0 { 4.0 } else { 0.0 };
            let base_score = 50.0 + rera_boost + enrichment_boost + unit_boost;
            score += base_score;

            if query_terms.is_empty() || score > base_score || area.is_some() {
                Some((score, build_result(society, root_source, score)))
            } else {
                None
            }
        })
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(40);

    scored
        .into_iter()
        .enumerate()
        .map(|(idx, (_, mut result))| {
            result.rank = (idx + 1) as u32;
            result.why_above_next = if idx == 0 {
                "Best local evidence fit for this query.".to_string()
            } else {
                "Ranked by local RERA trust, area match, and enrichment coverage.".to_string()
            };
            result
        })
        .collect()
}

fn build_result(society: &Society, root_source: RootSource, score: f64) -> SocietySearchResult {
    let overall_score = score.round().clamp(0.0, 100.0) as u32;
    let confidence = if root_source == RootSource::Rera && overall_score >= 80 {
        "high"
    } else if overall_score >= 65 {
        "moderate"
    } else {
        "low"
    }
    .to_string();

    let mut dimension_scores = HashMap::new();
    dimension_scores.insert(
        "builder_trust".to_string(),
        if root_source == RootSource::Rera {
            86
        } else {
            62
        },
    );
    dimension_scores.insert(
        "maintenance_quality".to_string(),
        sentiment_score(&society.maintenance_sentiment),
    );
    dimension_scores.insert(
        "family_friendly".to_string(),
        tag_score(
            &society.common_positives,
            &["family", "school", "community", "children"],
        ),
    );
    dimension_scores.insert(
        "calm_environment".to_string(),
        caution_inverse_score(
            &society.common_complaints,
            &["noise", "traffic", "construction"],
        ),
    );
    dimension_scores.insert("value".to_string(), 70);

    let mut signals = society.common_positives.clone();
    if root_source == RootSource::Rera {
        signals.insert(0, "RERA rooted".to_string());
    }
    if signals.is_empty() {
        signals.push("Local graph profile".to_string());
    }

    let mut cautions = society.common_complaints.clone();
    if cautions.is_empty() && root_source != RootSource::Rera {
        cautions.push("RERA match pending".to_string());
    }

    SocietySearchResult {
        slug: society.id.clone(),
        name: society.name.clone(),
        builder: society.builder_name.clone(),
        year_built: nonzero_u32(society.year_built),
        total_units: nonzero_u32(society.total_units),
        unit_types: None,
        price_range: None,
        summary: if society.summary.is_empty() {
            format!(
                "{} in {} by {}.",
                society.name, society.area, society.builder_name
            )
        } else {
            society.summary.clone()
        },
        overall_score,
        rank: 0,
        best_for_label: best_for_label(society, root_source),
        life_fit_reason: life_fit_reason(society, root_source),
        dimension_scores,
        confidence,
        evidence: SocietyEvidence {
            reddit_threads: if society.review_summary.is_empty() {
                0
            } else {
                1
            },
            society_threads: if society.review_summary.is_empty() {
                0
            } else {
                1
            },
            area_threads: 0,
            has_seed_data: root_source == RootSource::Rera,
            reddit_confidence: if society.review_summary.is_empty() {
                "low"
            } else {
                "moderate"
            }
            .to_string(),
        },
        photos: Vec::new(),
        signals,
        cautions,
        resident_quote: if society.review_summary.is_empty() {
            None
        } else {
            Some(society.review_summary.clone())
        },
        why_above_next: String::new(),
    }
}

fn area_context_for(area_name: &str, areas: &[AreaProfile]) -> Option<SocietyAreaContext> {
    let area = areas.iter().find(|area| {
        area.name.eq_ignore_ascii_case(area_name)
            || area.name.to_lowercase().contains(&area_name.to_lowercase())
            || area_name.to_lowercase().contains(&area.name.to_lowercase())
    })?;

    Some(SocietyAreaContext {
        name: area.name.clone(),
        city: area.city.clone(),
        median_price_per_sqft: area.median_price_per_sqft,
        trend_direction: area.trend_direction.clone(),
        trend_summary: area.trend_summary.clone(),
        metro_access_summary: area.metro_access_summary.clone(),
        traffic_summary: area.traffic_summary.clone(),
        livability_summary: area.livability_summary.clone(),
        infrastructure_tags: area.infrastructure_tags.clone(),
        externality_tags: area.externality_tags.clone(),
        community_notes: area.community_notes.clone(),
    })
}

fn enrichment_status(
    societies: &[Society],
    results: &[SocietySearchResult],
    graph: &KnowledgeGraph,
) -> EnrichmentStatus {
    let seed_matched = societies
        .iter()
        .filter(|society| {
            graph
                .get_node(&society_node_id(&society.id))
                .and_then(|node| node.root_source)
                == Some(RootSource::Rera)
        })
        .count();

    EnrichmentStatus {
        societies_discovered: societies.len(),
        societies_scored: results.len(),
        reddit_enriched: societies
            .iter()
            .filter(|society| !society.review_summary.is_empty())
            .count(),
        seed_matched,
        photos_available: 0,
        scored_at: Utc::now().to_rfc3339(),
    }
}

fn infer_area(query: &str, areas: &[AreaProfile]) -> Option<String> {
    let query_lower = query.to_lowercase();
    areas
        .iter()
        .filter(|area| !area.name.is_empty())
        .find(|area| query_lower.contains(&area.name.to_lowercase()))
        .map(|area| area.name.clone())
}

fn normalized_terms(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|term| term.len() >= 3)
        .map(|term| term.to_string())
        .collect()
}

fn default_weights() -> HashMap<String, f32> {
    HashMap::from([
        ("builder_trust".to_string(), 0.25),
        ("maintenance_quality".to_string(), 0.20),
        ("family_friendly".to_string(), 0.20),
        ("calm_environment".to_string(), 0.15),
        ("value".to_string(), 0.20),
    ])
}

fn sentiment_score(value: &str) -> u32 {
    let lower = value.to_lowercase();
    if lower.contains("positive") || lower.contains("good") || lower.contains("high") {
        82
    } else if lower.contains("negative") || lower.contains("poor") {
        45
    } else {
        68
    }
}

fn tag_score(tags: &[String], needles: &[&str]) -> u32 {
    if tags.iter().any(|tag| {
        let lower = tag.to_lowercase();
        needles.iter().any(|needle| lower.contains(needle))
    }) {
        84
    } else {
        66
    }
}

fn caution_inverse_score(tags: &[String], needles: &[&str]) -> u32 {
    if tags.iter().any(|tag| {
        let lower = tag.to_lowercase();
        needles.iter().any(|needle| lower.contains(needle))
    }) {
        52
    } else {
        78
    }
}

fn best_for_label(society: &Society, root_source: RootSource) -> String {
    if society
        .common_positives
        .iter()
        .any(|tag| tag.to_lowercase().contains("family"))
    {
        "Family shortlist".to_string()
    } else if root_source == RootSource::Rera {
        "Trust-first shortlist".to_string()
    } else {
        "Needs verification".to_string()
    }
}

fn life_fit_reason(society: &Society, root_source: RootSource) -> String {
    let trust = if root_source == RootSource::Rera {
        "RERA-rooted"
    } else {
        "discovered"
    };
    if !society.review_summary.is_empty() {
        format!(
            "{} profile with resident/context notes: {}",
            trust, society.review_summary
        )
    } else {
        format!(
            "{} profile in {} with local graph evidence available.",
            trust, society.area
        )
    }
}

fn nonzero_u32(value: u32) -> Option<u32> {
    if value == 0 {
        None
    } else {
        Some(value)
    }
}

fn r_area(result: &SocietySearchResult, societies: &[Society]) -> String {
    societies
        .iter()
        .find(|society| society.id == result.slug)
        .map(|society| society.area.clone())
        .unwrap_or_else(|| "Bengaluru".to_string())
}
