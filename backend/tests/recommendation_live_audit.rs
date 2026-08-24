use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use backend::api::build_app_router;
use backend::data_loader::load_app_state;
use backend::routes::enrichment::society_node_id;
use backend::scoring::scoring_policy;
use backend::serving::unique_society_aliases;
use serde_json::Value;
use tower::ServiceExt;

const MIN_COORDINATE_COVERAGE: f64 = 0.90;
const MAX_P95_LATENCY_MS: f64 = 200.0;

#[tokio::test]
async fn promoted_bundle_recommendations_preserve_trust_invariants() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend has a project root")
        .to_path_buf();
    let state = Arc::new(load_app_state(&project_root).await);

    let properties = state
        .properties
        .read()
        .await
        .iter()
        .map(|property| {
            (
                property.id.clone(),
                (property.bhk, normalize(&property.society_id)),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert!(!properties.is_empty(), "promoted inventory is empty");

    let recommendation_policy = &scoring_policy().recommendation_recall;
    let recall_channels = recommendation_policy
        .channels
        .iter()
        .filter(|channel| channel.enabled && channel.can_recall)
        .map(|channel| channel.id.as_str())
        .collect::<BTreeSet<_>>();
    let app = build_app_router(state.clone(), &project_root);
    let mut recommendation_count = 0usize;
    let mut empty_anchors = 0usize;
    let mut thin_anchors = 0usize;
    let mut latencies_ms = Vec::with_capacity(properties.len());

    for (anchor_id, (anchor_bhk, anchor_society)) in &properties {
        let started = Instant::now();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/properties/{anchor_id}/recommendations"))
                    .body(Body::empty())
                    .expect("recommendation request is valid"),
            )
            .await
            .expect("recommendation route responds");
        latencies_ms.push(started.elapsed().as_secs_f64() * 1_000.0);
        assert_eq!(response.status(), StatusCode::OK, "anchor={anchor_id}");
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("recommendation body is readable");
        let payload: Value = serde_json::from_slice(&body).expect("recommendation body is JSON");
        let items = payload["items"]
            .as_array()
            .expect("recommendation response has items");
        assert!(
            items.len() <= recommendation_policy.branch_limit,
            "anchor={anchor_id} exceeded the branch cap"
        );
        empty_anchors += usize::from(items.is_empty());
        thin_anchors += usize::from(items.len() < 3);
        recommendation_count += items.len();

        let mut target_ids = BTreeSet::new();
        let mut target_societies = BTreeSet::new();
        for item in items {
            let target = &item["property"];
            let target_id = target["id"].as_str().expect("target has an ID");
            let (target_bhk, target_society) = properties
                .get(target_id)
                .expect("recommended target belongs to promoted inventory");
            assert_ne!(target_id, anchor_id, "anchor recommended itself");
            assert_eq!(
                target_bhk, anchor_bhk,
                "anchor={anchor_id} target={target_id}"
            );
            assert_ne!(
                target_society, anchor_society,
                "anchor={anchor_id} repeated its own society"
            );
            assert!(
                target_ids.insert(target_id),
                "anchor={anchor_id} repeated target={target_id}"
            );
            assert!(
                target_societies.insert(target_society),
                "anchor={anchor_id} repeated a target society"
            );
            assert!(
                item["channels"]
                    .as_array()
                    .is_some_and(|channels| channels.iter().any(|channel| {
                        channel["channel"]
                            .as_str()
                            .is_some_and(|id| recall_channels.contains(id))
                    })),
                "anchor={anchor_id} target={target_id} lacks candidate-generating evidence"
            );
        }
    }

    latencies_ms.sort_by(f64::total_cmp);
    let p95_index = (latencies_ms.len() * 95).div_ceil(100).saturating_sub(1);
    let p95_ms = latencies_ms[p95_index];
    println!(
        "recommendation_live_metrics anchors={} recommendations={} empty={} thin={} p95_ms={p95_ms:.1}",
        properties.len(), recommendation_count, empty_anchors, thin_anchors
    );
    assert!(
        p95_ms <= MAX_P95_LATENCY_MS,
        "recommendation p95 {p95_ms:.1}ms exceeded {MAX_P95_LATENCY_MS:.1}ms"
    );

    let bundle = state.serving_bundle.read().await;
    let bundle = bundle
        .as_ref()
        .expect("the promoted serving bundle should be loaded");
    let aliases = unique_society_aliases(&bundle.entities)
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let runtime_properties = state.properties.read().await;
    let societies = runtime_properties
        .iter()
        .map(|property| property.society_id.as_str())
        .collect::<BTreeSet<_>>();
    let coordinate_societies = societies
        .iter()
        .filter(|society_id| {
            let alias = society_node_id(society_id);
            let entity_id = aliases.get(&alias).map(String::as_str).unwrap_or(&alias);
            bundle.spatial_index.point_for_entity(entity_id).is_some()
        })
        .count();
    let coordinate_coverage = coordinate_societies as f64 / societies.len() as f64;
    assert!(
        coordinate_coverage >= MIN_COORDINATE_COVERAGE,
        "society coordinate coverage {:.1}% is below {:.0}%",
        coordinate_coverage * 100.0,
        MIN_COORDINATE_COVERAGE * 100.0
    );
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(|character| character.to_lowercase())
        .collect()
}
