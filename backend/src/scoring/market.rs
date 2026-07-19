//! Market activity context: interest, days-on-market, and price-vs-median.
//!
//! This is market *context* derived from listing facts, not a hand-written
//! quality score. Buyer-facing quality now comes from evidence folds and the
//! livability brief, which read DAG-backed source facts.

use serde::Serialize;

use crate::models::{AreaProfile, Property};

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

/// Compute market activity response with display labels.
pub fn compute_market_activity(p: &Property, area: Option<&AreaProfile>) -> MarketActivityResponse {
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
