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

    pub fn matches_materialized_partition(&self, partition: &AssetPartition) -> bool {
        match self {
            Self::Global => partition.is_global(),
            Self::RunPartition => true,
            Self::Composite { coordinates } => {
                if partition.parts().len() != coordinates.len() {
                    return false;
                }
                coordinates.iter().all(|coordinate| match coordinate {
                    PartitionCoordinate::FromRun { key } => partition.value(key).is_some(),
                    PartitionCoordinate::Static { key, value } => {
                        partition.value(key) == Some(value.as_str())
                    }
                })
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyFanInPolicy {
    ResolvedPartition,
    AllCurrentPartitions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyFanInRule {
    pub dependency: AssetId,
    pub policy: DependencyFanInPolicy,
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
    #[serde(default)]
    pub partition_policy: AssetPartitionPolicy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependency_fan_in: Vec<DependencyFanInRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub optional_dependencies: Vec<AssetId>,
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
            dependency_fan_in: Vec::new(),
            optional_dependencies: Vec::new(),
        }
    }

    pub fn with_partition_policy(mut self, partition_policy: AssetPartitionPolicy) -> Self {
        self.partition_policy = partition_policy;
        self
    }

    pub fn with_dependency_fan_in_policy(
        mut self,
        dependency: &str,
        policy: DependencyFanInPolicy,
    ) -> Self {
        self.dependency_fan_in.push(DependencyFanInRule {
            dependency: AssetId::new(dependency).expect("valid static dependency id"),
            policy,
        });
        self
    }

    pub fn dependency_fan_in_policy(&self, dependency: &AssetId) -> DependencyFanInPolicy {
        self.dependency_fan_in
            .iter()
            .find(|rule| &rule.dependency == dependency)
            .map(|rule| rule.policy)
            .unwrap_or(DependencyFanInPolicy::ResolvedPartition)
    }

    pub fn with_optional_dependency(mut self, dependency: &str) -> Self {
        self.optional_dependencies
            .push(AssetId::new(dependency).expect("valid static dependency id"));
        self
    }

    pub fn is_optional_dependency(&self, dependency: &AssetId) -> bool {
        self.optional_dependencies.contains(dependency)
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

    pub fn from_config_file(path: &std::path::Path) -> Result<Self, RegistryError> {
        let contents = std::fs::read_to_string(path)
            .map_err(|err| RegistryError::ConfigIo(err.to_string()))?;
        let file: crate::dag_config::AssetRegistryFile = serde_json::from_str(&contents)
            .map_err(|err| RegistryError::ConfigParse(err.to_string()))?;
        Self::new(file.assets)
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

            let mut dependency_fan_in = HashMap::new();
            for rule in &definition.dependency_fan_in {
                if !definition.dependencies.contains(&rule.dependency) {
                    return Err(RegistryError::UnknownDependencyFanInRule {
                        asset_id: definition.id.clone(),
                        dependency: rule.dependency.clone(),
                    });
                }
                if dependency_fan_in
                    .insert(rule.dependency.clone(), rule.policy)
                    .is_some()
                {
                    return Err(RegistryError::DuplicateDependencyFanInRule {
                        asset_id: definition.id.clone(),
                        dependency: rule.dependency.clone(),
                    });
                }
            }
            let mut optional_dependencies = HashMap::new();
            for dependency in &definition.optional_dependencies {
                if !definition.dependencies.contains(dependency) {
                    return Err(RegistryError::UnknownOptionalDependency {
                        asset_id: definition.id.clone(),
                        dependency: dependency.clone(),
                    });
                }
                if optional_dependencies
                    .insert(dependency.clone(), ())
                    .is_some()
                {
                    return Err(RegistryError::DuplicateOptionalDependency {
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
    UnknownAsset(AssetId),
    MissingDependency {
        asset_id: AssetId,
        dependency: AssetId,
    },
    UnknownDependencyFanInRule {
        asset_id: AssetId,
        dependency: AssetId,
    },
    DuplicateDependencyFanInRule {
        asset_id: AssetId,
        dependency: AssetId,
    },
    UnknownOptionalDependency {
        asset_id: AssetId,
        dependency: AssetId,
    },
    DuplicateOptionalDependency {
        asset_id: AssetId,
        dependency: AssetId,
    },
    SelfDependency(AssetId),
    Cycle(AssetId),
    ConfigIo(String),
    ConfigParse(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateAsset(asset_id) => write!(f, "duplicate asset id: {asset_id}"),
            Self::UnknownAsset(asset_id) => write!(f, "unknown asset id: {asset_id}"),
            Self::MissingDependency {
                asset_id,
                dependency,
            } => write!(f, "asset {asset_id} depends on unknown asset {dependency}"),
            Self::UnknownDependencyFanInRule {
                asset_id,
                dependency,
            } => write!(
                f,
                "asset {asset_id} declares fan-in for non-dependency {dependency}"
            ),
            Self::DuplicateDependencyFanInRule {
                asset_id,
                dependency,
            } => write!(
                f,
                "asset {asset_id} declares duplicate fan-in policy for dependency {dependency}"
            ),
            Self::UnknownOptionalDependency {
                asset_id,
                dependency,
            } => write!(
                f,
                "asset {asset_id} declares non-dependency {dependency} as optional"
            ),
            Self::DuplicateOptionalDependency {
                asset_id,
                dependency,
            } => write!(
                f,
                "asset {asset_id} declares optional dependency {dependency} more than once"
            ),
            Self::SelfDependency(asset_id) => write!(f, "asset {asset_id} depends on itself"),
            Self::Cycle(asset_id) => write!(f, "asset cycle detected at {asset_id}"),
            Self::ConfigIo(err) => write!(f, "failed to read asset registry config: {err}"),
            Self::ConfigParse(err) => write!(f, "failed to parse asset registry config: {err}"),
        }
    }
}

impl std::error::Error for RegistryError {}

/// Preferred runtime registry: load `app/config/dag/asset_registry.json` when present,
/// otherwise fall back to the embedded default graph.
pub fn openestates_registry() -> AssetRegistry {
    let path = crate::dag_config::asset_registry_path();
    if path.exists() {
        match AssetRegistry::from_config_file(&path) {
            Ok(registry) => return registry,
            Err(err) => {
                eprintln!(
                    "warning: failed to load asset registry from {}: {err}; using embedded default",
                    path.display()
                );
            }
        }
    }
    default_openestates_registry()
}

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
            "google_places_weekly",
            AssetStage::Raw,
            "Weekly Google Maps place and review metadata with navigable source links.",
            &["canonical_society_nodes"],
            RefreshCadence::Weekly,
            CostTier::Cheap,
            TrustTier::Support,
        )
        .with_partition_policy(AssetPartitionPolicy::from_run_keys_with_static(
            &[],
            &[("source", "google")],
        )),
        asset(
            "google_review_facts",
            AssetStage::Silver,
            "Review-derived support facts for maintenance, amenities, and liveability.",
            &["google_places_weekly", "canonical_society_nodes"],
            RefreshCadence::OnChange,
            CostTier::Free,
            TrustTier::Support,
        )
        .with_partition_policy(AssetPartitionPolicy::from_run_keys_with_static(
            &[],
            &[("source", "google")],
        )),
        asset(
            "google_nearby_places_weekly",
            AssetStage::Raw,
            "Weekly Google Maps nearby place observations for schools, metro, hospitals, fitness, and offices.",
            &["canonical_society_nodes"],
            RefreshCadence::Weekly,
            CostTier::Cheap,
            TrustTier::Support,
        )
        .with_partition_policy(AssetPartitionPolicy::from_run_keys_with_static(
            &[],
            &[("source", "google")],
        )),
        asset(
            "google_nearby_place_facts",
            AssetStage::Silver,
            "Nearby place support facts grouped by society and place category.",
            &["google_nearby_places_weekly", "canonical_society_nodes"],
            RefreshCadence::OnChange,
            CostTier::Free,
            TrustTier::Support,
        )
        .with_partition_policy(AssetPartitionPolicy::from_run_keys_with_static(
            &[],
            &[("source", "google")],
        )),
        asset(
            "external_listings_weekly",
            AssetStage::Raw,
            "Weekly source-neutral property listing observations from external portals or feeds.",
            &["canonical_society_nodes"],
            RefreshCadence::Weekly,
            CostTier::Free,
            TrustTier::Support,
        )
        .with_partition_policy(AssetPartitionPolicy::from_run_keys_with_static(
            &[],
            &[("source", "external_listing")],
        )),
        asset(
            "external_listing_facts",
            AssetStage::Silver,
            "Source-neutral listing facts for price, area, BHK, and listing provenance.",
            &["external_listings_weekly", "canonical_society_nodes"],
            RefreshCadence::OnChange,
            CostTier::Free,
            TrustTier::Support,
        )
        .with_partition_policy(AssetPartitionPolicy::from_run_keys_with_static(
            &[],
            &[("source", "external_listing")],
        )),
        asset(
            "external_images_weekly",
            AssetStage::Raw,
            "Weekly source-neutral project image observations from external portals or feeds.",
            &["canonical_society_nodes"],
            RefreshCadence::Weekly,
            CostTier::Free,
            TrustTier::Support,
        )
        .with_partition_policy(AssetPartitionPolicy::from_run_keys_with_static(
            &[],
            &[("source", "external_image")],
        )),
        asset(
            "image_media_facts",
            AssetStage::Silver,
            "Source-backed image facts for hero images, galleries, and image provenance.",
            &["external_images_weekly", "canonical_society_nodes"],
            RefreshCadence::OnChange,
            CostTier::Free,
            TrustTier::Support,
        )
        .with_partition_policy(AssetPartitionPolicy::from_run_keys_with_static(
            &[],
            &[("source", "external_image")],
        )),
        asset(
            "builder_rera_aggregates",
            AssetStage::Silver,
            "Builder portfolio aggregates computed from the current RERA registry.",
            &["rera_registry_monthly", "canonical_society_nodes"],
            RefreshCadence::OnChange,
            CostTier::Free,
            TrustTier::Derived,
        ),
        asset(
            "home_state_signals",
            AssetStage::Silver,
            "Buyer-facing home state and age derived from durable society facts.",
            &["rera_legal_facts"],
            RefreshCadence::OnChange,
            CostTier::Free,
            TrustTier::Derived,
        ),
        asset(
            "approach_road_graph_facts",
            AssetStage::Silver,
            "Approach-road graph edges and road-segment facts derived from upstream RERA and Google location facts.",
            &[
                "canonical_society_nodes",
                "rera_legal_facts",
                "google_review_facts",
            ],
            RefreshCadence::OnChange,
            CostTier::Free,
            TrustTier::Support,
        ),
        asset(
            "society_groundwater_potential_facts",
            AssetStage::Silver,
            "Groundwater potential facts joined offline from society coordinates to OpenCity groundwater zones.",
            &[
                "canonical_society_nodes",
                "rera_legal_facts",
                "google_review_facts",
            ],
            RefreshCadence::Monthly,
            CostTier::Free,
            TrustTier::Support,
        ),
        asset(
            "bengaluru_metro_station_facts",
            AssetStage::Silver,
            "Static Bengaluru metro station coordinates and line metadata collected from public map data for offline radius and connectivity views.",
            &[],
            RefreshCadence::Monthly,
            CostTier::Free,
            TrustTier::Support,
        ),
        asset(
            "osm_power_line_facts",
            AssetStage::Silver,
            "OpenStreetMap transmission-line proximity facts with geometry for red-flag map overlays.",
            &["canonical_society_nodes"],
            RefreshCadence::Monthly,
            CostTier::Free,
            TrustTier::Support,
        ),
        asset(
            "stormwater_drain_facts",
            AssetStage::Silver,
            "Stormwater drain and Rajakaluve proximity facts with geometry for red-flag map overlays.",
            &["canonical_society_nodes"],
            RefreshCadence::Monthly,
            CostTier::Free,
            TrustTier::Support,
        ),
        asset(
            "current_project_facts",
            AssetStage::Gold,
            "Compacted current project fact rows for fast KG view and serving-bundle materialization. Graph-shaped assets stay as direct KG dependencies.",
            &[
                "rera_legal_facts",
                "google_review_facts",
                "google_nearby_place_facts",
                "external_listing_facts",
                "image_media_facts",
                "builder_rera_aggregates",
                "home_state_signals",
                "society_groundwater_potential_facts",
                "bengaluru_metro_station_facts",
                "osm_power_line_facts",
                "stormwater_drain_facts",
            ],
            RefreshCadence::OnChange,
            CostTier::Free,
            TrustTier::Derived,
        )
        .with_dependency_fan_in_policy(
            "google_review_facts",
            DependencyFanInPolicy::AllCurrentPartitions,
        )
        .with_dependency_fan_in_policy(
            "google_nearby_place_facts",
            DependencyFanInPolicy::AllCurrentPartitions,
        )
        .with_dependency_fan_in_policy(
            "external_listing_facts",
            DependencyFanInPolicy::AllCurrentPartitions,
        )
        .with_dependency_fan_in_policy("image_media_facts", DependencyFanInPolicy::AllCurrentPartitions)
        .with_optional_dependency("google_review_facts")
        .with_optional_dependency("google_nearby_place_facts")
        .with_optional_dependency("external_listing_facts")
        .with_optional_dependency("image_media_facts")
        .with_optional_dependency("home_state_signals")
        .with_optional_dependency("bengaluru_metro_station_facts"),
        asset(
            "kg_society_view",
            AssetStage::Gold,
            "Versioned society KG view merged by source precedence and fact policy.",
            &[
                "canonical_society_nodes",
                "current_project_facts",
                // Approach-road data includes road-segment entities and graph edges, so it bypasses
                // fact-row compaction and remains a direct KG input.
                "approach_road_graph_facts",
            ],
            RefreshCadence::OnChange,
            CostTier::Free,
            TrustTier::Derived,
        )
        .with_optional_dependency("approach_road_graph_facts")
        .with_optional_dependency("current_project_facts"),
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
        let run_partition = AssetPartition::new([("dt", "2026-07-13")]);

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
                    &AssetId::new("google_review_facts").unwrap(),
                    &run_partition
                )
                .unwrap(),
            AssetPartition::new([("source", "google")])
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
    fn default_registry_fans_support_facts_into_global_kg() {
        let registry = default_openestates_registry();
        let current_project_facts = registry
            .get(&AssetId::new("current_project_facts").unwrap())
            .unwrap();
        let kg = registry
            .get(&AssetId::new("kg_society_view").unwrap())
            .unwrap();

        assert_eq!(
            current_project_facts
                .dependency_fan_in_policy(&AssetId::new("google_review_facts").unwrap()),
            DependencyFanInPolicy::AllCurrentPartitions
        );
        assert_eq!(
            kg.dependency_fan_in_policy(&AssetId::new("current_project_facts").unwrap()),
            DependencyFanInPolicy::ResolvedPartition
        );
        assert_eq!(
            kg.dependency_fan_in_policy(&AssetId::new("approach_road_graph_facts").unwrap()),
            DependencyFanInPolicy::ResolvedPartition
        );
    }

    #[test]
    fn partition_policy_matches_materialized_partition_shape() {
        let policy =
            AssetPartitionPolicy::from_run_keys_with_static(&["dt"], &[("source", "google")]);

        assert!(policy.matches_materialized_partition(&AssetPartition::new([
            ("dt", "2026-07-13"),
            ("source", "google"),
        ])));
        assert!(!policy.matches_materialized_partition(&AssetPartition::global()));
        assert!(
            !policy.matches_materialized_partition(&AssetPartition::new([
                ("dt", "2026-07-13"),
                ("source", "external_listing"),
            ]))
        );
        assert!(
            !policy.matches_materialized_partition(&AssetPartition::new([
                ("city", "bengaluru"),
                ("dt", "2026-07-13"),
                ("source", "google"),
            ]))
        );
    }

    #[test]
    fn partition_policy_missing_run_key_is_error() {
        let registry = AssetRegistry::new(vec![asset(
            "daily_source",
            AssetStage::Raw,
            "daily source",
            &[],
            RefreshCadence::Daily,
            CostTier::Free,
            TrustTier::Support,
        )
        .with_partition_policy(AssetPartitionPolicy::from_run_keys(&["dt", "source"]))])
        .unwrap();
        let run_partition = AssetPartition::new([("dt", "2026-07-13")]);
        let err = registry
            .partition_for(&AssetId::new("daily_source").unwrap(), &run_partition)
            .unwrap_err();

        assert!(matches!(
            err,
            PartitionResolutionError::MissingRunPartitionKey {
                ref asset_id,
                ref key,
                ..
            } if asset_id == &AssetId::new("daily_source").unwrap()
                && key == "source"
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

    #[test]
    fn registry_rejects_fan_in_rule_for_non_dependency() {
        let result = AssetRegistry::new(vec![
            asset(
                "root_snapshot",
                AssetStage::Raw,
                "root",
                &[],
                RefreshCadence::Monthly,
                CostTier::Free,
                TrustTier::Root,
            ),
            asset(
                "derived_view",
                AssetStage::Gold,
                "bad fan-in",
                &["root_snapshot"],
                RefreshCadence::OnChange,
                CostTier::Free,
                TrustTier::Derived,
            )
            .with_dependency_fan_in_policy(
                "missing_dependency",
                DependencyFanInPolicy::AllCurrentPartitions,
            ),
        ]);

        assert!(matches!(
            result,
            Err(RegistryError::UnknownDependencyFanInRule { .. })
        ));
    }

    fn position(ordered: &[AssetId], id: &str) -> usize {
        let id = AssetId::new(id).unwrap();
        ordered
            .iter()
            .position(|asset_id| asset_id == &id)
            .expect("asset id in registry")
    }

    #[test]
    fn export_asset_registry_json_when_requested() {
        if std::env::var("OPENESTATES_EXPORT_ASSET_REGISTRY")
            .ok()
            .as_deref()
            != Some("1")
        {
            return;
        }

        let registry = default_openestates_registry();
        let file = crate::dag_config::AssetRegistryFile {
            version: 1,
            description: Some(
                "OpenEstates asset DAG. Exported from embedded registry; edit via DAG config."
                    .to_string(),
            ),
            assets: registry.definitions().to_vec(),
        };
        let path = crate::dag_config::asset_registry_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create app/config/dag");
        }
        let json = serde_json::to_string_pretty(&file).expect("serialize asset registry");
        std::fs::write(&path, json).expect("write asset_registry.json");
        eprintln!("exported asset registry to {}", path.display());
    }
}
