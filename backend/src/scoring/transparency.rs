//! Transparency Score: composite 0-100 score from document completeness,
//! society quality, builder quality, and RERA status.

use serde::Serialize;

use crate::models::Property;
use crate::routes::enrichment::ReraInfo;

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

/// Compute a composite transparency score (0-100) from property data.
///
/// Weights:
/// - document_completeness: 30%
/// - society_quality: 25%
/// - builder_quality: 25%
/// - RERA status: 20%
pub fn compute_transparency_score(
    property: &Property,
    rera: Option<&ReraInfo>,
) -> TransparencyScore {
    let doc_score = property.document_completeness_score * 30.0;
    let society_score = property.society_quality_score * 25.0;
    let builder_score = property.builder_quality_score * 25.0;

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
            if r.total_project_cost_inr.is_some() {
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
