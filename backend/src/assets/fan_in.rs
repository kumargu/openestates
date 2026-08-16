use std::fmt;

use crate::lake::LakeError;

use super::{
    AssetId, AssetMaterializationStore, AssetRegistry, DependencyFanInPolicy, MaterializationRecord,
};

pub async fn all_current_partition_dependency_records_for_asset(
    registry: &AssetRegistry,
    materializations: &AssetMaterializationStore,
    asset_id: &AssetId,
) -> Result<Vec<MaterializationRecord>, AssetFanInError> {
    let definition = registry
        .get(asset_id)
        .ok_or_else(|| AssetFanInError::UnknownAsset {
            asset_id: asset_id.clone(),
        })?;
    let mut records = Vec::new();
    for dependency in definition.dependencies.iter().filter(|dependency| {
        definition.dependency_fan_in_policy(dependency)
            == DependencyFanInPolicy::AllCurrentPartitions
    }) {
        records.extend(
            all_current_materialization_records_for_dependency(
                registry,
                materializations,
                dependency,
            )
            .await?,
        );
    }
    sort_materialization_records(&mut records);
    Ok(records)
}

pub async fn all_current_materialization_records_for_dependency(
    registry: &AssetRegistry,
    materializations: &AssetMaterializationStore,
    dependency: &AssetId,
) -> Result<Vec<MaterializationRecord>, AssetFanInError> {
    let dependency_definition =
        registry
            .get(dependency)
            .ok_or_else(|| AssetFanInError::UnknownAsset {
                asset_id: dependency.clone(),
            })?;
    let mut records = materializations
        .current_records_for_asset(dependency)
        .await?;
    records.retain(|record| {
        dependency_definition
            .partition_policy
            .matches_materialized_partition(&record.partition)
    });
    sort_materialization_records(&mut records);
    Ok(records)
}

/// Return current dependency partitions that are absent from a fan-in
/// materialization's pinned parents.
///
/// Partitioned leaves may advance independently, but a downstream global
/// checkpoint is only promotable after it has incorporated every partition
/// that is current at validation time.
pub async fn missing_current_dependency_records(
    registry: &AssetRegistry,
    materializations: &AssetMaterializationStore,
    record: &MaterializationRecord,
) -> Result<Vec<MaterializationRecord>, AssetFanInError> {
    let definition =
        registry
            .get(&record.asset_id)
            .ok_or_else(|| AssetFanInError::UnknownAsset {
                asset_id: record.asset_id.clone(),
            })?;
    let mut missing = Vec::new();
    for dependency in &definition.dependencies {
        let mut current = all_current_materialization_records_for_dependency(
            registry,
            materializations,
            dependency,
        )
        .await?;
        if definition.dependency_fan_in_policy(dependency)
            == DependencyFanInPolicy::ResolvedPartition
        {
            let expected_partition = registry
                .partition_for(dependency, &record.partition)
                .map_err(|error| AssetFanInError::Partition(error.to_string()))?;
            current.retain(|candidate| candidate.partition == expected_partition);
        }
        missing.extend(current.into_iter().filter(|candidate| {
            !record
                .parent_materializations
                .contains(&candidate.materialization_id)
        }));
    }
    sort_materialization_records(&mut missing);
    Ok(missing)
}

pub fn sort_materialization_records(records: &mut [MaterializationRecord]) {
    records.sort_by(|left, right| {
        left.partition
            .path_segments()
            .cmp(&right.partition.path_segments())
            .then(
                left.materialization_id
                    .to_string()
                    .cmp(&right.materialization_id.to_string()),
            )
    });
}

#[derive(Debug)]
pub enum AssetFanInError {
    UnknownAsset { asset_id: AssetId },
    Partition(String),
    Lake(LakeError),
}

impl fmt::Display for AssetFanInError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownAsset { asset_id } => {
                write!(f, "asset {asset_id} is not registered in the DAG")
            }
            Self::Partition(message) => write!(f, "asset fan-in partition error: {message}"),
            Self::Lake(err) => write!(f, "asset fan-in lake operation failed: {err}"),
        }
    }
}

impl std::error::Error for AssetFanInError {}

impl From<LakeError> for AssetFanInError {
    fn from(err: LakeError) -> Self {
        Self::Lake(err)
    }
}
