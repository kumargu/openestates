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
    subject_entity_types: Vec<String>,
    target_root_sources: Vec<String>,
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
        assert!(
            ontology.identity_bindings.iter().all(|binding| !binding
                .subject_entity_types
                .is_empty()
                && !binding.target_root_sources.is_empty()
                && !binding.edge_types.is_empty()),
            "identity bindings must define source, subject and relationship scopes"
        );
        ontology.identity_bindings
    })
}
#[derive(Debug, Clone)]
struct Membership {
    target: String,
    scope: String,
    reference: EvidenceRef,
    confidence: f32,
}
#[derive(Debug, Clone, Default)]
pub struct IdentityEvaluationIndex {
    entity_types: HashMap<String, String>,
    memberships: HashMap<(String, String), Vec<Membership>>,
    target_scopes: HashMap<String, HashSet<String>>,
    exclusive_scopes: HashSet<String>,
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
            exclusive_scopes: bindings()
                .iter()
                .filter(|binding| binding.exclusive)
                .flat_map(|binding| binding.edge_types.clone())
                .collect(),
            target_scopes: entities
                .iter()
                .filter_map(|entity| {
                    let source = entity.root_source.as_deref()?;
                    let scopes = bindings()
                        .iter()
                        .filter(|binding| {
                            binding.entity_type == entity.entity_type
                                && binding.target_root_sources.iter().any(|allowed| {
                                    crate::dag_config::normalize_source_type(allowed)
                                        == crate::dag_config::normalize_source_type(source)
                                })
                        })
                        .flat_map(|binding| binding.edge_types.clone())
                        .collect::<HashSet<_>>();
                    (!scopes.is_empty()).then(|| (entity.entity_id.clone(), scopes))
                })
                .collect(),
            snapshot_identity: snapshot_identity.to_string(),
            ..Self::default()
        };
        for edge in edges {
            if !crate::serving::admission::admit_identity_edge(edge)
                || !index.entity_types.contains_key(&edge.from_entity_id)
                || !index.entity_types.contains_key(&edge.to_entity_id)
            {
                continue;
            }
            for binding in bindings()
                .iter()
                .filter(|binding| binding.edge_types.contains(&edge.edge_type))
            {
                if index.entity_types.get(&edge.to_entity_id) != Some(&binding.entity_type)
                    || !binding
                        .subject_entity_types
                        .contains(&index.entity_types[&edge.from_entity_id])
                {
                    continue;
                }
                let (subject, target) = (&edge.from_entity_id, &edge.to_entity_id);
                index
                    .target_scopes
                    .entry(target.clone())
                    .or_default()
                    .insert(edge.edge_type.clone());
                index
                    .memberships
                    .entry((subject.clone(), binding.entity_type.clone()))
                    .or_default()
                    .push(Membership {
                        target: target.clone(),
                        scope: edge.edge_type.clone(),
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

    /// Serving validation uses the same evaluation and exact relationship receipt.
    pub(crate) fn relationship_admitted(&self, edge: &ServingEdgeRecord) -> Option<bool> {
        if !bindings()
            .iter()
            .any(|binding| binding.edge_types.contains(&edge.edge_type))
        {
            return None;
        }
        if !crate::serving::admission::admit_identity_edge(edge) {
            return Some(false);
        }
        let id = EvidenceId::Relationship(crate::serving::evidence::relationship_id(edge));
        Some(
            self.evaluate(
                &edge.from_entity_id,
                &edge.to_entity_id,
                &self.snapshot_identity,
            )
            .verified_matches
            .iter()
            .any(|witness| {
                witness
                    .evidence_refs
                    .iter()
                    .any(|reference| reference.evidence_id == id)
            }),
        )
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
            let Some(scopes) = self.target_scopes.get(expected) else {
                return BooleanEvaluation::unknown();
            };
            let mut matching = Vec::new();
            let mut exclusions = Vec::new();
            let mut exclusion_proven = true;
            for scope in scopes {
                let exclusive = self.exclusive_scopes.contains(scope);
                let scoped = memberships
                    .iter()
                    .filter(|membership| &membership.scope == scope)
                    .collect::<Vec<_>>();
                let targets = scoped
                    .iter()
                    .map(|membership| &membership.target)
                    .collect::<HashSet<_>>();
                if exclusive && targets.len() > 1 {
                    if scoped
                        .iter()
                        .any(|membership| membership.target == expected)
                    {
                        return BooleanEvaluation::unknown();
                    }
                    exclusion_proven = false;
                    continue;
                }
                matching.extend(
                    scoped
                        .iter()
                        .copied()
                        .filter(|membership| membership.target == expected),
                );
                exclusion_proven &= exclusive && targets.len() == 1;
                exclusions.extend(scoped);
            }
            matching.sort_by_key(|membership| membership.reference.stable_key());
            exclusions.sort_by_key(|membership| membership.reference.stable_key());
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
                // Negative identity requires unambiguous evidence in every known
                // scope of the target. Boundary and market-locality scopes differ.
                if !exclusion_proven {
                    return BooleanEvaluation::unknown();
                }
                (
                    false,
                    exclusions
                        .iter()
                        .map(|membership| membership.reference.clone())
                        .collect(),
                    exclusions
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
            algorithm_version: "catalog-identity-v2".to_string(),
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
