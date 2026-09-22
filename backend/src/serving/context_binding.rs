//! Offline, observation-backed context identity joins. No display-name recovery.
use super::{
    DerivedEvidence, EvidenceRef, ServingEdgeRecord, ServingEntityRecord, ServingFactRecord,
    PROVIDER_BINDING_EDGE,
};
use crate::knowledge::FactValue;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

pub const CONTEXT_TARGET_EDGE: &str = "context_target";
const ALGORITHM: &str = "context-binding-v1";

#[derive(Debug, Deserialize)]
pub(crate) struct ContextConfig {
    pub(crate) maximum_features: usize,
    pub(crate) geometry_fact_key: String,
    pub(crate) place_url_fact_key: String,
    pub(crate) attribute_fact_keys: Vec<String>,
    pub(crate) bindings: BTreeMap<String, ContextBinding>,
}
#[derive(Debug, Deserialize)]
pub(crate) struct ContextBinding {
    pub(crate) linked_entity_fact_keys: Vec<String>,
    pub(crate) attribute_fact_keys: Vec<String>,
}
pub(crate) fn config() -> &'static ContextConfig {
    static CONFIG: OnceLock<ContextConfig> = OnceLock::new();
    CONFIG.get_or_init(|| {
        #[derive(Deserialize)]
        struct Registry {
            property_context: ContextConfig,
        }
        let registry: Registry =
            crate::dag_config::load_json(&crate::dag_config::dag_root().join("fact_registry.json"))
                .expect("fact registry must bind property context");
        assert!(registry.property_context.maximum_features > 0);
        registry.property_context
    })
}

/// Replace bindings for this immutable snapshot using actual subject/source identity records.
/// Multiple provider IDs may name one canonical target; multiple canonical targets stay unbound.
pub fn materialize_context_bindings(
    entities: &[ServingEntityRecord],
    facts: &[ServingFactRecord],
    edges: &mut Vec<ServingEdgeRecord>,
    snapshot: &str,
) -> Result<(), String> {
    edges.retain(|edge| edge.edge_type != CONTEXT_TARGET_EDGE);
    let entity_ids = entities
        .iter()
        .map(|e| e.entity_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut providers = BTreeMap::<&str, Vec<&ServingEdgeRecord>>::new();
    for edge in edges
        .iter()
        .filter(|edge| edge.edge_type == PROVIDER_BINDING_EDGE)
    {
        providers.entry(&edge.to_entity_id).or_default().push(edge);
    }
    let observed = facts
        .iter()
        .filter(|fact| fact.observation.is_some() && fact.validate_observation().is_ok())
        .collect::<Vec<_>>();
    let mut urls = BTreeMap::<&str, Vec<&ServingFactRecord>>::new();
    let mut subjects = BTreeMap::<&str, Vec<&ServingFactRecord>>::new();
    for fact in &observed {
        subjects.entry(&fact.entity_id).or_default().push(fact);
        if fact.fact_key == config().place_url_fact_key {
            if let FactValue::Text(url) = &fact.value {
                urls.entry(url).or_default().push(fact);
            }
        }
    }
    let mut bindings = Vec::new();
    for fact in &observed {
        let Some(binding) = config().bindings.get(&fact.fact_key) else {
            continue;
        };
        let observation = fact.observation.as_ref().unwrap();
        let linked = subjects
            .get(fact.entity_id.as_str())
            .into_iter()
            .flatten()
            .filter(|candidate| {
                binding
                    .linked_entity_fact_keys
                    .contains(&candidate.fact_key)
            })
            .filter(|candidate| {
                candidate.observation.as_ref().is_some_and(|other| {
                    other.observation_id == observation.observation_id
                        || observation
                            .source_url
                            .as_ref()
                            .is_some_and(|url| other.source_url.as_ref() == Some(url))
                })
            })
            .filter_map(|candidate| match &candidate.value {
                FactValue::Text(id) => Some((id.as_str(), *candidate)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let candidates = if linked.is_empty() {
            observation
                .source_url
                .as_deref()
                .and_then(|url| urls.get(url))
                .into_iter()
                .flatten()
                .map(|candidate| (candidate.entity_id.as_str(), *candidate))
                .collect()
        } else {
            linked
        };
        let mut targets = BTreeMap::<String, (f32, Vec<EvidenceRef>)>::new();
        for (id, candidate) in candidates {
            if !entity_ids.contains(id) {
                continue;
            }
            let base =
                EvidenceRef::for_observation(snapshot, candidate.observation.as_ref().unwrap());
            if let Some(canonical_edges) = providers.get(id) {
                for edge in canonical_edges {
                    let Some(derivation) = &edge.derivation else {
                        continue;
                    };
                    let entry = targets
                        .entry(edge.from_entity_id.clone())
                        .or_insert((fact.confidence, Vec::new()));
                    entry.0 = entry.0.min(candidate.confidence).min(edge.confidence);
                    entry
                        .1
                        .extend([base.clone(), EvidenceRef::for_derivation(derivation)]);
                }
            } else {
                let entry = targets
                    .entry(id.to_string())
                    .or_insert((fact.confidence, Vec::new()));
                entry.0 = entry.0.min(candidate.confidence);
                entry.1.push(base);
            }
        }
        if targets.len() != 1 {
            continue;
        }
        let (target, (confidence, mut inputs)) = targets.into_iter().next().unwrap();
        inputs.push(EvidenceRef::for_observation(snapshot, observation));
        let derivation = DerivedEvidence::new(
            snapshot,
            &fact.entity_id,
            Some(target.clone()),
            CONTEXT_TARGET_EDGE,
            &fact.fact_key,
            None,
            None,
            ALGORITHM,
            confidence,
            inputs,
        )
        .map_err(|error| error.to_string())?;
        bindings.push(ServingEdgeRecord {
            from_entity_id: fact.entity_id.clone(),
            to_entity_id: target,
            edge_type: CONTEXT_TARGET_EDGE.to_string(),
            confidence,
            source_type: "Computed".to_string(),
            derivation: Some(derivation),
        });
    }
    bindings.sort_by(|a, b| {
        a.derivation
            .as_ref()
            .unwrap()
            .derivation_id
            .as_str()
            .cmp(b.derivation.as_ref().unwrap().derivation_id.as_str())
    });
    bindings.dedup();
    edges.extend(bindings);
    Ok(())
}
