use std::fs::File;

use backend::assets::{
    AssetId, AssetMaterializationStore, AssetPartition, RedditThreadSnapshotMaterializer,
    RedditThreadSnapshotRecord, SourceWatermark, REDDIT_THREADS_DAILY_ASSET_ID,
};
use backend::lake::{LakeKey, LakeStore};
use chrono::{TimeZone, Utc};
use parquet::file::reader::{FileReader, SerializedFileReader};
use tempfile::tempdir;

#[tokio::test]
async fn reddit_threads_snapshot_materializes_raw_parquet_with_current_pointer() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();

    let records = vec![
        RedditThreadSnapshotRecord {
            thread_id: "t3_alpha".to_string(),
            subreddit: "BangaloreRealEstates".to_string(),
            query: "whitefield greenery".to_string(),
            title: "Whitefield society with good tree cover?".to_string(),
            url: Some("https://reddit.com/r/BangaloreRealEstates/comments/alpha".to_string()),
            score: 42,
            num_comments: 11,
            created_utc: Some(1_776_000_000),
            selftext: Some("Looking for calm, green societies near Whitefield.".to_string()),
            fetched_at: Utc.with_ymd_and_hms(2026, 7, 13, 4, 30, 0).unwrap(),
            fetch_source: "reddit_api".to_string(),
        },
        RedditThreadSnapshotRecord {
            thread_id: "grounded_beta".to_string(),
            subreddit: "BangaloreRealEstates".to_string(),
            query: "whitefield greenery".to_string(),
            title: "Residents discuss large campuses around Whitefield".to_string(),
            url: None,
            score: 0,
            num_comments: 0,
            created_utc: None,
            selftext: Some("Grounded search summary from crawler fallback.".to_string()),
            fetched_at: Utc.with_ymd_and_hms(2026, 7, 13, 4, 31, 0).unwrap(),
            fetch_source: "grounded_search".to_string(),
        },
    ];

    let materialization = RedditThreadSnapshotMaterializer::new(lake.clone())
        .materialize_and_promote(
            "2026-07-13",
            "BangaloreRealEstates",
            "run-reddit-2026-07-13",
            &records,
            vec![SourceWatermark {
                source: "reddit:BangaloreRealEstates".to_string(),
                high_watermark: "2026-07-13T04:31:00Z".to_string(),
            }],
        )
        .await
        .unwrap();

    assert_eq!(materialization.manifest.thread_count, 2);
    assert_eq!(
        materialization.manifest.thread_parquet_key,
        "raw/source=reddit/dt=2026-07-13/subreddit=bangalorerealestates/run_id=run-reddit-2026-07-13/threads/part-00000.parquet"
    );
    assert_eq!(
        materialization.manifest.manifest_key,
        "raw/source=reddit/dt=2026-07-13/subreddit=bangalorerealestates/run_id=run-reddit-2026-07-13/manifest.json"
    );
    assert_eq!(materialization.record.row_count, 2);
    assert_eq!(
        materialization.record.source_watermarks[0].source,
        "reddit:BangaloreRealEstates"
    );

    let parquet_bytes = lake
        .get_bytes(&LakeKey::new(materialization.manifest.thread_parquet_key.clone()).unwrap())
        .await
        .unwrap();
    assert_is_parquet(&parquet_bytes);
    assert_eq!(parquet_rows(&parquet_bytes), 2);
    assert!(parquet_columns(&parquet_bytes).contains(&"fetch_source".to_string()));

    let store = AssetMaterializationStore::new(lake);
    let current = store
        .current_record(
            &AssetId::new(REDDIT_THREADS_DAILY_ASSET_ID).unwrap(),
            &AssetPartition::new([("dt", "2026-07-13"), ("subreddit", "BangaloreRealEstates")]),
        )
        .await
        .unwrap();
    assert_eq!(
        current.materialization_id,
        materialization.record.materialization_id
    );
    assert_eq!(current.version, "2026-07-13");
}

fn assert_is_parquet(bytes: &[u8]) {
    assert!(bytes.len() > 8);
    assert_eq!(&bytes[..4], b"PAR1");
    assert_eq!(&bytes[bytes.len() - 4..], b"PAR1");
}

fn parquet_rows(bytes: &[u8]) -> i64 {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), bytes).unwrap();
    let reader = SerializedFileReader::new(File::open(file.path()).unwrap()).unwrap();
    reader.metadata().file_metadata().num_rows()
}

fn parquet_columns(bytes: &[u8]) -> Vec<String> {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), bytes).unwrap();
    let reader = SerializedFileReader::new(File::open(file.path()).unwrap()).unwrap();
    reader
        .metadata()
        .file_metadata()
        .schema_descr()
        .columns()
        .iter()
        .map(|column| column.name().to_string())
        .collect()
}
