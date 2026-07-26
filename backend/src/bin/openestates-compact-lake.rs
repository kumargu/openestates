use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use arrow::record_batch::RecordBatch;
use chrono::Utc;
use datafusion::prelude::{ParquetReadOptions, SessionContext};
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::WriterProperties;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct CompactionConfig {
    policies: Vec<CompactionPolicy>,
}

#[derive(Debug, Deserialize)]
struct CompactionPolicy {
    id: String,
    enabled: bool,
    input_datasets: Vec<String>,
    output_dataset: String,
    target_file_size_mb: u64,
    small_file_threshold: u64,
    delta_row_threshold: u64,
    delta_byte_threshold_mb: u64,
    max_delta_age_hours: u64,
    reasoning: Vec<String>,
}

#[derive(Debug, Default)]
struct Args {
    policy_id: String,
    config_path: Option<PathBuf>,
    lake_root: Option<PathBuf>,
    output_dir: Option<PathBuf>,
    dry_run: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args(std::env::args().skip(1))?;
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend must live inside project root")
        .to_path_buf();
    let config_path = args
        .config_path
        .unwrap_or_else(|| project_root.join("app/config/dag/compaction_policies.json"));
    let lake_root = args
        .lake_root
        .unwrap_or_else(|| project_root.join("data/lake"));
    let config: CompactionConfig = serde_json::from_slice(&fs::read(&config_path)?)?;
    let policy = config
        .policies
        .iter()
        .find(|policy| policy.id == args.policy_id)
        .ok_or_else(|| format!("unknown compaction policy {:?}", args.policy_id))?;
    if !policy.enabled {
        println!("Compaction policy {:?} is disabled.", policy.id);
        return Ok(());
    }

    let input_files = collect_policy_inputs(&lake_root, policy)?;
    print_plan(policy, &lake_root, &input_files);
    if input_files.is_empty() || args.dry_run {
        return Ok(());
    }

    let output_dir = args.output_dir.unwrap_or_else(|| {
        lake_root
            .join(&policy.output_dataset)
            .join(format!("version={}", Utc::now().format("%Y%m%dT%H%M%SZ")))
    });
    fs::create_dir_all(&output_dir)?;
    let output_path = output_dir.join("part-00000.parquet");
    compact_parquet_files(&input_files, &output_path).await?;
    println!("Wrote compacted parquet: {}", output_path.display());
    Ok(())
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, Box<dyn Error>> {
    let mut parsed = Args {
        policy_id: "project_claim_facts".to_string(),
        ..Args::default()
    };
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--policy" => parsed.policy_id = required_value(&mut iter, "--policy")?,
            "--config" => {
                parsed.config_path = Some(PathBuf::from(required_value(&mut iter, "--config")?))
            }
            "--lake-root" => {
                parsed.lake_root = Some(PathBuf::from(required_value(&mut iter, "--lake-root")?))
            }
            "--output-dir" => {
                parsed.output_dir = Some(PathBuf::from(required_value(&mut iter, "--output-dir")?))
            }
            "--dry-run" => parsed.dry_run = true,
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument {other:?}").into()),
        }
    }
    Ok(parsed)
}

fn required_value(
    iter: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<String, Box<dyn Error>> {
    iter.next()
        .ok_or_else(|| format!("{flag} requires a value").into())
}

fn print_help() {
    println!("Compact immutable lake Parquet files into one policy output.");
    println!("  --policy <id>       Policy id from app/config/dag/compaction_policies.json");
    println!("  --lake-root <path>  Lake root, defaults to data/lake");
    println!(
        "  --output-dir <dir>  Output directory, defaults to policy output/version=<timestamp>"
    );
    println!("  --dry-run           Print the compaction plan without writing output");
}

fn print_plan(policy: &CompactionPolicy, lake_root: &Path, input_files: &[PathBuf]) {
    let bytes = input_files
        .iter()
        .filter_map(|path| fs::metadata(path).ok().map(|meta| meta.len()))
        .sum::<u64>();
    println!("Compaction policy: {}", policy.id);
    println!("Lake root: {}", lake_root.display());
    println!("Input datasets: {}", policy.input_datasets.join(", "));
    println!("Output dataset: {}", policy.output_dataset);
    println!("Input files: {}", input_files.len());
    println!("Input bytes: {}", bytes);
    println!(
        "Triggers: >{} small files, >{} delta rows, >{} MB delta bytes, >{}h delta age, target {} MB files",
        policy.small_file_threshold,
        policy.delta_row_threshold,
        policy.delta_byte_threshold_mb,
        policy.max_delta_age_hours,
        policy.target_file_size_mb
    );
    for reason in &policy.reasoning {
        println!("Reason: {reason}");
    }
}

fn collect_policy_inputs(
    lake_root: &Path,
    policy: &CompactionPolicy,
) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut files = Vec::new();
    for dataset in &policy.input_datasets {
        let root = lake_root.join(dataset);
        if root.exists() {
            collect_parquet_files(&root, &mut files)?;
        }
    }
    files.sort();
    Ok(files)
}

fn collect_parquet_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_parquet_files(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "parquet") {
            files.push(path);
        }
    }
    Ok(())
}

async fn compact_parquet_files(
    input_files: &[PathBuf],
    output_path: &Path,
) -> Result<(), Box<dyn Error>> {
    if input_files.is_empty() {
        return Err("no input parquet files to compact".into());
    }
    let context = SessionContext::new();
    let paths = input_files
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let frame = context
        .read_parquet(paths, ParquetReadOptions::default())
        .await?;
    let batches = frame.collect().await?;
    write_compacted_batches(&batches, output_path)?;
    Ok(())
}

fn write_compacted_batches(
    batches: &[RecordBatch],
    output_path: &Path,
) -> Result<(), Box<dyn Error>> {
    let Some(first) = batches.first() else {
        return Err("input parquet files produced no record batches".into());
    };
    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::try_new(3)?))
        .build();
    let file = fs::File::create(output_path)?;
    let mut writer = ArrowWriter::try_new(file, first.schema(), Some(props))?;
    for batch in batches {
        writer.write(batch)?;
    }
    writer.close()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_policy_and_dry_run() {
        let args = parse_args(["--dry-run".to_string()]).unwrap();
        assert_eq!(args.policy_id, "project_claim_facts");
        assert!(args.dry_run);
    }

    #[test]
    fn parses_policy_and_paths() {
        let args = parse_args([
            "--policy".to_string(),
            "rera".to_string(),
            "--lake-root".to_string(),
            "/tmp/lake".to_string(),
            "--output-dir".to_string(),
            "/tmp/out".to_string(),
        ])
        .unwrap();
        assert_eq!(args.policy_id, "rera");
        assert_eq!(args.lake_root.as_deref(), Some(Path::new("/tmp/lake")));
        assert_eq!(args.output_dir.as_deref(), Some(Path::new("/tmp/out")));
    }
}
