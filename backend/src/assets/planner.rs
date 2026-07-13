use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};

use crate::lake::LakeError;

use super::{
    AssetId, AssetMaterializationStore, AssetPartition, AssetRegistry, MaterializationRecord,
    RefreshCadence, RegistryError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedAsset {
    pub asset_id: AssetId,
    pub reason: PlanReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanReason {
    Missing,
    DependencyPending(AssetId),
    DependencyChanged(AssetId),
    Stale { cadence: RefreshCadence },
}

#[derive(Debug)]
pub enum PlannerError {
    Registry(RegistryError),
    Lake(LakeError),
}

pub struct AssetPlanner {
    registry: AssetRegistry,
    materializations: AssetMaterializationStore,
}

impl AssetPlanner {
    pub fn new(registry: AssetRegistry, materializations: AssetMaterializationStore) -> Self {
        Self {
            registry,
            materializations,
        }
    }

    pub async fn plan_global(&self, now: DateTime<Utc>) -> Result<Vec<PlannedAsset>, PlannerError> {
        self.plan_partition(&AssetPartition::global(), now).await
    }

    pub async fn plan_partition(
        &self,
        partition: &AssetPartition,
        now: DateTime<Utc>,
    ) -> Result<Vec<PlannedAsset>, PlannerError> {
        let ordered = self
            .registry
            .topological_order()
            .map_err(PlannerError::Registry)?;
        let mut records = HashMap::new();
        let mut planned_ids = HashSet::new();
        let mut plan = Vec::new();

        for asset_id in ordered {
            let definition = self.registry.get(&asset_id).expect("registry order asset");
            let current = self
                .materializations
                .current_record(&asset_id, partition)
                .await;
            let current = match current {
                Ok(record) => Some(record),
                Err(err) if err.is_not_found() => None,
                Err(err) => return Err(PlannerError::Lake(err)),
            };

            let reason = plan_reason(definition, current.as_ref(), &records, &planned_ids, now);
            if let Some(reason) = reason {
                planned_ids.insert(asset_id.clone());
                plan.push(PlannedAsset {
                    asset_id: asset_id.clone(),
                    reason,
                });
            }

            records.insert(asset_id, current);
        }

        Ok(plan)
    }
}

fn plan_reason(
    definition: &super::AssetDefinition,
    current: Option<&MaterializationRecord>,
    records: &HashMap<AssetId, Option<MaterializationRecord>>,
    planned_ids: &HashSet<AssetId>,
    now: DateTime<Utc>,
) -> Option<PlanReason> {
    let current = match current {
        Some(record) => record,
        None => return Some(PlanReason::Missing),
    };

    for dependency in &definition.dependencies {
        if planned_ids.contains(dependency) {
            return Some(PlanReason::DependencyPending(dependency.clone()));
        }

        let dependency_record = records.get(dependency).and_then(|record| record.as_ref());
        let dependency_record = match dependency_record {
            Some(record) => record,
            None => return Some(PlanReason::DependencyPending(dependency.clone())),
        };

        if !current
            .parent_materializations
            .contains(&dependency_record.materialization_id)
        {
            return Some(PlanReason::DependencyChanged(dependency.clone()));
        }
    }

    if cadence_is_stale(definition.refresh, current.created_at, now) {
        return Some(PlanReason::Stale {
            cadence: definition.refresh,
        });
    }

    None
}

fn cadence_is_stale(
    cadence: RefreshCadence,
    materialized_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> bool {
    let max_age = match cadence {
        RefreshCadence::Manual | RefreshCadence::OnChange => return false,
        RefreshCadence::Daily => Duration::days(1),
        RefreshCadence::Weekly => Duration::days(7),
        RefreshCadence::Monthly => Duration::days(31),
        RefreshCadence::Quarterly => Duration::days(92),
    };
    now.signed_duration_since(materialized_at) > max_age
}

impl std::fmt::Display for PlannerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Registry(err) => write!(f, "asset registry error: {err}"),
            Self::Lake(err) => write!(f, "asset lake error: {err}"),
        }
    }
}

impl std::error::Error for PlannerError {}
