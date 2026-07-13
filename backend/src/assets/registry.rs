use std::collections::HashMap;

use daggy::petgraph::algo::toposort;
use daggy::{Dag, NodeIndex};
use serde::{Deserialize, Serialize};

use super::{AssetId, AssetPartition, AssetStage};

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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AssetPartitionPolicy {
    #[default]
    Global,
    RunPartition,
    Composite {
        coordinates: Vec<PartitionCoordinate>,
    },
}

impl AssetPartitionPolicy {
    pub fn global() -> Self {
        Self::Global
    }

    pub fn run_partition() -> Self {
        Self::RunPartition
    }

    pub fn from_run_keys(keys: &[&str]) -> Self {
        Self::Composite {
            coordinates: keys
                .iter()
                .map(|key| PartitionCoordinate::from_run(*key))
                .collect(),
        }
    }

    pub fn from_run_keys_with_static(
        run_keys: &[&str],
        static_coordinates: &[(&str, &str)],
    ) -> Self {
        let mut coordinates: Vec<PartitionCoordinate> = run_keys
            .iter()
            .map(|key| PartitionCoordinate::from_run(*key))
            .collect();
        coordinates.extend(
            static_coordinates
                .iter()
                .map(|(key, value)| PartitionCoordinate::static_value(*key, *value)),
        );
        Self::Composite { coordinates }
    }

    pub fn resolve(
        &self,
        asset_id: &AssetId,
        run_partition: &AssetPartition,
    ) -> Result<AssetPartition, PartitionResolutionError> {
        match self {
            Self::Global => Ok(AssetPartition::global()),
            Self::RunPartition => Ok(run_partition.clone()),
            Self::Composite { coordinates } => {
                let mut parts = Vec::with_capacity(coordinates.len());
                for coordinate in coordinates {
                    match coordinate {
                        PartitionCoordinate::FromRun { key } => {
                            let Some(value) = run_partition.value(key) else {
                                return Err(PartitionResolutionError::MissingRunPartitionKey {
                                    asset_id: asset_id.clone(),
                                    key: key.clone(),
                                    run_partition: run_partition.clone(),
                                });
                            };
                            parts.push((key.clone(), value.to_string()));
                        }
                        PartitionCoordinate::Static { key, value } => {
                            parts.push((key.clone(), value.clone()));
                        }
                    }
                }
                Ok(AssetPartition::new(parts))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PartitionCoordinate {
    FromRun { key: String },
    Static { key: String, value: String },
}

impl PartitionCoordinate {
    pub fn from_run(key: impl Into<String>) -> Self {
        Self::FromRun { key: key.into() }
    }

    pub fn static_value(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self::Static {
            key: key.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartitionResolutionError {
    UnknownAsset {
        asset_id: AssetId,
    },
    MissingRunPartitionKey {
        asset_id: AssetId,
        key: String,
        run_partition: AssetPartition,
    },
}

impl std::fmt::Display for PartitionResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownAsset { asset_id } => {
                write!(f, "asset {asset_id} is not registered in the DAG")
            }
            Self::MissingRunPartitionKey {
                asset_id,
                key,
                run_partition,
            } => write!(
                f,
                "asset {asset_id} requires run partition key {key}, got {run_partition:?}"
            ),
        }
    }
}

impl std::error::Error for PartitionResolutionError {}

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
    #[serde(default)]
    pub partition_policy: AssetPartitionPolicy,
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
            partition_policy: AssetPartitionPolicy::default(),
        }
    }

    pub fn with_partition_policy(mut self, partition_policy: AssetPartitionPolicy) -> Self {
        self.partition_policy = partition_policy;
        self
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

    pub fn partition_for(
        &self,
        asset_id: &AssetId,
        run_partition: &AssetPartition,
    ) -> Result<AssetPartition, PartitionResolutionError> {
        let definition =
            self.get(asset_id)
                .ok_or_else(|| PartitionResolutionError::UnknownAsset {
                    asset_id: asset_id.clone(),
                })?;
        definition.partition_policy.resolve(asset_id, run_partition)
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
        )
        .with_partition_policy(AssetPartitionPolicy::from_run_keys(&["dt", "subreddit"])),
        asset(
            "reddit_resident_facts",
            AssetStage::Silver,
            "Resident-support facts extracted from Reddit evidence.",
            &["reddit_threads_daily", "canonical_society_nodes"],
            RefreshCadence::OnChange,
            CostTier::Cheap,
            TrustTier::Support,
        )
        .with_partition_policy(AssetPartitionPolicy::from_run_keys_with_static(
            &["dt"],
            &[("source", "reddit")],
        )),
        asset(
            "google_review_facts",
            AssetStage::Silver,
            "Review-derived support facts for maintenance, amenities, and liveability.",
            &["canonical_society_nodes"],
            RefreshCadence::Weekly,
            CostTier::Cheap,
            TrustTier::Support,
        )
        .with_partition_policy(AssetPartitionPolicy::from_run_keys_with_static(
            &["dt"],
            &[("source", "google")],
        )),
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
    fn default_registry_resolves_mixed_asset_partitions() {
        let registry = default_openestates_registry();
        let run_partition =
            AssetPartition::new([("dt", "2026-07-13"), ("subreddit", "BangaloreRealEstates")]);

        assert_eq!(
            registry
                .partition_for(
                    &AssetId::new("canonical_society_nodes").unwrap(),
                    &run_partition
                )
                .unwrap(),
            AssetPartition::global()
        );
        assert_eq!(
            registry
                .partition_for(
                    &AssetId::new("reddit_threads_daily").unwrap(),
                    &run_partition
                )
                .unwrap(),
            AssetPartition::new([("dt", "2026-07-13"), ("subreddit", "BangaloreRealEstates")])
        );
        assert_eq!(
            registry
                .partition_for(
                    &AssetId::new("reddit_resident_facts").unwrap(),
                    &run_partition
                )
                .unwrap(),
            AssetPartition::new([("dt", "2026-07-13"), ("source", "reddit")])
        );
        assert_eq!(
            registry
                .partition_for(
                    &AssetId::new("google_review_facts").unwrap(),
                    &run_partition
                )
                .unwrap(),
            AssetPartition::new([("dt", "2026-07-13"), ("source", "google")])
        );
        assert_eq!(
            registry
                .partition_for(
                    &AssetId::new("search_serving_bundle").unwrap(),
                    &run_partition
                )
                .unwrap(),
            AssetPartition::global()
        );
    }

    #[test]
    fn partition_policy_missing_run_key_is_error() {
        let registry = default_openestates_registry();
        let run_partition = AssetPartition::new([("dt", "2026-07-13")]);
        let err = registry
            .partition_for(
                &AssetId::new("reddit_threads_daily").unwrap(),
                &run_partition,
            )
            .unwrap_err();

        assert!(matches!(
            err,
            PartitionResolutionError::MissingRunPartitionKey {
                ref asset_id,
                ref key,
                ..
            } if asset_id == &AssetId::new("reddit_threads_daily").unwrap()
                && key == "subreddit"
        ));
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
