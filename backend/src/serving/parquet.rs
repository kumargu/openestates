use std::fmt;
use std::sync::Arc;

use arrow::array::{Array, ArrayRef, Float32Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;

use crate::knowledge::FactValue;
use crate::parquet_data::{
    float64_list_array, float64_list_field, optional_f64_list_column_value,
    optional_string_list_column_value, string_list_array, string_list_field, typed_value_arrays,
    typed_value_fields, typed_value_from_batch, OptionalListColumn, TypedFactValue,
    ANSWERS_PREFERENCES_COLUMN, SCORING_THRESHOLDS_COLUMN,
};

use super::{ServingEntityRecord, ServingEdgeRecord, ServingFactRecord, ServingSearchMetadataRecord};

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

pub fn read_entities_parquet(bytes: &[u8]) -> Result<Vec<ServingEntityRecord>, ParquetReadError> {
    let mut records = Vec::new();
    for batch in ParquetRecordBatchReaderBuilder::try_new(Bytes::copy_from_slice(bytes))?.build()? {
        let batch = batch?;
        let entity_id = string_column(&batch, "entity_id")?;
        let entity_type = string_column(&batch, "entity_type")?;
        let name = string_column(&batch, "name")?;
        let root_source = string_column(&batch, "root_source")?;
        let searchable_text = string_column(&batch, "searchable_text")?;

        for row in 0..batch.num_rows() {
            records.push(ServingEntityRecord {
                entity_id: required_string(entity_id, row, "entity_id")?,
                entity_type: required_string(entity_type, row, "entity_type")?,
                name: required_string(name, row, "name")?,
                root_source: optional_string(root_source, row),
                searchable_text: required_string(searchable_text, row, "searchable_text")?,
            });
        }
    }
    Ok(records)
}

pub fn write_facts_parquet(facts: &[ServingFactRecord]) -> Result<Vec<u8>, ParquetWriteError> {
    let typed_values = facts
        .iter()
        .map(|fact| {
            validate_fact_value_type(&fact.value_type, &fact.value)?;
            Ok(TypedFactValue::from_fact_value(&fact.value))
        })
        .collect::<Result<Vec<_>, ParquetWriteError>>()?;

    let mut fields = vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("fact_key", DataType::Utf8, false),
        Field::new("value_type", DataType::Utf8, false),
    ];
    fields.extend(typed_value_fields(true));
    fields.extend([
        Field::new("confidence", DataType::Float32, false),
        Field::new("source_type", DataType::Utf8, false),
        Field::new("source_url", DataType::Utf8, true),
        Field::new("model", DataType::Utf8, true),
        Field::new("skill_id", DataType::Utf8, true),
        Field::new("learned_at", DataType::Utf8, false),
    ]);
    let schema = Arc::new(Schema::new(fields));

    let mut columns = vec![
        string_array(facts.iter().map(|fact| fact.entity_id.clone())),
        string_array(facts.iter().map(|fact| fact.fact_key.clone())),
        string_array(facts.iter().map(|fact| fact.value_type.clone())),
    ];
    columns.extend(typed_value_arrays(&typed_values, true));
    columns.extend([
        Arc::new(Float32Array::from(
            facts.iter().map(|fact| fact.confidence).collect::<Vec<_>>(),
        )) as ArrayRef,
        string_array(facts.iter().map(|fact| fact.source_type.clone())),
        optional_string_array(facts.iter().map(|fact| fact.source_url.clone()).collect()),
        optional_string_array(facts.iter().map(|fact| fact.model.clone()).collect()),
        optional_string_array(facts.iter().map(|fact| fact.skill_id.clone()).collect()),
        string_array(facts.iter().map(|fact| fact.learned_at.to_rfc3339())),
    ]);

    let batch = RecordBatch::try_new(schema.clone(), columns).map_err(ParquetWriteError::Arrow)?;

    write_batch(batch)
}

pub fn write_search_metadata_parquet(
    records: &[ServingSearchMetadataRecord],
) -> Result<Vec<u8>, ParquetWriteError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("fact_key", DataType::Utf8, false),
        Field::new("display_template", DataType::Utf8, true),
        string_list_field(ANSWERS_PREFERENCES_COLUMN, false),
        Field::new("scoring_direction", DataType::Utf8, true),
        Field::new("scoring_weight", DataType::Float32, true),
        float64_list_field(SCORING_THRESHOLDS_COLUMN, false),
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
            string_list_array(
                records
                    .iter()
                    .map(|record| Some(record.answers_preferences.clone())),
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
            float64_list_array(
                records
                    .iter()
                    .map(|record| Some(record.scoring_thresholds.clone())),
            ),
        ],
    )
    .map_err(ParquetWriteError::Arrow)?;

    write_batch(batch)
}

pub fn read_facts_parquet(bytes: &[u8]) -> Result<Vec<ServingFactRecord>, ParquetReadError> {
    let mut records = Vec::new();
    for batch in ParquetRecordBatchReaderBuilder::try_new(Bytes::copy_from_slice(bytes))?.build()? {
        let batch = batch?;
        let entity_id = string_column(&batch, "entity_id")?;
        let fact_key = string_column(&batch, "fact_key")?;
        let value_type = string_column(&batch, "value_type")?;
        let value_text = optional_string_column(&batch, "value_text")?;
        let legacy_value_json = optional_string_column(&batch, "value_json")?;
        let confidence = float32_column(&batch, "confidence")?;
        let source_type = string_column(&batch, "source_type")?;
        let source_url = string_column(&batch, "source_url")?;
        let model = string_column(&batch, "model")?;
        let skill_id = string_column(&batch, "skill_id")?;
        let learned_at = string_column(&batch, "learned_at")?;

        for row in 0..batch.num_rows() {
            let value_type = required_string(value_type, row, "value_type")?;
            let typed_value = typed_value_from_batch(&batch, row);
            let value =
                fact_value_from_batch(typed_value.as_ref(), legacy_value_json, row, &value_type)?;
            let typed_value_text = typed_value.and_then(|value| value.value_text);
            records.push(ServingFactRecord {
                entity_id: required_string(entity_id, row, "entity_id")?,
                fact_key: required_string(fact_key, row, "fact_key")?,
                value_type,
                value_text: value_text
                    .and_then(|value_text| optional_string(value_text, row))
                    .or(typed_value_text),
                value,
                confidence: required_f32(confidence, row, "confidence")?,
                source_type: required_string(source_type, row, "source_type")?,
                source_url: optional_string(source_url, row),
                model: optional_string(model, row),
                skill_id: optional_string(skill_id, row),
                learned_at: DateTime::parse_from_rfc3339(&required_string(
                    learned_at,
                    row,
                    "learned_at",
                )?)?
                .with_timezone(&Utc),
            });
        }
    }
    Ok(records)
}

pub fn read_search_metadata_parquet(
    bytes: &[u8],
) -> Result<Vec<ServingSearchMetadataRecord>, ParquetReadError> {
    let mut records = Vec::new();
    for batch in ParquetRecordBatchReaderBuilder::try_new(Bytes::copy_from_slice(bytes))?.build()? {
        let batch = batch?;
        let entity_id = string_column(&batch, "entity_id")?;
        let fact_key = string_column(&batch, "fact_key")?;
        let display_template = string_column(&batch, "display_template")?;
        let legacy_answers_preferences_json =
            optional_string_column(&batch, "answers_preferences_json")?;
        let scoring_direction = string_column(&batch, "scoring_direction")?;
        let scoring_weight = float32_column(&batch, "scoring_weight")?;

        for row in 0..batch.num_rows() {
            let scoring_thresholds =
                match optional_f64_list_column_value(&batch, SCORING_THRESHOLDS_COLUMN, row)
                    .map_err(|message| ParquetReadError::InvalidTypedValue { row, message })?
                {
                    OptionalListColumn::Values(values) => values,
                    OptionalListColumn::Missing | OptionalListColumn::Null => Vec::new(),
                };
            records.push(ServingSearchMetadataRecord {
                entity_id: required_string(entity_id, row, "entity_id")?,
                fact_key: required_string(fact_key, row, "fact_key")?,
                display_template: optional_string(display_template, row),
                answers_preferences: answers_preferences_from_batch(
                    &batch,
                    legacy_answers_preferences_json,
                    row,
                )?,
                scoring_direction: optional_string(scoring_direction, row),
                scoring_weight: optional_f32(scoring_weight, row),
                scoring_thresholds,
            });
        }
    }
    Ok(records)
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

fn fact_value_from_batch(
    typed_value: Option<&TypedFactValue>,
    legacy_json: Option<&StringArray>,
    row: usize,
    value_type: &str,
) -> Result<FactValue, ParquetReadError> {
    if let Some(legacy_json) = legacy_json {
        let value = serde_json::from_str(&required_string(legacy_json, row, "value_json")?)?;
        validate_read_fact_value_type(value_type, &value, row)?;
        return Ok(value);
    }

    let typed_value = typed_value.ok_or_else(|| ParquetReadError::InvalidTypedValue {
        row,
        message: "missing typed fact value columns".to_string(),
    })?;
    let value = typed_value.to_fact_value(value_type).ok_or_else(|| {
        ParquetReadError::InvalidTypedValue {
            row,
            message: format!("typed columns do not match value_type {value_type}"),
        }
    })?;
    Ok(value)
}

fn validate_fact_value_type(value_type: &str, value: &FactValue) -> Result<(), ParquetWriteError> {
    if TypedFactValue::value_type_matches(value_type, value) {
        return Ok(());
    }
    Err(ParquetWriteError::InvalidFactValueType {
        value_type: value_type.to_string(),
        actual_type: TypedFactValue::value_type_for(value).to_string(),
    })
}

fn validate_read_fact_value_type(
    value_type: &str,
    value: &FactValue,
    row: usize,
) -> Result<(), ParquetReadError> {
    if TypedFactValue::value_type_matches(value_type, value) {
        return Ok(());
    }
    Err(ParquetReadError::InvalidTypedValue {
        row,
        message: format!(
            "value_type {value_type} does not match fact value type {}",
            TypedFactValue::value_type_for(value)
        ),
    })
}

fn answers_preferences_from_batch(
    batch: &RecordBatch,
    legacy_json: Option<&StringArray>,
    row: usize,
) -> Result<Vec<String>, ParquetReadError> {
    if let Some(legacy_json) = legacy_json {
        return Ok(serde_json::from_str(&required_string(
            legacy_json,
            row,
            "answers_preferences_json",
        )?)?);
    }
    Ok(
        match optional_string_list_column_value(batch, ANSWERS_PREFERENCES_COLUMN, row) {
            Ok(OptionalListColumn::Values(values)) => values,
            Ok(OptionalListColumn::Null) => Vec::new(),
            Ok(OptionalListColumn::Missing) => {
                return Err(ParquetReadError::InvalidTypedValue {
                    row,
                    message: format!("missing typed list column {ANSWERS_PREFERENCES_COLUMN}"),
                });
            }
            Err(message) => return Err(ParquetReadError::InvalidTypedValue { row, message }),
        },
    )
}

fn string_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a StringArray, ParquetReadError> {
    let index = batch.schema().index_of(name)?;
    batch
        .column(index)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| ParquetReadError::InvalidColumn {
            name: name.to_string(),
            expected: "Utf8",
        })
}

fn optional_string_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<Option<&'a StringArray>, ParquetReadError> {
    let Ok(index) = batch.schema().index_of(name) else {
        return Ok(None);
    };
    batch
        .column(index)
        .as_any()
        .downcast_ref::<StringArray>()
        .map(Some)
        .ok_or_else(|| ParquetReadError::InvalidColumn {
            name: name.to_string(),
            expected: "Utf8",
        })
}

fn float32_column<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a Float32Array, ParquetReadError> {
    let index = batch.schema().index_of(name)?;
    batch
        .column(index)
        .as_any()
        .downcast_ref::<Float32Array>()
        .ok_or_else(|| ParquetReadError::InvalidColumn {
            name: name.to_string(),
            expected: "Float32",
        })
}

fn required_string(
    array: &StringArray,
    row: usize,
    column: &str,
) -> Result<String, ParquetReadError> {
    if array.is_null(row) {
        return Err(ParquetReadError::UnexpectedNull {
            column: column.to_string(),
            row,
        });
    }
    Ok(array.value(row).to_string())
}

fn optional_string(array: &StringArray, row: usize) -> Option<String> {
    if array.is_null(row) {
        None
    } else {
        Some(array.value(row).to_string())
    }
}

fn required_f32(array: &Float32Array, row: usize, column: &str) -> Result<f32, ParquetReadError> {
    if array.is_null(row) {
        return Err(ParquetReadError::UnexpectedNull {
            column: column.to_string(),
            row,
        });
    }
    Ok(array.value(row))
}

fn optional_f32(array: &Float32Array, row: usize) -> Option<f32> {
    if array.is_null(row) {
        None
    } else {
        Some(array.value(row))
    }
}

#[derive(Debug)]
pub enum ParquetWriteError {
    Arrow(arrow::error::ArrowError),
    InvalidFactValueType {
        value_type: String,
        actual_type: String,
    },
    Parquet(parquet::errors::ParquetError),
}

impl fmt::Display for ParquetWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrow(err) => write!(f, "Arrow record batch error: {err}"),
            Self::InvalidFactValueType {
                value_type,
                actual_type,
            } => write!(
                f,
                "serving fact value_type {value_type} does not match fact value type {actual_type}"
            ),
            Self::Parquet(err) => write!(f, "Parquet write error: {err}"),
        }
    }
}

impl std::error::Error for ParquetWriteError {}

#[derive(Debug)]
pub enum ParquetReadError {
    Arrow(arrow::error::ArrowError),
    Chrono(chrono::ParseError),
    InvalidColumn {
        name: String,
        expected: &'static str,
    },
    InvalidTypedValue {
        row: usize,
        message: String,
    },
    Json(serde_json::Error),
    Parquet(parquet::errors::ParquetError),
    UnexpectedNull {
        column: String,
        row: usize,
    },
}

impl fmt::Display for ParquetReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrow(err) => write!(f, "Arrow record batch error: {err}"),
            Self::Chrono(err) => write!(f, "timestamp parse error: {err}"),
            Self::InvalidColumn { name, expected } => {
                write!(f, "column {name} is not {expected}")
            }
            Self::InvalidTypedValue { row, message } => {
                write!(f, "invalid typed fact value at row {row}: {message}")
            }
            Self::Json(err) => write!(f, "Parquet JSON compatibility error: {err}"),
            Self::Parquet(err) => write!(f, "Parquet read error: {err}"),
            Self::UnexpectedNull { column, row } => {
                write!(f, "column {column} is unexpectedly null at row {row}")
            }
        }
    }
}

impl std::error::Error for ParquetReadError {}

impl From<arrow::error::ArrowError> for ParquetReadError {
    fn from(err: arrow::error::ArrowError) -> Self {
        Self::Arrow(err)
    }
}

impl From<chrono::ParseError> for ParquetReadError {
    fn from(err: chrono::ParseError) -> Self {
        Self::Chrono(err)
    }
}

impl From<serde_json::Error> for ParquetReadError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

impl From<parquet::errors::ParquetError> for ParquetReadError {
    fn from(err: parquet::errors::ParquetError) -> Self {
        Self::Parquet(err)
    }
}

pub fn write_edges_parquet(edges: &[ServingEdgeRecord]) -> Result<Vec<u8>, ParquetWriteError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("from_entity_id", DataType::Utf8, false),
        Field::new("edge_type", DataType::Utf8, false),
        Field::new("to_entity_id", DataType::Utf8, false),
        Field::new("confidence", DataType::Float32, false),
        Field::new("source_type", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            string_array(edges.iter().map(|edge| edge.from_entity_id.clone())),
            string_array(edges.iter().map(|edge| edge.edge_type.clone())),
            string_array(edges.iter().map(|edge| edge.to_entity_id.clone())),
            Arc::new(Float32Array::from(
                edges.iter().map(|edge| edge.confidence).collect::<Vec<_>>(),
            )) as ArrayRef,
            string_array(edges.iter().map(|edge| edge.source_type.clone())),
        ],
    )
    .map_err(ParquetWriteError::Arrow)?;
    write_batch(batch)
}

pub fn read_edges_parquet(bytes: &[u8]) -> Result<Vec<ServingEdgeRecord>, ParquetReadError> {
    let mut records = Vec::new();
    for batch in ParquetRecordBatchReaderBuilder::try_new(Bytes::copy_from_slice(bytes))?.build()? {
        let batch = batch?;
        let from_entity_id = string_column(&batch, "from_entity_id")?;
        let edge_type = string_column(&batch, "edge_type")?;
        let to_entity_id = string_column(&batch, "to_entity_id")?;
        let confidence = batch
            .column_by_name("confidence")
            .and_then(|column| column.as_any().downcast_ref::<Float32Array>())
            .ok_or_else(|| ParquetReadError::InvalidColumn {
                name: "confidence".to_string(),
                expected: "float32",
            })?;
        let source_type = string_column(&batch, "source_type")?;

        for row in 0..batch.num_rows() {
            records.push(ServingEdgeRecord {
                from_entity_id: required_string(from_entity_id, row, "from_entity_id")?,
                edge_type: required_string(edge_type, row, "edge_type")?,
                to_entity_id: required_string(to_entity_id, row, "to_entity_id")?,
                confidence: confidence.value(row),
                source_type: required_string(source_type, row, "source_type")?,
            });
        }
    }
    Ok(records)
}
