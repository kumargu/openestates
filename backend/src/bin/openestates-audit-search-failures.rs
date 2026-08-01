use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use backend::serving::{
    read_entities_parquet, read_facts_parquet, read_search_metadata_parquet, ServingEntityRecord,
    ServingFactRecord, ServingSearchMetadataRecord,
};
use serde::{Deserialize, Serialize};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = CliOptions::parse()?;
    let entities = read_entities_parquet(&fs::read(
        options.bundle.join("entities/part-00000.parquet"),
    )?)?;
    let facts = read_facts_parquet(&fs::read(options.bundle.join("facts/part-00000.parquet"))?)?;
    let metadata = read_search_metadata_parquet(&fs::read(
        options.bundle.join("search_metadata/part-00000.parquet"),
    )?)?;
    let benchmark: BenchmarkOutput = serde_json::from_slice(&fs::read(&options.benchmark)?)?;

    let failed_cases = benchmark
        .results
        .iter()
        .filter(|case| case.status != "PASS")
        .map(|case| audit_case(case, &entities, &facts, &metadata))
        .collect::<Vec<_>>();

    let mut summary = AuditSummary::default();
    for case in &failed_cases {
        *summary
            .by_bucket
            .entry(
                case.failure_bucket
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
            )
            .or_default() += 1;
        *summary
            .by_parquet_status
            .entry(case.parquet_status.clone())
            .or_default() += 1;
    }
    summary.failed_cases = failed_cases.len();

    let report = FailureAuditReport {
        benchmark: benchmark.benchmark,
        serving_bundle: bundle_version(&options.bundle),
        summary,
        failed_cases,
    };

    let payload = serde_json::to_string_pretty(&report)?;
    if let Some(output) = options.output {
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(output, payload)?;
    } else {
        println!("{payload}");
    }
    Ok(())
}

fn audit_case(
    case: &BenchmarkCaseResult,
    entities: &[ServingEntityRecord],
    facts: &[ServingFactRecord],
    metadata: &[ServingSearchMetadataRecord],
) -> FailureAuditCase {
    let expected_texts = expected_texts(case);
    let expected_fact_keys = expected_fact_keys(case);
    let text_matches = expected_texts
        .iter()
        .map(|text| text_match_summary(text, entities, facts, metadata))
        .collect::<Vec<_>>();
    let fact_key_matches = expected_fact_keys
        .iter()
        .map(|fact_key| fact_key_match_summary(fact_key, facts, metadata))
        .collect::<Vec<_>>();

    let any_expected_text = !expected_texts.is_empty();
    let any_text_present = text_matches.iter().any(|item| item.present_anywhere);
    let any_expected_fact_key = !expected_fact_keys.is_empty();
    let any_fact_key_present = fact_key_matches.iter().any(|item| item.fact_rows > 0);
    let any_fact_key_searchable = fact_key_matches
        .iter()
        .any(|item| item.search_metadata_rows > 0);

    let parquet_status = if any_expected_text && !any_text_present {
        "expected_entity_or_text_absent"
    } else if any_expected_fact_key && !any_fact_key_present {
        "expected_fact_key_absent"
    } else if any_expected_fact_key && any_fact_key_present && !any_fact_key_searchable {
        "fact_present_without_search_metadata"
    } else if any_text_present || any_fact_key_searchable {
        "present_in_serving_search_should_trace_runtime"
    } else {
        "no_specific_oracle_to_check"
    }
    .to_string();

    FailureAuditCase {
        id: case.id.clone(),
        query: case.query.clone(),
        category: case.category.clone(),
        failure_bucket: case.failure_bucket.clone(),
        failed_checks: case
            .checks
            .iter()
            .filter(|check| !check.passed)
            .map(|check| format!("{}.{}", check.layer, check.check))
            .collect(),
        parquet_status,
        expected_texts,
        expected_fact_keys,
        text_matches,
        fact_key_matches,
        top_results: case.top_results.clone(),
    }
}

fn expected_texts(case: &BenchmarkCaseResult) -> Vec<String> {
    let mut values = Vec::new();
    push_string_array(&mut values, &case.expected, "top_title_any");
    push_string_array(&mut values, &case.expected, "result_title_any");
    push_string_array(&mut values, &case.expected, "resolved_place_any");
    push_string_array(&mut values, &case.oracle, "expected_society_names");
    values.sort();
    values.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    values
}

fn expected_fact_keys(case: &BenchmarkCaseResult) -> Vec<String> {
    let mut values = Vec::new();
    push_string_array(&mut values, &case.expected, "reason_fact_keys_any");
    values.sort();
    values.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    values
}

fn push_string_array(values: &mut Vec<String>, object: &serde_json::Value, key: &str) {
    let Some(items) = object.get(key).and_then(|value| value.as_array()) else {
        return;
    };
    for item in items {
        let Some(text) = item.as_str().map(str::trim).filter(|text| !text.is_empty()) else {
            continue;
        };
        values.push(text.to_string());
    }
}

fn text_match_summary(
    text: &str,
    entities: &[ServingEntityRecord],
    facts: &[ServingFactRecord],
    metadata: &[ServingSearchMetadataRecord],
) -> TextMatchSummary {
    let needle = text.to_ascii_lowercase();
    let entity_matches = entities
        .iter()
        .filter(|entity| {
            contains_case_insensitive(&entity.name, &needle)
                || contains_case_insensitive(&entity.searchable_text, &needle)
        })
        .take(5)
        .map(|entity| EntityHit {
            entity_id: entity.entity_id.clone(),
            entity_type: entity.entity_type.clone(),
            name: entity.name.clone(),
        })
        .collect::<Vec<_>>();
    let fact_matches = facts
        .iter()
        .filter(|fact| {
            fact.value_text
                .as_deref()
                .is_some_and(|value| contains_case_insensitive(value, &needle))
        })
        .take(5)
        .map(|fact| FactHit {
            entity_id: fact.entity_id.clone(),
            fact_key: fact.fact_key.clone(),
            value_text: fact.value_text.clone(),
            confidence: fact.confidence,
            source_type: fact.source_type.clone(),
        })
        .collect::<Vec<_>>();
    let metadata_matches = metadata
        .iter()
        .filter(|row| {
            row.display_template
                .as_deref()
                .is_some_and(|value| contains_case_insensitive(value, &needle))
                || row
                    .answers_preferences
                    .iter()
                    .any(|value| contains_case_insensitive(value, &needle))
        })
        .take(5)
        .map(|row| MetadataHit {
            entity_id: row.entity_id.clone(),
            fact_key: row.fact_key.clone(),
            answers_preferences: row.answers_preferences.clone(),
            scoring_direction: row.scoring_direction.clone(),
        })
        .collect::<Vec<_>>();

    TextMatchSummary {
        text: text.to_string(),
        present_anywhere: !entity_matches.is_empty()
            || !fact_matches.is_empty()
            || !metadata_matches.is_empty(),
        entity_matches,
        fact_matches,
        metadata_matches,
    }
}

fn fact_key_match_summary(
    fact_key: &str,
    facts: &[ServingFactRecord],
    metadata: &[ServingSearchMetadataRecord],
) -> FactKeyMatchSummary {
    let fact_rows = facts
        .iter()
        .filter(|fact| fact.fact_key.eq_ignore_ascii_case(fact_key))
        .count();
    let search_metadata_rows = metadata
        .iter()
        .filter(|row| row.fact_key.eq_ignore_ascii_case(fact_key))
        .count();
    let sample_facts = facts
        .iter()
        .filter(|fact| fact.fact_key.eq_ignore_ascii_case(fact_key))
        .take(5)
        .map(|fact| FactHit {
            entity_id: fact.entity_id.clone(),
            fact_key: fact.fact_key.clone(),
            value_text: fact.value_text.clone(),
            confidence: fact.confidence,
            source_type: fact.source_type.clone(),
        })
        .collect();
    let sample_metadata = metadata
        .iter()
        .filter(|row| row.fact_key.eq_ignore_ascii_case(fact_key))
        .take(5)
        .map(|row| MetadataHit {
            entity_id: row.entity_id.clone(),
            fact_key: row.fact_key.clone(),
            answers_preferences: row.answers_preferences.clone(),
            scoring_direction: row.scoring_direction.clone(),
        })
        .collect();
    FactKeyMatchSummary {
        fact_key: fact_key.to_string(),
        fact_rows,
        search_metadata_rows,
        sample_facts,
        sample_metadata,
    }
}

fn contains_case_insensitive(value: &str, lowercase_needle: &str) -> bool {
    value.to_ascii_lowercase().contains(lowercase_needle)
}

fn bundle_version(bundle: &Path) -> Option<String> {
    let manifest_path = bundle.join("manifest.json");
    fs::read_to_string(manifest_path)
        .ok()
        .and_then(|payload| serde_json::from_str::<serde_json::Value>(&payload).ok())
        .and_then(|manifest| {
            manifest
                .get("bundle_version")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
}

#[derive(Debug)]
struct CliOptions {
    benchmark: PathBuf,
    bundle: PathBuf,
    output: Option<PathBuf>,
}

impl CliOptions {
    fn parse() -> Result<Self, String> {
        let mut args = std::env::args().skip(1);
        let mut benchmark = None;
        let mut bundle = None;
        let mut output = None;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--benchmark" => {
                    benchmark = Some(PathBuf::from(
                        args.next()
                            .ok_or_else(|| "--benchmark requires a value".to_string())?,
                    ));
                }
                "--bundle" => {
                    bundle = Some(PathBuf::from(
                        args.next()
                            .ok_or_else(|| "--bundle requires a value".to_string())?,
                    ));
                }
                "--output" => {
                    output = Some(PathBuf::from(
                        args.next()
                            .ok_or_else(|| "--output requires a value".to_string())?,
                    ));
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument: {other}")),
            }
        }

        Ok(Self {
            benchmark: benchmark.ok_or_else(|| "--benchmark is required".to_string())?,
            bundle: bundle.ok_or_else(|| "--bundle is required".to_string())?,
            output,
        })
    }
}

fn print_help() {
    println!("Audit benchmark failures against promoted serving bundle parquet.");
    println!();
    println!("Usage:");
    println!("  cargo run --bin openestates-audit-search-failures -- --benchmark <benchmark.json> --bundle <search_bundle_dir> [--output <report.json>]");
}

#[derive(Debug, Deserialize)]
struct BenchmarkOutput {
    benchmark: Option<String>,
    results: Vec<BenchmarkCaseResult>,
}

#[derive(Debug, Deserialize)]
struct BenchmarkCaseResult {
    id: String,
    category: Option<String>,
    query: String,
    status: String,
    #[serde(default)]
    failure_bucket: Option<String>,
    #[serde(default)]
    expected: serde_json::Value,
    #[serde(default)]
    oracle: serde_json::Value,
    #[serde(default)]
    checks: Vec<BenchmarkCheck>,
    #[serde(default)]
    top_results: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct BenchmarkCheck {
    layer: String,
    check: String,
    passed: bool,
}

#[derive(Debug, Serialize)]
struct FailureAuditReport {
    benchmark: Option<String>,
    serving_bundle: Option<String>,
    summary: AuditSummary,
    failed_cases: Vec<FailureAuditCase>,
}

#[derive(Debug, Default, Serialize)]
struct AuditSummary {
    failed_cases: usize,
    by_bucket: BTreeMap<String, usize>,
    by_parquet_status: BTreeMap<String, usize>,
}

#[derive(Debug, Serialize)]
struct FailureAuditCase {
    id: String,
    query: String,
    category: Option<String>,
    failure_bucket: Option<String>,
    failed_checks: Vec<String>,
    parquet_status: String,
    expected_texts: Vec<String>,
    expected_fact_keys: Vec<String>,
    text_matches: Vec<TextMatchSummary>,
    fact_key_matches: Vec<FactKeyMatchSummary>,
    top_results: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct TextMatchSummary {
    text: String,
    present_anywhere: bool,
    entity_matches: Vec<EntityHit>,
    fact_matches: Vec<FactHit>,
    metadata_matches: Vec<MetadataHit>,
}

#[derive(Debug, Serialize)]
struct FactKeyMatchSummary {
    fact_key: String,
    fact_rows: usize,
    search_metadata_rows: usize,
    sample_facts: Vec<FactHit>,
    sample_metadata: Vec<MetadataHit>,
}

#[derive(Debug, Serialize)]
struct EntityHit {
    entity_id: String,
    entity_type: String,
    name: String,
}

#[derive(Debug, Serialize)]
struct FactHit {
    entity_id: String,
    fact_key: String,
    value_text: Option<String>,
    confidence: f32,
    source_type: String,
}

#[derive(Debug, Serialize)]
struct MetadataHit {
    entity_id: String,
    fact_key: String,
    answers_preferences: Vec<String>,
    scoring_direction: Option<String>,
}
