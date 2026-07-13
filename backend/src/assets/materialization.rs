use chrono::Utc;

use crate::lake::{ArtifactMetadata, LakeError, LakeKey, LakeStore};

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
        let key = LakeKey::new(pointer.materialization_key).expect("stored materialization key");
        self.lake.get_json(&key).await
    }
}
