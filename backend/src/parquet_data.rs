use std::sync::Arc;

use arrow::array::{
    Array, ArrayRef, BooleanArray, Float64Array, Float64Builder, ListArray, ListBuilder,
    StringArray, StringBuilder,
};
use arrow::datatypes::{DataType, Field};

use crate::knowledge::FactValue;

pub const VALUE_TEXT_COLUMN: &str = "value_text";
pub const VALUE_NUMBER_COLUMN: &str = "value_number";
pub const VALUE_BOOL_COLUMN: &str = "value_bool";
pub const VALUE_TAGS_COLUMN: &str = "value_tags";
pub const VALUE_SCORE_COLUMN: &str = "value_score";
pub const VALUE_SCORE_EXPLANATION_COLUMN: &str = "value_score_explanation";
pub const ANSWERS_PREFERENCES_COLUMN: &str = "answers_preferences";
pub const SCORING_THRESHOLDS_COLUMN: &str = "scoring_thresholds";

#[derive(Debug, Clone, PartialEq)]
pub struct TypedFactValue {
    pub value_text: Option<String>,
    pub value_number: Option<f64>,
    pub value_bool: Option<bool>,
    pub value_tags: Option<Vec<String>>,
    pub value_score: Option<f64>,
    pub value_score_explanation: Option<String>,
}

impl TypedFactValue {
    pub fn from_fact_value(value: &FactValue) -> Self {
        match value {
            FactValue::Numeric(value) => Self {
                value_text: Some(value.to_string()),
                value_number: Some(*value),
                value_bool: None,
                value_tags: None,
                value_score: None,
                value_score_explanation: None,
            },
            FactValue::Text(value) => Self {
                value_text: Some(value.clone()),
                value_number: None,
                value_bool: None,
                value_tags: None,
                value_score: None,
                value_score_explanation: None,
            },
            FactValue::Bool(value) => Self {
                value_text: Some(value.to_string()),
                value_number: None,
                value_bool: Some(*value),
                value_tags: None,
                value_score: None,
                value_score_explanation: None,
            },
            FactValue::Tags(values) => Self {
                value_text: Some(values.join(" ")),
                value_number: None,
                value_bool: None,
                value_tags: Some(values.clone()),
                value_score: None,
                value_score_explanation: None,
            },
            FactValue::Score { value, explanation } => Self {
                value_text: Some(format!("{value} {explanation}")),
                value_number: None,
                value_bool: None,
                value_tags: None,
                value_score: Some(*value),
                value_score_explanation: Some(explanation.clone()),
            },
        }
    }

    pub fn value_type_for(value: &FactValue) -> &'static str {
        match value {
            FactValue::Numeric(_) => "numeric",
            FactValue::Text(_) => "text",
            FactValue::Bool(_) => "bool",
            FactValue::Tags(_) => "tags",
            FactValue::Score { .. } => "score",
        }
    }

    pub fn value_type_matches(value_type: &str, value: &FactValue) -> bool {
        matches!(
            (value_type, value),
            ("numeric" | "number", FactValue::Numeric(_))
                | ("text", FactValue::Text(_))
                | ("bool" | "boolean", FactValue::Bool(_))
                | ("tags", FactValue::Tags(_))
                | ("score", FactValue::Score { .. })
        )
    }

    pub fn to_fact_value(&self, value_type: &str) -> Option<FactValue> {
        match value_type {
            "numeric" | "number" => self.value_number.map(FactValue::Numeric),
            "text" => self.value_text.clone().map(FactValue::Text),
            "bool" | "boolean" => self.value_bool.map(FactValue::Bool),
            "tags" => self.value_tags.clone().map(FactValue::Tags),
            "score" => Some(FactValue::Score {
                value: self.value_score?,
                explanation: self.value_score_explanation.clone().unwrap_or_default(),
            }),
            _ => None,
        }
    }
}

pub fn typed_value_fields(include_value_text: bool) -> Vec<Field> {
    let mut fields = Vec::new();
    if include_value_text {
        fields.push(Field::new(VALUE_TEXT_COLUMN, DataType::Utf8, true));
    }
    fields.extend([
        Field::new(VALUE_NUMBER_COLUMN, DataType::Float64, true),
        Field::new(VALUE_BOOL_COLUMN, DataType::Boolean, true),
        Field::new(
            VALUE_TAGS_COLUMN,
            DataType::List(Arc::new(Field::new_list_field(DataType::Utf8, true))),
            true,
        ),
        Field::new(VALUE_SCORE_COLUMN, DataType::Float64, true),
        Field::new(VALUE_SCORE_EXPLANATION_COLUMN, DataType::Utf8, true),
    ]);
    fields
}

pub fn typed_value_arrays(values: &[TypedFactValue], include_value_text: bool) -> Vec<ArrayRef> {
    let mut arrays = Vec::new();
    if include_value_text {
        arrays.push(optional_string_array(
            values.iter().map(|value| value.value_text.clone()),
        ));
    }
    arrays.extend([
        Arc::new(Float64Array::from(
            values
                .iter()
                .map(|value| value.value_number)
                .collect::<Vec<_>>(),
        )) as ArrayRef,
        Arc::new(BooleanArray::from(
            values
                .iter()
                .map(|value| value.value_bool)
                .collect::<Vec<_>>(),
        )) as ArrayRef,
        string_list_array(values.iter().map(|value| value.value_tags.clone())),
        Arc::new(Float64Array::from(
            values
                .iter()
                .map(|value| value.value_score)
                .collect::<Vec<_>>(),
        )) as ArrayRef,
        optional_string_array(
            values
                .iter()
                .map(|value| value.value_score_explanation.clone()),
        ),
    ]);
    arrays
}

pub fn typed_value_from_batch(
    batch: &arrow::record_batch::RecordBatch,
    row: usize,
) -> Option<TypedFactValue> {
    let value_tags = match optional_string_list_column_value(batch, VALUE_TAGS_COLUMN, row) {
        Ok(OptionalListColumn::Values(values)) => Some(values),
        Ok(OptionalListColumn::Missing | OptionalListColumn::Null) | Err(_) => None,
    };
    Some(TypedFactValue {
        value_text: optional_string_column_value(batch, VALUE_TEXT_COLUMN, row),
        value_number: optional_f64_column_value(batch, VALUE_NUMBER_COLUMN, row),
        value_bool: optional_bool_column_value(batch, VALUE_BOOL_COLUMN, row),
        value_tags,
        value_score: optional_f64_column_value(batch, VALUE_SCORE_COLUMN, row),
        value_score_explanation: optional_string_column_value(
            batch,
            VALUE_SCORE_EXPLANATION_COLUMN,
            row,
        ),
    })
}

pub fn string_list_field(name: &str, nullable: bool) -> Field {
    Field::new(
        name,
        DataType::List(Arc::new(Field::new_list_field(DataType::Utf8, true))),
        nullable,
    )
}

pub fn float64_list_field(name: &str, nullable: bool) -> Field {
    Field::new(
        name,
        DataType::List(Arc::new(Field::new_list_field(DataType::Float64, true))),
        nullable,
    )
}

pub fn string_list_array(values: impl Iterator<Item = Option<Vec<String>>>) -> ArrayRef {
    let mut builder = ListBuilder::new(StringBuilder::new());
    for value in values {
        match value {
            Some(items) => {
                for item in items {
                    builder.values().append_value(item);
                }
                builder.append(true);
            }
            None => builder.append(false),
        }
    }
    Arc::new(builder.finish())
}

pub fn float64_list_array(values: impl Iterator<Item = Option<Vec<f64>>>) -> ArrayRef {
    let mut builder = ListBuilder::new(Float64Builder::new());
    for value in values {
        match value {
            Some(items) => {
                for item in items {
                    builder.values().append_value(item);
                }
                builder.append(true);
            }
            None => builder.append(false),
        }
    }
    Arc::new(builder.finish())
}

pub fn optional_string_array(values: impl Iterator<Item = Option<String>>) -> ArrayRef {
    Arc::new(StringArray::from(values.collect::<Vec<_>>()))
}

pub enum OptionalListColumn<T> {
    Missing,
    Null,
    Values(Vec<T>),
}

pub fn optional_string_list_column_value(
    batch: &arrow::record_batch::RecordBatch,
    name: &str,
    row: usize,
) -> Result<OptionalListColumn<String>, String> {
    let index = match batch.schema().index_of(name) {
        Ok(index) => index,
        Err(_) => return Ok(OptionalListColumn::Missing),
    };
    let array = batch
        .column(index)
        .as_any()
        .downcast_ref::<ListArray>()
        .ok_or_else(|| format!("column {name} is not List<Utf8>"))?;
    if array.is_null(row) {
        return Ok(OptionalListColumn::Null);
    }
    let values = array.value(row);
    let strings = values
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| format!("column {name} values are not Utf8"))?;
    Ok(OptionalListColumn::Values(
        (0..strings.len())
            .filter(|index| !strings.is_null(*index))
            .map(|index| strings.value(index).to_string())
            .collect(),
    ))
}

pub fn optional_f64_list_column_value(
    batch: &arrow::record_batch::RecordBatch,
    name: &str,
    row: usize,
) -> Result<OptionalListColumn<f64>, String> {
    let index = match batch.schema().index_of(name) {
        Ok(index) => index,
        Err(_) => return Ok(OptionalListColumn::Missing),
    };
    let array = batch
        .column(index)
        .as_any()
        .downcast_ref::<ListArray>()
        .ok_or_else(|| format!("column {name} is not List<Float64>"))?;
    if array.is_null(row) {
        return Ok(OptionalListColumn::Null);
    }
    let values = array.value(row);
    let floats = values
        .as_any()
        .downcast_ref::<Float64Array>()
        .ok_or_else(|| format!("column {name} values are not Float64"))?;
    Ok(OptionalListColumn::Values(
        (0..floats.len())
            .filter(|index| !floats.is_null(*index))
            .map(|index| floats.value(index))
            .collect(),
    ))
}

fn optional_string_column_value(
    batch: &arrow::record_batch::RecordBatch,
    name: &str,
    row: usize,
) -> Option<String> {
    let index = batch.schema().index_of(name).ok()?;
    let array = batch.column(index).as_any().downcast_ref::<StringArray>()?;
    if array.is_null(row) {
        None
    } else {
        Some(array.value(row).to_string())
    }
}

fn optional_f64_column_value(
    batch: &arrow::record_batch::RecordBatch,
    name: &str,
    row: usize,
) -> Option<f64> {
    let index = batch.schema().index_of(name).ok()?;
    let array = batch
        .column(index)
        .as_any()
        .downcast_ref::<Float64Array>()?;
    if array.is_null(row) {
        None
    } else {
        Some(array.value(row))
    }
}

fn optional_bool_column_value(
    batch: &arrow::record_batch::RecordBatch,
    name: &str,
    row: usize,
) -> Option<bool> {
    let index = batch.schema().index_of(name).ok()?;
    let array = batch
        .column(index)
        .as_any()
        .downcast_ref::<BooleanArray>()?;
    if array.is_null(row) {
        None
    } else {
        Some(array.value(row))
    }
}
