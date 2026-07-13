use std::path::PathBuf;

use backend::assets::{
    all_current_partition_dependency_records_for_asset, default_openestates_registry,
    read_skill_fact_artifact_rows, AssetId, AssetMaterializationStore, KgSocietyViewMaterializer,
    SourceWatermark, KG_SOCIETY_VIEW_ASSET_ID,
};
use backend::knowledge::store as kg_store;
use backend::lake::LakeStore;
use backend::serving::SearchServingBundleMaterializer;
use chrono::Utc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = CliOptions::parse()?;
    let project_root = options.project_root.unwrap_or_else(default_project_root);
    let version = options.version.unwrap_or_else(default_bundle_version);

    let kg_dir = kg_store::knowledge_dir(&project_root);
    let graph = kg_store::load_graph(&kg_dir).ok_or_else(|| {
        format!(
            "No knowledge graph found at {}. Seed or load KG before building serving bundle.",
            kg_dir.display()
        )
    })?;
    let stats = graph.stats();

    let lake_root = project_root.join("data").join("lake");
    let lake = LakeStore::local(&lake_root)?;
    let registry = default_openestates_registry();
    let materializations = AssetMaterializationStore::new(lake.clone());
    let kg_asset_id = AssetId::new(KG_SOCIETY_VIEW_ASSET_ID)?;
    let support_records = all_current_partition_dependency_records_for_asset(
        &registry,
        &materializations,
        &kg_asset_id,
    )
    .await?;
    let support_rows = read_skill_fact_artifact_rows(&lake, &support_records).await?;
    let support_parent_materializations = support_records
        .iter()
        .map(|record| record.materialization_id.clone())
        .collect();
    let source_watermarks = vec![SourceWatermark {
        source: "knowledge_graph".to_string(),
        high_watermark: format!(
            "nodes={} edges={} facts={}",
            stats.total_nodes, stats.total_edges, stats.total_facts
        ),
    }];
    let kg_materialization = KgSocietyViewMaterializer::new(lake.clone())
        .materialize_and_promote_with_skill_facts(
            &graph,
            version.clone(),
            source_watermarks.clone(),
            support_parent_materializations,
            &support_rows.facts,
            &support_rows.fact_annotations,
        )
        .await?;
    let materialization = SearchServingBundleMaterializer::new(lake)
        .materialize_and_promote_from_kg_view(&kg_materialization, version)
        .await?;

    println!(
        "Promoted KG society view {}",
        kg_materialization.manifest.view_version
    );
    println!("  entities: {}", kg_materialization.manifest.entity_count);
    println!("  facts: {}", kg_materialization.manifest.fact_count);
    println!(
        "  materialization: {}",
        kg_materialization.record.materialization_id
    );
    println!(
        "  content hash: {}",
        kg_materialization.manifest.graph_content_hash
    );
    println!("  support fact materializations: {}", support_records.len());
    println!(
        "Promoted search serving bundle {}",
        materialization.manifest.bundle_version
    );
    println!("  entities: {}", materialization.manifest.entity_count);
    println!("  facts: {}", materialization.manifest.fact_count);
    println!(
        "  search metadata: {}",
        materialization.manifest.search_metadata_count
    );
    println!(
        "  manifest: serving/search_bundle/version={}/manifest.json",
        materialization
            .manifest
            .bundle_version
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                    ch.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .split('-')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("-")
    );
    println!(
        "  materialization: {}",
        materialization.record.materialization_id
    );
    println!("  lake root: {}", lake_root.display());

    Ok(())
}

#[derive(Default)]
struct CliOptions {
    project_root: Option<PathBuf>,
    version: Option<String>,
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
                "--version" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--version requires a value".to_string())?;
                    options.version = Some(value);
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => {
                    return Err(format!("unknown argument: {other}"));
                }
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

fn default_bundle_version() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn print_help() {
    println!("Build and promote the local search serving bundle.");
    println!();
    println!("Usage:");
    println!("  cargo run --bin openestates-build-serving-bundle -- [--version <version>] [--project-root <path>]");
}
