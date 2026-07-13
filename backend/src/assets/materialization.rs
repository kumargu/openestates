use chrono::Utc;

use crate::lake::{ArtifactMetadata, LakeError, LakeKey, LakePrefix, LakeStore};

use super::paths::AssetPathBuilder;
use super::{AssetId, AssetPartition, CurrentAssetPointer, MaterializationRecord};

#[derive(Clone)]
pub struct AssetMaterializationStore {
    lake: LakeStore,
}

impl AssetMaterializationStore {
    pub fn new(lake: LakeStore) -> Self {
        Self { lake }
    }

    pub async fn write_materialization(
        &self,
        record: &MaterializationRecord,
    ) -> Result<ArtifactMetadata, LakeError> {
        let key = AssetPathBuilder::materialization_record_key(
            &record.asset_id,
            &record.partition,
            &record.materialization_id,
        );
        self.lake.put_json(&key, record).await
    }

    pub async fn promote_current(
        &self,
        record: &MaterializationRecord,
    ) -> Result<ArtifactMetadata, LakeError> {
        let pointer = CurrentAssetPointer {
            asset_id: record.asset_id.clone(),
            partition: record.partition.clone(),
            materialization_id: record.materialization_id.clone(),
            materialization_key: AssetPathBuilder::materialization_record_key(
                &record.asset_id,
                &record.partition,
                &record.materialization_id,
            )
            .to_string(),
            version: record.version.clone(),
            updated_at: Utc::now(),
        };
        let key = AssetPathBuilder::current_pointer_key(&record.asset_id, &record.partition);
        self.lake.put_json(&key, &pointer).await
    }

    pub async fn current_pointer(
        &self,
        asset_id: &AssetId,
        partition: &AssetPartition,
    ) -> Result<CurrentAssetPointer, LakeError> {
        let key = AssetPathBuilder::current_pointer_key(asset_id, partition);
        self.lake.get_json(&key).await
    }

    pub async fn current_record(
        &self,
        asset_id: &AssetId,
        partition: &AssetPartition,
    ) -> Result<MaterializationRecord, LakeError> {
        let pointer = self.current_pointer(asset_id, partition).await?;
        if pointer.asset_id != *asset_id {
            return Err(LakeError::InvalidMetadata(format!(
                "current pointer for asset {asset_id} belongs to asset {}",
                pointer.asset_id
            )));
        }
        if pointer.partition != *partition {
            return Err(LakeError::InvalidMetadata(format!(
                "current pointer for asset {asset_id} has partition {:?}, expected {:?}",
                pointer.partition, partition
            )));
        }
        self.record_for_pointer(asset_id, &pointer).await
    }

    pub async fn current_records_for_asset(
        &self,
        asset_id: &AssetId,
    ) -> Result<Vec<MaterializationRecord>, LakeError> {
        let asset_prefix = format!("manifests/assets/{asset_id}/");
        let prefix =
            LakePrefix::new(asset_prefix.as_str()).expect("asset manifest prefix is valid");
        let current_keys = self.lake.list_keys(&prefix).await?;
        let mut records: Vec<MaterializationRecord> = Vec::new();

        for current_key in current_keys.into_iter().filter(|key| {
            key.as_str().starts_with(&asset_prefix) && key.as_str().ends_with("/current.json")
        }) {
            let pointer: CurrentAssetPointer = self.lake.get_json(&current_key).await?;
            if pointer.asset_id != *asset_id {
                return Err(LakeError::InvalidMetadata(format!(
                    "current pointer {} belongs to asset {}, expected {}",
                    current_key, pointer.asset_id, asset_id
                )));
            }
            records.push(self.record_for_pointer(asset_id, &pointer).await?);
        }

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
        Ok(records)
    }

    async fn record_for_pointer(
        &self,
        expected_asset_id: &AssetId,
        pointer: &CurrentAssetPointer,
    ) -> Result<MaterializationRecord, LakeError> {
        let record_key =
            LakeKey::new(pointer.materialization_key.clone()).map_err(LakeError::Key)?;
        let record: MaterializationRecord = self.lake.get_json(&record_key).await?;
        if record.asset_id != *expected_asset_id {
            return Err(LakeError::InvalidMetadata(format!(
                "current pointer for asset {expected_asset_id} points to materialization for asset {}",
                record.asset_id
            )));
        }
        if record.partition != pointer.partition {
            return Err(LakeError::InvalidMetadata(format!(
                "current pointer for asset {expected_asset_id} points to partition {:?}, expected {:?}",
                record.partition, pointer.partition
            )));
        }
        if record.materialization_id != pointer.materialization_id {
            return Err(LakeError::InvalidMetadata(format!(
                "current pointer for asset {expected_asset_id} points to materialization {}, expected {}",
                record.materialization_id, pointer.materialization_id
            )));
        }
        Ok(record)
    }
}
