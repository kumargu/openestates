use crate::knowledge::KnowledgeGraph;
use crate::models::{Property, Society};
use crate::routes::enrichment::enrich_property_card;
use crate::routes::search::graph_preference_score_detailed;

use super::intent::SearchIntent;
use super::{MatchExplanation, MatchReason, PreferenceCoverage, SearchResultCard};

/// Simple text-matching search engine.
///
/// Designed to be swappable with a vector search backend later — the interface
/// (query in, scored results out) stays the same.
pub struct TextSearch;

impl TextSearch {
    /// Intent-based search: filters by hard constraints, scores by relevance,
    /// and returns full PropertyCard data with match info.
    ///
    /// When `graph` is provided, preference scoring uses the graph's self-describing
    /// `answers_preferences` + `scoring_hint` metadata. Falls back to hardcoded
    /// scoring when the graph doesn't have relevant facts.
    pub fn search_with_intent(
        properties: &[Property],
        society_names: &std::collections::HashMap<String, String>,
        societies: &[Society],
        query: &str,
        intent: &SearchIntent,
        graph: Option<&KnowledgeGraph>,
    ) -> Vec<SearchResultCard> {
        let query_lower = query.to_lowercase();
        let terms: Vec<&str> = query_lower.split_whitespace().collect();

        let mut results: Vec<SearchResultCard> = properties
            .iter()
            .filter_map(|p| {
                // Hard constraint: BHK
                if let Some(bhk) = intent.bhk {
                    if p.bhk != bhk {
                        return None;
                    }
                }

                // Hard constraint: budget
                if let Some(budget_max) = intent.budget_max {
                    if p.price > budget_max {
                        return None;
                    }
                }

                // Soft constraint: area — exact match keeps full score,
                // nearby/sub-area match gets a penalty instead of exclusion.
                let area_penalty: f64 = if let Some(ref area) = intent.area {
                    if p.area.eq_ignore_ascii_case(area) {
                        0.0 // exact match
                    } else if area_is_nearby(&p.area, area) {
                        -2.0 // nearby: include but rank lower
                    } else {
                        return None; // unrelated area: exclude
                    }
                } else {
                    0.0
                };

                let society_name = society_names
                    .get(&p.society_id)
                    .map(|s| s.as_str())
                    .unwrap_or("");

                // Base text score
                let (mut score, mut reasons) = if terms.is_empty() {
                    (1.0, Vec::new())
                } else {
                    score_property(p, society_name, &terms)
                };
                score += area_penalty;

                // Boost for preference alignment — collect structured reasons
                let mut match_reasons: Vec<MatchReason> = Vec::new();
                let mut pref_coverage: Vec<PreferenceCoverage> = Vec::new();
                let mut graph_count: usize = 0;
                let mut legacy_count: usize = 0;
                let mut total_facts_consulted: usize = 0;

                for pref in &intent.preferences {
                    // Graph-first: check if the society's facts declare scoring for this preference
                    if let Some(g) = graph {
                        if let Some((gs, detail)) = graph_preference_score_detailed(g, &p.society_id, pref) {
                            total_facts_consulted += 1;
                            score += gs;
                            reasons.push(format!("matches preference: {}", pref));

                            // Normalize score to 0-1 range (graph scores are 0-2)
                            let norm_score = (gs / 2.0).min(1.0);
                            match_reasons.push(MatchReason {
                                preference: pref.clone(),
                                fact_key: detail.fact_key.clone(),
                                display: detail.display,
                                score: norm_score,
                                confidence: detail.confidence,
                                source_type: detail.source_type,
                                scoring_method: "graph".into(),
                            });
                            pref_coverage.push(PreferenceCoverage {
                                preference: pref.clone(),
                                status: if norm_score > 0.5 { "matched" } else { "partial" }.into(),
                                fact_key: Some(detail.fact_key),
                            });
                            graph_count += 1;
                            continue;
                        }
                    }

                    // Legacy fallback
                    let legacy = legacy_preference_score(p, pref);
                    if legacy > 0.0 {
                        score += legacy;
                        reasons.push(format!("matches preference: {}", pref));

                        let norm_score = (legacy / 2.0).min(1.0);
                        let fact_key = legacy_fact_key_for_preference(pref);
                        match_reasons.push(MatchReason {
                            preference: pref.clone(),
                            fact_key: fact_key.clone(),
                            display: format_legacy_display(pref, p),
                            score: norm_score,
                            confidence: 0.5,
                            source_type: "Seed".into(),
                            scoring_method: "legacy".into(),
                        });
                        pref_coverage.push(PreferenceCoverage {
                            preference: pref.clone(),
                            status: if norm_score > 0.5 { "matched" } else { "partial" }.into(),
                            fact_key: Some(fact_key),
                        });
                        legacy_count += 1;
                    } else {
                        pref_coverage.push(PreferenceCoverage {
                            preference: pref.clone(),
                            status: "no_data".into(),
                            fact_key: None,
                        });
                    }
                }

                // Build explanation only when there are preferences
                let match_explanation = if !intent.preferences.is_empty() {
                    let total = graph_count + legacy_count;
                    let graph_pct = if total > 0 {
                        (graph_count as f32 / total as f32) * 100.0
                    } else {
                        0.0
                    };
                    Some(MatchExplanation {
                        reasons: match_reasons,
                        preference_coverage: pref_coverage,
                        graph_driven_pct: graph_pct,
                        total_facts_consulted,
                    })
                } else {
                    None
                };

                // If we had hard constraints that passed, give a base score even if
                // text matching scored zero.
                let has_constraints =
                    intent.area.is_some() || intent.bhk.is_some() || intent.budget_max.is_some();
                if score <= 0.0 && has_constraints {
                    score = 1.0;
                    reasons.push("matches search criteria".to_string());
                }

                if score <= 0.0 {
                    return None;
                }

                // Use shared enrichment — same PropertyCard as /api/properties.
                // graph is always Some in practice (search always has KG access).
                let card = if let Some(g) = graph {
                    enrich_property_card(p, societies, g)
                } else {
                    // Fallback without graph — build minimal card
                    crate::models::PropertyCard {
                        id: p.id.clone(),
                        title: p.title.clone(),
                        area: p.area.clone(),
                        price: p.price,
                        price_per_sqft: p.price_per_sqft,
                        bhk: p.bhk,
                        sqft: p.carpet_area_sqft,
                        society_name: society_name.to_string(),
                        builder_name: p.builder_name.clone(),
                        hero_image: p.hero_image.clone(),
                        transparency_tags: p.transparency_tags.iter().take(3).cloned().collect(),
                        description_summary: p.description_summary.clone(),
                        possession_status: p.possession_status.clone(),
                        metro_distance_mins: p.metro_distance_mins,
                        floor: p.floor,
                        total_floors: p.total_floors,
                        facing: p.facing.clone(),
                        google_rating: None,
                        google_review_count: None,
                    }
                };

                // Normalize score to 0.0–1.0 range (rough normalization)
                let max_possible = 15.0; // approximate ceiling
                let normalized = (score / max_possible).min(1.0);
                let match_label = match_label_from_score(normalized);
                let match_reason = build_match_reason(intent, &reasons);

                Some(SearchResultCard {
                    card,
                    match_score: (normalized * 100.0).round() / 100.0,
                    match_label,
                    match_reason,
                    match_explanation,
                    semantic_score: None,
                    society_score: None,
                    society_confidence: None,
                    concerns: Vec::new(),
                    unmatched_preferences: Vec::new(),
                    explanation_card: None,
                    active_seller_count: None,
                    bid_stats: None,
                })
            })
            .collect();

        results.sort_by(|a, b| {
            b.match_score
                .partial_cmp(&a.match_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }
}

/// Score a property against search terms. Returns (score, match_reasons).
fn score_property(property: &Property, society_name: &str, terms: &[&str]) -> (f64, Vec<String>) {
    let fields: Vec<(&str, f64, &str)> = vec![
        (&property.title, 3.0, "title"),
        (&property.area, 2.5, "area"),
        (&property.builder_name, 2.0, "builder"),
        (society_name, 2.0, "society"),
        (&property.description_summary, 1.0, "description"),
        (&property.property_type, 1.5, "type"),
        (&property.possession_status, 1.0, "status"),
        (&property.facing, 0.5, "facing"),
        (&property.city, 1.5, "city"),
    ];

    let mut total_score = 0.0;
    let mut reasons = Vec::new();

    for term in terms {
        let mut term_matched = false;

        for (field_value, weight, field_name) in &fields {
            let field_lower = field_value.to_lowercase();
            if field_lower.contains(term) {
                total_score += weight;
                if !term_matched {
                    reasons.push(format!("matched '{}' in {}", term, field_name));
                    term_matched = true;
                }
            }
        }

        // Also check transparency tags.
        for tag in &property.transparency_tags {
            if tag.to_lowercase().contains(term) {
                total_score += 1.0;
                if !term_matched {
                    reasons.push(format!("matched '{}' in tags", term));
                    term_matched = true;
                }
            }
        }
    }

    (total_score, reasons)
}

/// Legacy hardcoded preference scoring — used when the graph doesn't have
/// self-describing scoring_hint metadata for this preference.
fn legacy_preference_score(property: &Property, preference: &str) -> f64 {
    match preference {
        "metro access" => {
            if property.metro_distance_mins <= 10 {
                2.0
            } else if property.metro_distance_mins <= 20 {
                1.0
            } else {
                0.0
            }
        }
        "quiet neighborhood" => {
            if property.noise_score < 0.3 {
                2.0
            } else if property.noise_score < 0.5 {
                1.0
            } else {
                0.0
            }
        }
        "value for money" => {
            if property.price_per_sqft < 8000 {
                2.0
            } else if property.price_per_sqft < 10000 {
                1.0
            } else {
                0.0
            }
        }
        "premium" => {
            if property.price_per_sqft >= 12000 {
                2.0
            } else if property.price_per_sqft >= 10000 {
                1.0
            } else {
                0.0
            }
        }
        "good society" => {
            if property.society_quality_score >= 0.8 {
                2.0
            } else if property.society_quality_score >= 0.6 {
                1.0
            } else {
                0.0
            }
        }
        "greenery" => property.greenery_score.unwrap_or(0.0) * 2.0,
        "new construction" | "ready to move" => {
            let status = property.possession_status.to_lowercase();
            if preference == "new construction"
                && (status.contains("under") || status.contains("new"))
            {
                2.0
            } else if preference == "ready to move" && status.contains("ready") {
                2.0
            } else {
                0.0
            }
        }
        "high floor" => {
            if property.total_floors > 0 && property.floor >= property.total_floors - 2 {
                2.0
            } else if property.floor >= 10 {
                1.0
            } else {
                0.0
            }
        }
        "east facing" => {
            if property.facing.to_lowercase().contains("east") {
                2.0
            } else {
                0.0
            }
        }
        _ => 0.0,
    }
}

/// Check if a property's area is "nearby" the canonical search area.
/// This catches sub-areas, micro-markets, and Gemini-assigned areas that
/// belong to the same macro area but don't exactly match the canonical name.
///
/// Checks: alias list membership, substring containment, and same-city
/// knowledge graph edges (future). Does NOT check exact match — caller does that.
fn area_is_nearby(property_area: &str, canonical_area: &str) -> bool {
    use super::intent::AREA_ALIASES;

    let prop_lower = property_area.to_lowercase();
    let canon_lower = canonical_area.to_lowercase();

    // 1. Property area is a known alias of the canonical area
    for (aliases, canonical) in AREA_ALIASES {
        if !canonical.eq_ignore_ascii_case(canonical_area) {
            continue;
        }
        for alias in *aliases {
            if prop_lower.contains(alias) || alias.contains(prop_lower.as_str()) {
                return true;
            }
        }
        break;
    }

    // 2. Property area maps to the same canonical area via its own aliases
    //    e.g. property area "Varthur" → canonical "Whitefield", search area is "Whitefield"
    for (aliases, canonical) in AREA_ALIASES {
        if !canonical.eq_ignore_ascii_case(canonical_area) {
            continue;
        }
        // Check if any word in the property area matches an alias
        for word in prop_lower.split_whitespace() {
            for alias in *aliases {
                if *alias == word {
                    return true;
                }
            }
        }
        break;
    }

    // 3. Substring containment (handles "East Whitefield" matching "Whitefield")
    if prop_lower.contains(&canon_lower) || canon_lower.contains(&prop_lower) {
        return true;
    }

    false
}

fn match_label_from_score(normalized: f64) -> String {
    if normalized >= 0.75 {
        "Strong match".to_string()
    } else if normalized >= 0.5 {
        "Good match".to_string()
    } else if normalized >= 0.25 {
        "Partial match".to_string()
    } else {
        "Weak match".to_string()
    }
}

/// Map a preference to its corresponding legacy fact key.
fn legacy_fact_key_for_preference(preference: &str) -> String {
    match preference {
        "metro access" => "metro_distance_mins",
        "quiet neighborhood" => "noise_score",
        "value for money" => "price_per_sqft",
        "premium" => "price_per_sqft",
        "good society" => "society_quality_score",
        "greenery" => "greenery_score",
        "new construction" | "ready to move" => "possession_status",
        "high floor" => "floor",
        "east facing" => "facing",
        _ => "unknown",
    }.to_string()
}

/// Build a human-readable display string for a legacy preference match.
fn format_legacy_display(preference: &str, property: &Property) -> String {
    match preference {
        "metro access" => format!("{} min to metro", property.metro_distance_mins),
        "quiet neighborhood" => {
            if property.noise_score < 0.3 { "Quiet neighborhood".into() }
            else { "Moderately quiet area".into() }
        }
        "value for money" => format!("{}/sqft — good value", property.price_per_sqft),
        "premium" => format!("{}/sqft — premium segment", property.price_per_sqft),
        "good society" => {
            if property.society_quality_score >= 0.8 { "Strong society quality".into() }
            else { "Decent society quality".into() }
        }
        "greenery" => "Green surroundings".into(),
        "new construction" => format!("Status: {}", property.possession_status),
        "ready to move" => format!("Status: {}", property.possession_status),
        "high floor" => format!("Floor {}/{}", property.floor, property.total_floors),
        "east facing" => format!("Facing: {}", property.facing),
        _ => format!("Matches {}", preference),
    }
}

fn build_match_reason(intent: &SearchIntent, reasons: &[String]) -> String {
    let mut parts = Vec::new();

    if let Some(ref area) = intent.area {
        parts.push(format!("Matches {}", area));
    }
    if let Some(bhk) = intent.bhk {
        parts.push(format!("{} BHK", bhk));
    }
    if let Some(budget) = intent.budget_max {
        let budget_str = if budget >= 1_00_00_000 {
            format!("{:.1} Cr", budget as f64 / 1_00_00_000.0)
        } else {
            format!("{:.0} L", budget as f64 / 1_00_000.0)
        };
        parts.push(format!("under {}", budget_str));
    }
    for pref in &intent.preferences {
        parts.push(pref.clone());
    }

    if parts.is_empty() {
        // Fall back to raw match reasons
        reasons.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
    } else {
        parts.join(", ")
    }
}
