//! Transparency Score: buyer-facing trust summary derived from shared scoring
//! policy signals.

use serde::Serialize;

use crate::models::Property;
use crate::routes::enrichment::ReraInfo;

use super::policy::{CandidateScore, FactAvailability};

#[derive(Debug, Clone, Serialize)]
pub struct TransparencyComponent {
    pub label: String,
    pub score: f64,
    pub max_score: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransparencyScore {
    pub overall: f64,
    pub components: Vec<TransparencyComponent>,
    pub explainer: String,
}

pub fn compute_transparency_score(
    property: &Property,
    rera: Option<&ReraInfo>,
    policy_score: Option<&CandidateScore>,
) -> TransparencyScore {
    if let Some(policy_score) = policy_score {
        return transparency_from_policy_score(policy_score);
    }

    let doc_score = property.document_completeness_score.unwrap_or(0.0) * 30.0;
    let society_score = property.society_quality_score.unwrap_or(0.0) * 25.0;
    let builder_score = property.builder_quality_score.unwrap_or(0.0) * 25.0;

    // RERA: registered = full credit, unregistered = 0
    let rera_raw = match rera {
        Some(r) if r.registered => {
            // Bonus for additional RERA data completeness
            let mut base = 0.8_f64;
            if r.registration_number.is_some() {
                base += 0.05;
            }
            if r.completion_date.is_some() {
                base += 0.05;
            }
            if r.complaints_count.is_some() {
                base += 0.05;
            }
            if r.total_units.is_some() {
                base += 0.05;
            }
            base.min(1.0)
        }
        Some(_) => 0.2, // present but not registered
        None => 0.0,    // no RERA data at all
    };
    let rera_score = rera_raw * 20.0;

    let overall = (doc_score + society_score + builder_score + rera_score).round();
    let overall = overall.clamp(0.0, 100.0);

    let explainer = if overall >= 80.0 {
        "High transparency — strong documentation, verified builder, and RERA registered.".into()
    } else if overall >= 60.0 {
        "Good transparency — most signals verified, some gaps remain.".into()
    } else if overall >= 40.0 {
        "Moderate transparency — several data points missing or unverified.".into()
    } else {
        "Low transparency — significant verification gaps. Proceed with caution.".into()
    };

    let components = vec![
        TransparencyComponent {
            label: "Document completeness".into(),
            score: doc_score,
            max_score: 30.0,
        },
        TransparencyComponent {
            label: "Society quality".into(),
            score: society_score,
            max_score: 25.0,
        },
        TransparencyComponent {
            label: "Builder reputation".into(),
            score: builder_score,
            max_score: 25.0,
        },
        TransparencyComponent {
            label: "RERA status".into(),
            score: rera_score,
            max_score: 20.0,
        },
    ];

    TransparencyScore {
        overall,
        components,
        explainer,
    }
}

fn transparency_from_policy_score(policy_score: &CandidateScore) -> TransparencyScore {
    let observed_count = policy_score
        .signals
        .iter()
        .filter(|signal| signal.availability != FactAvailability::Missing)
        .count();
    let missing_count = policy_score.signals.len().saturating_sub(observed_count);
    let overall = if observed_count == 0 {
        0.0
    } else {
        (policy_score.total_score * 100.0).round().clamp(0.0, 100.0)
    };

    let components = policy_score
        .signals
        .iter()
        .map(|signal| TransparencyComponent {
            label: signal_label(&signal.signal_id).to_string(),
            score: if signal.availability == FactAvailability::Missing {
                0.0
            } else {
                (signal.score * 100.0).round()
            },
            max_score: 100.0,
        })
        .collect();

    let explainer = if observed_count == 0 {
        "Transparency is not scored yet because serving evidence is missing.".into()
    } else if missing_count > 0 {
        format!(
            "Transparency is based on {observed_count} observed signal{}; {missing_count} evidence gap{} remain.",
            plural(observed_count),
            plural(missing_count)
        )
    } else if overall >= 75.0 {
        "Strong transparency from shared proof and safety signals.".into()
    } else if overall >= 50.0 {
        "Moderate transparency from shared proof and safety signals.".into()
    } else {
        "Limited transparency from the currently observed proof and safety signals.".into()
    };

    TransparencyScore {
        overall,
        components,
        explainer,
    }
}

fn signal_label(signal_id: &str) -> &str {
    match signal_id {
        "proof_strength" => "Proof strength",
        "legal_timeline_safety" => "Legal and timeline safety",
        "access_strength" => "Access strength",
        "price_value" => "Price value",
        _ => signal_id,
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}
