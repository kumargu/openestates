//! Exact membership evaluation over promoted catalog records, independent of recall.
use super::evaluation::{BooleanEvaluation, VerifiedMatch};
use crate::serving::{
    EvidenceId, EvidenceRef, LoadedServingBundle, ServingEdgeRecord, ServingEntityRecord,
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

#[derive(Deserialize)]
struct IdentityBinding {
    entity_type: String,
    edge_types: Vec<String>,
    exclusive: bool,
}
fn bindings() -> &'static [IdentityBinding] {
    static BINDINGS: OnceLock<Vec<IdentityBinding>> = OnceLock::new();
    BINDINGS.get_or_init(|| {
        #[derive(Deserialize)]
        struct Ontology {
            identity_bindings: Vec<IdentityBinding>,
        }
        let ontology: Ontology =
            crate::dag_config::load_json(&crate::dag_config::dag_root().join("ontology.json"))
                .expect("ontology must define identity bindings");
        ontology.identity_bindings
    })
}
#[derive(Debug, Clone)]
struct Membership {
    target: String,
    reference: EvidenceRef,
    confidence: f32,
}
#[derive(Debug, Clone, Default)]
pub struct IdentityEvaluationIndex {
    entity_types: HashMap<String, String>,
    memberships: HashMap<(String, String), Vec<Membership>>,
    exclusive_types: HashSet<String>,
    snapshot_identity: String,
}
impl IdentityEvaluationIndex {
    pub fn from_bundle(bundle: &LoadedServingBundle) -> Self {
        Self::from_records(
            &bundle.entities,
            &bundle.edges,
            bundle.manifest.proof_snapshot_identity(),
        )
    }
    pub fn from_records(
        entities: &[ServingEntityRecord],
        edges: &[ServingEdgeRecord],
        snapshot_identity: &str,
    ) -> Self {
        let mut index = Self {
            entity_types: entities
                .iter()
                .map(|entity| (entity.entity_id.clone(), entity.entity_type.clone()))
                .collect(),
            exclusive_types: bindings()
                .iter()
                .filter(|binding| binding.exclusive)
                .map(|binding| binding.entity_type.clone())
                .collect(),
            snapshot_identity: snapshot_identity.to_string(),
            ..Self::default()
        };
        for edge in edges {
            if edge.source_type.trim().is_empty() || !edge.confidence.is_finite() {
                continue;
            }
            for binding in bindings()
                .iter()
                .filter(|binding| binding.edge_types.contains(&edge.edge_type))
            {
                let (subject, target) = if index.entity_types.get(&edge.to_entity_id)
                    == Some(&binding.entity_type)
                {
                    (&edge.from_entity_id, &edge.to_entity_id)
                } else if index.entity_types.get(&edge.from_entity_id) == Some(&binding.entity_type)
                {
                    (&edge.to_entity_id, &edge.from_entity_id)
                } else {
                    continue;
                };
                index
                    .memberships
                    .entry((subject.clone(), binding.entity_type.clone()))
                    .or_default()
                    .push(Membership {
                        target: target.clone(),
                        confidence: edge.confidence,
                        reference: EvidenceRef {
                            snapshot_identity: index.snapshot_identity.clone(),
                            subject_entity_id: subject.clone(),
                            evidence_id: EvidenceId::Relationship(
                                crate::serving::evidence::relationship_id(edge),
                            ),
                        },
                    });
            }
        }
        for memberships in index.memberships.values_mut() {
            memberships.sort_by(|a, b| {
                a.target
                    .cmp(&b.target)
                    .then_with(|| a.reference.stable_key().cmp(&b.reference.stable_key()))
            });
        }
        index
    }

    pub fn evaluate(&self, subject: &str, expected: &str, snapshot: &str) -> BooleanEvaluation {
        if snapshot != self.snapshot_identity {
            return BooleanEvaluation::unknown();
        }
        let Some(expected_type) = self.entity_types.get(expected) else {
            return BooleanEvaluation::unknown();
        };
        let Some(subject_type) = self.entity_types.get(subject) else {
            return BooleanEvaluation::unknown();
        };
        let (matches, references, confidence) = if subject_type == expected_type {
            (
                subject == expected,
                vec![EvidenceRef {
                    snapshot_identity: snapshot.to_string(),
                    subject_entity_id: subject.to_string(),
                    evidence_id: EvidenceId::Entity(subject.to_string()),
                }],
                None,
            )
        } else {
            let Some(memberships) = self
                .memberships
                .get(&(subject.to_string(), expected_type.clone()))
            else {
                return BooleanEvaluation::unknown();
            };
            let matching = memberships
                .iter()
                .filter(|membership| membership.target == expected)
                .collect::<Vec<_>>();
            if !matching.is_empty() {
                (
                    true,
                    matching
                        .iter()
                        .map(|membership| membership.reference.clone())
                        .collect(),
                    matching
                        .iter()
                        .map(|membership| membership.confidence)
                        .max_by(f32::total_cmp),
                )
            } else {
                // A missing edge never proves exclusion. A configured exclusive
                // assignment can do so only when there is one unambiguous target.
                let targets = memberships
                    .iter()
                    .map(|membership| &membership.target)
                    .collect::<HashSet<_>>();
                if !self.exclusive_types.contains(expected_type) || targets.len() != 1 {
                    return BooleanEvaluation::unknown();
                }
                (
                    false,
                    memberships
                        .iter()
                        .map(|membership| membership.reference.clone())
                        .collect(),
                    memberships
                        .iter()
                        .map(|membership| membership.confidence)
                        .max_by(f32::total_cmp),
                )
            }
        };
        let verified = VerifiedMatch {
            constraint: None,
            subject_entity_id: subject.to_string(),
            target_entity_id: Some(expected.to_string()),
            predicate: expected.to_string(),
            relation: if matches {
                "identity"
            } else {
                "distinct_identity"
            }
            .to_string(),
            metric: "entity_identity".to_string(),
            value: None,
            unit: None,
            observation_ids: Vec::new(),
            evidence_refs: references,
            fact_key: Some("entity_identity".to_string()),
            derived_evidence: None,
            algorithm_version: "catalog-identity-v1".to_string(),
            confidence,
            snapshot_identity: snapshot.to_string(),
        };
        if matches {
            BooleanEvaluation::satisfied(vec![verified])
        } else {
            BooleanEvaluation::unsatisfied_with(vec![verified])
        }
    }
}
