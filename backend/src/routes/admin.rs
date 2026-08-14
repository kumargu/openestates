//! Admin endpoints for hot-reloading local state and running the asset DAG.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::assets::{
    AssetPartition, AssetRunManifestStore, AssetRunStepStatus, DagRunStatus, MaterializationId,
};
use crate::data_loader;
use crate::lake::LakeStoreLocation;
use crate::serving::LoadedServingBundle;
use crate::state::AppState;

const DEFAULT_SUBREDDIT: &str = "BangaloreRealEstates";

type AdminError = (StatusCode, Json<serde_json::Value>);

/// GET /api/admin/data-health
///
/// Bundle counts, bootstrap fact coverage, and preference coverage artifact path.
pub async fn data_health(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(err) = require_admin(&headers) {
        return err.into_response();
    }

    let bundle = state.serving_bundle.read().await;
    let properties = state.properties.read().await;
    let mut eligible_by_surface = BTreeMap::<String, usize>::new();
    let mut ineligible_by_reason = BTreeMap::<String, usize>::new();
    for property in properties.iter() {
        let mut property_reasons = std::collections::BTreeSet::new();
        for (surface, decision) in &property.buyer_eligibility.surfaces {
            if decision.eligible {
                *eligible_by_surface.entry(surface.clone()).or_default() += 1;
            }
            property_reasons.extend(decision.reason_codes.iter().cloned());
        }
        for reason in property_reasons {
            *ineligible_by_reason.entry(reason).or_default() += 1;
        }
    }
    let payload = bundle.as_ref().map(|loaded| {
        let all_facts = loaded.fact_index.all_facts();
        let reddit_theme_fact_count = all_facts
            .iter()
            .filter(|fact| fact.source_type.eq_ignore_ascii_case("RedditTheme"))
            .count() as u64;
        let reddit_theme_entity_count = all_facts
            .iter()
            .filter(|fact| fact.source_type.eq_ignore_ascii_case("RedditTheme"))
            .map(|fact| fact.entity_id.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len() as u64;

        serde_json::json!({
            "serving_bundle": serving_bundle_summary(loaded),
            "buyer_eligibility": {
                "policy_version": properties.first().map(|property| property.buyer_eligibility.policy_version),
                "candidate_count": properties.len(),
                "eligible_by_surface": eligible_by_surface,
                "ineligible_by_reason": ineligible_by_reason,
            },
            "reddit_theme": {
                "fact_count": reddit_theme_fact_count,
                "entity_count": reddit_theme_entity_count,
                "poc_import_path": "data/validation/reddit_poc_society_signals.json",
            },
            "preference_coverage_path": "data/validation/preference_coverage.json",
            "enrichment_gaps_path": "data/validation/enrichment_gaps.json",
            "enrichment_priority_path": "data/validation/enrichment_priority_queue.json",
        })
    });

    Json(serde_json::json!({
        "status": "ok",
        "serving_bundle_loaded": bundle.is_some(),
        "data": payload,
    }))
    .into_response()
}

/// POST /api/admin/serving-bundle/reload
///
/// Reloads the promoted search serving bundle from the configured local/S3 lake
/// into memory. This lets a just-finished DAG run become visible without a
/// backend restart.
pub async fn reload_serving_bundle(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(err) = require_admin(&headers) {
        return err.into_response();
    }

    match data_loader::load_serving_bundle(&state.project_root).await {
        Ok(Some(bundle)) => {
            let snapshot = data_loader::runtime_snapshot_from_serving_bundle(bundle);
            let summary = serving_bundle_summary(&snapshot.bundle);
            let legacy_bundle = snapshot.bundle.clone();
            let legacy_properties = snapshot.properties.to_vec();
            let legacy_societies = snapshot.societies.to_vec();
            let legacy_areas = snapshot.areas.to_vec();
            let legacy_search_index = snapshot.search_index.clone();

            state.search_runtime.store(Arc::new(snapshot));
            state.search_cache.clear().await;

            let mut current_bundle = state.serving_bundle.write().await;
            let mut properties = state.properties.write().await;
            let mut societies = state.societies.write().await;
            let mut areas = state.areas.write().await;
            let mut search_index = state.search_index.write().await;

            *current_bundle = Some(legacy_bundle);
            *properties = legacy_properties;
            *societies = legacy_societies;
            *areas = legacy_areas;
            *search_index = legacy_search_index;
            state.recommendation_cache.write().await.clear();

            Json(serde_json::json!({
                "status": "reloaded",
                "serving_bundle": summary,
            }))
            .into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "no promoted serving bundle found; current runtime state was left unchanged"
            })),
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error })),
        )
            .into_response(),
    }
}

/// GET /api/admin/asset-runs/current
///
/// Returns the latest durable DAG manifest summary for the selected partition.
/// By default this uses today's date and the Bangalore real estate subreddit
/// partition, which is the enrichment loop used in local development.
pub async fn current_asset_run(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<BTreeMap<String, String>>,
) -> impl IntoResponse {
    if let Err(err) = require_admin(&headers) {
        return err.into_response();
    }

    let partition = partition_from_query(&query);
    let lake_location = match LakeStoreLocation::from_env(&state.project_root) {
        Ok(location) => location,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
                .into_response()
        }
    };
    let lake = match lake_location.open() {
        Ok(lake) => lake,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
                .into_response()
        }
    };

    match AssetRunManifestStore::new(lake)
        .current_manifest(&partition)
        .await
    {
        Ok(manifest) => Json(asset_run_summary(&manifest)).into_response(),
        Err(error) if error.is_not_found() => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "no asset run manifest found for partition",
                "partition": partition.parts(),
            })),
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error.to_string() })),
        )
            .into_response(),
    }
}

/// POST /api/admin/asset-runs
///
/// Starts the existing `openestates-run-assets` binary in the background. The
/// request returns immediately; poll `/api/admin/asset-runs/current` for the
/// durable run result.
pub async fn trigger_asset_run(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    payload: Option<Json<TriggerAssetRunRequest>>,
) -> impl IntoResponse {
    if let Err(err) = require_admin(&headers) {
        return err.into_response();
    }

    let request = payload.map(|Json(value)| value).unwrap_or_default();
    let launch = match AssetRunLaunch::from_request(&state.project_root, request) {
        Ok(launch) => launch,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": error })),
            )
                .into_response()
        }
    };

    let log_file = match open_log_file(&launch.log_path) {
        Ok(file) => file,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
                .into_response()
        }
    };
    let log_file_for_stdout = match log_file.try_clone() {
        Ok(file) => file,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
                .into_response()
        }
    };

    let mut command = Command::new(&launch.program);
    command
        .args(&launch.args)
        .current_dir(&state.project_root)
        .stdout(Stdio::from(log_file_for_stdout))
        .stderr(Stdio::from(log_file));
    for (key, value) in &launch.env {
        command.env(key, value);
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
                .into_response()
        }
    };
    let pid = child.id();
    let request_id = launch.request_id.clone();
    tokio::spawn(async move {
        match child.wait().await {
            Ok(status) if status.success() => {
                eprintln!("asset DAG run {request_id} finished; pid={pid:?}");
            }
            Ok(status) => {
                eprintln!("asset DAG run {request_id} failed; pid={pid:?}; status={status:?}");
            }
            Err(error) => {
                eprintln!("asset DAG run {request_id} wait failed; pid={pid:?}; error={error}");
            }
        }
    });

    Json(serde_json::json!({
        "status": "accepted",
        "request_id": launch.request_id,
        "pid": pid,
        "version": launch.version,
        "partition": launch.partition.parts(),
        "log_path": launch.log_path,
        "poll": "/api/admin/asset-runs/current",
    }))
    .into_response()
}

fn require_admin(headers: &HeaderMap) -> Result<(), AdminError> {
    let expected = std::env::var("ADMIN_TOKEN").unwrap_or_else(|_| "dev".to_string());
    let provided = headers
        .get("x-admin-token")
        .and_then(|value| value.to_str().ok());
    if provided == Some(expected.as_str()) {
        Ok(())
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid or missing admin token" })),
        ))
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TriggerAssetRunRequest {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub partition: BTreeMap<String, String>,
    #[serde(default)]
    pub source_entities: Vec<String>,
    #[serde(default)]
    pub source_timeout_seconds: Option<u64>,
    /// Defaults to true until Reddit auth/networking is explicitly fixed.
    #[serde(default = "default_skip_reddit")]
    pub skip_reddit: bool,
}

#[derive(Debug)]
struct AssetRunLaunch {
    request_id: String,
    program: PathBuf,
    args: Vec<OsString>,
    env: Vec<(String, String)>,
    version: String,
    partition: AssetPartition,
    log_path: PathBuf,
}

impl AssetRunLaunch {
    fn from_request(project_root: &Path, request: TriggerAssetRunRequest) -> Result<Self, String> {
        let partition = partition_from_request(&request);
        let now = Utc::now();
        let version = request.version.unwrap_or_else(|| {
            format!(
                "{}-admin-enrichment-{}",
                now.format("%Y-%m-%d"),
                now.format("%H%M%S")
            )
        });
        let request_id = MaterializationId::new().to_string();
        let (program, mut args) = asset_runner_program(project_root);

        args.push(OsString::from("--project-root"));
        args.push(project_root.as_os_str().to_os_string());
        for (key, value) in partition.parts() {
            args.push(OsString::from("--partition"));
            args.push(OsString::from(format!("{key}={value}")));
        }
        args.push(OsString::from("--version"));
        args.push(OsString::from(version.clone()));
        args.push(OsString::from("--source-command"));
        args.push(OsString::from(
            std::env::var("OPENESTATES_SOURCE_PYTHON").unwrap_or_else(|_| "python3.11".to_string()),
        ));
        args.push(OsString::from("--source-arg"));
        args.push(OsString::from("-m"));
        args.push(OsString::from("--source-arg"));
        args.push(OsString::from("pipeline.collect_asset_sources"));
        args.push(OsString::from("--source-timeout-seconds"));
        args.push(OsString::from(
            request.source_timeout_seconds.unwrap_or(1800).to_string(),
        ));
        for source_entity in request.source_entities {
            let source_entity = source_entity.trim();
            if source_entity.is_empty() {
                return Err("source_entities cannot contain empty values".to_string());
            }
            args.push(OsString::from("--source-entity"));
            args.push(OsString::from(source_entity));
        }

        let mut env = Vec::new();
        if request.skip_reddit {
            env.push(("OPENESTATES_SKIP_REDDIT".to_string(), "1".to_string()));
        }

        Ok(Self {
            request_id: request_id.clone(),
            program,
            args,
            env,
            version,
            partition,
            log_path: project_root
                .join("data")
                .join("logs")
                .join("asset-runs")
                .join(format!("{request_id}.log")),
        })
    }
}

fn asset_runner_program(project_root: &Path) -> (PathBuf, Vec<OsString>) {
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            let runner = dir.join("openestates-run-assets");
            if runner.exists() {
                return (runner, Vec::new());
            }
        }
    }

    (
        PathBuf::from("cargo"),
        vec![
            OsString::from("run"),
            OsString::from("--manifest-path"),
            project_root
                .join("backend")
                .join("Cargo.toml")
                .as_os_str()
                .to_os_string(),
            OsString::from("--bin"),
            OsString::from("openestates-run-assets"),
            OsString::from("--"),
        ],
    )
}

fn open_log_file(path: &Path) -> std::io::Result<std::fs::File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}

fn default_skip_reddit() -> bool {
    true
}

fn partition_from_query(query: &BTreeMap<String, String>) -> AssetPartition {
    if query
        .get("global")
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes"))
    {
        return AssetPartition::global();
    }
    let parts = query
        .iter()
        .filter(|(key, value)| key.as_str() != "global" && !value.trim().is_empty())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Vec<_>>();
    if parts.is_empty() {
        default_dag_partition()
    } else {
        AssetPartition::new(parts)
    }
}

fn partition_from_request(request: &TriggerAssetRunRequest) -> AssetPartition {
    if request.partition.is_empty() {
        default_dag_partition()
    } else {
        AssetPartition::new(request.partition.clone())
    }
}

fn default_dag_partition() -> AssetPartition {
    AssetPartition::new([
        ("dt".to_string(), Utc::now().format("%Y-%m-%d").to_string()),
        ("subreddit".to_string(), DEFAULT_SUBREDDIT.to_string()),
    ])
}

#[derive(Debug, Serialize)]
struct ServingBundleSummary {
    bundle_version: String,
    entity_count: u64,
    fact_count: u64,
    search_metadata_count: u64,
}

fn serving_bundle_summary(bundle: &Arc<LoadedServingBundle>) -> ServingBundleSummary {
    ServingBundleSummary {
        bundle_version: bundle.manifest.bundle_version.clone(),
        entity_count: bundle.manifest.entity_count,
        fact_count: bundle.manifest.fact_count,
        search_metadata_count: bundle.manifest.search_metadata_count,
    }
}

#[derive(Debug, Serialize)]
struct AssetRunSummary {
    run_id: String,
    execution_version: String,
    status: DagRunStatus,
    partition: Vec<(String, String)>,
    created_at: chrono::DateTime<Utc>,
    completed_at: Option<chrono::DateTime<Utc>>,
    total_assets: usize,
    planned_count: usize,
    skipped_count: usize,
    succeeded_count: usize,
    failed_count: usize,
    blocked_count: usize,
    steps: Vec<AssetRunStepSummary>,
}

#[derive(Debug, Serialize)]
struct AssetRunStepSummary {
    asset_id: String,
    status: AssetRunStepStatus,
    row_count: Option<u64>,
    materialization_id: Option<String>,
    current_materialization_id: Option<String>,
    duration_ms: Option<u64>,
    error: Option<String>,
}

fn asset_run_summary(manifest: &crate::assets::AssetDagRunManifest) -> AssetRunSummary {
    AssetRunSummary {
        run_id: manifest.run_id.to_string(),
        execution_version: manifest.execution_version.clone(),
        status: manifest.status,
        partition: manifest.partition.parts().to_vec(),
        created_at: manifest.created_at,
        completed_at: manifest.completed_at,
        total_assets: manifest.total_assets,
        planned_count: manifest.planned_count,
        skipped_count: manifest.skipped_count,
        succeeded_count: manifest.succeeded_count,
        failed_count: manifest.failed_count,
        blocked_count: manifest.blocked_count,
        steps: manifest
            .steps
            .iter()
            .map(|step| AssetRunStepSummary {
                asset_id: step.asset_id.as_str().to_string(),
                status: step.status,
                row_count: step.row_count,
                materialization_id: step
                    .materialization_id
                    .as_ref()
                    .map(std::string::ToString::to_string),
                current_materialization_id: step
                    .current_materialization_id
                    .as_ref()
                    .map(std::string::ToString::to_string),
                duration_ms: step.duration_ms,
                error: step.error.clone(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_partition_targets_daily_bangalore_reddit_loop() {
        let partition = default_dag_partition();
        assert_eq!(partition.value("subreddit"), Some(DEFAULT_SUBREDDIT));
        assert!(partition.value("dt").is_some());
    }

    #[test]
    fn launch_command_uses_existing_asset_runner_contract() {
        let request = TriggerAssetRunRequest {
            version: Some("test-version".to_string()),
            partition: BTreeMap::from([
                ("dt".to_string(), "2026-07-15".to_string()),
                ("subreddit".to_string(), DEFAULT_SUBREDDIT.to_string()),
            ]),
            source_entities: vec!["society:test".to_string()],
            source_timeout_seconds: Some(60),
            skip_reddit: true,
        };
        let launch = AssetRunLaunch::from_request(Path::new("/tmp/openestates"), request)
            .expect("valid launch");
        let args = launch
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert!(args
            .windows(2)
            .any(|pair| pair == ["--version", "test-version"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--source-command", "python3.11"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--source-entity", "society:test"]));
        assert!(launch
            .env
            .iter()
            .any(|(key, value)| key == "OPENESTATES_SKIP_REDDIT" && value == "1"));
    }
}
