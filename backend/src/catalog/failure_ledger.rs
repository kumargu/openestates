use std::sync::Arc;

use arrow::array::{ArrayRef, BooleanArray, StringArray, UInt32Array, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;
use serde::{Deserialize, Serialize};

use crate::serving::ParquetWriteError;

pub(super) const CATALOG_FAILURE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct CatalogFailureRecord {
    pub failure_id: String,
    pub operation_id: String,
    pub base_revision: Option<u64>,
    pub published_revision: u64,
    pub scope: String,
    pub society_id: Option<String>,
    pub asset_id: Option<String>,
    pub stage: String,
    pub error_code: String,
    pub message: String,
    pub disposition: String,
    pub retryable: bool,
    pub producer_hash: String,
    pub context_json: String,
    pub recorded_at: String,
}

pub(super) fn write_catalog_failures_parquet(
    records: &[CatalogFailureRecord],
) -> Result<Vec<u8>, ParquetWriteError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("schema_version", DataType::UInt32, false),
        Field::new("failure_id", DataType::Utf8, false),
        Field::new("operation_id", DataType::Utf8, false),
        Field::new("base_revision", DataType::UInt64, true),
        Field::new("published_revision", DataType::UInt64, false),
        Field::new("scope", DataType::Utf8, false),
        Field::new("society_id", DataType::Utf8, true),
        Field::new("asset_id", DataType::Utf8, true),
        Field::new("stage", DataType::Utf8, false),
        Field::new("error_code", DataType::Utf8, false),
        Field::new("message", DataType::Utf8, false),
        Field::new("disposition", DataType::Utf8, false),
        Field::new("retryable", DataType::Boolean, false),
        Field::new("producer_hash", DataType::Utf8, false),
        Field::new("context_json", DataType::Utf8, false),
        Field::new("recorded_at", DataType::Utf8, false),
    ]));
    let columns: Vec<ArrayRef> = vec![
        Arc::new(UInt32Array::from(vec![
            CATALOG_FAILURE_SCHEMA_VERSION;
            records.len()
        ])),
        strings(records.iter().map(|record| record.failure_id.clone())),
        strings(records.iter().map(|record| record.operation_id.clone())),
        Arc::new(UInt64Array::from(
            records
                .iter()
                .map(|record| record.base_revision)
                .collect::<Vec<_>>(),
        )),
        Arc::new(UInt64Array::from(
            records
                .iter()
                .map(|record| record.published_revision)
                .collect::<Vec<_>>(),
        )),
        strings(records.iter().map(|record| record.scope.clone())),
        optional_strings(records.iter().map(|record| record.society_id.clone())),
        optional_strings(records.iter().map(|record| record.asset_id.clone())),
        strings(records.iter().map(|record| record.stage.clone())),
        strings(records.iter().map(|record| record.error_code.clone())),
        strings(records.iter().map(|record| record.message.clone())),
        strings(records.iter().map(|record| record.disposition.clone())),
        Arc::new(BooleanArray::from(
            records
                .iter()
                .map(|record| record.retryable)
                .collect::<Vec<_>>(),
        )),
        strings(records.iter().map(|record| record.producer_hash.clone())),
        strings(records.iter().map(|record| record.context_json.clone())),
        strings(records.iter().map(|record| record.recorded_at.clone())),
    ];
    let batch = RecordBatch::try_new(schema.clone(), columns).map_err(ParquetWriteError::Arrow)?;
    let properties = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::default()))
        .build();
    let mut bytes = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut bytes, schema, Some(properties))
        .map_err(ParquetWriteError::Parquet)?;
    writer.write(&batch).map_err(ParquetWriteError::Parquet)?;
    writer.close().map_err(ParquetWriteError::Parquet)?;
    Ok(bytes)
}

fn strings(values: impl Iterator<Item = String>) -> ArrayRef {
    Arc::new(StringArray::from(values.collect::<Vec<_>>()))
}

fn optional_strings(values: impl Iterator<Item = Option<String>>) -> ArrayRef {
    Arc::new(StringArray::from(values.collect::<Vec<_>>()))
}
