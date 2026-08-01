use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::lake::{ArtifactMetadata, LakeError, LakeKey, LakePrefix, LakeStore};

use super::paths::AssetPathBuilder;
use super::{AssetId, AssetPartition, CurrentAssetPointer, MaterializationRecord};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MaterializationLookup {
    asset_id: AssetId,
    partition: AssetPartition,
    materialization_key: String,
}

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
        let metadata = self.lake.put_json(&key, record).await?;
        let lookup = MaterializationLookup {
            asset_id: record.asset_id.clone(),
            partition: record.partition.clone(),
            materialization_key: key.to_string(),
        };
        self.lake
            .put_json(
                &AssetPathBuilder::materialization_lookup_key(&record.materialization_id),
                &lookup,
            )
            .await?;
        Ok(metadata)
    }

    pub async fn promote_current(&self, record: &MaterializationRecord) -> Result<bool, LakeError> {
        self.promote_current_for_run(record, record.created_at)
            .await
    }

    pub async fn force_promote_current(
        &self,
        record: &MaterializationRecord,
    ) -> Result<(), LakeError> {
        let pointer = current_pointer_for_record(record, Utc::now());
        self.lake
            .put_json(
                &AssetPathBuilder::current_pointer_key(&record.asset_id, &record.partition),
                &pointer,
            )
            .await?;
        Ok(())
    }

    pub async fn promote_current_for_run(
        &self,
        record: &MaterializationRecord,
        run_created_at: chrono::DateTime<Utc>,
    ) -> Result<bool, LakeError> {
        self.promote_current_for_run_if_current(record, run_created_at, None)
            .await
    }

    pub async fn promote_current_for_run_if_current(
        &self,
        record: &MaterializationRecord,
        run_created_at: chrono::DateTime<Utc>,
        expected_current: Option<&super::MaterializationId>,
    ) -> Result<bool, LakeError> {
        let mut pointer = current_pointer_for_record(record, Utc::now());
        pointer.run_created_at = Some(run_created_at);
        let key = AssetPathBuilder::current_pointer_key(&record.asset_id, &record.partition);
        self.lake
            .put_json_if(&key, &pointer, |current: Option<&CurrentAssetPointer>| {
                let Some(current) = current else {
                    return true;
                };
                let current_time = current.run_created_at.unwrap_or(current.updated_at);
                expected_current == Some(&current.materialization_id)
                    || run_created_at > current_time
                    || (run_created_at == current_time
                        && pointer.materialization_id.to_string()
                            > current.materialization_id.to_string())
            })
            .await
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

    pub async fn record(
        &self,
        asset_id: &AssetId,
        partition: &AssetPartition,
        materialization_id: &super::MaterializationId,
    ) -> Result<MaterializationRecord, LakeError> {
        let key =
            AssetPathBuilder::materialization_record_key(asset_id, partition, materialization_id);
        let record: MaterializationRecord = self.lake.get_json(&key).await?;
        if record.asset_id != *asset_id {
            return Err(LakeError::InvalidMetadata(format!(
                "materialization {materialization_id} belongs to asset {}, expected {asset_id}",
                record.asset_id
            )));
        }
        if record.partition != *partition {
            return Err(LakeError::InvalidMetadata(format!(
                "materialization {materialization_id} has partition {:?}, expected {:?}",
                record.partition, partition
            )));
        }
        if record.materialization_id != *materialization_id {
            return Err(LakeError::InvalidMetadata(format!(
                "materialization record at {key} has id {}, expected {materialization_id}",
                record.materialization_id
            )));
        }
        Ok(record)
    }

    pub async fn record_by_id_for_asset(
        &self,
        asset_id: &AssetId,
        materialization_id: &super::MaterializationId,
    ) -> Result<Option<MaterializationRecord>, LakeError> {
        let lookup_key = AssetPathBuilder::materialization_lookup_key(materialization_id);
        match self
            .lake
            .get_json::<MaterializationLookup>(&lookup_key)
            .await
        {
            Ok(lookup) => {
                if lookup.asset_id != *asset_id {
                    return Ok(None);
                }
                let key = LakeKey::new(lookup.materialization_key).map_err(LakeError::Key)?;
                let record: MaterializationRecord = self.lake.get_json(&key).await?;
                if record.asset_id != *asset_id
                    || record.partition != lookup.partition
                    || record.materialization_id != *materialization_id
                {
                    return Err(LakeError::InvalidMetadata(format!(
                        "materialization lookup {lookup_key} does not match {asset_id}/{materialization_id}"
                    )));
                }
                return Ok(Some(record));
            }
            Err(err) if err.is_not_found() => {}
            Err(err) => return Err(err),
        }

        let asset_prefix = format!("manifests/assets/{asset_id}/");
        let prefix =
            LakePrefix::new(asset_prefix.as_str()).expect("asset manifest prefix is valid");
        let suffix = format!("/materializations/{materialization_id}.json");
        let Some(key) = self
            .lake
            .list_keys(&prefix)
            .await?
            .into_iter()
            .find(|key| key.as_str().ends_with(&suffix))
        else {
            return Ok(None);
        };
        let record: MaterializationRecord = self.lake.get_json(&key).await?;
        if record.asset_id != *asset_id || record.materialization_id != *materialization_id {
            return Err(LakeError::InvalidMetadata(format!(
                "materialization record at {key} does not match {asset_id}/{materialization_id}"
            )));
        }
        let lookup = MaterializationLookup {
            asset_id: record.asset_id.clone(),
            partition: record.partition.clone(),
            materialization_key: key.to_string(),
        };
        self.lake.put_json(&lookup_key, &lookup).await?;
        Ok(Some(record))
    }

    pub async fn record_by_id(
        &self,
        materialization_id: &super::MaterializationId,
    ) -> Result<Option<MaterializationRecord>, LakeError> {
        let lookup_key = AssetPathBuilder::materialization_lookup_key(materialization_id);
        let lookup = match self
            .lake
            .get_json::<MaterializationLookup>(&lookup_key)
            .await
        {
            Ok(lookup) => lookup,
            Err(err) if err.is_not_found() => return Ok(None),
            Err(err) => return Err(err),
        };
        let key = LakeKey::new(lookup.materialization_key).map_err(LakeError::Key)?;
        let record: MaterializationRecord = self.lake.get_json(&key).await?;
        if record.asset_id != lookup.asset_id
            || record.partition != lookup.partition
            || record.materialization_id != *materialization_id
        {
            return Err(LakeError::InvalidMetadata(format!(
                "materialization lookup {lookup_key} does not match {materialization_id}"
            )));
        }
        Ok(Some(record))
    }

    pub async fn record_for_run_attempt(
        &self,
        asset_id: &AssetId,
        partition: &AssetPartition,
        run_id: &super::MaterializationId,
        attempt_started_at: chrono::DateTime<Utc>,
    ) -> Result<Option<MaterializationRecord>, LakeError> {
        let prefix = LakePrefix::new(format!(
            "manifests/assets/{asset_id}/{}/materializations",
            if partition.is_global() {
                "partition=global".to_string()
            } else {
                partition.path_segments().join("/")
            }
        ))
        .expect("materialization prefix is valid");
        let mut matches = Vec::new();
        for key in self.lake.list_keys(&prefix).await? {
            let record: MaterializationRecord = self.lake.get_json(&key).await?;
            if record.run_id == *run_id && record.created_at >= attempt_started_at {
                matches.push(record);
            }
        }
        match matches.len() {
            0 => Ok(None),
            1 => Ok(matches.pop()),
            count => Err(LakeError::InvalidMetadata(format!(
                "asset {asset_id} partition {partition:?} has {count} materializations for run {run_id} after attempt {attempt_started_at}"
            ))),
        }
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

fn current_pointer_for_record(
    record: &MaterializationRecord,
    updated_at: chrono::DateTime<Utc>,
) -> CurrentAssetPointer {
    CurrentAssetPointer {
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
        run_id: Some(record.run_id.clone()),
        run_created_at: Some(record.created_at),
        updated_at,
    }
}
