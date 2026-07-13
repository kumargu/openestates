use std::collections::HashMap;

use daggy::petgraph::algo::toposort;
use daggy::{Dag, NodeIndex};
use serde::{Deserialize, Serialize};

use super::{AssetId, AssetStage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshCadence {
    Manual,
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    OnChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostTier {
    Free,
    Cheap,
    Moderate,
    Expensive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustTier {
    Root,
    Authoritative,
    Support,
    Derived,
    Serving,
}

/// Durable definition for a data product in the OpenEstates DAG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetDefinition {
    pub id: AssetId,
    pub stage: AssetStage,
    pub description: String,
    pub dependencies: Vec<AssetId>,
    pub refresh: RefreshCadence,
    pub cost_tier: CostTier,
    pub trust_tier: TrustTier,
}

impl AssetDefinition {
    pub fn new(
        id: AssetId,
        stage: AssetStage,
        description: impl Into<String>,
        dependencies: Vec<AssetId>,
        refresh: RefreshCadence,
        cost_tier: CostTier,
        trust_tier: TrustTier,
    ) -> Self {
        Self {
            id,
            stage,
            description: description.into(),
            dependencies,
            refresh,
            cost_tier,
            trust_tier,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AssetRegistry {
    definitions: Vec<AssetDefinition>,
}

impl AssetRegistry {
    pub fn new(definitions: Vec<AssetDefinition>) -> Result<Self, RegistryError> {
        let registry = Self { definitions };
        registry.validate()?;
        Ok(registry)
    }

    pub fn definitions(&self) -> &[AssetDefinition] {
        &self.definitions
    }

    pub fn get(&self, id: &AssetId) -> Option<&AssetDefinition> {
        self.definitions
            .iter()
            .find(|definition| &definition.id == id)
    }

    pub fn topological_order(&self) -> Result<Vec<AssetId>, RegistryError> {
        let dag = self.build_dag()?;
        let ordered = toposort(dag.graph(), None)
            .map_err(|cycle| RegistryError::Cycle(dag.graph()[cycle.node_id()].clone()))?;
        Ok(ordered
            .into_iter()
            .map(|node| dag.graph()[node].clone())
            .collect())
    }

    fn validate(&self) -> Result<(), RegistryError> {
        let by_id = self.by_id()?;

        for definition in &self.definitions {
            for dependency in &definition.dependencies {
                if dependency == &definition.id {
                    return Err(RegistryError::SelfDependency(definition.id.clone()));
                }
                if !by_id.contains_key(dependency) {
                    return Err(RegistryError::MissingDependency {
                        asset_id: definition.id.clone(),
                        dependency: dependency.clone(),
                    });
                }
            }
        }

        self.build_dag()?;
        Ok(())
    }

    fn by_id(&self) -> Result<HashMap<AssetId, &AssetDefinition>, RegistryError> {
        let mut by_id = HashMap::new();
        for definition in &self.definitions {
            if by_id.insert(definition.id.clone(), definition).is_some() {
                return Err(RegistryError::DuplicateAsset(definition.id.clone()));
            }
        }
        Ok(by_id)
    }

    fn build_dag(&self) -> Result<Dag<AssetId, ()>, RegistryError> {
        let by_id = self.by_id()?;
        let mut dag = Dag::<AssetId, ()>::with_capacity(self.definitions.len(), 0);
        let mut nodes: HashMap<AssetId, NodeIndex> = HashMap::new();

        for definition in &self.definitions {
            let node = dag.add_node(definition.id.clone());
            nodes.insert(definition.id.clone(), node);
        }

        for definition in &self.definitions {
            let asset_node = nodes[&definition.id];
            for dependency in &definition.dependencies {
                if dependency == &definition.id {
                    return Err(RegistryError::SelfDependency(definition.id.clone()));
                }
                if !by_id.contains_key(dependency) {
                    return Err(RegistryError::MissingDependency {
                        asset_id: definition.id.clone(),
                        dependency: dependency.clone(),
                    });
                }

                let dependency_node = nodes[dependency];
                dag.add_edge(dependency_node, asset_node, ())
                    .map_err(|_| RegistryError::Cycle(definition.id.clone()))?;
            }
        }

        Ok(dag)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    DuplicateAsset(AssetId),
    MissingDependency {
        asset_id: AssetId,
        dependency: AssetId,
    },
    SelfDependency(AssetId),
    Cycle(AssetId),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateAsset(asset_id) => write!(f, "duplicate asset id: {asset_id}"),
            Self::MissingDependency {
                asset_id,
                dependency,
            } => write!(f, "asset {asset_id} depends on unknown asset {dependency}"),
            Self::SelfDependency(asset_id) => write!(f, "asset {asset_id} depends on itself"),
            Self::Cycle(asset_id) => write!(f, "asset cycle detected at {asset_id}"),
        }
    }
}

impl std::error::Error for RegistryError {}

pub fn default_openestates_registry() -> AssetRegistry {
    AssetRegistry::new(vec![
        asset(
            "rera_registry_monthly",
            AssetStage::Raw,
            "Monthly RERA registry snapshot for canonical project discovery.",
            &[],
            RefreshCadence::Monthly,
            CostTier::Free,
            TrustTier::Root,
        ),
        asset(
            "canonical_society_nodes",
            AssetStage::Gold,
            "RERA-rooted canonical society identities and aliases.",
            &["rera_registry_monthly"],
            RefreshCadence::OnChange,
            CostTier::Free,
            TrustTier::Authoritative,
        ),
        asset(
            "rera_legal_facts",
            AssetStage::Silver,
            "RERA proof facts such as registration, land area, status, and builder.",
            &["rera_registry_monthly", "canonical_society_nodes"],
            RefreshCadence::OnChange,
            CostTier::Free,
            TrustTier::Root,
        ),
        asset(
            "reddit_threads_daily",
            AssetStage::Raw,
            "Daily Reddit posts and comments for hot Bengaluru real-estate themes.",
            &["canonical_society_nodes"],
            RefreshCadence::Daily,
            CostTier::Free,
            TrustTier::Support,
        ),
        asset(
            "reddit_resident_facts",
            AssetStage::Silver,
            "Resident-support facts extracted from Reddit evidence.",
            &["reddit_threads_daily", "canonical_society_nodes"],
            RefreshCadence::OnChange,
            CostTier::Cheap,
            TrustTier::Support,
        ),
        asset(
            "google_review_facts",
            AssetStage::Silver,
            "Review-derived support facts for maintenance, amenities, and liveability.",
            &["canonical_society_nodes"],
            RefreshCadence::Weekly,
            CostTier::Cheap,
            TrustTier::Support,
        ),
        asset(
            "kg_society_view",
            AssetStage::Gold,
            "Versioned society KG view merged by source precedence and fact policy.",
            &[
                "canonical_society_nodes",
                "rera_legal_facts",
                "reddit_resident_facts",
                "google_review_facts",
            ],
            RefreshCadence::OnChange,
            CostTier::Free,
            TrustTier::Derived,
        ),
        asset(
            "search_serving_bundle",
            AssetStage::Serving,
            "Local request-path bundle for KG facts, schema config, aliases, and indexes.",
            &["kg_society_view"],
            RefreshCadence::OnChange,
            CostTier::Free,
            TrustTier::Serving,
        ),
    ])
    .expect("default asset registry is valid")
}

fn asset(
    id: &str,
    stage: AssetStage,
    description: &str,
    dependencies: &[&str],
    refresh: RefreshCadence,
    cost_tier: CostTier,
    trust_tier: TrustTier,
) -> AssetDefinition {
    AssetDefinition::new(
        AssetId::new(id).expect("valid static asset id"),
        stage,
        description,
        dependencies
            .iter()
            .map(|dependency| AssetId::new(*dependency).expect("valid static dependency id"))
            .collect(),
        refresh,
        cost_tier,
        trust_tier,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_orders_roots_before_serving_bundle() {
        let registry = default_openestates_registry();
        let ordered = registry.topological_order().unwrap();
        let rera_pos = position(&ordered, "rera_registry_monthly");
        let kg_pos = position(&ordered, "kg_society_view");
        let serving_pos = position(&ordered, "search_serving_bundle");

        assert!(rera_pos < kg_pos);
        assert!(kg_pos < serving_pos);
    }

    #[test]
    fn registry_rejects_missing_dependency() {
        let result = AssetRegistry::new(vec![asset(
            "search_serving_bundle",
            AssetStage::Serving,
            "bad test asset",
            &["missing_kg_view"],
            RefreshCadence::OnChange,
            CostTier::Free,
            TrustTier::Serving,
        )]);

        assert!(matches!(
            result,
            Err(RegistryError::MissingDependency { .. })
        ));
    }

    fn position(ordered: &[AssetId], id: &str) -> usize {
        let id = AssetId::new(id).unwrap();
        ordered
            .iter()
            .position(|asset_id| asset_id == &id)
            .expect("asset id in registry")
    }
}
