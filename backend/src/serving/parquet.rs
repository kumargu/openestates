use std::fmt;
use std::sync::Arc;

use arrow::array::{ArrayRef, Float32Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;

use super::{ServingEntityRecord, ServingFactRecord, ServingSearchMetadataRecord};

pub fn write_entities_parquet(
    entities: &[ServingEntityRecord],
) -> Result<Vec<u8>, ParquetWriteError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("entity_type", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("root_source", DataType::Utf8, true),
        Field::new("searchable_text", DataType::Utf8, false),
    ]));

    let root_sources: Vec<Option<String>> = entities
        .iter()
        .map(|entity| entity.root_source.clone())
        .collect();

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            string_array(entities.iter().map(|entity| entity.entity_id.clone())),
            string_array(entities.iter().map(|entity| entity.entity_type.clone())),
            string_array(entities.iter().map(|entity| entity.name.clone())),
            optional_string_array(root_sources),
            string_array(entities.iter().map(|entity| entity.searchable_text.clone())),
        ],
    )
    .map_err(ParquetWriteError::Arrow)?;

    write_batch(batch)
}

pub fn write_facts_parquet(facts: &[ServingFactRecord]) -> Result<Vec<u8>, ParquetWriteError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("fact_key", DataType::Utf8, false),
        Field::new("value_type", DataType::Utf8, false),
        Field::new("value_text", DataType::Utf8, true),
        Field::new("value_json", DataType::Utf8, false),
        Field::new("confidence", DataType::Float32, false),
        Field::new("source_type", DataType::Utf8, false),
        Field::new("source_url", DataType::Utf8, true),
        Field::new("model", DataType::Utf8, true),
        Field::new("skill_id", DataType::Utf8, true),
        Field::new("learned_at", DataType::Utf8, false),
    ]));

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            string_array(facts.iter().map(|fact| fact.entity_id.clone())),
            string_array(facts.iter().map(|fact| fact.fact_key.clone())),
            string_array(facts.iter().map(|fact| fact.value_type.clone())),
            optional_string_array(facts.iter().map(|fact| fact.value_text.clone()).collect()),
            string_array(facts.iter().map(|fact| fact.value_json.clone())),
            Arc::new(Float32Array::from(
                facts.iter().map(|fact| fact.confidence).collect::<Vec<_>>(),
            )),
            string_array(facts.iter().map(|fact| fact.source_type.clone())),
            optional_string_array(facts.iter().map(|fact| fact.source_url.clone()).collect()),
            optional_string_array(facts.iter().map(|fact| fact.model.clone()).collect()),
            optional_string_array(facts.iter().map(|fact| fact.skill_id.clone()).collect()),
            string_array(facts.iter().map(|fact| fact.learned_at.to_rfc3339())),
        ],
    )
    .map_err(ParquetWriteError::Arrow)?;

    write_batch(batch)
}

pub fn write_search_metadata_parquet(
    records: &[ServingSearchMetadataRecord],
) -> Result<Vec<u8>, ParquetWriteError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("fact_key", DataType::Utf8, false),
        Field::new("display_template", DataType::Utf8, true),
        Field::new("answers_preferences_json", DataType::Utf8, false),
        Field::new("scoring_direction", DataType::Utf8, true),
        Field::new("scoring_weight", DataType::Float32, true),
    ]));

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            string_array(records.iter().map(|record| record.entity_id.clone())),
            string_array(records.iter().map(|record| record.fact_key.clone())),
            optional_string_array(
                records
                    .iter()
                    .map(|record| record.display_template.clone())
                    .collect(),
            ),
            string_array(
                records
                    .iter()
                    .map(|record| record.answers_preferences_json.clone()),
            ),
            optional_string_array(
                records
                    .iter()
                    .map(|record| record.scoring_direction.clone())
                    .collect(),
            ),
            Arc::new(Float32Array::from(
                records
                    .iter()
                    .map(|record| record.scoring_weight)
                    .collect::<Vec<_>>(),
            )),
        ],
    )
    .map_err(ParquetWriteError::Arrow)?;

    write_batch(batch)
}

fn write_batch(batch: RecordBatch) -> Result<Vec<u8>, ParquetWriteError> {
    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::default()))
        .build();
    let mut bytes = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut bytes, batch.schema(), Some(props))
        .map_err(ParquetWriteError::Parquet)?;
    writer.write(&batch).map_err(ParquetWriteError::Parquet)?;
    writer.close().map_err(ParquetWriteError::Parquet)?;
    Ok(bytes)
}

fn string_array(values: impl Iterator<Item = String>) -> ArrayRef {
    Arc::new(StringArray::from(values.collect::<Vec<_>>()))
}

fn optional_string_array(values: Vec<Option<String>>) -> ArrayRef {
    Arc::new(StringArray::from(values))
}

#[derive(Debug)]
pub enum ParquetWriteError {
    Arrow(arrow::error::ArrowError),
    Parquet(parquet::errors::ParquetError),
}

impl fmt::Display for ParquetWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrow(err) => write!(f, "Arrow record batch error: {err}"),
            Self::Parquet(err) => write!(f, "Parquet write error: {err}"),
        }
    }
}

impl std::error::Error for ParquetWriteError {}
