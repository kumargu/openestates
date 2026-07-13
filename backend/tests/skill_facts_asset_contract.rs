use std::fs::File;

use backend::assets::{
    read_skill_fact_artifact_rows, AssetId, AssetMaterializationStore, AssetPartition,
    RedditThreadSnapshotMaterializer, RedditThreadSnapshotRecord, SkillFactAnnotationRecord,
    SkillFactMaterializeError, SkillFactMaterializer, SkillFactRecord, SourceWatermark,
};
use backend::knowledge::FactValue;
use backend::lake::{LakeKey, LakeStore};
use chrono::{TimeZone, Utc};
use parquet::file::reader::{FileReader, SerializedFileReader};
use tempfile::tempdir;

#[tokio::test]
async fn reddit_resident_facts_materialize_as_silver_parquet_with_raw_lineage() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();

    let raw = RedditThreadSnapshotMaterializer::new(lake.clone())
        .materialize_and_promote(
            "2026-07-13",
            "BangaloreRealEstates",
            "run-reddit-2026-07-13",
            &[RedditThreadSnapshotRecord {
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
            }],
            vec![SourceWatermark {
                source: "reddit:BangaloreRealEstates".to_string(),
                high_watermark: "2026-07-13T04:30:00Z".to_string(),
            }],
        )
        .await
        .unwrap();

    let run_id = "run-reddit-facts-2026-07-13";
    let materialization = SkillFactMaterializer::new(lake.clone())
        .materialize_and_promote(
            "reddit_resident_facts",
            "reddit",
            "2026-07-13",
            run_id,
            &[SkillFactRecord {
                entity_id: "society:large-green".to_string(),
                fact_key: "resident_greenery_signal".to_string(),
                value_type: "text".to_string(),
                value_json:
                    r#"{"type":"Text","data":"Residents mention calm roads and many trees"}"#
                        .to_string(),
                confidence: 0.7,
                source_type: "Reddit".to_string(),
                source_url: Some(
                    "https://reddit.com/r/BangaloreRealEstates/comments/alpha".to_string(),
                ),
                model: None,
                skill_id: Some("reddit_resident_fact_extractor".to_string()),
                triggered_by: Some("3bhk whitefield greenery".to_string()),
                learned_at: Utc.with_ymd_and_hms(2026, 7, 13, 4, 35, 0).unwrap(),
                run_id: run_id.to_string(),
                input_hash: "sha256:reddit-alpha".to_string(),
            }],
            &[SkillFactAnnotationRecord {
                entity_id: "society:large-green".to_string(),
                fact_key: "resident_greenery_signal".to_string(),
                display_template: Some("Resident greenery signal: {value}".to_string()),
                answers_preferences_json: r#"["greenery","quiet neighborhood"]"#.to_string(),
                scoring_direction: Some("TextMatch".to_string()),
                scoring_weight: Some(1.4),
                scoring_thresholds_json: "[]".to_string(),
            }],
            vec![raw.record.materialization_id.clone()],
            vec![SourceWatermark {
                source: "reddit:BangaloreRealEstates".to_string(),
                high_watermark: "2026-07-13T04:35:00Z".to_string(),
            }],
        )
        .await
        .unwrap();

    assert_eq!(materialization.manifest.fact_count, 1);
    assert_eq!(materialization.manifest.format_version, 2);
    assert_eq!(materialization.manifest.fact_annotation_count, 1);
    assert_eq!(
        materialization.manifest.fact_parquet_key,
        "silver/reddit_resident_facts/source=reddit/dt=2026-07-13/run_id=run-reddit-facts-2026-07-13/facts/part-00000.parquet"
    );
    assert_eq!(
        materialization.manifest.fact_annotation_parquet_key,
        "silver/reddit_resident_facts/source=reddit/dt=2026-07-13/run_id=run-reddit-facts-2026-07-13/fact_annotations/part-00000.parquet"
    );
    assert_eq!(
        materialization.record.parent_materializations,
        vec![raw.record.materialization_id]
    );
    assert_eq!(materialization.record.row_count, 1);

    let fact_bytes = lake
        .get_bytes(&LakeKey::new(materialization.manifest.fact_parquet_key.clone()).unwrap())
        .await
        .unwrap();
    let annotation_bytes = lake
        .get_bytes(
            &LakeKey::new(materialization.manifest.fact_annotation_parquet_key.clone()).unwrap(),
        )
        .await
        .unwrap();

    assert_is_parquet(&fact_bytes);
    assert_is_parquet(&annotation_bytes);
    assert_eq!(parquet_rows(&fact_bytes), 1);
    assert_eq!(parquet_rows(&annotation_bytes), 1);
    let fact_columns = parquet_columns(&fact_bytes);
    let annotation_columns = parquet_columns(&annotation_bytes);
    assert!(fact_columns.contains(&"input_hash".to_string()));
    assert!(fact_columns.contains(&"value_text".to_string()));
    assert!(fact_columns.contains(&"value_number".to_string()));
    assert!(fact_columns.contains(&"value_tags".to_string()));
    assert!(!fact_columns.contains(&"value_json".to_string()));
    assert!(!fact_columns.contains(&"answers_preferences_json".to_string()));
    assert!(annotation_columns.contains(&"answers_preferences".to_string()));
    assert!(annotation_columns.contains(&"scoring_thresholds".to_string()));
    assert!(!annotation_columns.contains(&"answers_preferences_json".to_string()));
    assert!(!annotation_columns.contains(&"scoring_thresholds_json".to_string()));

    let current = AssetMaterializationStore::new(lake)
        .current_record(
            &AssetId::new("reddit_resident_facts").unwrap(),
            &AssetPartition::new([("dt", "2026-07-13"), ("source", "reddit")]),
        )
        .await
        .unwrap();
    assert_eq!(
        current.materialization_id,
        materialization.record.materialization_id
    );
}

#[tokio::test]
async fn empty_skill_fact_batches_do_not_promote_current_pointer() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();

    let err = SkillFactMaterializer::new(lake.clone())
        .materialize_and_promote(
            "reddit_resident_facts",
            "reddit",
            "2026-07-13",
            "run-empty",
            &[],
            &[],
            Vec::new(),
            Vec::new(),
        )
        .await
        .unwrap_err();

    assert!(matches!(err, SkillFactMaterializeError::EmptyFacts));
    let current = AssetMaterializationStore::new(lake)
        .current_record(
            &AssetId::new("reddit_resident_facts").unwrap(),
            &AssetPartition::new([("dt", "2026-07-13"), ("source", "reddit")]),
        )
        .await;
    assert!(current.unwrap_err().is_not_found());
}

#[tokio::test]
async fn skill_fact_typed_parquet_round_trips_values_and_annotations() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let learned_at = Utc.with_ymd_and_hms(2026, 7, 13, 5, 0, 0).unwrap();
    let values = [
        ("numeric_fact", FactValue::Numeric(42.5), "numeric"),
        (
            "text_fact",
            FactValue::Text("calm layout".to_string()),
            "text",
        ),
        ("bool_fact", FactValue::Bool(true), "bool"),
        (
            "tags_fact",
            FactValue::Tags(vec!["trees".to_string(), "pool".to_string()]),
            "tags",
        ),
        (
            "score_fact",
            FactValue::Score {
                value: 0.82,
                explanation: "strong resident signal".to_string(),
            },
            "score",
        ),
    ];
    let facts = values
        .iter()
        .map(|(key, value, value_type)| SkillFactRecord {
            entity_id: "society:typed-home".to_string(),
            fact_key: key.to_string(),
            value_type: value_type.to_string(),
            value_json: serde_json::to_string(value).unwrap(),
            confidence: 0.8,
            source_type: "Reddit".to_string(),
            source_url: None,
            model: None,
            skill_id: Some("typed_fact_test".to_string()),
            triggered_by: None,
            learned_at,
            run_id: "run-typed-facts".to_string(),
            input_hash: format!("sha256:{key}"),
        })
        .collect::<Vec<_>>();
    let annotations = vec![SkillFactAnnotationRecord {
        entity_id: "society:typed-home".to_string(),
        fact_key: "text_fact".to_string(),
        display_template: Some("Signal: {value}".to_string()),
        answers_preferences_json: r#"["greenery","quiet"]"#.to_string(),
        scoring_direction: Some("TextMatch".to_string()),
        scoring_weight: Some(1.2),
        scoring_thresholds_json: "[0.8,0.5]".to_string(),
    }];

    let materialization = SkillFactMaterializer::new(lake.clone())
        .materialize_and_promote(
            "reddit_resident_facts",
            "reddit",
            "2026-07-13",
            "run-typed-facts",
            &facts,
            &annotations,
            Vec::new(),
            Vec::new(),
        )
        .await
        .unwrap();
    let fact_bytes = lake
        .get_bytes(&LakeKey::new(materialization.manifest.fact_parquet_key.clone()).unwrap())
        .await
        .unwrap();
    let annotation_bytes = lake
        .get_bytes(
            &LakeKey::new(materialization.manifest.fact_annotation_parquet_key.clone()).unwrap(),
        )
        .await
        .unwrap();

    let fact_columns = parquet_columns(&fact_bytes);
    assert!(fact_columns.contains(&"value_number".to_string()));
    assert!(fact_columns.contains(&"value_bool".to_string()));
    assert!(fact_columns.contains(&"value_tags".to_string()));
    assert!(fact_columns.contains(&"value_score".to_string()));
    assert!(!fact_columns.contains(&"value_json".to_string()));
    let annotation_columns = parquet_columns(&annotation_bytes);
    assert!(annotation_columns.contains(&"answers_preferences".to_string()));
    assert!(annotation_columns.contains(&"scoring_thresholds".to_string()));
    assert!(!annotation_columns.contains(&"answers_preferences_json".to_string()));

    let rows = read_skill_fact_artifact_rows(&lake, &[materialization.record])
        .await
        .unwrap();
    assert_eq!(rows.facts, facts);
    assert_eq!(rows.fact_annotations, annotations);
}

#[tokio::test]
async fn skill_fact_reader_rejects_corrupt_artifact_metadata() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let materialization = SkillFactMaterializer::new(lake.clone())
        .materialize_and_promote(
            "reddit_resident_facts",
            "reddit",
            "2026-07-13",
            "run-reddit-facts-2026-07-13",
            &[SkillFactRecord {
                entity_id: "society:large-green".to_string(),
                fact_key: "resident_greenery_signal".to_string(),
                value_type: "text".to_string(),
                value_json: r#"{"type":"Text","data":"Residents mention trees"}"#.to_string(),
                confidence: 0.7,
                source_type: "Reddit".to_string(),
                source_url: Some(
                    "https://reddit.com/r/BangaloreRealEstates/comments/alpha".to_string(),
                ),
                model: None,
                skill_id: Some("reddit_resident_fact_extractor".to_string()),
                triggered_by: Some("3bhk whitefield greenery".to_string()),
                learned_at: Utc.with_ymd_and_hms(2026, 7, 13, 4, 35, 0).unwrap(),
                run_id: "run-reddit-facts-2026-07-13".to_string(),
                input_hash: "sha256:reddit-alpha".to_string(),
            }],
            &[SkillFactAnnotationRecord {
                entity_id: "society:large-green".to_string(),
                fact_key: "resident_greenery_signal".to_string(),
                display_template: Some("Resident greenery signal: {value}".to_string()),
                answers_preferences_json: r#"["greenery"]"#.to_string(),
                scoring_direction: Some("TextMatch".to_string()),
                scoring_weight: Some(1.4),
                scoring_thresholds_json: "[]".to_string(),
            }],
            Vec::new(),
            Vec::new(),
        )
        .await
        .unwrap();
    let mut corrupt_record = materialization.record.clone();
    let fact_artifact = corrupt_record
        .artifacts
        .iter_mut()
        .find(|artifact| artifact.key.ends_with("facts/part-00000.parquet"))
        .unwrap();
    fact_artifact.content_type = "application/json".to_string();

    let err = read_skill_fact_artifact_rows(&lake, &[corrupt_record])
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        SkillFactMaterializeError::InvalidArtifactMetadata { .. }
    ));
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
    let mut reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
        bytes::Bytes::copy_from_slice(bytes),
    )
    .unwrap()
    .build()
    .unwrap();
    let batch = reader.next().unwrap().unwrap();
    batch
        .schema()
        .fields()
        .iter()
        .map(|field| field.name().to_string())
        .collect()
}
