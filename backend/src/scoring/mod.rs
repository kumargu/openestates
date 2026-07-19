//! Scoring module.
//!
//! Buyer-facing quality signals now come from DAG-backed evidence folds and the
//! livability brief (see `crate::livability_brief`). This module keeps only:
//! - `market`: listing-derived market activity context (interest, days-on-market).
//! - `transparency`: an internal composite trust score used for ranking/decisions,
//!   not rendered as a raw number to buyers.
//!
//! The old hand-written `CompareThemes`/`compute_tradeoffs` heuristics over seed
//! Property fields were removed in favor of source-backed facts.

mod market;
mod transparency;

pub use market::{compute_market_activity, MarketActivityResponse, PriceVsMedian};
pub use transparency::{compute_transparency_score, TransparencyScore};
