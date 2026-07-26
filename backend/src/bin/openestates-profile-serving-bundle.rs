use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use backend::data_loader::properties_from_serving_bundle;
use backend::lake::LakeStoreLocation;
use backend::serving::{ServingBundleLoader, ServingFactRecord, ServingSearchMetadataRecord};
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
    let bundle = ServingBundleLoader::new(lake, cache_root)
        .load_current_search_bundle()
        .await?
        .ok_or("no promoted search serving bundle found")?;

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
    let profile = ServingBundleProfile {
        bundle_version: bundle.manifest.bundle_version.clone(),
        entity_count: bundle.entities.len(),
        property_count: properties.len(),
        fact_count: bundle.manifest.fact_count,
        search_metadata_count: bundle.manifest.search_metadata_count,
        semantic_embedding_rows: bundle.semantic_embeddings.len(),
        fact_keys,
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
    semantic_embedding_rows: usize,
    fact_keys: Vec<FactKeyStats>,
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
    limit: Option<usize>,
    markdown: bool,
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
                "--markdown" => {
                    options.markdown = true;
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
    println!("Profile fact-key coverage in the promoted search serving bundle.");
    println!();
    println!("Usage:");
    println!("  cargo run --bin openestates-profile-serving-bundle -- [--limit N] [--markdown]");
}

fn print_markdown(profile: &ServingBundleProfile) {
    println!("# Serving Bundle Fact Profile");
    println!();
    println!("- Bundle: `{}`", profile.bundle_version);
    println!("- Entities: {}", profile.entity_count);
    println!("- Properties: {}", profile.property_count);
    println!("- Facts: {}", profile.fact_count);
    println!("- Search metadata rows: {}", profile.search_metadata_count);
    println!(
        "- Semantic embedding rows: {}",
        profile.semantic_embedding_rows
    );
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
