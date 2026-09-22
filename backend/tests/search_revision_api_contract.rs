use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::body::{to_bytes, Body};
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use backend::api::build_app_router_with_lake;
use backend::graph::GraphIndex;
use backend::knowledge::FactValue;
use backend::lake::LakeStore;
use backend::models::Property;
use backend::search::geo::SpatialEntityIndex;
use backend::search::proof::{issue_proof_token, ProofIssueRequest};
use backend::search::{
    decode_signed_search_context, issue_signed_search_context, SearchCapabilityIndex, SearchEngine,
    SearchIndex, SearchRevisionOperation, SearchRuntimeVersion,
};
use backend::security::ExecutionLanes;
use backend::serving::{
    DerivedEvidence, EvidenceRef, LoadedServingBundle, ReraEvidenceIndex, ServingBundleManifest,
    ServingEdgeRecord, ServingEntityAliasIndex, ServingEntityRecord, ServingFactIndex,
    ServingFactRecord, SourceObservation, SpatialServingIndex, TantivyRecallIndex,
};
use backend::state::{AppState, SearchLogMessage, SearchResponseCache, SearchRuntimeSnapshot};
use chrono::{TimeZone, Utc};
use serde_json::{json, Value};
use tempfile::tempdir;
use tokio::sync::{mpsc, RwLock};
use tower::ServiceExt;

#[tokio::test]
async fn named_identity_retains_a_durable_witness_after_recall() {
    let app = test_app().await;
    let search = get_search(&app, "3BHK in Fixture Home", 221).await;
    assert_eq!(search.0, StatusCode::OK);
    let results = search.1["active"]["results"]["resultSets"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|set| set["results"].as_array().unwrap())
        .collect::<Vec<_>>();
    assert!(!results.is_empty());
    let mut identity_receipts = 0;
    for result in results {
        for reason in result["reasons"].as_array().unwrap() {
            let resolved = post_proof(
                &app,
                json!({"proofToken": reason["proofToken"], "propertyId": result["id"]}),
                222,
            )
            .await;
            assert_eq!(resolved.0, StatusCode::OK);
            if resolved.1["factKey"] == "entity_identity" {
                assert!(!resolved.1["catalogEvidence"].as_array().unwrap().is_empty());
                identity_receipts += 1;
            }
        }
    }
    assert!(
        identity_receipts > 0,
        "named identity was admitted without a durable witness"
    );
}

#[test]
fn catalog_membership_preserves_missing_negative_and_row_order_semantics() {
    use backend::search::{identity::IdentityEvaluationIndex, EvaluationState};
    let root = tempdir().unwrap();
    let bundle = test_bundle_with_options(root.path(), true, "identity-states", false, true);
    let snapshot = bundle.manifest.proof_snapshot_identity();
    let index = IdentityEvaluationIndex::from_bundle(&bundle);
    let positive = index.evaluate("society:second-home", "area:hoodi", snapshot);
    assert_eq!(positive.state, EvaluationState::Satisfied);
    assert!(!positive.verified_matches[0].evidence_refs.is_empty());
    let negative = index
        .evaluate("society:second-home", "area:sarjapur", snapshot)
        .negated();
    assert_eq!(negative.state, EvaluationState::Satisfied);
    assert!(!negative.verified_matches[0].evidence_refs.is_empty());
    let missing = IdentityEvaluationIndex::from_records(&bundle.entities, &[], snapshot);
    assert_eq!(
        missing
            .evaluate("society:second-home", "area:sarjapur", snapshot)
            .negated()
            .state,
        EvaluationState::Unknown
    );
    let mut edges = bundle.edges.clone();
    edges.reverse();
    let reordered = IdentityEvaluationIndex::from_records(&bundle.entities, &edges, snapshot);
    assert_eq!(
        positive.verified_matches,
        reordered
            .evaluate("society:second-home", "area:hoodi", snapshot)
            .verified_matches
    );
    let mut conflicting = edges
        .iter()
        .find(|edge| {
            edge.from_entity_id == "society:second-home" && edge.edge_type == "in_market_locality"
        })
        .unwrap()
        .clone();
    conflicting.to_entity_id = "area:sarjapur".to_string();
    conflicting.derivation = None;
    edges.push(conflicting);
    let ambiguous = IdentityEvaluationIndex::from_records(&bundle.entities, &edges, snapshot);
    assert_eq!(
        ambiguous
            .evaluate("society:second-home", "area:cell:hoodi", snapshot)
            .negated()
            .state,
        EvaluationState::Unknown
    );
}

#[tokio::test]
#[ignore = "requires OPENESTATES_TEST_LAKE_ROOT and the pinned proof_handoff_live bundle"]
async fn pinned_live_bundle_resolves_search_receipts_into_visible_scene_evidence() {
    let bank: Value =
        serde_json::from_str(include_str!("../../data/validation/search_query_bank.json")).unwrap();
    let suite = bank["suites"]
        .as_array()
        .unwrap()
        .iter()
        .find(|suite| suite["id"] == "proof_handoff_live")
        .unwrap();
    let lake =
        LakeStore::local(std::env::var("OPENESTATES_TEST_LAKE_ROOT").expect("test lake root"))
            .unwrap();
    let cache = tempdir().unwrap();
    let bundle = backend::serving::ServingBundleLoader::new(lake, cache.path())
        .load_search_bundle(suite["required_serving_bundle_version"].as_str().unwrap())
        .await
        .unwrap();
    let (app, state) = test_app_with_state().await;
    state.search_runtime.store(Arc::new(
        backend::data_loader::runtime_snapshot_from_serving_bundle(Arc::new(bundle)),
    ));
    for case in bank["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["group"] == "proof_handoff_live")
    {
        let search = get_search(&app, case["query"].as_str().unwrap(), 180).await;
        assert_eq!(search.0, StatusCode::OK);
        let expected = &case["expected"];
        let ids = result_ids(&search.1);
        for (index, id) in expected["leading_property_ids"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            assert_eq!(ids[index], id.as_str().unwrap());
            let card = search.1["active"]["results"]["resultSets"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|set| set["results"].as_array().unwrap())
                .find(|card| card["id"] == *id)
                .unwrap();
            let mut applied = false;
            for reason in card["reasons"].as_array().unwrap() {
                let token = reason["proofToken"].as_str().unwrap();
                let proof =
                    post_proof(&app, json!({"proofToken":token,"propertyId":id}), 181).await;
                assert_eq!(proof.0, StatusCode::OK);
                if proof.1["factKey"] != expected["proof_fact_key"] {
                    continue;
                }
                assert_eq!(proof.1["targetLabel"], expected["target_label"]);
                let default = get_context(&app, id.as_str().unwrap(), None, 182).await;
                let focused = get_context(&app, id.as_str().unwrap(), Some(token), 183).await;
                assert_eq!(focused.0, StatusCode::OK);
                assert_eq!(focused.1["matchedProof"], proof.1);
                assert!(focused.1.get("surfaceId").is_none());
                for feature in default.1["features"].as_array().unwrap() {
                    assert!(
                        focused.1["features"].as_array().unwrap().contains(feature),
                        "focus hid an existing feature"
                    );
                }
                applied = true;
            }
            assert!(applied, "{} has no focusable named-place receipt", id);
        }
    }
}

#[tokio::test]
async fn populated_collections_preserve_requirements_exclusions_and_geographic_membership() {
    let (app, state, _events) = test_app_fixture().await;
    install_collection_inventory(&state, true);
    let initial = get_search(
        &app,
        "3BHK in Hoodi under 2.4 Cr within 1 km of Fixture School 1 not in Sarjapur",
        210,
    )
    .await;
    assert_eq!(initial.0, StatusCode::OK, "{}", initial.1);
    let collections = initial.1["active"]["collections"].as_array().unwrap();
    assert_eq!(
        collections
            .iter()
            .map(|c| c["strategy"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "same_geography_wider_budget",
            "adjacent_areas",
            "other_areas"
        ],
        "{}",
        initial.1
    );
    assert_collection_planner_contract(&initial.1);
    for (rail, prefix) in collections.iter().zip(["wider", "nearby", "other"]) {
        assert_eq!(
            rail["cards"].as_array().unwrap().len(),
            if prefix == "other" { 4 } else { 2 },
            "{rail}"
        );
        for card in rail["cards"].as_array().unwrap() {
            assert!(
                card["id"].as_str().unwrap().starts_with(prefix)
                    || (prefix == "other" && card["id"].as_str().unwrap().starts_with("remote")),
                "{card}"
            );
            assert_eq!(card["bhk"], 3);
            assert!(
                card["price"].as_u64().unwrap()
                    <= if prefix == "wider" {
                        28_800_000
                    } else {
                        24_000_000
                    }
            );
        }
    }
    let edit = post_revision(
        &app,
        revision_request(&initial.1, "under 2.5cr", "rail-edit"),
        211,
    )
    .await;
    assert_eq!(edit.0, StatusCode::OK);
    assert_collection_planner_contract(&edit.1);
    let resume = post_resume(&app, resume_request(&edit.1), 212).await;
    assert_eq!(
        resume.1["active"]["collections"],
        edit.1["active"]["collections"]
    );
    assert_eq!(resume.1["active"]["intent"], edit.1["active"]["intent"]);
    let negated = get_search(&app, "3BHK in Hoodi not under 2.4 Cr", 213).await;
    assert!(negated.1["active"]["collections"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["strategy"] != "same_geography_wider_budget"));
    let multi = get_search(
        &app,
        "3BHK in Hoodi under 2.4 Cr or 2BHK in Sarjapur under 2 Cr",
        214,
    )
    .await;
    assert_collection_planner_contract(&multi.1);
    assert!(!multi.1["active"]["collections"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn missing_topology_uses_constrained_nonempty_shelves_without_geographic_claims() {
    let (app, state, _events) = test_app_fixture().await;
    install_collection_inventory(&state, false);
    let search = get_search(
        &app,
        "3BHK in Hoodi under 2.4 Cr within 1 km of Fixture School 1 not in Sarjapur",
        215,
    )
    .await;
    assert_eq!(search.0, StatusCode::OK);
    let collections = search.1["active"]["collections"].as_array().unwrap();
    assert!(!collections.is_empty(), "{}", search.1);
    assert_collection_planner_contract(&search.1);
    for rail in collections {
        assert_eq!(rail["strategy"], "configured_fallback");
        for card in rail["cards"].as_array().unwrap() {
            assert_eq!(card["bhk"], 3);
            assert!(card["price"].as_u64().unwrap() <= 24_000_000);
            assert!(!card["id"].as_str().unwrap().starts_with("excluded"));
            assert!(!card["id"].as_str().unwrap().starts_with("unproven"));
        }
    }
    let unresolved = get_search(&app, "3BHK in Unmapped Township under 2.4 Cr", 216).await;
    assert!(unresolved.1["active"]["collections"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(unresolved.1["active"]["buyerBrief"]
        .as_str()
        .unwrap()
        .to_lowercase()
        .contains("unmapped"));
    assert!(
        !unresolved.1["active"]["intent"]["branches"][0]["unresolvedRequirements"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let resumed = post_resume(&app, resume_request(&unresolved.1), 217).await;
    assert_eq!(
        resumed.1["active"]["intent"],
        unresolved.1["active"]["intent"]
    );
    assert!(resumed.1["active"]["results"]["orderedResultIds"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn inventory_and_proof_use_proof_identity_independently_of_catalog_version() {
    let (app, _state, _events) = test_app_fixture_with_identity(true, true).await;
    let parent = get_search(&app, "3bhk in Hoodi under 3cr near schools", 201).await;
    assert_eq!(parent.0, StatusCode::OK);
    assert!(!result_ids(&parent.1).is_empty(), "{}", parent.1);
    let revised = post_revision(
        &app,
        revision_request(&parent.1, "under 2.5cr", "distinct-proof"),
        202,
    )
    .await;
    assert_eq!(revised.0, StatusCode::OK);
    assert_journey_envelope(&revised.1, "revision", "activated");
    let resumed = post_resume(&app, resume_request(&revised.1), 203).await;
    assert_eq!(result_ids(&resumed.1), result_ids(&revised.1));
    assert_eq!(resumed.1["active"]["intent"], revised.1["active"]["intent"]);
    let result = &revised.1["active"]["results"]["resultSets"][0]["results"][0];
    let token = result["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .find(|reason| reason["showOnCard"] == true)
        .unwrap()["proofToken"]
        .as_str()
        .unwrap();
    let surface = get_context(&app, result["id"].as_str().unwrap(), Some(token), 204).await;
    assert_eq!(surface.0, StatusCode::OK, "{}", surface.1);
}

#[tokio::test]
async fn search_bank_chained_edits_and_resume_keep_active_contract() {
    let bank: Value =
        serde_json::from_str(include_str!("../../data/validation/search_query_bank.json")).unwrap();
    let cases = bank["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["group"] == "journey_api")
        .collect::<Vec<_>>();
    assert!(!cases.is_empty());
    for case in cases {
        let app = test_app().await;
        let (status, mut active) = get_search(&app, case["query"].as_str().unwrap(), 201).await;
        assert_eq!(status, StatusCode::OK, "case={}", case["id"]);
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let previous_ids = result_ids(&active);
            let (status, next) = if step["resume"] == true {
                post_resume(&app, resume_request(&active), 202 + index as u8).await
            } else {
                let mut request = revision_request(
                    &active,
                    step["utterance"].as_str().unwrap(),
                    &format!("{}-{index}", case["id"]),
                );
                if let Some(dimension) = step["target_dimension"].as_str() {
                    let predicate = intent_predicates(&active)
                        .into_iter()
                        .find(|predicate| predicate["dimension"] == dimension)
                        .unwrap();
                    request["target"] = json!({ "kind": "predicate", "branchId": active["active"]["intent"]["branches"][0]["id"], "predicateId": predicate["id"] });
                }
                post_revision(&app, request, 202 + index as u8).await
            };
            assert_eq!(
                status,
                StatusCode::OK,
                "case={} step={index}: {next}",
                case["id"]
            );
            assert_eq!(
                next["attempt"]["outcome"], step["outcome"],
                "case={} step={index}: {next}",
                case["id"]
            );
            if step["same_results"] == true {
                assert_eq!(result_ids(&next), previous_ids);
            }
            if let Some(operation) = step.get("operation") {
                assert_eq!(
                    &next["attempt"]["operation"], operation,
                    "case={} step={index}",
                    case["id"]
                );
            }
            if let Some(excluded) = step["brief_excludes"].as_array() {
                for text in excluded {
                    assert!(
                        !next["active"]["buyerBrief"]
                            .as_str()
                            .unwrap()
                            .contains(text.as_str().unwrap()),
                        "case={} step={index}: {} retained {text}",
                        case["id"],
                        next["active"]["buyerBrief"]
                    );
                }
            }
            for text in step["brief_contains"].as_array().unwrap() {
                assert!(
                    next["active"]["buyerBrief"]
                        .as_str()
                        .unwrap()
                        .contains(text.as_str().unwrap()),
                    "case={} step={index}: {} missing {text}",
                    case["id"],
                    next["active"]["buyerBrief"]
                );
            }
            active = next;
        }
    }
}

#[tokio::test]
async fn regression_zero_result_attempt_explains_selected_home() {
    let app = test_app().await;
    let parent = get_search(&app, "3BHK in Hoodi under 2.4 Cr", 181).await;
    let mut request = revision_request(&parent.1, "Only 4BHK", "review-zero");
    request["selectedPropertyId"] = json!(result_ids(&parent.1)[0]);
    let revised = post_revision(&app, request, 182).await;
    let consequence = &revised.1["attempt"]["selectedPropertyConsequence"];
    assert!(
        consequence["failedPredicateIds"]
            .as_array()
            .is_some_and(|ids| !ids.is_empty()),
        "Rejected refinement lacks failed predicates: outcome={}, explanation={}",
        consequence["outcome"],
        consequence["explanation"]
    );
}

#[tokio::test]
async fn regression_expansion_inherits_explicit_branch_budget() {
    let app = test_app().await;
    let parent = get_search(
        &app,
        "3BHK in Hoodi under 2.4 Cr or 3BHK in Sarjapur under 2.35 Cr",
        183,
    )
    .await;
    let branches = parent.1["active"]["intent"]["branches"].as_array().unwrap();
    assert_eq!(branches.len(), 2);
    let mut request = revision_request(&parent.1, "Also consider Hoodi", "review-targeted-expand");
    request["target"] = json!({"kind":"branch", "branchId":branches[1]["id"]});
    let revised = post_revision(&app, request, 184).await;
    let branches = revised.1["active"]["intent"]["branches"]
        .as_array()
        .unwrap();
    let alternative = branches.last().unwrap().to_string();
    assert!(
        alternative.contains("23500000"),
        "Alternative of the targeted branch should inherit its budget: {}",
        revised.1["active"]["intent"]
    );
}

#[tokio::test]
async fn regression_buyer_brief_keeps_spatial_distance() {
    let app = test_app().await;
    let search = get_search(
        &app,
        "3BHK within 1 km of Fixture School 6 under 2.4 Cr",
        185,
    )
    .await;
    let brief = search.1["active"]["buyerBrief"].as_str().unwrap();
    assert!(
        brief.contains("1 km"),
        "Distance must survive in the buyer brief: {brief}; intent={}",
        search.1["active"]["intent"]
    );
}

#[tokio::test]
async fn regression_basic_search_brief_has_buyer_copy() {
    let app = test_app().await;
    let search = get_search(&app, "3BHK in Hoodi under 2.4 Cr", 186).await;
    let brief = search.1["active"]["buyerBrief"].as_str().unwrap();
    assert!(
        !brief.contains("max="),
        "Buyer brief exposes storage formatting: {brief}"
    );
}

#[tokio::test]
async fn regression_surface_keeps_exact_resolved_receipt() {
    let (app, state) = test_app_with_state().await;
    let root = tempdir().unwrap();
    let mut bundle = test_bundle_with_options(root.path(), true, "review-exact", false, true);
    let mut facts = bundle
        .fact_index
        .all_facts()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    let exact = fact_nth(&facts, "society:fixture-home", "nearby_schools", 5).clone();
    let mut competing = exact.clone();
    competing.source_url = Some("https://example.test/different-receipt".to_string());
    competing.observation = Some(
        SourceObservation::new(
            "Google",
            "different-receipt",
            &competing.entity_id,
            competing.learned_at,
            competing.source_url.clone(),
            vec!["asset:competing".to_string()],
        )
        .unwrap(),
    );
    let position = facts
        .iter()
        .position(|fact| fact.observation == exact.observation)
        .unwrap();
    facts.insert(position, competing);
    bundle.fact_index = ServingFactIndex::from_records(facts, Vec::new());
    bundle.evidence_index = backend::serving::ServingEvidenceIndex::from_records(
        bundle.fact_index.all_facts(),
        &bundle.edges,
    )
    .unwrap();
    let properties = test_properties(false);
    let search_index =
        SearchIndex::build_with_serving_graph(&properties, &bundle.entities, &bundle.edges);
    state
        .search_runtime
        .store(Arc::new(SearchRuntimeSnapshot::new(
            Arc::new(bundle),
            properties,
            Vec::new(),
            search_index,
        )));
    let references = [EvidenceRef::for_observation(
        "review-exact",
        exact.observation.as_ref().unwrap(),
    )];
    let token = issue_proof_token(
        &state.search_runtime.load_full(),
        ProofIssueRequest {
            claim: None,
            constraint: None,
            snapshot_identity: "review-exact",
            semantic_fingerprint: "review",
            property_id: "fixture-home-3bhk",
            branch_id: "review-branch",
            predicate_id: "review-predicate",
            subject_entity_id: "society:fixture-home",
            target_entity_id: Some("place:fixture-school-6"),
            fact_key: "nearby_schools",
            relation: "supports",
            evidence_refs: &references,
        },
    )
    .unwrap();
    let resolved = post_proof(&app, json!({"proofToken":token}), 187).await;
    assert_eq!(resolved.0, StatusCode::OK);
    let surface = get_context(&app, "fixture-home-3bhk", Some(&token), 188).await;
    assert_eq!(surface.0, StatusCode::OK);
    assert_eq!(
        surface.1["matchedProof"], resolved.1,
        "Context must preserve the exact resolved receipt independently of presentation"
    );
}

#[tokio::test]
async fn regression_ambiguous_price_edit_does_not_remove_budget() {
    let app = test_app().await;
    let parent = get_search(&app, "3BHK in Hoodi under 2.4 Cr", 189).await;
    let price = intent_predicates(&parent.1)
        .into_iter()
        .find(|predicate| predicate["dimension"] == "price")
        .unwrap();
    let mut request = revision_request(&parent.1, "Make it a bit cheaper", "review-cheaper");
    request["target"] = json!({"kind":"predicate", "branchId":parent.1["active"]["intent"]["branches"][0]["id"], "predicateId":price["id"]});
    let revised = post_revision(&app, request, 190).await;
    assert_eq!(revised.0, StatusCode::OK);
    assert!(
        intent_predicates(&revised.1)
            .iter()
            .any(|predicate| predicate["dimension"] == "price"),
        "An ambiguous targeted price edit must preserve the budget: outcome={}, intent={}",
        revised.1["attempt"]["outcome"],
        revised.1["active"]["intent"]
    );
}

#[tokio::test]
async fn regression_negative_condition_is_not_explained_as_positive() {
    let app = test_app().await;
    let search = get_search(&app, "Homes under 2.4 Cr, not 4 BHK", 191).await;
    let predicate = intent_predicates(&search.1)
        .into_iter()
        .find(|predicate| predicate["dimension"] == "bhk" && predicate["polarity"] == "negative")
        .unwrap();
    let reason = search.1["active"]["results"]["resultSets"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|set| set["results"].as_array().unwrap())
        .flat_map(|result| result["reasons"].as_array().unwrap())
        .find(|reason| reason["predicateId"] == predicate["id"])
        .unwrap();
    assert!(
        reason["explanation"].as_str().unwrap().contains("not"),
        "A negative BHK constraint must not claim a positive BHK match: {}",
        reason["explanation"]
    );
}

#[tokio::test]
async fn one_envelope_contract_covers_start_revision_and_resume() {
    let (app, mut events) = test_app_with_events().await;
    let parent = get_search(&app, "3BHK in Hoodi under 2.4 Cr", 21).await;
    assert_eq!(parent.0, StatusCode::OK, "response={}", parent.1);
    assert_journey_envelope(&parent.1, "initial", "activated");
    assert_collection_planner_contract(&parent.1);
    assert_eq!(
        parent.1["active"]["latestUtterance"],
        "3BHK in Hoodi under 2.4 Cr"
    );
    assert!(parent.1["active"]["revision"]["stateToken"]
        .as_str()
        .unwrap()
        .starts_with("v2."));
    let _ = events.recv().await.expect("initial search event");

    let revised = post_revision(
        &app,
        revision_request(&parent.1, "Make it under 2.5Cr", "budget-refine"),
        22,
    )
    .await;
    assert_eq!(revised.0, StatusCode::OK, "response={}", revised.1);
    assert_journey_envelope(&revised.1, "revision", "activated");
    assert_collection_planner_contract(&revised.1);
    assert_eq!(
        revised.1["active"]["latestUtterance"],
        "Make it under 2.5Cr"
    );
    let expected_brief = revised.1["active"]["buyerBrief"].as_str().unwrap();
    let mut observed_candidate_event = false;
    for _ in 0..2 {
        let SearchLogMessage::SearchEvent(event) = events.recv().await.expect("revision event");
        observed_candidate_event |= event.query == expected_brief;
    }
    assert!(observed_candidate_event);

    let chained = post_revision(
        &app,
        revision_request(
            &revised.1,
            "Actually, keep it under 2.35Cr",
            "chained-budget-refine",
        ),
        23,
    )
    .await;
    assert_eq!(chained.0, StatusCode::OK, "response={}", chained.1);
    assert_journey_envelope(&chained.1, "revision", "activated");
    assert_collection_planner_contract(&chained.1);
    assert_eq!(
        chained.1["active"]["latestUtterance"],
        "Actually, keep it under 2.35Cr"
    );
    assert_ne!(
        chained.1["active"]["buyerBrief"],
        revised.1["active"]["buyerBrief"]
    );

    let resumed = post_resume(&app, resume_request(&chained.1), 24).await;
    assert_eq!(resumed.0, StatusCode::OK, "response={}", resumed.1);
    assert_journey_envelope(&resumed.1, "resume", "resumed");
    assert_collection_planner_contract(&resumed.1);
    assert_eq!(
        resumed.1["active"]["collections"],
        chained.1["active"]["collections"]
    );
    assert_eq!(result_ids(&resumed.1), result_ids(&chained.1));
    assert_eq!(
        resumed.1["active"]["latestUtterance"],
        chained.1["active"]["latestUtterance"]
    );
}

#[tokio::test]
async fn nested_intent_preferences_and_stable_ids_survive_revision_and_resume() {
    let app = test_app().await;
    let parent = get_search(
        &app,
        "2BHK under 2 Cr or 3BHK under 2.4 Cr, not 4 BHK, quiet neighborhood",
        31,
    )
    .await;
    assert_eq!(parent.0, StatusCode::OK, "response={}", parent.1);
    let parent_predicates = intent_predicates(&parent.1);
    assert_eq!(parent.1["active"]["intent"]["root"]["op"], "any");
    assert!(parent.1["active"]["intent"]
        .to_string()
        .contains("\"kind\":\"not\""));
    let price = parent_predicates
        .iter()
        .find(|predicate| predicate["dimension"] == "price")
        .expect("price predicate");
    let branch_id = parent.1["active"]["intent"]["branches"][0]["id"]
        .as_str()
        .unwrap();
    let preference_ids = intent_preference_ids(&parent.1);
    assert!(!preference_ids.is_empty(), "response={}", parent.1);

    let mut request = revision_request(&parent.1, "Make it under 2.5Cr", "typed-price-target");
    request["target"] = json!({
        "kind": "predicate",
        "branchId": branch_id,
        "predicateId": price["id"]
    });
    let revised = post_revision(&app, request, 32).await;
    assert_eq!(revised.0, StatusCode::OK, "response={}", revised.1);
    assert_eq!(revised.1["attempt"]["outcome"], "activated");
    let revised_ids = intent_predicate_ids(&revised.1);
    for predicate in parent_predicates {
        assert!(revised_ids.contains(&predicate["id"].as_str().unwrap().to_string()));
    }
    assert_eq!(intent_preference_ids(&revised.1), preference_ids);

    let mut preference_request =
        revision_request(&revised.1, "avoid traffic", "typed-preference-target");
    preference_request["target"] = json!({
        "kind": "predicate",
        "branchId": branch_id,
        "predicateId": preference_ids[0]
    });
    let preference_revised = post_revision(&app, preference_request, 33).await;
    assert_eq!(
        preference_revised.1["attempt"]["outcome"], "activated",
        "response={}",
        preference_revised.1
    );
    assert!(!intent_preference_ids(&preference_revised.1).contains(&preference_ids[0]));

    let resumed = post_resume(&app, resume_request(&preference_revised.1), 34).await;
    assert_eq!(
        resumed.1["active"]["intent"],
        preference_revised.1["active"]["intent"]
    );
    assert_eq!(result_ids(&resumed.1), result_ids(&preference_revised.1));
}

#[tokio::test]
async fn semantic_failures_preserve_the_refreshed_parent() {
    let app = test_app().await;
    let parent = get_search(&app, "3BHK in Hoodi under 2.4 Cr", 41).await;
    let selected = result_ids(&parent.1)[0].clone();

    let mut zero_request = revision_request(&parent.1, "Only 4BHK", "zero-result");
    zero_request["selectedPropertyId"] = json!(selected);
    let zero = post_revision(&app, zero_request, 42).await;
    assert_eq!(zero.1["attempt"]["outcome"], "preservedParent");
    assert_eq!(zero.1["active"]["collections"], json!([]));
    assert_eq!(zero.1["attempt"]["clarification"]["code"], "zeroResults");
    assert_eq!(
        zero.1["attempt"]["selectedPropertyConsequence"]["outcome"],
        "retained"
    );
    assert_eq!(
        zero.1["attempt"]["selectedPropertyConsequence"]["cause"],
        "intentRefinement"
    );
    assert_eq!(zero.1["active"]["latestUtterance"], "Only 4BHK");
    assert_eq!(
        decode_signed_search_context(zero.1["active"]["revision"]["stateToken"].as_str().unwrap())
            .unwrap()
            .latest_utterance,
        "Only 4BHK"
    );
    assert_eq!(result_ids(&zero.1), result_ids(&parent.1));

    let second = result_ids(&parent.1)[1].clone();
    let mut exclusion_request =
        revision_request(&parent.1, "Make it under 2.35Cr", "selected-excluded");
    exclusion_request["selectedPropertyId"] = json!(second);
    let excluded = post_revision(&app, exclusion_request, 142).await;
    assert_eq!(excluded.1["attempt"]["outcome"], "activated");
    let consequence = &excluded.1["attempt"]["selectedPropertyConsequence"];
    assert_eq!(consequence["outcome"], "excluded");
    assert_eq!(consequence["cause"], "intentRefinement");
    assert!(!consequence["branchIds"].as_array().unwrap().is_empty());
    assert!(!consequence["failedPredicateIds"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(consequence["explanation"]
        .as_str()
        .unwrap()
        .contains("Conditions not met"));

    let unsupported = post_revision(
        &app,
        revision_request(
            &parent.1,
            "Instead, search for plots in North Bengaluru under 1.5Cr",
            "unsupported-inventory",
        ),
        43,
    )
    .await;
    assert_eq!(unsupported.1["attempt"]["outcome"], "preservedParent");
    assert_eq!(
        unsupported.1["attempt"]["clarification"]["code"],
        "requiredCapabilityUnavailable"
    );

    let clarification = post_revision(
        &app,
        revision_request(&parent.1, "Make it closer", "ambiguous-change"),
        44,
    )
    .await;
    assert_eq!(
        clarification.1["attempt"]["outcome"],
        "clarificationRequired"
    );
    assert_eq!(result_ids(&clarification.1), result_ids(&parent.1));

    let limit_parent = get_search(
        &app,
        "2BHK under 1Cr or 2BHK under 1.1Cr or 2BHK under 1.2Cr or 2BHK under 1.3Cr or 2BHK under 1.4Cr or 2BHK under 1.5Cr or 2BHK under 1.6Cr or 3BHK under 2.4Cr",
        45,
    )
    .await;
    let limit = post_revision(
        &app,
        revision_request(&limit_parent.1, "Also consider Sarjapur", "ninth-branch"),
        46,
    )
    .await;
    assert_eq!(limit.1["attempt"]["outcome"], "limitReached");
}

#[tokio::test]
async fn explicit_branch_target_resolves_an_ambiguous_edit_only_for_that_branch() {
    let app = test_app().await;
    let parent = get_search(
        &app,
        "3BHK in Hoodi under 2.4Cr or 3BHK in Sarjapur under 2.4Cr",
        51,
    )
    .await;
    let untargeted = post_revision(
        &app,
        revision_request(&parent.1, "Make it under 2.3Cr", "untargeted-budget"),
        52,
    )
    .await;
    assert_eq!(untargeted.1["attempt"]["outcome"], "clarificationRequired");

    let branches = parent.1["active"]["intent"]["branches"].as_array().unwrap();
    let target_id = branches[0]["id"].as_str().unwrap();
    let untouched_id = branches[1]["id"].as_str().unwrap();
    let untouched_before = branches[1]["constraints"].clone();
    let mut targeted = revision_request(&parent.1, "Make it under 2.3Cr", "targeted-budget");
    targeted["target"] = json!({"kind": "branch", "branchId": target_id});
    let targeted = post_revision(&app, targeted, 53).await;
    assert_eq!(targeted.0, StatusCode::OK, "response={}", targeted.1);
    assert_eq!(targeted.1["attempt"]["outcome"], "activated");
    let branches_after = targeted.1["active"]["intent"]["branches"]
        .as_array()
        .unwrap();
    assert_eq!(
        branches_after
            .iter()
            .find(|branch| branch["id"] == untouched_id)
            .unwrap()["constraints"],
        untouched_before
    );
    let mut full_targeted = revision_request(
        &parent.1,
        "3BHK in Hoodi under 2.5Cr",
        "targeted-full-query",
    );
    full_targeted["target"] = json!({"kind": "branch", "branchId": target_id});
    let full_targeted = post_revision(&app, full_targeted, 54).await;
    assert_eq!(full_targeted.0, StatusCode::OK);
    assert_eq!(full_targeted.1["attempt"]["outcome"], "activated");
    assert_eq!(
        full_targeted.1["active"]["intent"]["branches"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        full_targeted.1["active"]["intent"]["branches"]
            .as_array()
            .unwrap()
            .iter()
            .find(|branch| branch["id"] == untouched_id)
            .unwrap()["constraints"],
        untouched_before
    );
}

#[tokio::test]
async fn catalog_and_intent_deltas_are_computed_from_distinct_baselines() {
    let (app, state) = test_app_with_state().await;
    let parent = get_search(&app, "3BHK under 2.4 Cr", 61).await;
    assert_eq!(result_ids(&parent.1).len(), 2, "response={}", parent.1);

    install_runtime(&state, "journey-fixture-v2", true, true);
    let revised = post_revision(
        &app,
        revision_request(
            &parent.1,
            "Make it under 2.35Cr",
            "catalog-and-intent-delta",
        ),
        62,
    )
    .await;
    assert_eq!(revised.0, StatusCode::OK, "response={}", revised.1);
    assert_eq!(revised.1["attempt"]["catalogRebased"], true);
    assert!(revised.1["attempt"]["catalogDelta"]["added"]
        .as_array()
        .unwrap()
        .contains(&json!("fresh-home-3bhk")));
    assert!(revised.1["attempt"]["intentDelta"]["removed"]
        .as_array()
        .unwrap()
        .contains(&json!("second-home-3bhk")));
    assert!(revised.1["attempt"]["catalogDelta"]
        .get("reordered")
        .is_none());
    assert!(revised.1["attempt"]["catalogDelta"]["moved"].is_array());
    assert!(revised.1["attempt"]["intentDelta"]
        .get("reordered")
        .is_none());
    assert_eq!(
        revised.1["runtimeVersion"]["servingBundleVersion"],
        "journey-fixture-v2"
    );
}

#[tokio::test]
async fn selected_property_catalog_exclusion_is_structured() {
    let (app, state) = test_app_with_state().await;
    let parent = get_search(&app, "3BHK under 2.4 Cr", 63).await;
    let selected = result_ids(&parent.1)[1].clone();
    install_runtime_without_second_home(&state, "journey-fixture-v2");
    let mut request = revision_request(
        &parent.1,
        "Make it under 2.5Cr",
        "selected-catalog-exclusion",
    );
    request["selectedPropertyId"] = json!(selected);
    let revised = post_revision(&app, request, 64).await;
    assert_eq!(revised.0, StatusCode::OK, "response={}", revised.1);
    let consequence = &revised.1["attempt"]["selectedPropertyConsequence"];
    assert_eq!(consequence["outcome"], "excluded");
    assert_eq!(consequence["cause"], "catalogRefresh");
    assert!(consequence["propertyId"].is_string());
    assert!(consequence["explanation"]
        .as_str()
        .unwrap()
        .contains("refreshed catalog"));
}

#[tokio::test]
async fn resume_executes_the_signed_ast_even_when_the_brief_is_not_parseable() {
    let (app, state) = test_app_with_state().await;
    let parent = get_search(&app, "3BHK under 2.4 Cr", 71).await;
    let ids = result_ids(&parent.1);
    let snapshot = state.search_runtime.load_full();
    let plan = SearchEngine::new(&snapshot).compile_initial("3BHK under 2.4 Cr", "root");
    let runtime_version = runtime_version(&snapshot);
    let issued = issue_signed_search_context(
        None,
        "non-parseable-brief",
        SearchRevisionOperation::Initial,
        1,
        "⚑ [presentation only] :: never parse this".to_string(),
        "Keep the saved search".to_string(),
        plan,
        runtime_version,
        ids.clone(),
    )
    .unwrap();
    assert_eq!(
        decode_signed_search_context(&issued.state_token)
            .unwrap()
            .buyer_brief,
        "⚑ [presentation only] :: never parse this"
    );

    let resumed = post_resume(
        &app,
        json!({"parentToken": issued.state_token, "knownResultIds": ids}),
        72,
    )
    .await;
    assert_eq!(resumed.0, StatusCode::OK, "response={}", resumed.1);
    assert_eq!(result_ids(&resumed.1), result_ids(&parent.1));
    assert_eq!(
        resumed.1["active"]["latestUtterance"],
        "Keep the saved search"
    );
}

#[tokio::test]
async fn exact_proof_resolution_and_surface_focus_share_one_identity() {
    let (app, state) = test_app_with_state().await;
    remove_hydration_markers(&state);
    let search = get_search(&app, "3BHK near Fixture School 6 under 2.4 Cr", 81).await;
    assert_eq!(search.0, StatusCode::OK, "response={}", search.1);
    let proof_token = proof_token_for_fact(&app, &search.1, "nearby_schools", 82).await;
    let resolved = post_proof(
        &app,
        json!({"proofToken": proof_token, "propertyId": "fixture-home-3bhk"}),
        83,
    )
    .await;
    assert_eq!(resolved.0, StatusCode::OK, "response={}", resolved.1);
    assert_eq!(resolved.1["value"]["type"], "Numeric");
    assert_eq!(resolved.1["value"]["data"], 0.8);
    assert_eq!(resolved.1["unit"], "km");
    assert_eq!(resolved.1["targetLabel"], "Fixture School 6");
    assert_eq!(resolved.1["sourceObservations"][0]["provider"], "Google");

    let default_scene = get_context(&app, "fixture-home-3bhk", None, 84).await;
    let focused_scene = get_context(&app, "fixture-home-3bhk", Some(&proof_token), 85).await;
    assert_eq!(
        default_scene.0,
        StatusCode::OK,
        "response={}",
        default_scene.1
    );
    assert_eq!(
        focused_scene.0,
        StatusCode::OK,
        "response={}",
        focused_scene.1
    );
    assert_eq!(focused_scene.1["matchedProof"], resolved.1);
    assert!(
        default_scene.1["features"].as_array().unwrap().len() >= 6,
        "Domain context must not apply a presentation cap"
    );
    for feature in default_scene.1["features"].as_array().unwrap() {
        assert!(focused_scene.1["features"]
            .as_array()
            .unwrap()
            .contains(feature));
    }

    let revised = post_revision(
        &app,
        revision_request(
            &search.1,
            "Make it under 2.5Cr",
            "proof-no-parquet-revision",
        ),
        86,
    )
    .await;
    let resumed = post_resume(&app, resume_request(&revised.1), 87).await;
    assert_eq!(revised.0, StatusCode::OK);
    assert_eq!(resumed.0, StatusCode::OK);
}

#[tokio::test]
async fn proof_tokens_reject_tampering_stale_snapshots_wrong_properties_and_missing_evidence() {
    let (app, state) = test_app_with_state().await;
    let search = get_search(
        &app,
        "3BHK within 1 km of Fixture School 6 under 2.4 Cr",
        91,
    )
    .await;
    let token = proof_token_for_fact(&app, &search.1, "nearby_schools", 92).await;

    let mut tampered = token.clone();
    let replacement = if tampered.ends_with('0') { "1" } else { "0" };
    tampered.replace_range(tampered.len() - 1.., replacement);
    let invalid = post_proof(&app, json!({"proofToken": tampered}), 93).await;
    assert_eq!(invalid.0, StatusCode::BAD_REQUEST);
    assert_eq!(invalid.1["code"], "invalid_proof_token");
    let invalid_surface = get_context(&app, "fixture-home-3bhk", Some(&tampered), 193).await;
    assert_eq!(invalid_surface.0, StatusCode::BAD_REQUEST);
    assert_eq!(invalid_surface.1["error"], "invalid_proof_token");

    let wrong_property = post_proof(
        &app,
        json!({"proofToken": token, "propertyId": "second-home-3bhk"}),
        94,
    )
    .await;
    assert_eq!(wrong_property.0, StatusCode::CONFLICT);
    assert_eq!(wrong_property.1["code"], "proof_property_mismatch");
    let wrong_surface = get_context(&app, "second-home-3bhk", Some(&token), 95).await;
    assert_eq!(wrong_surface.0, StatusCode::BAD_REQUEST);
    assert_eq!(wrong_surface.1["error"], "proof_property_mismatch");

    let fake_observation = SourceObservation::new(
        "Google",
        "missing-proof-observation",
        "society:fixture-home",
        Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
        Some("https://example.test/missing".to_string()),
        vec!["asset:missing-proof/v1".to_string()],
    )
    .unwrap();
    let fake_reference = EvidenceRef::for_observation("journey-fixture-v1", &fake_observation);
    let refused = issue_proof_token(
        &state.search_runtime.load_full(),
        ProofIssueRequest {
            claim: None,
            constraint: None,
            snapshot_identity: "journey-fixture-v1",
            semantic_fingerprint: "sha256:missing-proof",
            property_id: "fixture-home-3bhk",
            branch_id: "branch-1",
            predicate_id: "predicate:missing",
            subject_entity_id: "society:fixture-home",
            target_entity_id: None,
            fact_key: "nearby_schools",
            relation: "supports",
            evidence_refs: &[fake_reference],
        },
    );
    assert!(refused.is_err(), "issuer must reject absent evidence");

    // A damaged snapshot must return a distinct missing-evidence outcome for an
    // already-issued receipt, rather than reclassifying it as stale or invalid.
    let intact = state.search_runtime.load_full();
    let damaged_root = tempfile::tempdir().unwrap();
    let mut damaged_bundle =
        test_bundle_with_options(damaged_root.path(), true, "journey-fixture-v1", false, true);
    damaged_bundle.evidence_index = backend::serving::ServingEvidenceIndex::default();
    state
        .search_runtime
        .store(Arc::new(SearchRuntimeSnapshot::new(
            Arc::new(damaged_bundle),
            test_properties(false),
            Vec::new(),
            SearchIndex::build(&test_properties(false)),
        )));
    let missing = post_proof(&app, json!({"proofToken": token}), 96).await;
    assert_eq!(missing.0, StatusCode::NOT_FOUND);
    assert_eq!(missing.1["resolutionStatus"], "missingEvidence");
    let retired_surface = get_context(&app, "fixture-home-3bhk", Some(&token), 196).await;
    assert_eq!(retired_surface.0, StatusCode::NOT_FOUND);
    assert_eq!(retired_surface.1["error"], "proof_evidence_missing");
    state.search_runtime.store(intact);

    let current = state.search_runtime.load_full();
    let mut remapped_properties = test_properties(false);
    remapped_properties[0].society_id = "second-home".to_string();
    let remapped_index = SearchIndex::build(&remapped_properties);
    state
        .search_runtime
        .store(Arc::new(SearchRuntimeSnapshot::new(
            current.bundle.clone(),
            remapped_properties,
            Vec::new(),
            remapped_index,
        )));
    let wrong_subject = post_proof(&app, json!({"proofToken": token}), 97).await;
    assert_eq!(wrong_subject.0, StatusCode::CONFLICT);
    assert_eq!(wrong_subject.1["code"], "proof_subject_mismatch");

    install_runtime(&state, "journey-fixture-v2", true, true);
    let stale = post_proof(&app, json!({"proofToken": token}), 98).await;
    assert_eq!(stale.0, StatusCode::CONFLICT);
    assert_eq!(stale.1["code"], "stale_proof_snapshot");
    let stale_surface = get_context(&app, "fixture-home-3bhk", Some(&token), 99).await;
    assert_eq!(stale_surface.0, StatusCode::CONFLICT);
    assert_eq!(stale_surface.1["error"], "stale_proof_snapshot");
}

#[tokio::test]
async fn authenticated_transport_idempotency_and_retained_rebind_are_explicit() {
    let (app, state) = test_app_with_state().await;
    let parent = get_search(&app, "3BHK in Hoodi under 2.4 Cr", 101).await;
    let token = parent.1["active"]["revision"]["stateToken"]
        .as_str()
        .unwrap();
    let payload = serde_json::to_value(decode_signed_search_context(token).unwrap()).unwrap();
    assert!(payload.get("intentAst").is_some());
    assert!(!payload.to_string().contains("orderedResultIds"));

    let request = revision_request(&parent.1, "Make it under 2.5Cr", "same-mutation");
    let (first, second) = tokio::join!(
        post_revision(&app, request.clone(), 102),
        post_revision(&app, request, 103),
    );
    assert_eq!(first, second);
    let conflict = post_revision(
        &app,
        revision_request(&parent.1, "Only 4BHK", "same-mutation"),
        104,
    )
    .await;
    assert_eq!(conflict.1["code"], "client_mutation_id_conflict");

    let mut mismatch = revision_request(&parent.1, "Make it under 2.5Cr", "mismatch");
    mismatch["parentResultIds"] = json!(["not-the-parent"]);
    assert_eq!(
        post_revision(&app, mismatch, 105).await.0,
        StatusCode::CONFLICT
    );
    let oversized = "a".repeat(
        backend::security::security_tuning()
            .requests
            .max_search_query_bytes
            + 1,
    );
    assert_eq!(
        post_revision(
            &app,
            revision_request(&parent.1, &oversized, "oversized"),
            106,
        )
        .await
        .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let mut too_many_ids =
        revision_request(&parent.1, "Make it under 2.5Cr", "too-many-parent-results");
    too_many_ids["parentResultIds"] = json!((0..=backend::search::schema::ranking_policy()
        .result_limit)
        .map(|index| format!("home-{index}"))
        .collect::<Vec<_>>());
    assert_eq!(
        post_revision(&app, too_many_ids, 109).await.0,
        StatusCode::BAD_REQUEST
    );
    let mut duplicate_ids =
        revision_request(&parent.1, "Make it under 2.5Cr", "duplicate-parent-results");
    duplicate_ids["parentResultIds"] = json!(["fixture-home-3bhk", "fixture-home-3bhk"]);
    assert_eq!(
        post_revision(&app, duplicate_ids, 112).await.0,
        StatusCode::BAD_REQUEST
    );
    let mut oversized_target =
        revision_request(&parent.1, "Make it under 2.5Cr", "oversized-target");
    oversized_target["target"] = json!({
        "kind": "branch",
        "branchId": "b".repeat(
            backend::security::security_tuning()
                .search_journey
                .max_target_id_bytes
                + 1
        )
    });
    assert_eq!(
        post_revision(&app, oversized_target, 113).await.0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let oversized_body = "a".repeat(
        backend::security::security_tuning()
            .requests
            .search_body_bytes,
    );
    let response = post_revision(
        &app,
        revision_request(&parent.1, &oversized_body, "oversized-body"),
        110,
    )
    .await;
    assert_eq!(response.0, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(response.1["code"], "search_body_too_large");

    let spatial_parent = get_search(
        &app,
        "3BHK within 1 km of Fixture School 6 under 2.4 Cr",
        107,
    )
    .await;
    install_runtime(&state, "journey-fixture-v2", true, false);
    let retained = post_resume(&app, resume_request(&spatial_parent.1), 108).await;
    assert_eq!(retained.0, StatusCode::OK, "response={}", retained.1);
    assert_eq!(retained.1["active"]["results"]["kind"], "retained");
    assert_eq!(result_ids(&retained.1), result_ids(&spatial_parent.1));
}

#[tokio::test]
async fn exact_society_revision_does_not_require_optional_geo_cell_topology() {
    let (app, _state, _events) = test_app_fixture_with_society_topology(false).await;
    let parent = get_search(&app, "Fixture Home 3BHK", 111).await;
    assert!(!result_ids(&parent.1).is_empty(), "response={}", parent.1);
    assert!(parent.1["active"]["collections"]
        .as_array()
        .unwrap()
        .iter()
        .all(|collection| collection["strategy"] == "configured_fallback"));
    let revised = post_revision(
        &app,
        revision_request(&parent.1, "Make it 3BHK", "exact-society-without-cells"),
        112,
    )
    .await;
    assert_eq!(revised.0, StatusCode::OK, "response={}", revised.1);
    assert_eq!(result_ids(&revised.1), result_ids(&parent.1));
}

async fn test_app() -> Router {
    test_app_with_state().await.0
}

async fn test_app_with_state() -> (Router, Arc<AppState>) {
    let (app, state, _events) = test_app_fixture().await;
    (app, state)
}

async fn test_app_with_events() -> (Router, mpsc::Receiver<SearchLogMessage>) {
    let (app, _state, events) = test_app_fixture().await;
    (app, events)
}

fn install_runtime(
    state: &Arc<AppState>,
    bundle_version: &str,
    include_fresh_home: bool,
    include_schools: bool,
) {
    let root = tempdir().expect("replacement runtime fixture").keep();
    let bundle = Arc::new(test_bundle_with_options(
        &root,
        true,
        bundle_version,
        include_fresh_home,
        include_schools,
    ));
    let properties = test_properties(include_fresh_home);
    let search_index =
        SearchIndex::build_with_serving_graph(&properties, &bundle.entities, &bundle.edges);
    state
        .search_runtime
        .store(Arc::new(SearchRuntimeSnapshot::new(
            bundle,
            properties,
            Vec::new(),
            search_index,
        )));
}

fn install_runtime_without_second_home(state: &Arc<AppState>, bundle_version: &str) {
    let root = tempdir().expect("replacement runtime fixture").keep();
    let bundle = Arc::new(test_bundle_with_options(
        &root,
        true,
        bundle_version,
        true,
        true,
    ));
    let mut properties = test_properties(true);
    properties.retain(|property| property.id != "second-home-3bhk");
    let search_index =
        SearchIndex::build_with_serving_graph(&properties, &bundle.entities, &bundle.edges);
    state
        .search_runtime
        .store(Arc::new(SearchRuntimeSnapshot::new(
            bundle,
            properties,
            Vec::new(),
            search_index,
        )));
}

fn runtime_version(snapshot: &SearchRuntimeSnapshot) -> SearchRuntimeVersion {
    SearchRuntimeVersion {
        snapshot_identity: snapshot
            .bundle
            .manifest
            .proof_snapshot_identity()
            .to_string(),
        serving_bundle_version: snapshot.version_key.serving_bundle_version.clone(),
        scoring_policy_version: snapshot.version_key.scoring_policy_version,
        search_engine_version: snapshot.version_key.search_engine_version.clone(),
        semantic_contract_digest: snapshot.version_key.semantic_contract_digest.clone(),
    }
}

fn remove_hydration_markers(state: &Arc<AppState>) {
    let runtime = state.search_runtime.load();
    let root = runtime
        .bundle
        .cache_dir
        .parent()
        .expect("fixture cache has a parent");
    for key in ["entities.parquet", "facts.parquet", "search.parquet"] {
        std::fs::remove_file(root.join(key)).expect("hydration marker is removable");
    }
}

async fn test_app_fixture() -> (Router, Arc<AppState>, mpsc::Receiver<SearchLogMessage>) {
    test_app_fixture_with_society_topology(true).await
}

async fn test_app_fixture_with_society_topology(
    include_society_topology: bool,
) -> (Router, Arc<AppState>, mpsc::Receiver<SearchLogMessage>) {
    test_app_fixture_with_identity(include_society_topology, false).await
}

async fn test_app_fixture_with_identity(
    include_society_topology: bool,
    distinct_proof_identity: bool,
) -> (Router, Arc<AppState>, mpsc::Receiver<SearchLogMessage>) {
    let root = tempdir().expect("temporary API fixture root").keep();
    let lake = LakeStore::local(root.join("lake")).expect("temporary lake");
    let bundle = Arc::new(test_bundle_with_options(
        &root,
        include_society_topology,
        "journey-fixture-v1",
        false,
        true,
    ));
    let bundle = if distinct_proof_identity {
        let mut bundle = Arc::try_unwrap(bundle).ok().unwrap();
        bundle.manifest.bundle_version = "journey-catalog-with-independent-proof".to_string();
        Arc::new(bundle)
    } else {
        bundle
    };
    let properties = test_properties(false);
    let search_index =
        SearchIndex::build_with_serving_graph(&properties, &bundle.entities, &bundle.edges);
    let runtime = SearchRuntimeSnapshot::new(
        bundle.clone(),
        properties.clone(),
        Vec::new(),
        search_index.clone(),
    );
    let (search_event_tx, search_event_rx) = mpsc::channel(8);
    let state = Arc::new(AppState {
        execution: ExecutionLanes::current(),
        search_runtime: ArcSwap::from_pointee(runtime),
        search_cache: SearchResponseCache::new(8),
        search_revision_caches: backend::state::SearchRevisionCaches::new(
            8,
            8 * 1024 * 1024,
            8,
            8 * 1024 * 1024,
        ),
        property_catalog_cache: tokio::sync::Mutex::new(None),
        search_event_tx,
        search_log_dropped_count: AtomicU64::new(0),
        properties: RwLock::new(properties),
        search_index: RwLock::new(search_index),
        recommendation_cache: RwLock::new(HashMap::new()),

        societies: RwLock::new(Vec::new()),
        discovery_config: backend::discovery::load_discovery_config(),
        map_overlays: Arc::new(backend::routes::map_overlays::CityMapOverlays::default()),
        project_root: root,
        process_started_at: Utc::now(),
        interest_counter: AtomicU64::new(0),
        interest_write_lock: tokio::sync::Mutex::new(()),
    });
    (
        build_app_router_with_lake(state.clone(), lake),
        state,
        search_event_rx,
    )
}

fn test_bundle_with_options(
    root: &std::path::Path,
    include_society_topology: bool,
    bundle_version: &str,
    include_fresh_home: bool,
    include_schools: bool,
) -> LoadedServingBundle {
    let mut entities = vec![
        serving_entity("area:hoodi", "area", "Hoodi"),
        serving_entity("area:sarjapur", "area", "Sarjapur"),
        serving_entity("area:cell:hoodi", "area", "Internal search cell"),
        serving_entity("area:cell:sarjapur", "area", "Internal search cell"),
        serving_entity("society:fixture-home", "society", "Fixture Home"),
        serving_entity(
            "property:fixture-home-3bhk",
            "property",
            "Fixture Home 3 BHK",
        ),
        serving_entity("society:second-home", "society", "Second Home"),
        serving_entity("property:second-home-3bhk", "property", "Second Home 3 BHK"),
    ];
    let mut facts = vec![
        topology_fact(
            "area:cell:hoodi",
            "geo.geometry_geojson",
            FactValue::Text(square_geometry(77.70, 77.72)),
        ),
        topology_fact(
            "area:cell:sarjapur",
            "geo.geometry_geojson",
            FactValue::Text(square_geometry(77.77, 77.79)),
        ),
        topology_fact(
            "society:fixture-home",
            "geo.latitude",
            FactValue::Numeric(12.975),
        ),
        topology_fact(
            "society:fixture-home",
            "geo.longitude",
            FactValue::Numeric(77.715),
        ),
    ];
    facts.push(topology_fact(
        "society:fixture-home",
        "controlled_inventory_option",
        FactValue::Text(json!({"bhk": 3, "price": 23_000_000, "area_sqft": 1_550}).to_string()),
    ));
    facts.extend([
        topology_fact(
            "society:second-home",
            "geo.latitude",
            FactValue::Numeric(13.20),
        ),
        topology_fact(
            "society:second-home",
            "geo.longitude",
            FactValue::Numeric(77.90),
        ),
        topology_fact(
            "society:second-home",
            "controlled_inventory_option",
            FactValue::Text(json!({"bhk": 3, "price": 24_000_000, "area_sqft": 1_600}).to_string()),
        ),
    ]);
    if include_fresh_home {
        entities.extend([
            serving_entity("society:fresh-home", "society", "Fresh Home"),
            serving_entity("property:fresh-home-3bhk", "property", "Fresh Home 3 BHK"),
        ]);
        facts.extend([
            topology_fact(
                "society:fresh-home",
                "geo.latitude",
                FactValue::Numeric(12.98),
            ),
            topology_fact(
                "society:fresh-home",
                "geo.longitude",
                FactValue::Numeric(77.72),
            ),
            topology_fact(
                "society:fresh-home",
                "controlled_inventory_option",
                FactValue::Text(
                    json!({"bhk": 3, "price": 22_000_000, "area_sqft": 1_500}).to_string(),
                ),
            ),
        ]);
    }
    if include_schools {
        for index in 1..=6 {
            let entity_id = format!("place:fixture-school-{index}");
            entities.push(serving_entity(
                &entity_id,
                "place",
                &format!("Fixture School {index}"),
            ));
            facts.extend([
                topology_fact(
                    &entity_id,
                    "place.category",
                    FactValue::Text("school".to_string()),
                ),
                topology_fact(
                    &entity_id,
                    "geo.latitude",
                    FactValue::Numeric(12.975 + f64::from(index) * 0.001),
                ),
                topology_fact(
                    &entity_id,
                    "geo.longitude",
                    FactValue::Numeric(77.715 + f64::from(index) * 0.001),
                ),
                topology_fact(
                    "society:fixture-home",
                    "nearby_schools",
                    FactValue::Text(format!(
                        "Fixture School {index} (0.8 km, 4.5 rating, {} reviews)",
                        1_000 - index * 100
                    )),
                ),
            ]);
            // Production projects one raw nearby record onto both the society
            // and the place. Observation IDs remain subject-scoped.
            let nearby = facts.last().unwrap().observation.clone().unwrap();
            for fact in facts.iter_mut().filter(|fact| fact.entity_id == entity_id) {
                fact.observation = Some(
                    SourceObservation::new(
                        &nearby.provider,
                        &nearby.provider_observation_id,
                        &entity_id,
                        nearby.observed_at,
                        nearby.source_url.clone(),
                        nearby.asset_lineage.clone(),
                    )
                    .unwrap(),
                );
            }
        }
    }

    let fixture_inventory = fact(
        &facts,
        "society:fixture-home",
        "controlled_inventory_option",
    );
    let second_inventory = fact(&facts, "society:second-home", "controlled_inventory_option");
    let mut edges = vec![
        topology_edge(
            bundle_version,
            "property:fixture-home-3bhk",
            "in_society",
            "society:fixture-home",
            fixture_inventory,
        ),
        topology_edge(
            bundle_version,
            "property:second-home-3bhk",
            "in_society",
            "society:second-home",
            second_inventory,
        ),
    ];
    if include_fresh_home {
        edges.push(topology_edge(
            bundle_version,
            "property:fresh-home-3bhk",
            "in_society",
            "society:fresh-home",
            fact(&facts, "society:fresh-home", "controlled_inventory_option"),
        ));
    }
    if include_schools {
        for index in 1..=6 {
            edges.push(proximity_edge(
                bundle_version,
                "society:fixture-home",
                &format!("place:fixture-school-{index}"),
                fact(
                    &facts,
                    &format!("place:fixture-school-{index}"),
                    "geo.latitude",
                ),
                0.2 + index as f64 * 0.1,
            ));
        }
    }
    if include_society_topology {
        edges.extend([
            topology_edge(
                bundle_version,
                "society:fixture-home",
                "in_market_locality",
                "area:hoodi",
                &facts[2],
            ),
            topology_edge(
                bundle_version,
                "area:hoodi",
                "covers_geo_cell",
                "area:cell:hoodi",
                &facts[0],
            ),
            topology_edge(
                bundle_version,
                "area:sarjapur",
                "covers_geo_cell",
                "area:cell:sarjapur",
                &facts[1],
            ),
            topology_edge(
                bundle_version,
                "society:fixture-home",
                "occupies_geo_cell",
                "area:cell:hoodi",
                &facts[2],
            ),
            topology_edge(
                bundle_version,
                "society:second-home",
                "in_market_locality",
                "area:hoodi",
                second_inventory,
            ),
        ]);
        if include_fresh_home {
            edges.push(topology_edge(
                bundle_version,
                "society:fresh-home",
                "in_market_locality",
                "area:hoodi",
                fact(&facts, "society:fresh-home", "controlled_inventory_option"),
            ));
        }
    }
    backend::serving::context_binding::materialize_context_bindings(
        &entities,
        &facts,
        &mut edges,
        bundle_version,
    )
    .unwrap();
    let fact_index = ServingFactIndex::from_records(facts.clone(), Vec::new());
    let evidence_index =
        backend::serving::ServingEvidenceIndex::from_records(fact_index.all_facts(), &edges)
            .expect("journey fixture evidence index");
    let recall_dir = root.join("tantivy");
    let recall_index = TantivyRecallIndex::build_in_dir(&recall_dir, &entities, &facts, &[])
        .expect("fixture recall index");

    for key in ["entities.parquet", "facts.parquet", "search.parquet"] {
        std::fs::write(root.join(key), b"hydration marker").expect("fixture hydration marker");
    }
    let graph_index = GraphIndex::from_serving_bundle(&entities, &edges, bundle_version);
    LoadedServingBundle {
        manifest: ServingBundleManifest {
            bundle_version: bundle_version.to_string(),
            proof_snapshot_identity: bundle_version.to_string(),
            format_version: 1,
            created_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            entity_count: entities.len() as u64,
            entity_alias_count: 0,
            fact_count: facts.len() as u64,
            search_metadata_count: 0,
            rera_evidence_count: 0,
            excluded_rera_evidence_society_ids: Vec::new(),
            edge_count: edges.len() as u64,
            eligibility_policy_version: 0,
            quarantined_society_count: 0,
            quarantine_reason_counts: Default::default(),
            entity_parquet_key: "entities.parquet".to_string(),
            entity_alias_parquet_key: "aliases.parquet".to_string(),
            fact_parquet_key: "facts.parquet".to_string(),
            search_metadata_parquet_key: "search.parquet".to_string(),
            rera_evidence_parquet_key: "rera.parquet".to_string(),
            edge_parquet_key: "edges.parquet".to_string(),
            quarantine_report_key: "quarantine.json".to_string(),
            schema_key: "schema.json".to_string(),
            tantivy_index_prefix: "tantivy".to_string(),
            artifacts: Vec::new(),
        },
        entity_alias_index: ServingEntityAliasIndex::default(),
        graph_index,
        entity_index: SpatialEntityIndex::from_serving_bundle(&entities, &fact_index),
        spatial_index: SpatialServingIndex::from_serving_bundle_with_edges(
            &entities,
            &fact_index,
            &edges,
        ),
        search_capabilities: SearchCapabilityIndex::from_bundle(&entities, &fact_index),
        rera_evidence_index: ReraEvidenceIndex::default(),
        recall_index,
        fact_index,
        evidence_index,
        entities,
        edges,
        cache_dir: recall_dir,
    }
}

// Extend the existing journey fixture, retaining the same observation/topology
// constructors and production runtime hydration used by the other API scenarios.
fn install_collection_inventory(state: &Arc<AppState>, topology: bool) {
    let root = tempdir().unwrap().keep();
    let mut bundle = test_bundle_with_options(&root, topology, "collection-fixture", false, true);
    let identity = bundle.manifest.proof_snapshot_identity().to_string();
    let mut facts = bundle
        .fact_index
        .all_facts()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    let mut properties = test_properties(false);
    for (area, min, max) in [
        ("neighbor", 77.77, 77.79),
        ("distant", 77.90, 77.92),
        ("remote", 78.10, 78.12),
    ] {
        let market = format!("area:{area}");
        let cell = format!("area:cell:{area}");
        bundle.entities.extend([
            serving_entity(&market, "area", area),
            serving_entity(&cell, "area", "Internal cell"),
        ]);
        let geometry = topology_fact(
            &cell,
            "geo.geometry_geojson",
            FactValue::Text(square_geometry(min, max)),
        );
        if topology {
            bundle.edges.push(topology_edge(
                &identity,
                &market,
                "covers_geo_cell",
                &cell,
                &geometry,
            ));
            if area == "neighbor" || area == "remote" {
                bundle.edges.push(topology_edge(
                    &identity,
                    "area:cell:hoodi",
                    "adjacent_area",
                    &cell,
                    &geometry,
                ));
            }
        }
        facts.push(geometry);
    }
    for (prefix, area, lon, price, bhk, proof, count) in [
        ("wider", "hoodi", 77.715, 26_000_000, 3, true, 2),
        ("nearby", "neighbor", 77.775, 22_000_000, 3, true, 2),
        ("other", "distant", 77.915, 22_000_000, 3, true, 2),
        ("excluded", "sarjapur", 77.78, 22_000_000, 3, true, 2),
        ("wrong", "neighbor", 77.775, 22_000_000, 2, true, 2),
        ("unproven", "neighbor", 77.775, 22_000_000, 3, false, 2),
        ("remote", "remote", 78.115, 22_000_000, 3, true, 2),
        ("unlocated", "distant", 77.915, 22_000_000, 3, true, 2),
    ] {
        for index in 0..count {
            let slug = format!("{prefix}-{index}");
            let society = format!("society:{slug}");
            let id = format!("{slug}-{bhk}bhk");
            let entity_id = format!("property:{id}");
            let mut property = test_property(&id, &slug, &slug, price);
            property.bhk = bhk;
            property.area = area.to_string();
            property.area_id = area.to_string();
            property.hero_image = "/landing/tiles/04-rera-evidence.webp".to_string();
            bundle.entities.extend([
                serving_entity(&society, "society", &slug),
                serving_entity(&entity_id, "property", &id),
            ]);
            let inventory = topology_fact(
                &society,
                "controlled_inventory_option",
                FactValue::Text(json!({"bhk":bhk,"price":price,"area_sqft":1550}).to_string()),
            );
            bundle.edges.push(topology_edge(
                &identity,
                &entity_id,
                "in_society",
                &society,
                &inventory,
            ));
            if prefix != "unlocated" {
                bundle.edges.push(topology_edge(
                    &identity,
                    &society,
                    "in_market_locality",
                    &format!("area:{area}"),
                    &inventory,
                ));
                if topology {
                    bundle.edges.push(topology_edge(
                        &identity,
                        &society,
                        "occupies_geo_cell",
                        &format!("area:cell:{area}"),
                        &inventory,
                    ));
                }
            }
            facts.extend([
                inventory,
                topology_fact(&society, "geo.latitude", FactValue::Numeric(12.975)),
                topology_fact(&society, "geo.longitude", FactValue::Numeric(lon)),
            ]);
            if proof {
                let school = topology_fact(
                    &society,
                    "nearby_schools",
                    FactValue::Text("Fixture School 1 (0.8 km)".to_string()),
                );
                bundle.edges.push(proximity_edge(
                    &identity,
                    &society,
                    "place:fixture-school-1",
                    &school,
                    0.8,
                ));
                facts.push(school);
            }
            properties.push(property);
        }
    }
    bundle.fact_index = ServingFactIndex::from_records(facts.clone(), Vec::new());
    bundle.graph_index =
        GraphIndex::from_serving_bundle(&bundle.entities, &bundle.edges, &identity);
    bundle.entity_index = SpatialEntityIndex::from_serving_bundle_with_edges(
        &bundle.entities,
        &bundle.fact_index,
        &bundle.edges,
    );
    bundle.spatial_index = SpatialServingIndex::from_serving_bundle_with_edges(
        &bundle.entities,
        &bundle.fact_index,
        &bundle.edges,
    );
    bundle.evidence_index = backend::serving::ServingEvidenceIndex::from_records(
        bundle.fact_index.all_facts(),
        &bundle.edges,
    )
    .unwrap();
    bundle.search_capabilities =
        SearchCapabilityIndex::from_bundle(&bundle.entities, &bundle.fact_index);
    bundle.recall_index = TantivyRecallIndex::build_in_dir(
        root.join("extended-recall"),
        &bundle.entities,
        &facts,
        &[],
    )
    .unwrap();
    let search_index =
        SearchIndex::build_with_serving_graph(&properties, &bundle.entities, &bundle.edges);
    state
        .search_runtime
        .store(Arc::new(SearchRuntimeSnapshot::new(
            Arc::new(bundle),
            properties,
            Vec::new(),
            search_index,
        )));
}

fn serving_entity(entity_id: &str, entity_type: &str, name: &str) -> ServingEntityRecord {
    ServingEntityRecord {
        entity_id: entity_id.to_string(),
        entity_type: entity_type.to_string(),
        name: name.to_string(),
        root_source: Some("revision_api_contract".to_string()),
        visibility: Default::default(),
        searchable_text: name.to_string(),
    }
}

fn topology_fact(entity_id: &str, fact_key: &str, value: FactValue) -> ServingFactRecord {
    let learned_at = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
    let value_identity = serde_json::to_string(&value).expect("fixture value serializes");
    let source_url = Some("https://example.test/openstreetmap".to_string());
    let source_type = if matches!(
        fact_key,
        "geo.latitude" | "geo.longitude" | "place.category"
    ) || fact_key.starts_with("nearby_")
    {
        "Google"
    } else {
        "OpenStreetMap"
    };
    backend::serving::measurements::normalize_distance_fact(ServingFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        value_type: match &value {
            FactValue::Numeric(_) | FactValue::Score { .. } => "numeric",
            FactValue::Text(_) => "text",
            FactValue::Bool(_) => "bool",
            FactValue::Tags(_) => "tags",
        }
        .to_string(),
        value_text: None,
        value,
        confidence: 0.9,
        source_type: source_type.to_string(),
        source_url: source_url.clone(),
        model: None,
        skill_id: Some("search_revision_api_contract".to_string()),
        learned_at,
        observation: Some(
            SourceObservation::new(
                source_type,
                if matches!(fact_key, "geo.latitude" | "geo.longitude") {
                    format!("{entity_id}:coordinates")
                } else {
                    format!("{entity_id}:{fact_key}:{value_identity}")
                },
                entity_id,
                learned_at,
                source_url,
                vec!["asset:search-revision-api-contract/v1".to_string()],
            )
            .expect("revision topology observation"),
        ),
    })
}

fn topology_edge(
    snapshot_identity: &str,
    from: &str,
    relation: &str,
    to: &str,
    evidence_fact: &ServingFactRecord,
) -> ServingEdgeRecord {
    let evidence = EvidenceRef::for_observation(
        snapshot_identity,
        evidence_fact
            .observation
            .as_ref()
            .expect("revision topology edge evidence"),
    );
    ServingEdgeRecord {
        from_entity_id: from.to_string(),
        edge_type: relation.to_string(),
        to_entity_id: to.to_string(),
        confidence: 0.9,
        source_type: "OpenStreetMap".to_string(),
        derivation: Some(
            DerivedEvidence::new(
                snapshot_identity,
                from,
                Some(to.to_string()),
                relation,
                "controlled_topology",
                Some(1.0),
                Some("boolean".to_string()),
                "search-revision-api-contract-v1",
                0.9,
                vec![evidence],
            )
            .expect("revision topology derivation"),
        ),
    }
}

fn proximity_edge(
    snapshot_identity: &str,
    from: &str,
    to: &str,
    evidence_fact: &ServingFactRecord,
    distance_km: f64,
) -> ServingEdgeRecord {
    let evidence = EvidenceRef::for_observation(
        snapshot_identity,
        evidence_fact
            .observation
            .as_ref()
            .expect("proximity edge observation"),
    );
    ServingEdgeRecord {
        from_entity_id: from.to_string(),
        edge_type: "near_place".to_string(),
        to_entity_id: to.to_string(),
        confidence: 0.9,
        source_type: "Computed".to_string(),
        derivation: Some(
            DerivedEvidence::new(
                snapshot_identity,
                from,
                Some(to.to_string()),
                "near_place",
                "trusted_point_distance",
                Some(distance_km),
                Some("km".to_string()),
                "search-revision-api-contract-v1",
                0.9,
                vec![evidence],
            )
            .expect("proximity derivation"),
        ),
    }
}

fn fact<'a>(
    facts: &'a [ServingFactRecord],
    entity_id: &str,
    fact_key: &str,
) -> &'a ServingFactRecord {
    fact_nth(facts, entity_id, fact_key, 0)
}

fn fact_nth<'a>(
    facts: &'a [ServingFactRecord],
    entity_id: &str,
    fact_key: &str,
    index: usize,
) -> &'a ServingFactRecord {
    facts
        .iter()
        .filter(|fact| fact.entity_id == entity_id && fact.fact_key == fact_key)
        .nth(index)
        .expect("fixture fact exists")
}

fn square_geometry(min_lon: f64, max_lon: f64) -> String {
    format!(
        "{{\"type\":\"Polygon\",\"coordinates\":[[[{min_lon},12.96],[{max_lon},12.96],[{max_lon},12.99],[{min_lon},12.99],[{min_lon},12.96]]]}}"
    )
}

fn test_properties(include_fresh_home: bool) -> Vec<Property> {
    let mut properties = vec![
        test_property(
            "fixture-home-3bhk",
            "Fixture Home",
            "fixture-home",
            23_000_000,
        ),
        test_property("second-home-3bhk", "Second Home", "second-home", 24_000_000),
    ];
    if include_fresh_home {
        properties.push(test_property(
            "fresh-home-3bhk",
            "Fresh Home",
            "fresh-home",
            22_000_000,
        ));
    }
    properties
}

fn test_property(id: &str, title: &str, society_id: &str, price: u64) -> Property {
    Property {
        id: id.to_string(),
        title: title.to_string(),
        area: "Hoodi".to_string(),
        area_id: "hoodi".to_string(),
        city: "Bengaluru".to_string(),
        society_id: society_id.to_string(),
        builder_name: "Fixture Builder".to_string(),
        property_type: "Apartment".to_string(),
        listing_type: "Resale".to_string(),
        bhk: 3,
        price,
        price_min: None,
        price_max: None,
        price_per_sqft: 12_000,
        carpet_area_sqft: 1_200,
        super_builtup_sqft: 1_550,
        area_measurement: None,
        floor: 8,
        total_floors: 20,
        facing: "East".to_string(),
        possession_status: "Ready to Move".to_string(),
        metro_distance_mins: 8,
        maintenance_cost_monthly: 6_000,
        society_quality_score: Some(0.7),
        builder_quality_score: Some(0.7),
        document_completeness_score: Some(0.8),
        litigation_risk: Some(0.1),
        noise_score: Some(0.2),
        sunlight_score: Some(0.7),
        airport_noise_score: Some(0.1),
        waterlogging_risk_score: Some(0.2),
        traffic_score: Some(0.4),
        days_on_market: 20,
        greenery_score: Some(0.6),
        open_space_score: Some(0.6),
        resale_strength_score: Some(0.7),
        interest_level: None,
        saves_last_7d: None,
        offers_last_7d: None,
        images: Vec::new(),
        hero_image: String::new(),
        description_summary: "Revision API contract fixture".to_string(),

        source_reference: "revision_api_contract".to_string(),
    }
}

fn revision_request(parent: &Value, utterance: &str, key: &str) -> Value {
    json!({
        "parentToken": parent["active"]["revision"]["stateToken"],
        "parentResultIds": parent["active"]["results"]["orderedResultIds"],
        "utterance": utterance,
        "clientMutationId": key
    })
}

fn resume_request(parent: &Value) -> Value {
    json!({
        "parentToken": parent["active"]["revision"]["stateToken"],
        "knownResultIds": parent["active"]["results"]["orderedResultIds"]
    })
}

fn result_ids(envelope: &Value) -> Vec<String> {
    envelope["active"]["results"]["orderedResultIds"]
        .as_array()
        .expect("journey results expose ordered IDs")
        .iter()
        .map(|id| id.as_str().unwrap().to_string())
        .collect()
}

fn assert_journey_envelope(envelope: &Value, attempt_kind: &str, outcome: &str) {
    assert_eq!(envelope["contractVersion"], 1);
    assert!(envelope["runtimeVersion"]["servingBundleVersion"].is_string());
    assert!(envelope["active"]["revision"]["stateToken"].is_string());
    assert!(envelope["active"]["buyerBrief"].is_string());
    assert!(envelope["active"]["latestUtterance"].is_string());
    assert!(envelope["active"]["intent"]["branches"].is_array());
    let collections = envelope["active"]["collections"]
        .as_array()
        .expect("every active journey exposes collections");
    assert!(collections.len() <= 3);
    assert_eq!(envelope["active"]["results"]["kind"], "current");
    assert_eq!(envelope["attempt"]["kind"], attempt_kind);
    assert_eq!(envelope["attempt"]["outcome"], outcome);
    for removed in [
        "query",
        "revisionId",
        "intentBreakdown",
        "match_reason",
        "verifiedMatches",
        "proofFocuses",
    ] {
        assert!(!envelope.as_object().unwrap().contains_key(removed));
    }
    for result in envelope["active"]["results"]["resultSets"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|set| set["results"].as_array().unwrap())
    {
        assert!(result["reasons"].is_array());
        for removed in [
            "match_reason",
            "matchReason",
            "match_score",
            "score",
            "signals",
            "images",
            "kg_entity_refs",
            "description_summary",
            "matchExplanation",
            "verifiedMatches",
            "proofFocuses",
        ] {
            assert!(result.get(removed).is_none(), "result={result}");
        }
    }
}

fn assert_collection_planner_contract(envelope: &Value) {
    let collections = envelope["active"]["collections"].as_array().unwrap();
    assert!(serde_json::to_vec(collections).unwrap().len() < 80 * 1024);
    let strategy_order = [
        "same_geography_wider_budget",
        "adjacent_areas",
        "other_areas",
    ];
    let mut previous_strategy = None;
    let mut societies = envelope["active"]["results"]["resultSets"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|set| set["results"].as_array().unwrap())
        .filter_map(|result| result["society_id"].as_str().map(str::to_string))
        .collect::<std::collections::HashSet<_>>();
    for collection in collections {
        let cards = collection["cards"].as_array().unwrap();
        assert!(cards.len() <= 12);
        if let Some(index) = strategy_order
            .iter()
            .position(|strategy| collection["strategy"] == *strategy)
        {
            assert!(previous_strategy.is_none_or(|previous| index > previous));
            previous_strategy = Some(index);
        }
        let actual_min = cards
            .iter()
            .map(|card| {
                card["price_min"]
                    .as_u64()
                    .unwrap_or_else(|| card["price"].as_u64().unwrap())
            })
            .min()
            .unwrap();
        let actual_max = cards
            .iter()
            .map(|card| {
                card["price_max"]
                    .as_u64()
                    .unwrap_or_else(|| card["price"].as_u64().unwrap())
            })
            .max()
            .unwrap();
        assert_eq!(collection["priceBand"]["min"], actual_min);
        assert_eq!(collection["priceBand"]["max"], actual_max);
        assert!(!collection["title"]
            .as_str()
            .unwrap()
            .contains(collection["priceBand"]["label"].as_str().unwrap()));
        for card in cards {
            assert!(societies.insert(card["society_id"].as_str().unwrap().to_string()));
        }
    }
}

fn intent_predicates(envelope: &Value) -> Vec<Value> {
    fn collect(expression: &Value, output: &mut Vec<Value>) {
        match expression["kind"].as_str() {
            Some("predicate") => output.push(expression["predicate"].clone()),
            Some("all" | "any") => {
                for clause in expression["clauses"].as_array().unwrap() {
                    collect(clause, output);
                }
            }
            Some("not") => collect(&expression["clause"], output),
            _ => {}
        }
    }
    let mut output = Vec::new();
    for branch in envelope["active"]["intent"]["branches"].as_array().unwrap() {
        collect(&branch["constraints"], &mut output);
    }
    output
}

fn intent_predicate_ids(envelope: &Value) -> Vec<String> {
    intent_predicates(envelope)
        .into_iter()
        .map(|predicate| predicate["id"].as_str().unwrap().to_string())
        .collect()
}

fn intent_preference_ids(envelope: &Value) -> Vec<String> {
    envelope["active"]["intent"]["branches"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|branch| branch["preferences"].as_array().unwrap())
        .map(|preference| preference["id"].as_str().unwrap().to_string())
        .collect()
}

async fn proof_token_for_fact(app: &Router, envelope: &Value, fact_key: &str, peer: u8) -> String {
    let tokens = envelope["active"]["results"]["resultSets"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|set| set["results"].as_array().unwrap())
        .flat_map(|result| result["reasons"].as_array().unwrap())
        .filter_map(|reason| reason["proofToken"].as_str())
        .map(str::to_string)
        .collect::<Vec<_>>();
    for token in tokens {
        let response = post_proof(app, json!({"proofToken": token}), peer).await;
        if response.0 == StatusCode::OK && response.1["factKey"] == fact_key {
            return token;
        }
    }
    panic!("no result reason resolved {fact_key}: {envelope}")
}

async fn get_search(app: &Router, query: &str, peer: u8) -> (StatusCode, Value) {
    let encoded = query.replace(' ', "%20");
    send(
        app,
        Method::GET,
        &format!("/api/search?q={encoded}"),
        Body::empty(),
        peer,
    )
    .await
}

async fn post_revision(app: &Router, payload: Value, peer: u8) -> (StatusCode, Value) {
    send(
        app,
        Method::POST,
        "/api/search/revisions",
        Body::from(payload.to_string()),
        peer,
    )
    .await
}

async fn post_resume(app: &Router, payload: Value, peer: u8) -> (StatusCode, Value) {
    send(
        app,
        Method::POST,
        "/api/search/resume",
        Body::from(payload.to_string()),
        peer,
    )
    .await
}

async fn post_proof(app: &Router, payload: Value, peer: u8) -> (StatusCode, Value) {
    send(
        app,
        Method::POST,
        "/api/search/proofs/resolve",
        Body::from(payload.to_string()),
        peer,
    )
    .await
}

async fn get_context(
    app: &Router,
    property_id: &str,
    proof_token: Option<&str>,
    peer: u8,
) -> (StatusCode, Value) {
    let suffix = proof_token
        .map(|token| format!("?proofToken={token}"))
        .unwrap_or_default();
    send(
        app,
        Method::GET,
        &format!("/api/properties/{property_id}/context{suffix}"),
        Body::empty(),
        peer,
    )
    .await
}

async fn send(
    app: &Router,
    method: Method,
    uri: &str,
    body: Body,
    peer: u8,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .extension(ConnectInfo(SocketAddr::from(([192, 0, 2, peer], 41000))))
        .body(body)
        .expect("contract request is valid");
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("revision route responds");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .expect("response body is readable");
    let value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) if status == StatusCode::UNPROCESSABLE_ENTITY => {
            Value::String(String::from_utf8_lossy(&bytes).into_owned())
        }
        Err(error) => panic!(
            "response is JSON: {error}; body={}",
            String::from_utf8_lossy(&bytes)
        ),
    };
    (status, value)
}
