use std::path::Path;
use std::sync::Arc;

use backend::lake::{LakeKey, LakePrefix, LakeStore, LakeStoreLocation};
use object_store::memory::InMemory;
use object_store::path::Path as ObjectPath;
use object_store::prefix::PrefixStore;
use object_store::ObjectStoreExt;
use serde::{Deserialize, Serialize};

#[test]
fn lake_location_defaults_to_the_project_lake_and_parses_explicit_urls() {
    let project_root = Path::new("/workspace/openestates");

    assert_eq!(
        LakeStoreLocation::parse(project_root, None).unwrap(),
        LakeStoreLocation::Local("/workspace/openestates/data/lake".into())
    );
    assert_eq!(
        LakeStoreLocation::parse(project_root, Some("file:///var/lib/openestates/lake")).unwrap(),
        LakeStoreLocation::Local("/var/lib/openestates/lake".into())
    );
    assert_eq!(
        LakeStoreLocation::parse(
            project_root,
            Some("s3://property-data/openestates/prod/lake")
        )
        .unwrap(),
        LakeStoreLocation::S3 {
            bucket: "property-data".to_string(),
            prefix: Some("openestates/prod/lake".to_string()),
        }
    );
    assert_eq!(
        LakeStoreLocation::parse(project_root, Some("s3://property-data")).unwrap(),
        LakeStoreLocation::S3 {
            bucket: "property-data".to_string(),
            prefix: None,
        }
    );
}

#[test]
fn lake_location_rejects_ambiguous_or_unsafe_urls() {
    let project_root = Path::new("/workspace/openestates");
    for value in [
        "relative/lake",
        "https://example.com/lake",
        "file://remote-host/var/lake",
        "s3:///missing-bucket",
        "s3://bucket/path?version=1",
        "s3://bucket/path#fragment",
        "s3://user@bucket/path",
        "s3://bucket/a/%2E%2E/private",
    ] {
        assert!(
            LakeStoreLocation::parse(project_root, Some(value)).is_err(),
            "expected {value:?} to be rejected"
        );
    }
}

#[cfg(feature = "s3")]
#[test]
fn s3_store_construction_does_not_fetch_credentials() {
    LakeStoreLocation::S3 {
        bucket: "configuration-only-fixture".to_string(),
        prefix: Some("openestates/test".to_string()),
    }
    .open()
    .unwrap();
}

#[tokio::test]
async fn prefixed_object_store_preserves_logical_lake_keys() {
    let backing = Arc::new(InMemory::new());
    let prefixed = PrefixStore::new(
        Arc::clone(&backing),
        ObjectPath::from("openestates/prod/lake"),
    );
    let lake = LakeStore::from_object_store(Arc::new(prefixed));
    let logical_key = LakeKey::new("gold/view=kg_society/version=v1/part-00000.parquet").unwrap();

    lake.put_text(&logical_key, "parquet fixture")
        .await
        .unwrap();

    assert_eq!(
        lake.get_text(&logical_key).await.unwrap(),
        "parquet fixture"
    );
    assert_eq!(
        lake.list_keys(&LakePrefix::new("gold/view=kg_society").unwrap())
            .await
            .unwrap(),
        vec![logical_key]
    );
    assert_eq!(
        backing
            .get(&ObjectPath::from(
                "openestates/prod/lake/gold/view=kg_society/version=v1/part-00000.parquet"
            ))
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .as_ref(),
        b"parquet fixture"
    );
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GenerationPointer {
    generation: u64,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_object_store_promotions_cannot_replace_a_newer_pointer() {
    let lake = LakeStore::from_object_store(Arc::new(InMemory::new()));
    let key = LakeKey::new("manifests/assets/search/partition=global/current.json").unwrap();
    lake.put_json(&key, &GenerationPointer { generation: 0 })
        .await
        .unwrap();

    let mut tasks = Vec::new();
    for generation in 1..=64 {
        let lake = lake.clone();
        let key = key.clone();
        tasks.push(tokio::spawn(async move {
            let next = GenerationPointer { generation };
            lake.put_json_if(&key, &next, |current: Option<&GenerationPointer>| {
                current.is_none_or(|current| generation > current.generation)
            })
            .await
        }));
    }
    for task in tasks {
        task.await.unwrap().unwrap();
    }

    assert_eq!(
        lake.get_json::<GenerationPointer>(&key)
            .await
            .unwrap()
            .generation,
        64
    );
}
