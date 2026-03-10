//! Theme, tradeoff, and market activity computation.
//! KG-facts-first: when the knowledge graph has pre-scored facts from skills
//! (e.g., score_society's overall_score, top_signals, top_cautions), use them
//! directly. Fall back to seed data thresholds only when KG facts are absent.

use serde::Serialize;

use crate::knowledge::KnowledgeGraph;
use crate::models::{AreaProfile, Property, Society};
use crate::routes::enrichment::{kg_numeric, kg_tags, kg_text, society_node_id};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeLabel {
    Strong,
    Good,
    Mixed,
    Weak,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThemeResult {
    pub label: ThemeLabel,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompareThemes {
    pub value: ThemeResult,
    pub commute: ThemeResult,
    pub society: ThemeResult,
    pub greenery: ThemeResult,
    pub risk: ThemeResult,
    pub resale: ThemeResult,
    pub market: ThemeResult,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThemeComponent {
    pub label: String,
    pub level: ThemeLabel,
}

#[derive(Debug, Clone, Serialize)]
pub struct TradeoffsResponse {
    pub headline: String,
    pub strengths: Vec<String>,
    pub cautions: Vec<String>,
    pub components: Vec<ThemeComponent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PriceVsMedian {
    pub pct_diff: i32,
    pub verdict: String,
    pub verdict_class: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketActivityResponse {
    pub interest_level: String,
    pub saves_last_7d: Option<u32>,
    pub offers_last_7d: Option<u32>,
    pub days_on_market: u32,
    pub days_on_market_label: String,
    pub interest_label: String,
    pub area_trend_summary: String,
    pub price_vs_median: Option<PriceVsMedian>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn score_to_label(score: f64) -> ThemeLabel {
    if score >= 0.8 {
        ThemeLabel::Strong
    } else if score >= 0.6 {
        ThemeLabel::Good
    } else if score >= 0.4 {
        ThemeLabel::Mixed
    } else {
        ThemeLabel::Weak
    }
}

/// Invert risk: lower average risk score = better label.
fn risk_label(avg_risk: f64) -> ThemeLabel {
    if avg_risk < 0.15 {
        ThemeLabel::Strong
    } else if avg_risk < 0.25 {
        ThemeLabel::Good
    } else if avg_risk < 0.35 {
        ThemeLabel::Mixed
    } else {
        ThemeLabel::Weak
    }
}

fn label_is_strong_or_good(l: &ThemeLabel) -> bool {
    matches!(l, ThemeLabel::Strong | ThemeLabel::Good)
}

fn label_is_weak_or_mixed(l: &ThemeLabel) -> bool {
    matches!(l, ThemeLabel::Weak | ThemeLabel::Mixed)
}

// ---------------------------------------------------------------------------
// Theme computation — each dimension checks KG first
// ---------------------------------------------------------------------------

fn compute_value(p: &Property, area: Option<&AreaProfile>, graph: &KnowledgeGraph) -> ThemeResult {
    let node_id = society_node_id(&p.society_id);

    // KG-first: score_value_for_money from score_society skill (0-100)
    if let Some(score) = kg_numeric(graph, &node_id, "score_value_for_money") {
        let normalized = score / 100.0;
        let summary = kg_text(graph, &node_id, "value_reasoning")
            .unwrap_or_else(|| fallback_value_summary(p, area));
        return ThemeResult {
            label: score_to_label(normalized),
            summary,
        };
    }

    // Fallback: seed data price_per_sqft vs area median
    let Some(area) = area else {
        return ThemeResult {
            label: ThemeLabel::Mixed,
            summary: "Area data unavailable".into(),
        };
    };
    if area.median_price_per_sqft == 0 || p.price_per_sqft == 0 {
        return ThemeResult {
            label: ThemeLabel::Mixed,
            summary: "Area pricing data unavailable".into(),
        };
    }
    let ratio = p.price_per_sqft as f64 / area.median_price_per_sqft as f64;
    let pct_diff = ((1.0 - ratio) * 100.0).round() as i32;
    let pct_abs = (pct_diff as i64).unsigned_abs() as u32;

    if ratio < 0.9 {
        ThemeResult {
            label: ThemeLabel::Strong,
            summary: format!(
                "{}% below {} median — strong value",
                pct_abs,
                area.name
            ),
        }
    } else if ratio <= 1.05 {
        ThemeResult {
            label: ThemeLabel::Good,
            summary: format!("Near {} median — fair market pricing", area.name),
        }
    } else if ratio <= 1.2 {
        ThemeResult {
            label: ThemeLabel::Mixed,
            summary: format!(
                "{}% above {} median — premium pricing",
                pct_abs,
                area.name
            ),
        }
    } else {
        ThemeResult {
            label: ThemeLabel::Weak,
            summary: format!(
                "{}% above {} median — significant premium",
                pct_abs,
                area.name
            ),
        }
    }
}

fn fallback_value_summary(p: &Property, area: Option<&AreaProfile>) -> String {
    let Some(area) = area else {
        return "Area data unavailable".into();
    };
    if area.median_price_per_sqft == 0 || p.price_per_sqft == 0 {
        return "Area pricing data unavailable".into();
    }
    let ratio = p.price_per_sqft as f64 / area.median_price_per_sqft as f64;
    let pct_diff = ((1.0 - ratio) * 100.0).round() as i32;
    let pct_abs = (pct_diff as i64).unsigned_abs() as u32;
    if ratio < 0.9 {
        format!(
            "{}% below {} median — strong value",
            pct_abs,
            area.name
        )
    } else if ratio <= 1.05 {
        format!("Near {} median — fair market pricing", area.name)
    } else {
        format!(
            "{}% above {} median — premium pricing",
            pct_abs,
            area.name
        )
    }
}

fn compute_commute(p: &Property, graph: &KnowledgeGraph) -> ThemeResult {
    let node_id = society_node_id(&p.society_id);

    // KG-first: score_connectivity from score_society skill (0-100)
    if let Some(score) = kg_numeric(graph, &node_id, "score_connectivity") {
        let normalized = score / 100.0;
        let summary = kg_text(graph, &node_id, "connectivity_reasoning").unwrap_or_else(|| {
            format!(
                "{} min to metro{}",
                p.metro_distance_mins,
                if p.traffic_score >= 0.7 {
                    ", heavy traffic area"
                } else {
                    ""
                }
            )
        });
        return ThemeResult {
            label: score_to_label(normalized),
            summary,
        };
    }

    // Fallback: seed data metro_distance_mins + traffic_score
    let metro = p.metro_distance_mins;
    let traffic = p.traffic_score;

    if metro <= 15 && traffic < 0.6 {
        ThemeResult {
            label: ThemeLabel::Strong,
            summary: "Close metro access, manageable traffic".into(),
        }
    } else if metro <= 20 && traffic < 0.7 {
        ThemeResult {
            label: ThemeLabel::Good,
            summary: format!("{} min to metro, moderate traffic", metro),
        }
    } else if metro <= 30 {
        ThemeResult {
            label: ThemeLabel::Mixed,
            summary: format!(
                "{} min to metro{}",
                metro,
                if traffic >= 0.7 {
                    ", heavy traffic area"
                } else {
                    ""
                }
            ),
        }
    } else {
        ThemeResult {
            label: ThemeLabel::Weak,
            summary: format!(
                "{} min to metro{}",
                metro,
                if traffic >= 0.7 {
                    ", congested corridor"
                } else {
                    ", distant from metro"
                }
            ),
        }
    }
}

fn compute_society(
    p: &Property,
    society: Option<&Society>,
    graph: &KnowledgeGraph,
) -> ThemeResult {
    let node_id = society_node_id(&p.society_id);

    // KG-first: overall_score from score_society skill (0-100)
    if let Some(score) = kg_numeric(graph, &node_id, "overall_score") {
        let normalized = score / 100.0;
        let summary = kg_text(graph, &node_id, "one_line_verdict")
            .unwrap_or_else(|| format!("Society scored {}/100", score as u32));
        return ThemeResult {
            label: score_to_label(normalized),
            summary,
        };
    }

    // Fallback: seed data society_quality_score
    let score = p.society_quality_score;
    let label = score_to_label(score);

    let summary = match society {
        None => {
            if score >= 0.8 {
                "High quality society".into()
            } else if score >= 0.6 {
                "Average society quality".into()
            } else {
                "Below average society quality".into()
            }
        }
        Some(soc) => {
            let sentiment_note = if soc.maintenance_sentiment == "good" {
                "well-maintained"
            } else {
                "average maintenance"
            };
            if score >= 0.85 {
                format!("Premium society, {}", sentiment_note)
            } else if score >= 0.7 {
                format!(
                    "Good society by {}, {}",
                    soc.builder_name, sentiment_note
                )
            } else {
                format!("Average society quality, {}", sentiment_note)
            }
        }
    };

    ThemeResult { label, summary }
}

fn compute_greenery(p: &Property, graph: &KnowledgeGraph) -> ThemeResult {
    let node_id = society_node_id(&p.society_id);

    // KG-first: check for amenities score as proxy
    if let Some(score) = kg_numeric(graph, &node_id, "score_amenities") {
        let normalized = score / 100.0;
        // Blend with seed data if available
        let seed_avg = {
            let g = p.greenery_score.unwrap_or(0.5);
            let o = p.open_space_score.unwrap_or(0.5);
            (g + o) / 2.0
        };
        let blended = (normalized + seed_avg) / 2.0;
        let label = score_to_label(blended);
        let summary = if blended >= 0.8 {
            "Lush greenery, spacious open areas".into()
        } else if blended >= 0.65 {
            "Good green cover and reasonable open space".into()
        } else if blended >= 0.45 {
            "Moderate greenery, denser built environment".into()
        } else {
            "Limited greenery, dense surroundings".into()
        };
        return ThemeResult { label, summary };
    }

    // Fallback: seed data greenery_score + open_space_score
    let g = p.greenery_score.unwrap_or(0.5);
    let o = p.open_space_score.unwrap_or(0.5);
    let avg = (g + o) / 2.0;
    let label = score_to_label(avg);

    let summary = if avg >= 0.8 {
        "Lush greenery, spacious open areas"
    } else if avg >= 0.65 {
        "Good green cover and reasonable open space"
    } else if avg >= 0.45 {
        "Moderate greenery, denser built environment"
    } else {
        "Limited greenery, dense surroundings"
    };

    ThemeResult {
        label,
        summary: summary.into(),
    }
}

fn compute_risk(p: &Property, graph: &KnowledgeGraph) -> ThemeResult {
    let node_id = society_node_id(&p.society_id);

    // KG-first: score_safety from score_society skill (0-100, higher = safer)
    if let Some(score) = kg_numeric(graph, &node_id, "score_safety") {
        let normalized = score / 100.0;
        let summary = kg_text(graph, &node_id, "safety_reasoning")
            .unwrap_or_else(|| fallback_risk_summary(p));
        return ThemeResult {
            label: score_to_label(normalized),
            summary,
        };
    }

    // Fallback: seed data risk scores (lower = better)
    let avg_risk = (p.litigation_risk + p.waterlogging_risk_score + p.noise_score) / 3.0;
    let label = risk_label(avg_risk);
    let summary = fallback_risk_summary(p);

    ThemeResult { label, summary }
}

fn fallback_risk_summary(p: &Property) -> String {
    let avg_risk = (p.litigation_risk + p.waterlogging_risk_score + p.noise_score) / 3.0;
    let mut risks = Vec::new();
    if p.litigation_risk >= 0.15 {
        risks.push("litigation concern");
    }
    if p.waterlogging_risk_score >= 0.3 {
        risks.push("waterlogging risk");
    }
    if p.noise_score >= 0.4 {
        risks.push("elevated noise");
    }

    if avg_risk < 0.15 {
        "Low risk across all dimensions".into()
    } else if avg_risk < 0.25 {
        if risks.is_empty() {
            "Low overall risk profile".into()
        } else {
            format!("Generally low risk — note: {}", risks.join(", "))
        }
    } else if avg_risk < 0.35 {
        if risks.is_empty() {
            "Some risk factors to consider".into()
        } else {
            format!("Mixed risk — {}", risks.join(", "))
        }
    } else {
        format!("Elevated risk — {}", risks.join(", "))
    }
}

fn compute_resale(p: &Property) -> ThemeResult {
    let score = p.resale_strength_score.unwrap_or(0.6);
    let label = score_to_label(score);
    let summary = if score >= 0.85 {
        "Strong resale demand, moves fast"
    } else if score >= 0.7 {
        "Good resale potential in this micro-market"
    } else if score >= 0.5 {
        "Average resale strength"
    } else {
        "Weaker resale market — may take longer to sell"
    };
    ThemeResult {
        label,
        summary: summary.into(),
    }
}

fn compute_market_theme(p: &Property) -> ThemeResult {
    let interest = p
        .interest_level
        .as_deref()
        .unwrap_or("moderate");
    let saves = p.saves_last_7d.unwrap_or(0);
    let dom = p.days_on_market;

    if interest == "high" && dom < 30 {
        ThemeResult {
            label: ThemeLabel::Strong,
            summary: format!(
                "High interest, {} saves this week, {} days on market",
                saves, dom
            ),
        }
    } else if interest == "high" || (interest == "moderate" && dom < 45) {
        ThemeResult {
            label: ThemeLabel::Good,
            summary: format!(
                "{} interest, {} days on market",
                if interest == "high" { "High" } else { "Moderate" },
                dom
            ),
        }
    } else if interest == "moderate" {
        ThemeResult {
            label: ThemeLabel::Mixed,
            summary: format!("Moderate interest, {} days on market", dom),
        }
    } else {
        ThemeResult {
            label: ThemeLabel::Weak,
            summary: format!("Low interest, {} days on market", dom),
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Compute all 7 theme dimensions for a property.
pub fn compute_themes(
    p: &Property,
    area: Option<&AreaProfile>,
    society: Option<&Society>,
    graph: &KnowledgeGraph,
) -> CompareThemes {
    CompareThemes {
        value: compute_value(p, area, graph),
        commute: compute_commute(p, graph),
        society: compute_society(p, society, graph),
        greenery: compute_greenery(p, graph),
        risk: compute_risk(p, graph),
        resale: compute_resale(p),
        market: compute_market_theme(p),
    }
}

/// Compute tradeoffs: headline, strengths, cautions, and component badges.
pub fn compute_tradeoffs(
    p: &Property,
    area: Option<&AreaProfile>,
    society: Option<&Society>,
    graph: &KnowledgeGraph,
) -> TradeoffsResponse {
    let node_id = society_node_id(&p.society_id);

    // KG-first: top_signals and top_cautions from score_society skill
    let kg_signals = kg_tags(graph, &node_id, "top_signals");
    let kg_cautions = kg_tags(graph, &node_id, "top_cautions");

    let mut strengths = kg_signals.unwrap_or_default();
    let mut cautions = kg_cautions.unwrap_or_default();

    // Supplement with seed-data-derived tradeoffs if KG didn't provide enough
    if strengths.len() < 2 {
        if let Some(a) = area {
            if a.median_price_per_sqft > 0 {
                let ratio = p.price_per_sqft as f64 / a.median_price_per_sqft as f64;
                if ratio < 0.9 {
                    strengths.push(format!("Strong value relative to {} median", a.name));
                }
            }
        }
        if p.metro_distance_mins <= 18 {
            strengths.push("Good metro access".into());
        }
        if p.document_completeness_score >= 0.93 {
            strengths.push("Strong documentation".into());
        }
        if p.society_quality_score >= 0.85 {
            strengths.push("Premium society quality".into());
        }
        let green_avg =
            (p.greenery_score.unwrap_or(0.5) + p.open_space_score.unwrap_or(0.5)) / 2.0;
        if green_avg >= 0.75 {
            strengths.push("Greener, more open surroundings".into());
        }
    }

    if cautions.is_empty() {
        if let Some(a) = area {
            if a.median_price_per_sqft > 0 {
                let ratio = p.price_per_sqft as f64 / a.median_price_per_sqft as f64;
                if ratio > 1.1 {
                    cautions.push(format!(
                        "Premium pricing vs other {} listings",
                        a.name
                    ));
                }
            }
        }
        if p.metro_distance_mins >= 35 {
            cautions.push("Distant from metro".into());
        }
        if p.document_completeness_score < 0.85 {
            cautions.push("Document completeness below standard — verify".into());
        }
        if p.traffic_score >= 0.7 {
            cautions.push("Heavier traffic in this corridor".into());
        }
        if p.litigation_risk >= 0.15 {
            cautions.push("Elevated litigation risk — engage a lawyer".into());
        }
        if p.waterlogging_risk_score >= 0.3 {
            cautions.push("Waterlogging risk — verify monsoon history".into());
        }
        let green_avg =
            (p.greenery_score.unwrap_or(0.5) + p.open_space_score.unwrap_or(0.5)) / 2.0;
        if green_avg < 0.45 {
            cautions.push("Dense built environment, limited greenery".into());
        }
    }

    strengths.truncate(3);
    cautions.truncate(2);

    // Build headline from themes
    let themes = compute_themes(p, area, society, graph);
    let headline = build_headline(&themes);

    // Build component badges
    let avg_risk = (p.litigation_risk + p.waterlogging_risk_score + p.noise_score) / 3.0;
    let components = vec![
        ThemeComponent {
            label: "Value".into(),
            level: themes.value.label,
        },
        ThemeComponent {
            label: "Commute & Access".into(),
            level: themes.commute.label,
        },
        ThemeComponent {
            label: "Society Quality".into(),
            level: themes.society.label,
        },
        ThemeComponent {
            label: "Greenery / Open Space".into(),
            level: themes.greenery.label,
        },
        ThemeComponent {
            label: "Document Trust".into(),
            level: score_to_label(p.document_completeness_score),
        },
        ThemeComponent {
            label: "Risk Profile".into(),
            level: risk_label(avg_risk),
        },
    ];

    TradeoffsResponse {
        headline,
        strengths,
        cautions,
        components,
    }
}

fn build_headline(themes: &CompareThemes) -> String {
    let mut strengths = Vec::new();
    let mut caution_labels = Vec::new();

    if matches!(themes.value.label, ThemeLabel::Strong) {
        strengths.push("strong value");
    } else if matches!(themes.value.label, ThemeLabel::Weak) {
        caution_labels.push("premium pricing");
    }

    if label_is_strong_or_good(&themes.commute.label) {
        strengths.push("good metro access");
    } else if matches!(themes.commute.label, ThemeLabel::Weak) {
        caution_labels.push("commute tradeoff");
    }

    if matches!(themes.society.label, ThemeLabel::Strong) {
        strengths.push("premium society");
    }

    if label_is_strong_or_good(&themes.greenery.label) {
        strengths.push("green surroundings");
    }

    if label_is_weak_or_mixed(&themes.risk.label) {
        caution_labels.push("elevated risk signals");
    }

    if strengths.len() >= 2 {
        format!(
            "Strong match for {} and {}.",
            strengths[0], strengths[1]
        )
    } else if strengths.len() == 1 && !caution_labels.is_empty() {
        format!(
            "Good option for {}, but {}.",
            strengths[0], caution_labels[0]
        )
    } else if strengths.len() == 1 {
        format!("Solid choice for {}.", strengths[0])
    } else if !caution_labels.is_empty() {
        format!("Consider carefully — {}.", caution_labels.join(", "))
    } else {
        "Balanced option across key dimensions.".into()
    }
}

/// Compute market activity response with display labels.
pub fn compute_market_activity(
    p: &Property,
    area: Option<&AreaProfile>,
) -> MarketActivityResponse {
    let interest_level = p
        .interest_level
        .clone()
        .unwrap_or_else(|| "moderate".into());

    let days_on_market_label = match p.days_on_market {
        d if d <= 14 => "Recently listed".into(),
        d if d <= 30 => "Listed this month".into(),
        d if d <= 60 => "On market for a while".into(),
        _ => "Long on market — may negotiate".into(),
    };

    let interest_label = match interest_level.as_str() {
        "high" => "High interest area".into(),
        "moderate" => "Moderate interest".into(),
        _ => "Limited interest".into(),
    };

    let area_trend_summary = area
        .map(|a| a.trend_summary.clone())
        .unwrap_or_else(|| "Trend data unavailable".into());

    let price_vs_median = area.and_then(|a| {
        if a.median_price_per_sqft == 0 {
            return None; // Can't compute without median
        }
        let pct_diff = (((p.price_per_sqft as f64 - a.median_price_per_sqft as f64)
            / a.median_price_per_sqft as f64)
            * 100.0)
            .round() as i32;

        let (verdict, verdict_class) = if pct_diff <= -10 {
            ("Good value", "positive")
        } else if pct_diff <= 5 {
            ("Near market", "neutral")
        } else {
            ("Premium pricing", "warning")
        };

        Some(PriceVsMedian {
            pct_diff,
            verdict: verdict.into(),
            verdict_class: verdict_class.into(),
        })
    });

    MarketActivityResponse {
        interest_level,
        saves_last_7d: p.saves_last_7d,
        offers_last_7d: p.offers_last_7d,
        days_on_market: p.days_on_market,
        days_on_market_label,
        interest_label,
        area_trend_summary,
        price_vs_median,
    }
}
