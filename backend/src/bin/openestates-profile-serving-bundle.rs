use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use backend::assets::MaterializationId;
use backend::data_loader::properties_from_serving_bundle;
use backend::knowledge::FactValue;
use backend::lake::LakeStoreLocation;
use backend::serving::{ServingBundleLoader, ServingFactRecord, ServingSearchMetadataRecord};
use chrono::{DateTime, Utc};
use serde::Serialize;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = CliOptions::parse()?;
    let project_root = options
        .project_root
        .clone()
        .unwrap_or_else(default_project_root);
    let lake = LakeStoreLocation::from_env(&project_root)?.open()?;
    let cache_root = project_root.join("data").join("cache").join("serving");
    let loader = ServingBundleLoader::new(lake, cache_root);
    let bundle = match &options.serving_materialization_id {
        Some(materialization_id) => loader
            .load_search_bundle_by_materialization(materialization_id)
            .await?
            .ok_or_else(|| format!("serving materialization {materialization_id} was not found"))?,
        None => loader
            .load_current_search_bundle()
            .await?
            .ok_or("no promoted search serving bundle found")?,
    };

    let mut fact_stats = BTreeMap::<String, FactKeyStats>::new();
    let mut metadata_stats = BTreeMap::<String, MetadataStats>::new();

    for (entity_id, rows) in bundle.fact_index.rows() {
        for fact in &rows.facts {
            fact_stats
                .entry(fact.fact_key.clone())
                .or_insert_with(|| FactKeyStats::new(&fact.fact_key))
                .record(entity_id, fact);
        }
        for metadata in &rows.search_metadata {
            metadata_stats
                .entry(metadata.fact_key.clone())
                .or_insert_with(|| MetadataStats::new(&metadata.fact_key))
                .record(metadata);
        }
    }

    let mut fact_keys = fact_stats
        .into_values()
        .map(|mut stats| {
            if let Some(metadata) = metadata_stats.get(&stats.fact_key) {
                stats.search_metadata_rows = metadata.rows;
                stats.answers_preferences = metadata.answers_preferences.iter().cloned().collect();
                stats.scoring_directions = metadata.scoring_directions.iter().cloned().collect();
            }
            stats.finish()
        })
        .collect::<Vec<_>>();
    fact_keys.sort_by(|left, right| {
        right
            .confident_rows
            .cmp(&left.confident_rows)
            .then_with(|| right.rows.cmp(&left.rows))
            .then_with(|| left.fact_key.cmp(&right.fact_key))
    });
    if let Some(limit) = options.limit {
        fact_keys.truncate(limit);
    }

    let properties = properties_from_serving_bundle(&bundle);
    let selected_facts = selected_fact_rows(&bundle, &options);
    let profile = ServingBundleProfile {
        bundle_version: bundle.manifest.bundle_version.clone(),
        entity_count: bundle.entities.len(),
        property_count: properties.len(),
        fact_count: bundle.manifest.fact_count,
        search_metadata_count: bundle.manifest.search_metadata_count,
        fact_keys,
        selected_facts,
    };

    if options.markdown {
        print_markdown(&profile);
    } else {
        println!("{}", serde_json::to_string_pretty(&profile)?);
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
struct ServingBundleProfile {
    bundle_version: String,
    entity_count: usize,
    property_count: usize,
    fact_count: u64,
    search_metadata_count: u64,
    fact_keys: Vec<FactKeyStats>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    selected_facts: Vec<SelectedFactRow>,
}

#[derive(Debug, Clone, Serialize)]
struct SelectedFactRow {
    entity_id: String,
    entity_type: String,
    entity_name: String,
    fact_key: String,
    value: FactValue,
    value_text: Option<String>,
    confidence: f32,
    source_type: String,
    learned_at: DateTime<Utc>,
    answers_preferences: Vec<String>,
}

fn selected_fact_rows(
    bundle: &backend::serving::LoadedServingBundle,
    options: &CliOptions,
) -> Vec<SelectedFactRow> {
    if options.fact_keys.is_empty() && options.entity_ids.is_empty() {
        return Vec::new();
    }

    let entities = bundle
        .entities
        .iter()
        .map(|entity| {
            (
                entity.entity_id.as_str(),
                (entity.entity_type.as_str(), entity.name.as_str()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut rows = Vec::new();
    for (entity_id, entity_rows) in bundle.fact_index.rows() {
        if !options.entity_ids.is_empty() && !options.entity_ids.contains(entity_id) {
            continue;
        }
        for fact in &entity_rows.facts {
            if !options.fact_keys.is_empty() && !options.fact_keys.contains(&fact.fact_key) {
                continue;
            }
            let (entity_type, entity_name) =
                entities.get(entity_id).copied().unwrap_or(("unknown", ""));
            let answers_preferences = entity_rows
                .search_metadata_for_fact_key(&fact.fact_key)
                .flat_map(|metadata| metadata.answers_preferences.iter().cloned())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            rows.push(SelectedFactRow {
                entity_id: entity_id.to_string(),
                entity_type: entity_type.to_string(),
                entity_name: entity_name.to_string(),
                fact_key: fact.fact_key.clone(),
                value: fact.value.clone(),
                value_text: fact.value_text.clone(),
                confidence: fact.confidence,
                source_type: fact.source_type.clone(),
                learned_at: fact.learned_at,
                answers_preferences,
            });
        }
    }
    rows.sort_by(|left, right| {
        left.fact_key
            .cmp(&right.fact_key)
            .then_with(|| left.entity_id.cmp(&right.entity_id))
            .then_with(|| right.confidence.total_cmp(&left.confidence))
            .then_with(|| right.learned_at.cmp(&left.learned_at))
    });
    rows
}

#[derive(Debug, Clone, Serialize)]
struct FactKeyStats {
    fact_key: String,
    rows: u64,
    confident_rows: u64,
    entity_count: usize,
    min_confidence: f32,
    max_confidence: f32,
    avg_confidence: f32,
    source_counts: BTreeMap<String, u64>,
    value_type_counts: BTreeMap<String, u64>,
    example_entities: Vec<String>,
    search_metadata_rows: u64,
    answers_preferences: Vec<String>,
    scoring_directions: Vec<String>,
    #[serde(skip)]
    confidence_sum: f64,
    #[serde(skip)]
    entities: BTreeSet<String>,
    #[serde(skip)]
    examples: BTreeSet<String>,
}

impl FactKeyStats {
    fn new(fact_key: &str) -> Self {
        Self {
            fact_key: fact_key.to_string(),
            rows: 0,
            confident_rows: 0,
            entity_count: 0,
            min_confidence: f32::MAX,
            max_confidence: f32::MIN,
            avg_confidence: 0.0,
            source_counts: BTreeMap::new(),
            value_type_counts: BTreeMap::new(),
            example_entities: Vec::new(),
            search_metadata_rows: 0,
            answers_preferences: Vec::new(),
            scoring_directions: Vec::new(),
            confidence_sum: 0.0,
            entities: BTreeSet::new(),
            examples: BTreeSet::new(),
        }
    }

    fn record(&mut self, entity_id: &str, fact: &ServingFactRecord) {
        self.rows += 1;
        if fact.confidence >= 0.6 {
            self.confident_rows += 1;
        }
        self.min_confidence = self.min_confidence.min(fact.confidence);
        self.max_confidence = self.max_confidence.max(fact.confidence);
        self.confidence_sum += f64::from(fact.confidence);
        *self
            .source_counts
            .entry(fact.source_type.clone())
            .or_insert(0) += 1;
        *self
            .value_type_counts
            .entry(fact.value_type.clone())
            .or_insert(0) += 1;
        self.entities.insert(entity_id.to_string());
        if self.examples.len() < 5 {
            self.examples.insert(entity_id.to_string());
        }
    }

    fn finish(mut self) -> Self {
        self.entity_count = self.entities.len();
        self.example_entities = self.examples.iter().cloned().collect();
        self.avg_confidence = if self.rows == 0 {
            0.0
        } else {
            (self.confidence_sum / self.rows as f64) as f32
        };
        self.min_confidence = if self.rows == 0 {
            0.0
        } else {
            self.min_confidence
        };
        self.max_confidence = if self.rows == 0 {
            0.0
        } else {
            self.max_confidence
        };
        self
    }
}

#[derive(Debug, Clone)]
struct MetadataStats {
    rows: u64,
    answers_preferences: BTreeSet<String>,
    scoring_directions: BTreeSet<String>,
}

impl MetadataStats {
    fn new(_fact_key: &str) -> Self {
        Self {
            rows: 0,
            answers_preferences: BTreeSet::new(),
            scoring_directions: BTreeSet::new(),
        }
    }

    fn record(&mut self, metadata: &ServingSearchMetadataRecord) {
        self.rows += 1;
        for preference in &metadata.answers_preferences {
            self.answers_preferences.insert(preference.clone());
        }
        if let Some(direction) = &metadata.scoring_direction {
            self.scoring_directions.insert(direction.clone());
        }
    }
}

#[derive(Default)]
struct CliOptions {
    project_root: Option<PathBuf>,
    serving_materialization_id: Option<MaterializationId>,
    limit: Option<usize>,
    markdown: bool,
    fact_keys: BTreeSet<String>,
    entity_ids: BTreeSet<String>,
}

impl CliOptions {
    fn parse() -> Result<Self, String> {
        let mut options = CliOptions::default();
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--project-root" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--project-root requires a value".to_string())?;
                    options.project_root = Some(PathBuf::from(value));
                }
                "--limit" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--limit requires a value".to_string())?;
                    options.limit = Some(
                        value
                            .parse()
                            .map_err(|_| "--limit requires a positive integer".to_string())?,
                    );
                }
                "--serving" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--serving requires a materialization UUID".to_string())?;
                    options.serving_materialization_id = Some(value.parse().map_err(|err| {
                        format!("--serving requires a materialization UUID: {err}")
                    })?);
                }
                "--markdown" => {
                    options.markdown = true;
                }
                "--fact-key" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--fact-key requires a value".to_string())?;
                    options.fact_keys.insert(value);
                }
                "--entity" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--entity requires a value".to_string())?;
                    options.entity_ids.insert(value);
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument: {other}")),
            }
        }
        Ok(options)
    }
}

fn default_project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend crate should live under project root")
        .to_path_buf()
}

fn print_help() {
    println!("Profile fact-key coverage in a search serving bundle.");
    println!();
    println!("Usage:");
    println!(
        "  cargo run --bin openestates-profile-serving-bundle -- [--serving <materialization-uuid>] [--limit N] [--fact-key KEY] [--entity ID] [--markdown]"
    );
}

fn print_markdown(profile: &ServingBundleProfile) {
    println!("# Serving Bundle Fact Profile");
    println!();
    println!("- Bundle: `{}`", profile.bundle_version);
    println!("- Entities: {}", profile.entity_count);
    println!("- Properties: {}", profile.property_count);
    println!("- Facts: {}", profile.fact_count);
    println!("- Search metadata rows: {}", profile.search_metadata_count);
    println!();
    println!(
        "| Fact key | Rows | Confident | Avg conf | Sources | Answers preferences | Examples |"
    );
    println!("|---|---:|---:|---:|---|---|---|");
    for fact in &profile.fact_keys {
        println!(
            "| `{}` | {} | {} | {:.2} | {} | {} | {} |",
            fact.fact_key,
            fact.rows,
            fact.confident_rows,
            fact.avg_confidence,
            join_counts(&fact.source_counts),
            fact.answers_preferences.join(", "),
            fact.example_entities.join(", ")
        );
    }
}

fn join_counts(counts: &BTreeMap<String, u64>) -> String {
    counts
        .iter()
        .map(|(key, count)| format!("{key}:{count}"))
        .collect::<Vec<_>>()
        .join(", ")
}
