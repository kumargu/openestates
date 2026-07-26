use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

use arrow::array::{
    Array, ArrayRef, BooleanArray, Float32Array, Float64Array, Int64Array, StringArray,
    UInt32Array, UInt64Array,
};
use arrow::datatypes::DataType;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use serde_json::{json, Map, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = CliOptions::parse()?;
    let file = File::open(&options.path)?;
    let reader = ParquetRecordBatchReaderBuilder::try_new(file)?
        .with_batch_size(1024)
        .build()?;

    let mut emitted = 0_usize;
    for batch in reader {
        let batch = batch?;
        for row in 0..batch.num_rows() {
            if let Some((column, expected, mode)) = &options.filter {
                let Some(index) = batch.schema().index_of(column).ok() else {
                    continue;
                };
                let value = cell_to_json(batch.column(index), row);
                if value_matches(&value, expected, *mode) {
                    print_row(&batch, row)?;
                    emitted += 1;
                }
            } else {
                print_row(&batch, row)?;
                emitted += 1;
            }

            if emitted >= options.limit {
                return Ok(());
            }
        }
    }
    Ok(())
}

fn print_row(
    batch: &arrow::record_batch::RecordBatch,
    row: usize,
) -> Result<(), serde_json::Error> {
    let mut object = Map::new();
    for (index, field) in batch.schema().fields().iter().enumerate() {
        object.insert(field.name().clone(), cell_to_json(batch.column(index), row));
    }
    println!("{}", serde_json::to_string_pretty(&Value::Object(object))?);
    Ok(())
}

fn cell_to_json(column: &ArrayRef, row: usize) -> Value {
    if column.is_null(row) {
        return Value::Null;
    }
    match column.data_type() {
        DataType::Utf8 => downcast::<StringArray>(column)
            .map(|array| json!(array.value(row)))
            .unwrap_or(Value::Null),
        DataType::Float32 => downcast::<Float32Array>(column)
            .map(|array| json!(array.value(row)))
            .unwrap_or(Value::Null),
        DataType::Float64 => downcast::<Float64Array>(column)
            .map(|array| json!(array.value(row)))
            .unwrap_or(Value::Null),
        DataType::Int64 => downcast::<Int64Array>(column)
            .map(|array| json!(array.value(row)))
            .unwrap_or(Value::Null),
        DataType::UInt32 => downcast::<UInt32Array>(column)
            .map(|array| json!(array.value(row)))
            .unwrap_or(Value::Null),
        DataType::UInt64 => downcast::<UInt64Array>(column)
            .map(|array| json!(array.value(row)))
            .unwrap_or(Value::Null),
        DataType::Boolean => downcast::<BooleanArray>(column)
            .map(|array| json!(array.value(row)))
            .unwrap_or(Value::Null),
        _ => json!(format!("{:?}", column.slice(row, 1))),
    }
}

fn downcast<T: 'static>(column: &Arc<dyn Array>) -> Option<&T> {
    column.as_any().downcast_ref::<T>()
}

fn value_matches(value: &Value, expected: &str, mode: MatchMode) -> bool {
    match value {
        Value::String(text) => match mode {
            MatchMode::Exact => text == expected,
            MatchMode::Contains => text.to_lowercase().contains(&expected.to_lowercase()),
        },
        Value::Number(number) => match mode {
            MatchMode::Exact => number.to_string() == expected,
            MatchMode::Contains => number.to_string().contains(expected),
        },
        Value::Bool(flag) => match mode {
            MatchMode::Exact => flag.to_string() == expected,
            MatchMode::Contains => flag.to_string().contains(&expected.to_lowercase()),
        },
        Value::Null => matches!(mode, MatchMode::Exact) && expected.eq_ignore_ascii_case("null"),
        other => {
            let rendered = other.to_string();
            match mode {
                MatchMode::Exact => rendered == expected,
                MatchMode::Contains => rendered.to_lowercase().contains(&expected.to_lowercase()),
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MatchMode {
    Exact,
    Contains,
}

#[derive(Debug)]
struct CliOptions {
    path: PathBuf,
    limit: usize,
    filter: Option<(String, String, MatchMode)>,
}

impl CliOptions {
    fn parse() -> Result<Self, String> {
        let mut args = std::env::args().skip(1);
        let mut path = None;
        let mut limit = 20_usize;
        let mut filter = None;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--path" => {
                    path = Some(PathBuf::from(
                        args.next()
                            .ok_or_else(|| "--path requires a value".to_string())?,
                    ));
                }
                "--limit" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--limit requires a value".to_string())?;
                    limit = value
                        .parse()
                        .map_err(|_| "--limit requires a positive integer".to_string())?;
                    if limit == 0 {
                        return Err("--limit requires a positive integer".to_string());
                    }
                }
                "--filter" | "--filter-contains" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--filter requires column=value".to_string())?;
                    let Some((column, expected)) = value.split_once('=') else {
                        return Err("--filter requires column=value".to_string());
                    };
                    let mode = if arg == "--filter-contains" {
                        MatchMode::Contains
                    } else {
                        MatchMode::Exact
                    };
                    filter = Some((column.to_string(), expected.to_string(), mode));
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument: {other}")),
            }
        }

        Ok(Self {
            path: path.ok_or_else(|| "--path is required".to_string())?,
            limit,
            filter,
        })
    }
}

fn print_help() {
    println!("Dump rows from a local Parquet file as JSON.");
    println!();
    println!("Usage:");
    println!("  cargo run --bin openestates-dump-parquet -- --path <file> [--limit N] [--filter column=value] [--filter-contains column=value]");
}
