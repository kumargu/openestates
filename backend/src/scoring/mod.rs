//! Scoring module: computes themes, tradeoffs, and market activity for properties.
//! KG-facts-first: checks knowledge graph for pre-scored facts before falling back
//! to seed data thresholds.

mod themes;
mod transparency;

pub use themes::{
    CompareThemes, MarketActivityResponse, TradeoffsResponse, compute_market_activity,
    compute_themes, compute_tradeoffs,
};
pub use transparency::{TransparencyScore, compute_transparency_score};
