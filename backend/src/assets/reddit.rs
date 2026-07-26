use std::fmt;
use std::sync::Arc;

use arrow::array::{ArrayRef, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Utc};
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;
use serde::{Deserialize, Serialize};

use crate::lake::{LakeError, LakeStore};

use super::{
    ArtifactRef, AssetId, AssetMaterializationStore, AssetPartition, AssetPathBuilder, AssetStage,
    MaterializationId, MaterializationRecord, SourceWatermark,
};

pub const REDDIT_THREADS_DAILY_ASSET_ID: &str = "reddit_threads_daily";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedditThreadSnapshotRecord {
    pub thread_id: String,
    pub subreddit: String,
    pub query: String,
    pub title: String,
    pub url: Option<String>,
    pub score: i64,
    pub num_comments: i64,
    pub created_utc: Option<i64>,
    pub selftext: Option<String>,
    pub fetched_at: DateTime<Utc>,
    pub fetch_source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedditThreadSnapshotManifest {
    pub asset_id: String,
    pub format_version: u32,
    pub snapshot_date: String,
    pub subreddit: String,
    pub run_id: String,
    pub created_at: DateTime<Utc>,
    pub thread_count: u64,
    pub thread_parquet_key: String,
    pub manifest_key: String,
    pub artifacts: Vec<ArtifactRef>,
}

#[derive(Debug, Clone)]
pub struct RedditThreadSnapshotMaterialization {
    pub manifest: RedditThreadSnapshotManifest,
    pub record: MaterializationRecord,
}

#[derive(Clone)]
pub struct RedditThreadSnapshotMaterializer {
    lake: LakeStore,
    materializations: AssetMaterializationStore,
}

impl RedditThreadSnapshotMaterializer {
    pub fn new(lake: LakeStore) -> Self {
        let materializations = AssetMaterializationStore::new(lake.clone());
        Self {
            lake,
            materializations,
        }
    }

    pub async fn materialize_and_promote(
        &self,
        snapshot_date: impl Into<String>,
        subreddit: impl Into<String>,
        run_id: impl Into<String>,
        records: &[RedditThreadSnapshotRecord],
        source_watermarks: Vec<SourceWatermark>,
    ) -> Result<RedditThreadSnapshotMaterialization, RedditThreadSnapshotMaterializeError> {
        let snapshot_date = snapshot_date.into();
        let subreddit = subreddit.into();
        let run_id = run_id.into();
        let partition = AssetPartition::new([
            ("dt", snapshot_date.as_str()),
            ("subreddit", subreddit.as_str()),
        ]);
        let materialization = self
            .materialize_for_run(
                snapshot_date,
                subreddit,
                run_id,
                records,
                Vec::new(),
                source_watermarks,
                MaterializationId::new(),
                partition,
            )
            .await?;
        self.materializations
            .promote_current(&materialization.record)
            .await?;
        Ok(materialization)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn materialize_for_run(
        &self,
        snapshot_date: impl Into<String>,
        subreddit: impl Into<String>,
        run_id: impl Into<String>,
        records: &[RedditThreadSnapshotRecord],
        parent_materializations: Vec<MaterializationId>,
        source_watermarks: Vec<SourceWatermark>,
        dag_run_id: MaterializationId,
        record_partition: AssetPartition,
    ) -> Result<RedditThreadSnapshotMaterialization, RedditThreadSnapshotMaterializeError> {
        let snapshot_date = snapshot_date.into();
        let subreddit = subreddit.into();
        let run_id = run_id.into();
        let artifact_partition = AssetPartition::new([
            ("dt", snapshot_date.as_str()),
            ("subreddit", subreddit.as_str()),
        ]);
        let thread_key = AssetPathBuilder::raw_snapshot_key(
            "reddit",
            &artifact_partition,
            &run_id,
            "threads/part-00000.parquet",
        );
        let thread_meta = self
            .lake
            .put_bytes(&thread_key, write_threads_parquet(records)?)
            .await?;

        let manifest_key = AssetPathBuilder::raw_snapshot_key(
            "reddit",
            &artifact_partition,
            &run_id,
            "manifest.json",
        );
        let mut artifacts = vec![ArtifactRef::parquet(thread_meta)];
        let manifest = RedditThreadSnapshotManifest {
            asset_id: REDDIT_THREADS_DAILY_ASSET_ID.to_string(),
            format_version: 1,
            snapshot_date: snapshot_date.clone(),
            subreddit: subreddit.clone(),
            run_id: run_id.clone(),
            created_at: Utc::now(),
            thread_count: records.len() as u64,
            thread_parquet_key: thread_key.to_string(),
            manifest_key: manifest_key.to_string(),
            artifacts: artifacts.clone(),
        };
        let manifest_meta = self.lake.put_json(&manifest_key, &manifest).await?;
        artifacts.push(ArtifactRef::json(manifest_meta));
        artifacts.sort_by(|left, right| left.key.cmp(&right.key));

        let record = MaterializationRecord::succeeded(
            AssetId::new(REDDIT_THREADS_DAILY_ASSET_ID)
                .expect("static reddit threads asset id is valid"),
            AssetStage::Raw,
            record_partition,
            snapshot_date,
            artifacts,
        )
        .with_run_id(dag_run_id)
        .with_parent_materializations(parent_materializations)
        .with_source_watermarks(source_watermarks)
        .with_row_count(records.len() as u64);

        self.materializations.write_materialization(&record).await?;

        Ok(RedditThreadSnapshotMaterialization { manifest, record })
    }
}

fn write_threads_parquet(
    records: &[RedditThreadSnapshotRecord],
) -> Result<Vec<u8>, RedditThreadSnapshotMaterializeError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("thread_id", DataType::Utf8, false),
        Field::new("subreddit", DataType::Utf8, false),
        Field::new("query", DataType::Utf8, false),
        Field::new("title", DataType::Utf8, false),
        Field::new("url", DataType::Utf8, true),
        Field::new("score", DataType::Int64, false),
        Field::new("num_comments", DataType::Int64, false),
        Field::new("created_utc", DataType::Int64, true),
        Field::new("selftext", DataType::Utf8, true),
        Field::new("fetched_at", DataType::Utf8, false),
        Field::new("fetch_source", DataType::Utf8, false),
    ]));

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            string_array(records.iter().map(|record| record.thread_id.clone())),
            string_array(records.iter().map(|record| record.subreddit.clone())),
            string_array(records.iter().map(|record| record.query.clone())),
            string_array(records.iter().map(|record| record.title.clone())),
            optional_string_array(records.iter().map(|record| record.url.clone())),
            Arc::new(Int64Array::from(
                records
                    .iter()
                    .map(|record| record.score)
                    .collect::<Vec<_>>(),
            )),
            Arc::new(Int64Array::from(
                records
                    .iter()
                    .map(|record| record.num_comments)
                    .collect::<Vec<_>>(),
            )),
            Arc::new(Int64Array::from(
                records
                    .iter()
                    .map(|record| record.created_utc)
                    .collect::<Vec<_>>(),
            )),
            optional_string_array(records.iter().map(|record| record.selftext.clone())),
            string_array(records.iter().map(|record| record.fetched_at.to_rfc3339())),
            string_array(records.iter().map(|record| record.fetch_source.clone())),
        ],
    )
    .map_err(RedditThreadSnapshotMaterializeError::Arrow)?;

    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::default()))
        .build();
    let mut bytes = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut bytes, batch.schema(), Some(props))
        .map_err(RedditThreadSnapshotMaterializeError::Parquet)?;
    writer
        .write(&batch)
        .map_err(RedditThreadSnapshotMaterializeError::Parquet)?;
    writer
        .close()
        .map_err(RedditThreadSnapshotMaterializeError::Parquet)?;
    Ok(bytes)
}

fn string_array(values: impl Iterator<Item = String>) -> ArrayRef {
    Arc::new(StringArray::from(values.collect::<Vec<_>>()))
}

fn optional_string_array(values: impl Iterator<Item = Option<String>>) -> ArrayRef {
    Arc::new(StringArray::from(values.collect::<Vec<_>>()))
}

#[derive(Debug)]
pub enum RedditThreadSnapshotMaterializeError {
    Arrow(arrow::error::ArrowError),
    Lake(LakeError),
    Parquet(parquet::errors::ParquetError),
}

impl fmt::Display for RedditThreadSnapshotMaterializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrow(err) => write!(f, "reddit snapshot Arrow record batch error: {err}"),
            Self::Lake(err) => write!(f, "reddit snapshot lake error: {err}"),
            Self::Parquet(err) => write!(f, "reddit snapshot Parquet error: {err}"),
        }
    }
}

impl std::error::Error for RedditThreadSnapshotMaterializeError {}

impl From<LakeError> for RedditThreadSnapshotMaterializeError {
    fn from(err: LakeError) -> Self {
        Self::Lake(err)
    }
}
