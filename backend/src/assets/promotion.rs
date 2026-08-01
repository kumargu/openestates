use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::lake::LakeError;
use crate::serving::SEARCH_SERVING_BUNDLE_ASSET_ID;

use super::{
    AssetMaterializationStore, MaterializationId, MaterializationRecord, MaterializationStatus,
    CURRENT_PROJECT_FACTS_ASSET_ID, KG_SOCIETY_VIEW_ASSET_ID,
};

#[derive(Debug, Clone)]
pub struct ServingReleasePromotion {
    pub serving_materialization_id: MaterializationId,
    pub kg_materialization_id: MaterializationId,
    pub current_project_facts_materialization_id: MaterializationId,
    pub promoted_materializations: Vec<MaterializationRecord>,
}

pub async fn promote_search_serving_release(
    store: &AssetMaterializationStore,
    serving_record: &MaterializationRecord,
    force: bool,
) -> Result<ServingReleasePromotion, ServingReleasePromotionError> {
    if serving_record.asset_id.as_str() != SEARCH_SERVING_BUNDLE_ASSET_ID {
        return Err(ServingReleasePromotionError::InvalidTarget(format!(
            "release promotion requires {SEARCH_SERVING_BUNDLE_ASSET_ID}, got {}",
            serving_record.asset_id
        )));
    }

    let ordered = resolve_lineage(store, serving_record).await?;
    let kg_record = direct_parent(&ordered, serving_record, KG_SOCIETY_VIEW_ASSET_ID)?;
    let current_project_facts_record =
        direct_parent(&ordered, kg_record, CURRENT_PROJECT_FACTS_ASSET_ID)?;

    for (index, record) in ordered.iter().enumerate() {
        if record.materialization_id == serving_record.materialization_id {
            verify_current_records(store, &ordered[..index]).await?;
        }
        if force {
            store.force_promote_current(record).await?;
        } else {
            let promoted = store.promote_current(record).await?;
            let current = store
                .current_record(&record.asset_id, &record.partition)
                .await?;
            if !promoted && current.materialization_id != record.materialization_id {
                return Err(ServingReleasePromotionError::PromotionRejected {
                    asset_id: record.asset_id.to_string(),
                    desired: record.materialization_id.clone(),
                    current: current.materialization_id,
                });
            }
        }
    }
    verify_current_records(store, &ordered).await?;

    Ok(ServingReleasePromotion {
        serving_materialization_id: serving_record.materialization_id.clone(),
        kg_materialization_id: kg_record.materialization_id.clone(),
        current_project_facts_materialization_id: current_project_facts_record
            .materialization_id
            .clone(),
        promoted_materializations: ordered,
    })
}

async fn verify_current_records(
    store: &AssetMaterializationStore,
    records: &[MaterializationRecord],
) -> Result<(), ServingReleasePromotionError> {
    for record in records {
        let current = store
            .current_record(&record.asset_id, &record.partition)
            .await?;
        if current.materialization_id != record.materialization_id {
            return Err(ServingReleasePromotionError::PromotionRejected {
                asset_id: record.asset_id.to_string(),
                desired: record.materialization_id.clone(),
                current: current.materialization_id,
            });
        }
    }
    Ok(())
}

async fn resolve_lineage(
    store: &AssetMaterializationStore,
    target: &MaterializationRecord,
) -> Result<Vec<MaterializationRecord>, ServingReleasePromotionError> {
    let mut records = HashMap::<MaterializationId, MaterializationRecord>::new();
    let mut pending = vec![target.clone()];
    while let Some(record) = pending.pop() {
        if records.contains_key(&record.materialization_id) {
            continue;
        }
        if record.status != MaterializationStatus::Succeeded {
            return Err(ServingReleasePromotionError::InvalidLineage(format!(
                "{} materialization {} did not succeed",
                record.asset_id, record.materialization_id
            )));
        }
        for parent_id in &record.parent_materializations {
            let parent = store
                .record_by_id(parent_id)
                .await?
                .ok_or_else(|| ServingReleasePromotionError::MissingParent(parent_id.clone()))?;
            pending.push(parent);
        }
        records.insert(record.materialization_id.clone(), record);
    }

    let mut partitions = HashMap::<String, MaterializationId>::new();
    for record in records.values() {
        let key = format!(
            "{}/{}",
            record.asset_id,
            if record.partition.is_global() {
                "partition=global".to_string()
            } else {
                record.partition.path_segments().join("/")
            }
        );
        if let Some(existing) = partitions.insert(key.clone(), record.materialization_id.clone()) {
            if existing != record.materialization_id {
                return Err(ServingReleasePromotionError::PartitionConflict {
                    partition: key,
                    first: existing,
                    second: record.materialization_id.clone(),
                });
            }
        }
    }

    let mut ordered = Vec::with_capacity(records.len());
    let mut emitted = HashSet::<MaterializationId>::new();
    while ordered.len() < records.len() {
        let mut ready = records
            .values()
            .filter(|record| !emitted.contains(&record.materialization_id))
            .filter(|record| {
                record
                    .parent_materializations
                    .iter()
                    .all(|parent| emitted.contains(parent))
            })
            .cloned()
            .collect::<Vec<_>>();
        ready.sort_by(|left, right| {
            left.asset_id
                .as_str()
                .cmp(right.asset_id.as_str())
                .then(
                    left.partition
                        .path_segments()
                        .cmp(&right.partition.path_segments()),
                )
                .then(
                    left.materialization_id
                        .to_string()
                        .cmp(&right.materialization_id.to_string()),
                )
        });
        if ready.is_empty() {
            return Err(ServingReleasePromotionError::InvalidLineage(
                "materialization lineage contains a cycle".to_string(),
            ));
        }
        for record in ready {
            emitted.insert(record.materialization_id.clone());
            ordered.push(record);
        }
    }

    if ordered.last().map(|record| &record.materialization_id) != Some(&target.materialization_id) {
        return Err(ServingReleasePromotionError::InvalidLineage(
            "search serving bundle is not the lineage commit point".to_string(),
        ));
    }
    Ok(ordered)
}

fn direct_parent<'a>(
    lineage: &'a [MaterializationRecord],
    child: &MaterializationRecord,
    expected_asset_id: &str,
) -> Result<&'a MaterializationRecord, ServingReleasePromotionError> {
    let matches = lineage
        .iter()
        .filter(|candidate| {
            child
                .parent_materializations
                .contains(&candidate.materialization_id)
                && candidate.asset_id.as_str() == expected_asset_id
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [record] => Ok(*record),
        _ => Err(ServingReleasePromotionError::InvalidLineage(format!(
            "{} materialization {} must have exactly one direct {expected_asset_id} parent",
            child.asset_id, child.materialization_id
        ))),
    }
}

#[derive(Debug)]
pub enum ServingReleasePromotionError {
    InvalidTarget(String),
    InvalidLineage(String),
    MissingParent(MaterializationId),
    PartitionConflict {
        partition: String,
        first: MaterializationId,
        second: MaterializationId,
    },
    PromotionRejected {
        asset_id: String,
        desired: MaterializationId,
        current: MaterializationId,
    },
    Lake(LakeError),
}

impl fmt::Display for ServingReleasePromotionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTarget(message) | Self::InvalidLineage(message) => {
                formatter.write_str(message)
            }
            Self::MissingParent(id) => write!(formatter, "missing parent materialization {id}"),
            Self::PartitionConflict {
                partition,
                first,
                second,
            } => write!(
                formatter,
                "release lineage contains conflicting materializations {first} and {second} for {partition}"
            ),
            Self::PromotionRejected {
                asset_id,
                desired,
                current,
            } => write!(
                formatter,
                "promotion of {asset_id}/{desired} was rejected because current is {current}; use --force for an intentional rollback"
            ),
            Self::Lake(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ServingReleasePromotionError {}

impl From<LakeError> for ServingReleasePromotionError {
    fn from(error: LakeError) -> Self {
        Self::Lake(error)
    }
}

#[cfg(test)]
mod tests {
    use crate::assets::{AssetId, AssetPartition, AssetStage};
    use crate::lake::LakeStore;
    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn promotes_complete_lineage_before_serving_commit_point() {
        let root = tempdir().unwrap();
        let store = AssetMaterializationStore::new(LakeStore::local(root.path()).unwrap());
        let old = release_records("old");
        write_records(&store, &old).await;
        for record in &old {
            store.force_promote_current(record).await.unwrap();
        }

        let new = release_records("new");
        write_records(&store, &new).await;
        let promotion = promote_search_serving_release(&store, &new[2], false)
            .await
            .unwrap();

        assert_eq!(promotion.promoted_materializations, new);
        assert_eq!(promotion.kg_materialization_id, new[1].materialization_id);
        assert_eq!(
            promotion.current_project_facts_materialization_id,
            new[0].materialization_id
        );
        for record in &new {
            let current = store
                .current_record(&record.asset_id, &record.partition)
                .await
                .unwrap();
            assert_eq!(current.materialization_id, record.materialization_id);
        }
    }

    #[tokio::test]
    async fn rejects_serving_bundle_without_direct_kg_parent_before_writes() {
        let root = tempdir().unwrap();
        let store = AssetMaterializationStore::new(LakeStore::local(root.path()).unwrap());
        let current = release_records("current");
        write_records(&store, &current).await;
        for record in &current {
            store.force_promote_current(record).await.unwrap();
        }

        let project = record(CURRENT_PROJECT_FACTS_ASSET_ID, "invalid", Vec::new());
        let serving = record(
            SEARCH_SERVING_BUNDLE_ASSET_ID,
            "invalid",
            vec![project.materialization_id.clone()],
        );
        write_records(&store, &[project, serving.clone()]).await;

        let error = promote_search_serving_release(&store, &serving, true)
            .await
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("must have exactly one direct kg_society_view parent"));
        let current_serving = store
            .current_record(&serving.asset_id, &serving.partition)
            .await
            .unwrap();
        assert_eq!(
            current_serving.materialization_id,
            current[2].materialization_id
        );
    }

    fn release_records(version: &str) -> Vec<MaterializationRecord> {
        let project = record(CURRENT_PROJECT_FACTS_ASSET_ID, version, Vec::new());
        let kg = record(
            KG_SOCIETY_VIEW_ASSET_ID,
            version,
            vec![project.materialization_id.clone()],
        );
        let serving = record(
            SEARCH_SERVING_BUNDLE_ASSET_ID,
            version,
            vec![kg.materialization_id.clone()],
        );
        vec![project, kg, serving]
    }

    fn record(
        asset_id: &str,
        version: &str,
        parents: Vec<MaterializationId>,
    ) -> MaterializationRecord {
        MaterializationRecord::succeeded(
            AssetId::new(asset_id).unwrap(),
            match asset_id {
                SEARCH_SERVING_BUNDLE_ASSET_ID => AssetStage::Serving,
                _ => AssetStage::Gold,
            },
            AssetPartition::global(),
            version,
            Vec::new(),
        )
        .with_parent_materializations(parents)
    }

    async fn write_records(store: &AssetMaterializationStore, records: &[MaterializationRecord]) {
        for record in records {
            store.write_materialization(record).await.unwrap();
        }
    }
}
