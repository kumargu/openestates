use crate::{
    model::{Fixture, Policy},
    validate, Result,
};
use arrow::{
    array::{Array, ArrayRef, BooleanArray, Int64Array, StringArray},
    datatypes::{DataType, Field, Schema},
    record_batch::RecordBatch,
};
use parquet::arrow::{arrow_reader::ParquetRecordBatchReaderBuilder, ArrowWriter};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    path::Path,
    sync::Arc,
};

fn schema(table: &str) -> Schema {
    let (strings, numbers, booleans): (&[&str], &[&str], &[&str]) = match table {
        "homes" => (
            &["id", "title", "location", "area_basis", "image", "scenario"],
            &["bhk", "area_sqft", "floor"],
            &[],
        ),
        "observations" => (
            &[
                "id",
                "home_id",
                "provider_id",
                "advertisement_id",
                "source_label",
                "source_url",
                "relation",
                "seller",
                "availability",
                "predecessor_id",
                "area_basis",
                "observed_on",
                "first_seen",
                "last_seen",
            ],
            &["amount_inr", "bhk", "area_sqft", "floor"],
            &["current", "reliable"],
        ),
        "signals" => (
            &["id", "observation_id", "label", "value"],
            &[],
            &["disagrees"],
        ),
        _ => unreachable!("closed storage table contract"),
    };
    Schema::new(
        strings
            .iter()
            .map(|name| Field::new(*name, DataType::Utf8, true))
            .chain(
                numbers
                    .iter()
                    .map(|name| Field::new(*name, DataType::Int64, true)),
            )
            .chain(
                booleans
                    .iter()
                    .map(|name| Field::new(*name, DataType::Boolean, true)),
            )
            .collect::<Vec<_>>(),
    )
}

fn write<T: Serialize>(root: &Path, table: &str, rows: &[T]) -> Result<()> {
    let rows = rows
        .iter()
        .map(serde_json::to_value)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let schema = Arc::new(schema(table));
    let columns = schema
        .fields()
        .iter()
        .map(|field| -> ArrayRef {
            match field.data_type() {
                DataType::Utf8 => Arc::new(StringArray::from(
                    rows.iter()
                        .map(|r| r[field.name()].as_str())
                        .collect::<Vec<_>>(),
                )),
                DataType::Int64 => Arc::new(Int64Array::from(
                    rows.iter()
                        .map(|r| r[field.name()].as_i64())
                        .collect::<Vec<_>>(),
                )),
                DataType::Boolean => Arc::new(BooleanArray::from(
                    rows.iter()
                        .map(|r| r[field.name()].as_bool())
                        .collect::<Vec<_>>(),
                )),
                _ => unreachable!(),
            }
        })
        .collect();
    let batch = RecordBatch::try_new(schema.clone(), columns)?;
    let mut writer = ArrowWriter::try_new(
        File::create(root.join(format!("{table}.parquet")))?,
        schema,
        None,
    )?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(())
}

fn read<T: DeserializeOwned>(root: &Path, table: &str) -> Result<Vec<T>> {
    let reader = ParquetRecordBatchReaderBuilder::try_new(File::open(
        root.join(format!("{table}.parquet")),
    )?)?;
    if reader.schema().as_ref() != &schema(table) {
        return Err(format!("Invalid {table} schema").into());
    }
    let mut rows = Vec::new();
    for batch in reader.build()? {
        let batch = batch?;
        for index in 0..batch.num_rows() {
            let mut row = Map::new();
            for (field, column) in batch.schema().fields().iter().zip(batch.columns()) {
                let value = if column.is_null(index) {
                    Value::Null
                } else {
                    match field.data_type() {
                        DataType::Utf8 => Value::from(
                            column
                                .as_any()
                                .downcast_ref::<StringArray>()
                                .unwrap()
                                .value(index),
                        ),
                        DataType::Int64 => Value::from(
                            column
                                .as_any()
                                .downcast_ref::<Int64Array>()
                                .unwrap()
                                .value(index),
                        ),
                        DataType::Boolean => Value::from(
                            column
                                .as_any()
                                .downcast_ref::<BooleanArray>()
                                .unwrap()
                                .value(index),
                        ),
                        _ => return Err("Unexpected Parquet column".into()),
                    }
                };
                row.insert(field.name().clone(), value);
            }
            rows.push(serde_json::from_value(Value::Object(row))?);
        }
    }
    Ok(rows)
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    kind: String,
    contract_version: u32,
    snapshot_id: String,
    files: BTreeMap<String, String>,
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn snapshot_id(files: &BTreeMap<String, String>) -> Result<String> {
    Ok(format!(
        "inventory-preview-{}",
        digest(&serde_json::to_vec(files)?)
    ))
}
const FILES: [&str; 4] = [
    "homes.parquet",
    "observations.parquet",
    "signals.parquet",
    "policy.json",
];

/// Offline fixture materialization. Refuse to overwrite an existing snapshot.
pub fn materialize(fixture: &Fixture, policy: &Policy, root: &Path) -> Result<()> {
    validate(fixture, policy)?;
    fs::create_dir(root)?;
    write(root, "homes", &fixture.homes)?;
    write(root, "observations", &fixture.observations)?;
    write(root, "signals", &fixture.signals)?;
    fs::write(root.join("policy.json"), serde_json::to_vec_pretty(policy)?)?;
    let files: BTreeMap<String, String> = FILES
        .iter()
        .map(|name| Ok((name.to_string(), digest(&fs::read(root.join(name))?))))
        .collect::<Result<_>>()?;
    let manifest = Manifest {
        kind: "inventory_design_fixture".into(),
        contract_version: 1,
        snapshot_id: snapshot_id(&files)?,
        files,
    };
    fs::write(
        root.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(())
}

pub fn load(root: &Path) -> Result<(Fixture, Policy, String)> {
    let manifest: Manifest = serde_json::from_slice(&fs::read(root.join("manifest.json"))?)?;
    if manifest.kind != "inventory_design_fixture"
        || manifest.contract_version != 1
        || manifest.files.len() != FILES.len()
    {
        return Err("Not a supported preview snapshot".into());
    }
    for name in FILES {
        if manifest.files.get(name) != Some(&digest(&fs::read(root.join(name))?)) {
            return Err(format!("Snapshot checksum mismatch: {name}").into());
        }
    }
    if manifest.snapshot_id != snapshot_id(&manifest.files)? {
        return Err("Snapshot identity mismatch".into());
    }
    let fixture = Fixture {
        kind: manifest.kind,
        homes: read(root, "homes")?,
        observations: read(root, "observations")?,
        signals: read(root, "signals")?,
    };
    let policy = serde_json::from_slice(&fs::read(root.join("policy.json"))?)?;
    validate(&fixture, &policy)?;
    Ok((fixture, policy, manifest.snapshot_id))
}
