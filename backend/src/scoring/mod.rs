//! Scoring module: computes themes, tradeoffs, and market activity for properties.
//! KG-facts-first: checks knowledge graph for pre-scored facts before falling back
//! to Property struct field thresholds.

mod themes;
mod transparency;

pub use themes::{
    compute_market_activity, compute_themes, compute_tradeoffs, CompareThemes,
    MarketActivityResponse, TradeoffsResponse,
};
pub use transparency::{compute_transparency_score, TransparencyScore};
