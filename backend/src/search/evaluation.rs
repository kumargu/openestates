use serde::{Deserialize, Serialize};

use crate::models::Property;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryOption {
    pub property_id: String,
    pub society_id: String,
    pub bhk: Option<u32>,
    pub price_min: Option<u64>,
    pub price_max: Option<u64>,
    pub size_sqft: Option<u32>,
    pub evidence_reference: String,
}

impl InventoryOption {
    pub fn from_property(property: &Property) -> Self {
        let exact_price = (property.price > 0).then_some(property.price);
        Self {
            property_id: property.id.clone(),
            society_id: property.society_id.clone(),
            bhk: (property.bhk > 0).then_some(property.bhk),
            price_min: property.price_min.or(exact_price),
            price_max: property.price_max.or(exact_price),
            size_sqft: (property.super_builtup_sqft > 0).then_some(property.super_builtup_sqft),
            evidence_reference: format!("inventory-option:{}", property.id),
        }
    }

    pub fn matches_bhk(&self, expected: u32) -> bool {
        self.bhk == Some(expected)
    }

    pub fn matches_budget(&self, min: Option<u64>, max: Option<u64>) -> bool {
        let (Some(option_min), Some(option_max)) = (self.price_min, self.price_max) else {
            return false;
        };
        min.is_none_or(|bound| option_max >= bound) && max.is_none_or(|bound| option_min <= bound)
    }
}

/// Evidence produced by exact predicate evaluation after candidate recall.
/// Ranking and proof projections must use this record rather than re-deriving
/// a claim from query text or recall membership.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedMatch {
    pub subject_entity_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_entity_id: Option<String>,
    pub predicate: String,
    pub relation: String,
    pub metric: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observation_ids: Vec<String>,
    pub algorithm_version: String,
    pub confidence: f32,
    pub snapshot_identity: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationEvidence {
    pub predicate: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceGap {
    pub predicate: String,
    pub reason: String,
}

/// Four-state predicate result. `Unknown` and `Unsupported` are deliberately
/// not booleans: negation must preserve both states instead of turning missing
/// evidence into a verified match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", content = "detail", rename_all = "camelCase")]
pub enum PredicateEvaluation {
    Satisfied(VerifiedMatch),
    Unsatisfied(EvaluationEvidence),
    Unknown(EvidenceGap),
    Unsupported(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BooleanEvaluation {
    pub state: EvaluationState,
    pub verified_matches: Vec<VerifiedMatch>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvaluationState {
    Satisfied,
    Unsatisfied,
    Unknown,
    Unsupported,
}

impl BooleanEvaluation {
    pub fn satisfied(matches: Vec<VerifiedMatch>) -> Self {
        Self {
            state: EvaluationState::Satisfied,
            verified_matches: matches,
        }
    }

    pub fn from_predicate(evaluation: PredicateEvaluation) -> Self {
        match evaluation {
            PredicateEvaluation::Satisfied(verified) => Self::satisfied(vec![verified]),
            PredicateEvaluation::Unsatisfied(_) => Self {
                state: EvaluationState::Unsatisfied,
                verified_matches: Vec::new(),
            },
            PredicateEvaluation::Unknown(_) => Self {
                state: EvaluationState::Unknown,
                verified_matches: Vec::new(),
            },
            PredicateEvaluation::Unsupported(_) => Self {
                state: EvaluationState::Unsupported,
                verified_matches: Vec::new(),
            },
        }
    }

    pub fn is_satisfied(&self) -> bool {
        self.state == EvaluationState::Satisfied
    }

    pub fn negated(mut self) -> Self {
        self.state = match self.state {
            EvaluationState::Satisfied => EvaluationState::Unsatisfied,
            EvaluationState::Unsatisfied => EvaluationState::Satisfied,
            EvaluationState::Unknown => EvaluationState::Unknown,
            EvaluationState::Unsupported => EvaluationState::Unsupported,
        };
        if self.state != EvaluationState::Satisfied {
            self.verified_matches.clear();
        }
        self
    }

    pub fn all(evaluations: impl IntoIterator<Item = Self>) -> Self {
        combine(evaluations, true)
    }

    pub fn any(evaluations: impl IntoIterator<Item = Self>) -> Self {
        combine(evaluations, false)
    }
}

fn combine(
    evaluations: impl IntoIterator<Item = BooleanEvaluation>,
    all: bool,
) -> BooleanEvaluation {
    let evaluations = evaluations.into_iter().collect::<Vec<_>>();
    if evaluations.is_empty() {
        return BooleanEvaluation {
            state: if all {
                EvaluationState::Satisfied
            } else {
                EvaluationState::Unsatisfied
            },
            verified_matches: Vec::new(),
        };
    }

    let decisive = if all {
        evaluations
            .iter()
            .find(|evaluation| evaluation.state == EvaluationState::Unsatisfied)
            .map(|_| EvaluationState::Unsatisfied)
    } else {
        evaluations
            .iter()
            .find(|evaluation| evaluation.state == EvaluationState::Satisfied)
            .map(|_| EvaluationState::Satisfied)
    };
    let state = decisive.unwrap_or_else(|| {
        if evaluations
            .iter()
            .any(|evaluation| evaluation.state == EvaluationState::Unknown)
        {
            EvaluationState::Unknown
        } else if evaluations
            .iter()
            .any(|evaluation| evaluation.state == EvaluationState::Unsupported)
        {
            EvaluationState::Unsupported
        } else if all {
            EvaluationState::Satisfied
        } else {
            EvaluationState::Unsatisfied
        }
    });
    let verified_matches = if state == EvaluationState::Satisfied {
        evaluations
            .into_iter()
            .filter(|evaluation| all || evaluation.state == EvaluationState::Satisfied)
            .flat_map(|evaluation| evaluation.verified_matches)
            .collect()
    } else {
        Vec::new()
    };
    BooleanEvaluation {
        state,
        verified_matches,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(state: EvaluationState) -> BooleanEvaluation {
        BooleanEvaluation {
            state,
            verified_matches: Vec::new(),
        }
    }

    #[test]
    fn negation_preserves_unknown_and_unsupported() {
        assert_eq!(
            state(EvaluationState::Unknown).negated().state,
            EvaluationState::Unknown
        );
        assert_eq!(
            state(EvaluationState::Unsupported).negated().state,
            EvaluationState::Unsupported
        );
    }

    #[test]
    fn boolean_composition_is_fail_closed() {
        assert_eq!(
            BooleanEvaluation::all([
                state(EvaluationState::Satisfied),
                state(EvaluationState::Unknown),
            ])
            .state,
            EvaluationState::Unknown
        );
        assert_eq!(
            BooleanEvaluation::any([
                state(EvaluationState::Unsatisfied),
                state(EvaluationState::Unknown),
            ])
            .state,
            EvaluationState::Unknown
        );
    }
}
