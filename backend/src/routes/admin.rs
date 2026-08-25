//! Admin endpoints for hot-reloading local state and running the asset DAG.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use crate::assets::{
    AssetPartition, AssetRunManifestStore, AssetRunStepStatus, DagRunStatus, MaterializationId,
};
use crate::data_loader;
use crate::lake::LakeStoreLocation;
use crate::security::admin_run::{release_asset_run, try_reserve_asset_run};
use crate::security::require_admin;
use crate::security::retention::prune_asset_run_logs;
use crate::security::security_tuning;
use crate::serving::LoadedServingBundle;
use crate::state::AppState;

const DEFAULT_SUBREDDIT: &str = "BangaloreRealEstates";

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
            "reddit_theme": {
                "fact_count": reddit_theme_fact_count,
                "entity_count": reddit_theme_entity_count,
                "poc_import_path": "data/validation/reddit_poc_society_signals.json",
            },
            "preference_coverage_path": "data/validation/preference_coverage.json",
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

    let project_root = state.project_root.clone();
    let load_result = state
        .execution
        .run_internal(async move {
            data_loader::load_serving_bundle(&project_root)
                .await
                .map(|bundle| bundle.map(data_loader::runtime_snapshot_from_serving_bundle))
        })
        .await;

    match load_result {
        Ok(Ok(Some(snapshot))) => {
            let summary = serving_bundle_summary(&snapshot.bundle);
            let legacy_bundle = snapshot.bundle.clone();
            let legacy_properties = snapshot.properties.to_vec();
            let legacy_societies = snapshot.societies.to_vec();
            let legacy_areas = snapshot.areas.to_vec();
            let legacy_search_index = snapshot.search_index.clone();

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
            *state.property_catalog_cache.lock().await = None;
            state.search_cache.clear().await;
            // Publish the immutable runtime last while legacy writers are held,
            // so new readers cannot observe a half-applied reload.
            state.search_runtime.store(Arc::new(snapshot));

            Json(serde_json::json!({
                "status": "reloaded",
                "serving_bundle": summary,
            }))
            .into_response()
        }
        Ok(Ok(None)) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "no promoted serving bundle found; current runtime state was left unchanged"
            })),
        )
            .into_response(),
        Ok(Err(error)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error })),
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("internal reload task failed: {error}")
            })),
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

    if !try_reserve_asset_run(&state.asset_run_active) {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "an asset run is already active" })),
        )
            .into_response();
    }

    if let Err(error) = prepare_log_file(&launch.log_path) {
        release_asset_run(&state.asset_run_active);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error.to_string() })),
        )
            .into_response();
    }
    if let Some(log_dir) = launch.log_path.parent() {
        prune_asset_run_logs(log_dir, Some(&launch.log_path));
    }

    let request_id = launch.request_id.clone();
    let response_request_id = launch.request_id.clone();
    let response_version = launch.version.clone();
    let response_partition = launch.partition.parts().to_vec();
    let response_log_path = launch.log_path.clone();
    let log_path = launch.log_path.clone();
    let project_root = state.project_root.clone();
    let run_state = state.clone();
    let (spawn_tx, spawn_rx) = tokio::sync::oneshot::channel();
    state.execution.spawn_internal(async move {
        let mut command = Command::new(&launch.program);
        command
            .args(&launch.args)
            .current_dir(project_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for (key, value) in &launch.env {
            command.env(key, value);
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let _ = spawn_tx.send(Err(error.to_string()));
                release_asset_run(&run_state.asset_run_active);
                return;
            }
        };
        let pid = child.id();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let _ = spawn_tx.send(Ok(pid));
        let written = Arc::new(AtomicU64::new(0));
        let stdout_drain = drain_child_output(stdout, log_path.clone(), written.clone());
        let stderr_drain = drain_child_output(stderr, log_path, written);
        let (status, _, _) = tokio::join!(child.wait(), stdout_drain, stderr_drain);
        match status {
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
        release_asset_run(&run_state.asset_run_active);
    });

    let pid = match spawn_rx.await {
        Ok(Ok(pid)) => pid,
        Ok(Err(error)) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": error })),
            )
                .into_response()
        }
        Err(error) => {
            release_asset_run(&state.asset_run_active);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("internal asset launcher stopped: {error}")
                })),
            )
                .into_response();
        }
    };

    Json(serde_json::json!({
        "status": "accepted",
        "request_id": response_request_id,
        "pid": pid,
        "version": response_version,
        "partition": response_partition,
        "log_path": response_log_path,
        "poll": "/api/admin/asset-runs/current",
    }))
    .into_response()
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
        validate_trigger_request(&request)?;
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
            request
                .source_timeout_seconds
                .unwrap_or(security_tuning().admin.default_source_timeout_seconds)
                .to_string(),
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

fn prepare_log_file(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::File::create(path).map(drop)
}

async fn drain_child_output<R>(reader: Option<R>, log_path: PathBuf, written: Arc<AtomicU64>)
where
    R: AsyncRead + Unpin,
{
    let Some(mut reader) = reader else {
        return;
    };
    let mut log = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .await
        .ok();
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let count = match reader.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(count) => count,
        };
        let reserved = reserve_log_bytes(&written, count as u64) as usize;
        if reserved == 0 {
            continue;
        }
        if let Some(file) = log.as_mut() {
            if file.write_all(&buffer[..reserved]).await.is_err() {
                log = None;
            }
        }
    }
}

fn reserve_log_bytes(written: &AtomicU64, requested: u64) -> u64 {
    loop {
        let current = written.load(Ordering::Relaxed);
        let available = security_tuning()
            .admin
            .max_asset_run_log_bytes
            .saturating_sub(current);
        let reserved = requested.min(available);
        if reserved == 0 {
            return 0;
        }
        if written
            .compare_exchange_weak(
                current,
                current + reserved,
                Ordering::Relaxed,
                Ordering::Relaxed,
            )
            .is_ok()
        {
            return reserved;
        }
    }
}

fn validate_trigger_request(request: &TriggerAssetRunRequest) -> Result<(), String> {
    let tuning = &security_tuning().admin;
    if request.source_entities.len() > tuning.max_source_entities {
        return Err(format!(
            "source_entities supports at most {} values",
            tuning.max_source_entities
        ));
    }
    if request.partition.len() > tuning.max_partition_parts {
        return Err(format!(
            "partition supports at most {} values",
            tuning.max_partition_parts
        ));
    }
    if let Some(timeout) = request.source_timeout_seconds {
        if timeout == 0 || timeout > tuning.max_source_timeout_seconds {
            return Err(format!(
                "source_timeout_seconds must be between 1 and {}",
                tuning.max_source_timeout_seconds
            ));
        }
    }
    if let Some(version) = request.version.as_deref() {
        validate_admin_field("version", version, tuning.max_field_bytes)?;
    }
    for (key, value) in &request.partition {
        validate_admin_field("partition key", key, tuning.max_field_bytes)?;
        validate_admin_field("partition value", value, tuning.max_field_bytes)?;
    }
    for entity in &request.source_entities {
        validate_admin_field("source entity", entity, tuning.max_source_entity_bytes)?;
    }
    Ok(())
}

fn validate_admin_field(label: &str, value: &str, max_bytes: usize) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{label} cannot be empty"));
    }
    if value.len() > max_bytes {
        return Err(format!("{label} cannot exceed {max_bytes} bytes"));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{label} cannot contain control characters"));
    }
    Ok(())
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

    #[test]
    fn launch_rejects_unbounded_admin_inputs() {
        let tuning = &security_tuning().admin;
        let excessive_timeout = TriggerAssetRunRequest {
            source_timeout_seconds: Some(tuning.max_source_timeout_seconds + 1),
            ..TriggerAssetRunRequest::default()
        };
        assert!(
            AssetRunLaunch::from_request(Path::new("/tmp/openestates"), excessive_timeout).is_err()
        );

        let too_many_entities = TriggerAssetRunRequest {
            source_entities: (0..=tuning.max_source_entities)
                .map(|index| format!("society:{index}"))
                .collect(),
            ..TriggerAssetRunRequest::default()
        };
        assert!(
            AssetRunLaunch::from_request(Path::new("/tmp/openestates"), too_many_entities).is_err()
        );
    }

    #[test]
    fn asset_log_budget_is_shared_and_strict() {
        let written = AtomicU64::new(security_tuning().admin.max_asset_run_log_bytes - 2);
        assert_eq!(reserve_log_bytes(&written, 8), 2);
        assert_eq!(reserve_log_bytes(&written, 1), 0);
    }
}
