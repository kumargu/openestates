use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use inventory_truth_preview::{
    model::{Fixture, Policy},
    parquet, router, validate, Inventory,
};
use serde_json::Value;
use tower::ServiceExt;

fn fixture() -> Fixture {
    serde_json::from_str(include_str!("../fixtures/scenarios.json")).unwrap()
}
fn policy() -> Policy {
    serde_json::from_str(include_str!(
        "../../../../app/config/dag/inventory_preview_policy.json"
    ))
    .unwrap()
}

async fn read(app: axum::Router, uri: &str) -> (u16, Value) {
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let body = to_bytes(response.into_body(), 1_000_000).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn materialized_observations_define_api_prices_receipts_and_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("snapshot");
    parquet::materialize(&fixture(), &policy(), &root).unwrap();
    let app = router(Inventory::from_snapshot(&root).unwrap());
    let (status, catalog) = read(app.clone(), "/api/inventory/homes").await;
    assert_eq!(status, 200);
    assert_eq!(catalog["homes"].as_array().unwrap().len(), 10);
    let snapshot = catalog["snapshot_id"].as_str().unwrap();
    let mut results = std::collections::BTreeMap::new();
    for home in catalog["homes"].as_array().unwrap() {
        let id = home["home"]["id"].as_str().unwrap();
        let (status, detail) = read(
            app.clone(),
            &format!("/api/inventory/homes/{id}?snapshot={snapshot}"),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(detail["summary"], *home);
        results.insert(id, detail);
    }
    let four = &results["four-ads"];
    assert_eq!(four["summary"]["advertisement_count"], 4);
    assert_eq!(four["summary"]["ask"]["min_inr"], 25_800_000);
    assert_eq!(four["summary"]["ask"]["max_inr"], 27_200_000);
    assert!(four["advertisements"]
        .as_array()
        .unwrap()
        .iter()
        .all(|a| a["history"].as_array().unwrap().is_empty()));
    let uncertain = &results["uncertain"];
    assert_eq!(uncertain["summary"]["ask"]["min_inr"], 25_800_000);
    assert_eq!(
        uncertain["candidates"][0]["observation"]["amount_inr"],
        24_900_000
    );
    assert_eq!(
        uncertain["summary"]["conflicts"][0]["label"],
        "Floor differs"
    );
    assert_eq!(results["one-active"]["summary"]["active_count"], 1);
    assert_eq!(
        results["one-active"]["summary"]["ask"]["min_inr"],
        25_800_000
    );
    assert!(results["withdrawn"]["summary"]["ask"].is_null());
    assert!(results["sparse"]["summary"]["ask"].is_null());
    assert_eq!(
        results["reduction"]["advertisements"][0]["history"][0]["amount_inr"],
        26_500_000
    );
    assert_eq!(results["reduction"]["summary"]["advertisement_count"], 1);
    assert_eq!(
        results["reduction"]["summary"]["price_change"]["difference_inr"],
        -700_000
    );
    assert_eq!(
        results["reduction"]["summary"]["price_change"]["previous_observation_id"],
        "reduced-before"
    );
    assert!(four["summary"]["price_change"].is_null());
    assert_eq!(
        results["comparables"]["comparables"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        results["comparables"]["registrations"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        results["comparables"]["summary"]["ask"]["min_inr"],
        25_800_000
    );
    assert_eq!(
        read(app.clone(), "/api/inventory/homes/four-ads?snapshot=other")
            .await
            .0,
        409
    );
    assert_eq!(
        read(
            app,
            &format!("/api/inventory/homes/not-found?snapshot={snapshot}")
        )
        .await
        .0,
        404
    );
    // Reordering Parquet input cannot change projection, price witnesses or receipts.
    let mut reversed = fixture();
    reversed.homes.reverse();
    reversed.observations.reverse();
    reversed.signals.reverse();
    let reordered = router(Inventory::project(reversed, policy(), snapshot.into()).unwrap());
    assert_eq!(
        read(reordered.clone(), "/api/inventory/homes").await.1,
        catalog
    );
    for (id, expected) in results {
        assert_eq!(
            read(
                reordered.clone(),
                &format!("/api/inventory/homes/{id}?snapshot={snapshot}")
            )
            .await
            .1,
            expected
        );
    }
    let mut changed_dates = fixture();
    for row in &mut changed_dates.observations {
        row.observed_on = "1999-01-01".into();
        row.last_seen = Some("1999-01-01".into());
    }
    let changed = read(
        router(Inventory::project(changed_dates, policy(), snapshot.into()).unwrap()),
        "/api/inventory/homes",
    )
    .await
    .1;
    for (before, after) in catalog["homes"]
        .as_array()
        .unwrap()
        .iter()
        .zip(changed["homes"].as_array().unwrap())
    {
        assert_eq!(before["ask"], after["ask"]);
        assert_eq!(before["active_count"], after["active_count"]);
        assert_eq!(before["price_change"], after["price_change"]);
    }
    // A content-addressed snapshot cannot be silently modified in place.
    std::fs::write(root.join("observations.parquet"), b"corrupt").unwrap();
    assert!(Inventory::from_snapshot(&root).is_err());
}

#[test]
fn admission_rejects_ambiguous_bindings_broken_history_and_invalid_measurements() {
    let original = fixture();
    for mutation in [
        "duplicate",
        "two-current",
        "unbound",
        "negative",
        "area",
        "cross-ad",
        "cycle",
        "orphan",
        "confirmed-conflict",
    ] {
        let mut data = original.clone();
        match mutation {
            "duplicate" => data.observations.push(data.observations[0].clone()),
            "two-current" => {
                let mut row = data.observations[0].clone();
                row.id = "another".into();
                data.observations.push(row);
            }
            "unbound" => data.observations[0].home_id = "missing".into(),
            "negative" => data.observations[0].amount_inr = Some(-1),
            "area" => data.observations[0].area_sqft = Some(1200),
            "cross-ad" => data.observations[0].predecessor_id = Some("reduced-before".into()),
            "cycle" => {
                data.observations
                    .iter_mut()
                    .find(|r| r.id == "reduced-before")
                    .unwrap()
                    .predecessor_id = Some("reduced-before".into())
            }
            "orphan" => {
                data.observations
                    .iter_mut()
                    .find(|r| r.id == "reduced-current")
                    .unwrap()
                    .predecessor_id = None
            }
            "confirmed-conflict" => data.signals[0].disagrees = true,
            _ => unreachable!(),
        }
        assert!(validate(&data, &policy()).is_err(), "accepted {mutation}");
    }
}
