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
    Lake(LakeError),
}

impl fmt::Display for AssetFanInError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownAsset { asset_id } => {
                write!(f, "asset {asset_id} is not registered in the DAG")
            }
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
