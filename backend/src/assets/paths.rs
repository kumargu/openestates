use crate::lake::LakeKey;

use super::{AssetId, AssetPartition, MaterializationId};

/// Builds canonical local/S3 keys for asset artifacts and manifests.
pub struct AssetPathBuilder;

impl AssetPathBuilder {
    pub fn raw_snapshot_key(
        source: &str,
        partition: &AssetPartition,
        run_id: &str,
        file_name: &str,
    ) -> LakeKey {
        let mut parts = vec![
            "raw".to_string(),
            format!("source={}", slug_segment(source)),
        ];
        parts.extend(partition.path_segments());
        parts.push(format!("run_id={}", slug_segment(run_id)));
        parts.push(file_name.to_string());
        LakeKey::new(parts.join("/")).expect("valid raw snapshot key")
    }

    pub fn silver_fact_key(
        entity_type: &str,
        fact_key: &str,
        source: &str,
        partition: &AssetPartition,
        file_name: &str,
    ) -> LakeKey {
        let mut parts = vec![
            "silver".to_string(),
            "facts".to_string(),
            format!("entity_type={}", slug_segment(entity_type)),
            format!("fact_key={}", slug_segment(fact_key)),
            format!("source={}", slug_segment(source)),
        ];
        parts.extend(partition.path_segments());
        parts.push(file_name.to_string());
        LakeKey::new(parts.join("/")).expect("valid silver fact key")
    }

    pub fn silver_asset_key(
        asset_id: &str,
        source: &str,
        dt: &str,
        run_id: &str,
        file_name: &str,
    ) -> LakeKey {
        LakeKey::join(&[
            "silver",
            &slug_segment(asset_id),
            &format!("source={}", slug_segment(source)),
            &format!("dt={}", slug_segment(dt)),
            &format!("run_id={}", slug_segment(run_id)),
            file_name,
        ])
        .expect("valid silver asset key")
    }

    pub fn gold_kg_key(version: &str, file_name: &str) -> LakeKey {
        LakeKey::join(&[
            "gold",
            "kg",
            &format!("version={}", slug_segment(version)),
            file_name,
        ])
        .expect("valid gold KG key")
    }

    pub fn gold_asset_key(asset_id: &str, version: &str, file_name: &str) -> LakeKey {
        LakeKey::join(&[
            "gold",
            &slug_segment(asset_id),
            &format!("version={}", slug_segment(version)),
            file_name,
        ])
        .expect("valid gold asset key")
    }

    pub fn serving_bundle_key(version: &str, file_name: &str) -> LakeKey {
        LakeKey::join(&[
            "serving",
            "search_bundle",
            &format!("version={}", slug_segment(version)),
            file_name,
        ])
        .expect("valid serving bundle key")
    }

    pub fn materialization_record_key(
        asset_id: &AssetId,
        partition: &AssetPartition,
        materialization_id: &MaterializationId,
    ) -> LakeKey {
        let mut parts = vec![
            "manifests".to_string(),
            "assets".to_string(),
            asset_id.as_str().to_string(),
        ];
        parts.extend(partition_or_global(partition));
        parts.push("materializations".to_string());
        parts.push(format!("{materialization_id}.json"));
        LakeKey::new(parts.join("/")).expect("valid materialization key")
    }

    pub fn current_pointer_key(asset_id: &AssetId, partition: &AssetPartition) -> LakeKey {
        let mut parts = vec![
            "manifests".to_string(),
            "assets".to_string(),
            asset_id.as_str().to_string(),
        ];
        parts.extend(partition_or_global(partition));
        parts.push("current.json".to_string());
        LakeKey::new(parts.join("/")).expect("valid current pointer key")
    }
}

fn partition_or_global(partition: &AssetPartition) -> Vec<String> {
    if partition.is_global() {
        vec!["partition=global".to_string()]
    } else {
        partition.path_segments()
    }
}

fn slug_segment(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_keys_keep_source_partition_and_run_id() {
        let key = AssetPathBuilder::raw_snapshot_key(
            "RERA",
            &AssetPartition::new([("state", "ka"), ("dt", "2026-07")]),
            "00000000-0000-0000-0000-000000000000",
            "projects/part-00000.parquet",
        );
        assert_eq!(
            key.as_str(),
            "raw/source=rera/dt=2026-07/state=ka/run_id=00000000-0000-0000-0000-000000000000/projects/part-00000.parquet"
        );
    }

    #[test]
    fn serving_bundle_keys_are_versioned() {
        let key = AssetPathBuilder::serving_bundle_key("2026-07-12T10:00Z", "manifest.json");
        assert_eq!(
            key.as_str(),
            "serving/search_bundle/version=2026-07-12t10-00z/manifest.json"
        );
    }
}
