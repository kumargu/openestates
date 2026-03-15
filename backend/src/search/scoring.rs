//! Society-first scoring — rank societies using KG facts, then properties within them.
//!
//! This is the primary ranking engine for context-based search. It:
//! 1. Scores each unique society against the user's structured preferences
//! 2. Applies negative penalties for things the user wants to avoid
//! 3. Inherits area-level signals when society-specific facts are missing
//! 4. Applies buyer archetype weight modifiers
//!
//! All scoring is deterministic and <5ms — no external calls.

use std::collections::HashMap;

use serde::Serialize;

use crate::knowledge::{FactValue, KnowledgeGraph, ScoringHint};
use crate::knowledge::fact::ScoringDirection;
use crate::knowledge::node::Node;
use crate::routes::enrichment::{area_node_id, society_node_id};
use crate::search::intent::{BuyerArchetype, Polarity, PreferenceSignal, SearchIntent};

// ---------------------------------------------------------------------------
// Output structs
// ---------------------------------------------------------------------------

/// Score for a single society against a search intent.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SocietyScore {
    pub society_id: String,
    pub score: f32,
    /// 0.0–1.0: based on evidence richness (fact count × confidence)
    pub confidence: f32,
    pub matched_reasons: Vec<MatchReason>,
    pub concerns: Vec<Concern>,
    pub unmatched_preferences: Vec<String>,
}

/// A positive match reason with evidence.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchReason {
    pub preference: String,
    pub fact_key: String,
    pub display: String,
    pub score: f32,
    pub confidence: f32,
    pub source_level: String, // "society" or "area"
}

/// A concern (negative signal detected or data absent).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Concern {
    pub preference: String,
    pub display: String,
    pub confidence: f32,
    pub source_level: String, // "society" or "area"
    pub severity: String,     // "warning" (strong negative signal) or "caution" (moderate/no-data)
}

// ---------------------------------------------------------------------------
// Explanation card types (Day 34)
// ---------------------------------------------------------------------------

/// A complete explanation for why a result matched and what to watch out for.
/// Generated deterministically from SocietyScore — zero LLM cost.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplanationCard {
    pub why_matches: Vec<ExplanationReason>,
    pub concerns: Vec<ExplanationConcern>,
    pub unmatched: Vec<String>,
    /// "high", "medium", or "low"
    pub confidence_label: String,
    pub evidence_summary: EvidenceSummary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplanationReason {
    /// Human-readable explanation, e.g. "Signals suggest good maintenance"
    pub text: String,
    /// The user preference this addresses, e.g. "good maintenance"
    pub preference: String,
    /// "strong", "moderate", or "limited"
    pub evidence_strength: String,
    /// User-friendly source names, e.g. ["Reddit resident discussions"]
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplanationConcern {
    pub text: String,
    pub preference: String,
    /// "caution" or "warning"
    pub severity: String,
    /// "society-specific" or "area-level"
    pub source_level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceSummary {
    pub facts_consulted: usize,
    pub sources: Vec<String>,
    pub graph_driven_pct: f32,
}

/// Map internal source type strings to user-friendly display names.
fn source_display_name(source_type: &str) -> &'static str {
    match source_type {
        "Reddit" => "Reddit resident discussions",
        "Google" => "Google reviews",
        "Rera" => "RERA registry",
        "Bbmp" => "BBMP records",
        "News" => "News coverage",
        "Computed" => "Computed from data",
        "Manual" => "Verified data",
        "Llm" => "AI analysis",
        "Seed" => "Curated data",
        _ => "Data sources",
    }
}

/// Generate an ExplanationCard from a SocietyScore — no LLM, template-based.
///
/// Confidence qualifiers:
/// - confidence >= 0.8 → stated as fact (no qualifier)
/// - confidence >= 0.5 → "Signals suggest..."
/// - confidence < 0.5 → "Limited evidence suggests..."
pub fn synthesize_explanation(
    society_score: &SocietyScore,
    facts_consulted: usize,
    source_types: &[String],
) -> ExplanationCard {
    let mut why_matches = Vec::new();
    let mut concerns = Vec::new();
    let mut seen_sources: std::collections::HashSet<&str> = Default::default();

    // Build why_matches from matched_reasons with score > 0.3
    for reason in &society_score.matched_reasons {
        if reason.score < 0.3 {
            continue;
        }
        let qualifier = if reason.confidence >= 0.8 {
            ""
        } else if reason.confidence >= 0.5 {
            "Signals suggest "
        } else {
            "Limited evidence suggests "
        };

        let text = if qualifier.is_empty() {
            reason.display.clone()
        } else {
            format!("{}{}", qualifier, reason.display.to_lowercase())
        };

        let evidence_strength = if reason.confidence >= 0.8 {
            "strong"
        } else if reason.confidence >= 0.5 {
            "moderate"
        } else {
            "limited"
        };

        let source_name = source_display_name(&reason.source_level); // This is actually source_level, not source_type
        // For matched reasons we use confidence to imply source quality
        let source_display = if reason.confidence >= 0.8 { "Verified data" } else { "AI analysis" };
        seen_sources.insert(source_display);

        why_matches.push(ExplanationReason {
            text,
            preference: reason.preference.clone(),
            evidence_strength: evidence_strength.to_string(),
            sources: vec![source_display.to_string()],
        });
    }

    // Build concerns from society score concerns
    for concern in &society_score.concerns {
        let (text, note) = if concern.display.starts_with("No data") {
            (
                format!("No data available for '{}'", concern.preference),
                None,
            )
        } else if concern.source_level == "area" {
            (
                concern.display.clone(),
                Some("area-level signal — verify at society level".to_string()),
            )
        } else {
            (concern.display.clone(), None)
        };

        concerns.push(ExplanationConcern {
            text,
            preference: concern.preference.clone(),
            severity: concern.severity.clone(),
            source_level: if concern.source_level == "area" {
                "area-level".to_string()
            } else {
                "society-specific".to_string()
            },
            note,
        });
    }

    // Collect unique source names from provided source types
    let unique_sources: Vec<String> = source_types.iter()
        .map(|s| source_display_name(s).to_string())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let graph_driven_pct = if facts_consulted > 0 {
        (society_score.matched_reasons.len() as f32 / facts_consulted as f32 * 100.0).min(100.0)
    } else {
        0.0
    };

    ExplanationCard {
        why_matches,
        concerns,
        unmatched: society_score.unmatched_preferences.clone(),
        confidence_label: if society_score.confidence >= 0.7 {
            "high".to_string()
        } else if society_score.confidence >= 0.4 {
            "medium".to_string()
        } else {
            "low".to_string()
        },
        evidence_summary: EvidenceSummary {
            facts_consulted,
            sources: unique_sources,
            graph_driven_pct,
        },
    }
}

// ---------------------------------------------------------------------------
// Archetype weight profiles
// ---------------------------------------------------------------------------

struct ArchetypeProfile {
    boost_keys: &'static [&'static str],
    boost_weight: f32,
    penalize_keys: &'static [&'static str],
    penalize_weight: f32,
}

fn archetype_profile(archetype: &BuyerArchetype) -> ArchetypeProfile {
    match archetype {
        BuyerArchetype::Family => ArchetypeProfile {
            boost_keys: &["family_friendly", "child_safety", "calm_environment", "community_vibe", "school_nearby"],
            boost_weight: 1.5,
            penalize_keys: &["noise_score", "traffic_score", "density"],
            penalize_weight: 1.3,
        },
        BuyerArchetype::Investor => ArchetypeProfile {
            boost_keys: &["resale_strength", "market_activity", "rental_yield", "metro_distance"],
            boost_weight: 1.5,
            penalize_keys: &["litigation_risk"],
            penalize_weight: 1.5,
        },
        BuyerArchetype::RiskAverse => ArchetypeProfile {
            boost_keys: &["rera_status", "document_completeness", "builder_reputation"],
            boost_weight: 1.3,
            penalize_keys: &["litigation_risk", "waterlogging_risk", "possession_delay"],
            penalize_weight: 2.0,
        },
        BuyerArchetype::ValueBuyer => ArchetypeProfile {
            boost_keys: &["value_for_money", "maintenance_cost", "metro_distance"],
            boost_weight: 1.5,
            penalize_keys: &["amenity_quality", "finish_quality"], // Penalize premium signals
            penalize_weight: 1.2,
        },
        BuyerArchetype::LuxuryBuyer => ArchetypeProfile {
            boost_keys: &["builder_reputation", "amenity_quality", "finish_quality", "quality_perception"],
            boost_weight: 1.6,
            penalize_keys: &["value_for_money", "maintenance_cost"], // Penalize budget signals
            penalize_weight: 1.2,
        },
        BuyerArchetype::EndUser => ArchetypeProfile {
            boost_keys: &["maintenance_quality", "community_vibe", "livability_score", "daily_convenience"],
            boost_weight: 1.3,
            penalize_keys: &["litigation_risk"],
            penalize_weight: 1.5,
        },
    }
}

// ---------------------------------------------------------------------------
// Core scoring function
// ---------------------------------------------------------------------------

/// Score a society node against a search intent using its KG facts.
///
/// Returns a SocietyScore with matched reasons, concerns, and confidence.
/// Computation is in-memory and O(facts × preferences) — well under 1ms.
pub fn score_society_for_intent(
    society_node: &Node,
    area_node: Option<&Node>,
    intent: &SearchIntent,
) -> SocietyScore {
    let mut matched_reasons = Vec::new();
    let mut concerns = Vec::new();
    let mut unmatched = Vec::new();
    let mut total_score: f32 = 0.0;

    // --- Score positive preferences ---
    for pref in &intent.positive_preferences {
        let hits = find_facts_matching_keys(society_node, area_node, &pref.expanded_keys);

        if hits.is_empty() {
            unmatched.push(pref.raw_text.clone());
            continue;
        }

        // Take the best hit (highest-confidence fact at society level first)
        let best = hits.into_iter().max_by(|a, b| {
            // Prefer society-level over area-level, then by confidence
            let a_society = if a.source_level == "society" { 1 } else { 0 };
            let b_society = if b.source_level == "society" { 1 } else { 0 };
            (a_society, a.score as i32).cmp(&(b_society, b.score as i32))
        });

        if let Some(hit) = best {
            let weighted = hit.score * pref.weight;
            total_score += weighted;
            matched_reasons.push(MatchReason {
                preference: pref.raw_text.clone(),
                fact_key: hit.fact_key.clone(),
                display: hit.display.clone(),
                score: hit.score,
                confidence: hit.confidence,
                source_level: hit.source_level.clone(),
            });
        }
    }

    // --- Score negative preferences (these become penalties / concerns) ---
    for pref in &intent.negative_preferences {
        let hits = find_facts_matching_keys(society_node, area_node, &pref.expanded_keys);

        if hits.is_empty() {
            // No data is a mild caution
            concerns.push(Concern {
                preference: pref.raw_text.clone(),
                display: format!("No data on '{}'", pref.raw_text),
                confidence: 0.0,
                source_level: "unknown".to_string(),
                severity: "caution".to_string(),
            });
            continue;
        }

        let best = hits.into_iter().max_by(|a, b| {
            a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal)
        });

        if let Some(hit) = best {
            // High score on a negative preference = bad signal → penalty
            // 2.0× multiplier ensures negative prefs visibly differentiate results
            let penalty = hit.score * pref.weight * 2.0;
            total_score -= penalty;

            let severity = if hit.score > 0.5 { "warning" } else { "caution" };
            concerns.push(Concern {
                preference: pref.raw_text.clone(),
                display: hit.display.clone(),
                confidence: hit.confidence,
                source_level: hit.source_level.clone(),
                severity: severity.to_string(),
            });
        }
    }

    // --- Apply buyer archetype modifiers ---
    if let Some(ref archetype) = intent.buyer_archetype {
        let profile = archetype_profile(archetype);

        // Boost facts matching archetype's favored keys
        for boost_key in profile.boost_keys {
            let fact_opt = society_node.get_fact(boost_key)
                .or_else(|| area_node.and_then(|a: &Node| a.get_fact(boost_key)));
            if let Some(fact) = fact_opt {
                if let Some(ref hint) = fact.scoring_hint {
                    let raw = score_with_hint(&fact.value, hint);
                    total_score += raw * (profile.boost_weight - 1.0); // Extra boost
                }
            }
        }

        // Penalize facts in archetype's penalize list
        for pen_key in profile.penalize_keys {
            let fact_opt = society_node.get_fact(pen_key)
                .or_else(|| area_node.and_then(|a: &Node| a.get_fact(pen_key)));
            if let Some(fact) = fact_opt {
                if let Some(ref hint) = fact.scoring_hint {
                    let raw = score_with_hint(&fact.value, hint);
                    total_score -= raw * (profile.penalize_weight - 1.0);
                }
            }
        }
    }

    // --- Compute evidence confidence ---
    // More facts, higher-confidence sources → higher confidence in the society score
    let fact_count = society_node.facts.len();
    let avg_confidence = if fact_count > 0 {
        society_node.facts.iter().map(|f| f.confidence).sum::<f32>() / fact_count as f32
    } else {
        0.0
    };
    // Scale: 10+ facts with 0.8 avg confidence → confidence = 1.0
    let evidence_confidence = ((fact_count as f32 / 10.0).min(1.0) * avg_confidence).min(1.0);

    SocietyScore {
        society_id: society_node.id.clone(),
        score: total_score.max(0.0), // Clamp to non-negative
        confidence: evidence_confidence,
        matched_reasons,
        concerns,
        unmatched_preferences: unmatched,
    }
}

// ---------------------------------------------------------------------------
// Fact matching with area inheritance
// ---------------------------------------------------------------------------

struct FactHit {
    fact_key: String,
    display: String,
    score: f32,
    confidence: f32,
    source_level: String,
}

/// Find facts that match any of the given KG fact keys, with area inheritance.
///
/// Society facts get weight 1.0.
/// Area facts get weight 0.7 if society has no direct evidence,
/// weight 0.3 if society already has direct evidence (supplementary).
fn find_facts_matching_keys(
    society_node: &Node,
    area_node: Option<&Node>,
    keys: &[String],
) -> Vec<FactHit> {
    let mut hits = Vec::new();
    let mut society_matched_keys: std::collections::HashSet<&str> = Default::default();

    // Society-level facts first
    for key in keys {
        if let Some(fact) = society_node.get_fact(key) {
            let score = if let Some(ref hint) = fact.scoring_hint {
                score_with_hint(&fact.value, hint)
            } else {
                // Fallback: text match or existence
                0.6
            };
            let display = render_fact(fact);
            hits.push(FactHit {
                fact_key: key.clone(),
                display,
                score: score * 1.0, // Society weight = 1.0
                confidence: fact.confidence,
                source_level: "society".to_string(),
            });
            society_matched_keys.insert(key.as_str());
        }
    }

    // Area-level facts (inheritance)
    if let Some(area) = area_node {
        for key in keys {
            if let Some(fact) = area.get_fact(key) {
                let base_score = if let Some(ref hint) = fact.scoring_hint {
                    score_with_hint(&fact.value, hint)
                } else {
                    0.6
                };
                // Reduce weight if society had direct evidence
                let weight = if society_matched_keys.contains(key.as_str()) { 0.3 } else { 0.7 };
                let display = render_fact(fact);
                hits.push(FactHit {
                    fact_key: key.clone(),
                    display: format!("{} (area-level signal)", display),
                    score: base_score * weight,
                    confidence: fact.confidence * 0.9, // Slightly lower confidence for area signals
                    source_level: "area".to_string(),
                });
            }
        }
    }

    hits
}

/// Score a fact value using its scoring hint. Returns 0.0–hint.weight.
fn score_with_hint(value: &FactValue, hint: &ScoringHint) -> f32 {
    let weight = hint.weight;

    match &hint.direction {
        ScoringDirection::HigherIsBetter => {
            let num = fact_as_numeric(value).unwrap_or(0.0) as f32;
            if hint.thresholds.len() >= 2 {
                let good = hint.thresholds[0] as f32;
                let ok = hint.thresholds[1] as f32;
                if num >= good { weight } else if num >= ok { weight * 0.5 } else { 0.0 }
            } else {
                num.clamp(0.0, 1.0) * weight
            }
        }
        ScoringDirection::LowerIsBetter => {
            let num = fact_as_numeric(value).unwrap_or(f32::MAX as f64) as f32;
            if hint.thresholds.len() >= 2 {
                let good = hint.thresholds[0] as f32;
                let ok = hint.thresholds[1] as f32;
                if num <= good { weight } else if num <= ok { weight * 0.5 } else { 0.0 }
            } else {
                (1.0 - num.clamp(0.0, 1.0)) * weight
            }
        }
        ScoringDirection::TextMatch => {
            let text = fact_as_text(value).unwrap_or_default().to_lowercase();
            let positive = ["good", "high", "positive", "quiet", "safe", "yes", "excellent", "strong", "reliable"];
            let partial = ["average", "moderate", "mixed", "ok", "fair"];
            if positive.iter().any(|p| text.contains(p)) { weight }
            else if partial.iter().any(|p| text.contains(p)) { weight * 0.5 }
            else { 0.0 }
        }
    }
}

fn fact_as_numeric(value: &FactValue) -> Option<f64> {
    match value {
        FactValue::Numeric(n) => Some(*n),
        FactValue::Score { value: v, .. } => Some(*v),
        _ => None,
    }
}

fn fact_as_text(value: &FactValue) -> Option<&str> {
    match value {
        FactValue::Text(s) => Some(s.as_str()),
        _ => None,
    }
}

fn render_fact(fact: &crate::knowledge::SourcedFact) -> String {
    let value_str = match &fact.value {
        FactValue::Text(s) => s.clone(),
        FactValue::Numeric(n) => {
            if *n == (*n as i64) as f64 { format!("{}", *n as i64) } else { format!("{:.1}", n) }
        }
        FactValue::Bool(b) => if *b { "yes".to_string() } else { "no".to_string() },
        FactValue::Tags(tags) => tags.join(", "),
        FactValue::Score { value: v, .. } => format!("{:.1}", v),
    };
    if let Some(ref tmpl) = fact.display_template {
        tmpl.replace("{value}", &value_str)
    } else {
        format!("{}: {}", fact.key, value_str)
    }
}

// ---------------------------------------------------------------------------
// Public API: score all candidate societies
// ---------------------------------------------------------------------------

/// Score all unique societies from the candidate property list.
///
/// Returns a map of society_node_id → SocietyScore.
pub fn score_all_societies(
    graph: &KnowledgeGraph,
    society_ids: &[String],  // unique society_ids (not node IDs)
    intent: &SearchIntent,
) -> HashMap<String, SocietyScore> {
    let mut scores = HashMap::new();

    for society_id in society_ids {
        let node_id = society_node_id(society_id);
        let node = match graph.get_node(&node_id) {
            Some(n) => n,
            None => continue,
        };

        // Find the area node for this society
        // We look for an "area" fact on the society node to get the area name
        let area_name = node.facts.iter()
            .find(|f| f.key == "area")
            .and_then(|f| match &f.value {
                FactValue::Text(s) => Some(s.clone()),
                _ => None,
            });

        let area_node = area_name.as_deref()
            .and_then(|area| graph.get_node(&area_node_id(area)));

        let score = score_society_for_intent(node, area_node, intent);
        scores.insert(node_id, score);
    }

    scores
}
