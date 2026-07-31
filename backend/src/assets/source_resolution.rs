use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use super::{CanonicalSocietyRows, SourceEntitySeed};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceEntityResolutionScope {
    #[default]
    Production,
    Scoped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceEntityResolutionError {
    Ambiguous {
        selector: String,
        candidates: Vec<String>,
    },
    Conflicting {
        entity_id: String,
        project_key: String,
        candidates: Vec<String>,
    },
    Unresolved {
        entity_id: String,
        project_key: Option<String>,
    },
}

impl fmt::Display for SourceEntityResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ambiguous {
                selector,
                candidates,
            } => write!(
                f,
                "source entity selector {selector:?} is ambiguous across {}",
                candidates.join(", ")
            ),
            Self::Conflicting {
                entity_id,
                project_key,
                candidates,
            } => write!(
                f,
                "source entity selectors {entity_id:?} and {project_key:?} conflict across {}",
                candidates.join(", ")
            ),
            Self::Unresolved {
                entity_id,
                project_key,
            } => write!(
                f,
                "source entity {entity_id:?} with project key {:?} is unresolved",
                project_key.as_deref().unwrap_or("")
            ),
        }
    }
}

impl std::error::Error for SourceEntityResolutionError {}

pub struct SourceEntityResolver {
    allowed_ids: HashSet<String>,
    direct_ids: HashMap<String, BTreeSet<String>>,
    aliases: HashMap<String, BTreeSet<String>>,
    project_keys: HashMap<String, BTreeSet<String>>,
}

impl SourceEntityResolver {
    pub fn new(
        canonical: &CanonicalSocietyRows,
        scoped_seeds: &[SourceEntitySeed],
        scope: SourceEntityResolutionScope,
    ) -> Self {
        let canonical_ids = canonical
            .entities
            .iter()
            .filter(|entity| entity.entity_type == "society")
            .map(|entity| entity.entity_id.clone())
            .collect::<HashSet<_>>();
        let scoped_ids = scoped_seeds
            .iter()
            .map(|seed| seed.entity_id.clone())
            .collect::<HashSet<_>>();
        let allowed_ids = match scope {
            SourceEntityResolutionScope::Production => canonical_ids,
            SourceEntityResolutionScope::Scoped => scoped_ids,
        };

        let mut resolver = Self {
            allowed_ids,
            direct_ids: HashMap::new(),
            aliases: HashMap::new(),
            project_keys: HashMap::new(),
        };
        for entity_id in resolver.allowed_ids.clone() {
            add_mapping(&mut resolver.direct_ids, &entity_id, &entity_id);
        }
        for mapping in &canonical.mappings {
            if !resolver.allowed_ids.contains(&mapping.canonical_entity_id) {
                continue;
            }
            add_mapping(
                &mut resolver.project_keys,
                &mapping.project_key,
                &mapping.canonical_entity_id,
            );
            if let Some(alias) = mapping.alias_entity_id.as_deref() {
                add_mapping(&mut resolver.aliases, alias, &mapping.canonical_entity_id);
            }
        }
        if scope == SourceEntityResolutionScope::Scoped {
            for seed in scoped_seeds {
                if let Some(alias) = seed.alias_entity_id.as_deref() {
                    add_mapping(&mut resolver.aliases, alias, &seed.entity_id);
                }
                if let Some(project_key) = seed.project_key.as_deref() {
                    add_mapping(&mut resolver.project_keys, project_key, &seed.entity_id);
                }
            }
        }
        resolver
    }

    pub fn resolve(
        &self,
        entity_id: &str,
        project_key: Option<&str>,
    ) -> Result<String, SourceEntityResolutionError> {
        let entity_candidate = self
            .direct_ids
            .get(entity_id)
            .or_else(|| self.aliases.get(entity_id))
            .map(|candidates| unique_candidate(entity_id, candidates))
            .transpose()?;
        let project_candidate = project_key
            .and_then(|key| {
                self.project_keys
                    .get(key)
                    .map(|candidates| (key, candidates))
            })
            .map(|(key, candidates)| unique_candidate(key, candidates))
            .transpose()?;

        match (entity_candidate, project_candidate) {
            (Some(entity), Some(project)) if entity != project => {
                let mut candidates = vec![entity, project];
                candidates.sort();
                candidates.dedup();
                Err(SourceEntityResolutionError::Conflicting {
                    entity_id: entity_id.to_string(),
                    project_key: project_key.unwrap_or_default().to_string(),
                    candidates,
                })
            }
            (Some(entity), _) => Ok(entity),
            (None, Some(project)) => Ok(project),
            (None, None) => Err(SourceEntityResolutionError::Unresolved {
                entity_id: entity_id.to_string(),
                project_key: project_key.map(str::to_string),
            }),
        }
    }
}

fn add_mapping(index: &mut HashMap<String, BTreeSet<String>>, selector: &str, entity_id: &str) {
    index
        .entry(selector.to_string())
        .or_default()
        .insert(entity_id.to_string());
}

fn unique_candidate(
    selector: &str,
    candidates: &BTreeSet<String>,
) -> Result<String, SourceEntityResolutionError> {
    if candidates.len() != 1 {
        return Err(SourceEntityResolutionError::Ambiguous {
            selector: selector.to_string(),
            candidates: candidates.iter().cloned().collect(),
        });
    }
    Ok(candidates.iter().next().cloned().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{CanonicalSocietyRows, ReraCanonicalMappingRecord};

    fn seed(entity_id: &str, alias: Option<&str>, project_key: Option<&str>) -> SourceEntitySeed {
        SourceEntitySeed {
            entity_id: entity_id.to_string(),
            alias_entity_id: alias.map(str::to_string),
            name: entity_id.to_string(),
            area: None,
            city: Some("Bengaluru".to_string()),
            project_key: project_key.map(str::to_string),
            latitude: None,
            longitude: None,
        }
    }

    #[test]
    fn scoped_resolver_accepts_only_scoped_seed_identities() {
        let canonical = CanonicalSocietyRows {
            entities: Vec::new(),
            edges: Vec::new(),
            mappings: Vec::new(),
        };
        let seeds = vec![seed(
            "society:rera-seed",
            Some("society:seed-alias"),
            Some("PRM-SEED"),
        )];
        let resolver =
            SourceEntityResolver::new(&canonical, &seeds, SourceEntityResolutionScope::Scoped);

        assert_eq!(
            resolver.resolve("society:rera-seed", None).unwrap(),
            "society:rera-seed"
        );
        assert_eq!(
            resolver.resolve("society:seed-alias", None).unwrap(),
            "society:rera-seed"
        );
        assert_eq!(
            resolver.resolve("unknown", Some("PRM-SEED")).unwrap(),
            "society:rera-seed"
        );
        assert!(matches!(
            resolver.resolve("unknown", None),
            Err(SourceEntityResolutionError::Unresolved { .. })
        ));
    }

    #[test]
    fn scoped_resolver_rejects_unselected_canonical_entity() {
        let canonical = CanonicalSocietyRows {
            entities: Vec::new(),
            edges: Vec::new(),
            mappings: vec![ReraCanonicalMappingRecord {
                project_key: "PRM-GLOBAL".to_string(),
                canonical_entity_id: "society:global".to_string(),
                alias_entity_id: Some("society:global-alias".to_string()),
                project_name: "Global".to_string(),
                registration_number: None,
                ack_number: None,
            }],
        };
        let resolver = SourceEntityResolver::new(
            &canonical,
            &[seed("society:scoped", None, None)],
            SourceEntityResolutionScope::Scoped,
        );

        assert!(resolver
            .resolve("society:global-alias", Some("PRM-GLOBAL"))
            .is_err());
    }

    #[test]
    fn ambiguous_project_key_fails_closed() {
        let canonical = CanonicalSocietyRows {
            entities: Vec::new(),
            edges: Vec::new(),
            mappings: Vec::new(),
        };
        let seeds = vec![
            seed("society:a", None, Some("PRM-SAME")),
            seed("society:b", None, Some("PRM-SAME")),
        ];
        let resolver =
            SourceEntityResolver::new(&canonical, &seeds, SourceEntityResolutionScope::Scoped);

        assert!(matches!(
            resolver.resolve("unknown", Some("PRM-SAME")),
            Err(SourceEntityResolutionError::Ambiguous { .. })
        ));
    }
}
