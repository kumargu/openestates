import unittest
import json
from pathlib import Path

from pipeline.benchmark_search_quality import (
    case_result,
    evaluate_case,
    evaluate_proof_handoffs,
    flattened_results,
    load_suite,
    public_quality_summary,
    serving_bundle_requirement_error,
)


def search_diagnostics() -> dict:
    return {
        "layerTimings": [
            {"layer": layer, "durationMs": 1.0}
            for layer in (
                "intent_parse",
                "structured_recall",
                "tantivy_recall",
                "ranking",
                "total",
            )
        ],
        "resolved": {
            "entities": [
                {
                    "entityId": "place:metro:kudlu-gate",
                    "entityType": "place",
                    "name": "Kudlu Gate Metro Station",
                    "matchSource": "serving_entity",
                    "matchedText": "Kudlu Gate Metro Station",
                    "polarity": "positive",
                }
            ]
        },
    }


class SearchQualityBenchmarkTests(unittest.TestCase):
    def test_every_live_suite_uses_only_public_response_expectations(self) -> None:
        bank_path = Path("data/validation/search_query_bank.json")
        bank = json.loads(bank_path.read_text(encoding="utf-8"))

        for suite in bank["suites"]:
            if suite["runner"] == "live_api":
                load_suite(bank, suite["id"], bank_path)

    def test_live_suite_rejects_private_response_expectations(self) -> None:
        bank = {
            "case_groups": [{"id": "private"}],
            "suites": [
                {"id": "private", "runner": "live_api", "case_groups": ["private"]}
            ],
            "cases": [
                {
                    "id": "PRIVATE-001",
                    "group": "private",
                    "query": "3BHK in Whitefield",
                    "expected": {"area": "Whitefield"},
                }
            ],
        }

        with self.assertRaisesRegex(SystemExit, "non-public expectations"):
            load_suite(bank, "private", Path("query-bank.json"))

    def test_unified_query_bank_selects_one_live_suite(self) -> None:
        bank_path = Path("data/validation/search_query_bank.json")
        bank = json.loads(bank_path.read_text(encoding="utf-8"))

        suite, cases, sources = load_suite(bank, "mixed_south_experiment", bank_path)

        self.assertEqual(suite["required_serving_bundle_version"], "search-experiment-mixed-south-45-2026-08-22-v1")
        self.assertEqual(len(cases), 59)
        self.assertEqual(len({case["id"] for case in cases}), 59)
        self.assertEqual(sources, [f"{bank_path}#mixed_south_experiment"])

    def test_public_result_sets_are_flattened_with_branch_provenance(self) -> None:
        response = {
            "resultSets": [
                {
                    "branchId": "branch-1",
                    "label": "2 BHK",
                    "results": [{"id": "property:two", "bhk": 2}],
                },
                {
                    "branchId": "branch-2",
                    "label": "3 BHK",
                    "results": [{"id": "property:three", "bhk": 3}],
                },
            ]
        }

        results = flattened_results(response)

        self.assertEqual([result["id"] for result in results], ["property:two", "property:three"])
        self.assertEqual(results[1]["_benchmark_branch_id"], "branch-2")
        self.assertEqual(results[1]["_benchmark_branch_label"], "3 BHK")

    def test_public_contract_checks_hard_constraints_tiers_and_proof(self) -> None:
        case = {
            "id": "PUBLIC-CONTRACT",
            "query": "Godrej Air 3BHK near Hoodi Metro",
            "expected": {
                "state": "results",
                "total_matches": 1,
                "branch_labels": ["Godrej Air"],
                "branch_result_ids": [["discovered-godrej-air-3bhk"]],
                "result_ids_all": ["discovered-godrej-air-3bhk"],
                "forbidden_result_ids": ["discovered-godrej-air-2bhk"],
                "ordered_result_ids_prefix": ["discovered-godrej-air-3bhk"],
                "result_bhks_all": [3],
                "result_price_max": 35_000_000,
                "result_match_tiers_all": ["exact"],
                "proof_focus_matches_all": [
                    {
                        "fact_key": "nearby_metro_stations",
                        "entity_id": "place:hoodi",
                        "distance_m": 100,
                    }
                ],
            },
        }
        response = {
            "query": case["query"],
            "resultSets": [
                {
                    "branchId": "branch-1",
                    "label": "Godrej Air",
                    "results": [
                        {
                            "id": "discovered-godrej-air-3bhk",
                            "bhk": 3,
                            "price": 31_750_000,
                            "match_tier": "exact",
                            "match_explanation": {"reasons": []},
                            "proof_focuses": [
                                {
                                    "factKey": "nearby_metro_stations",
                                    "entityId": "place:hoodi",
                                    "distanceM": 100,
                                }
                            ],
                        }
                    ],
                }
            ],
            "totalMatches": 1,
            "state": "results",
            "_request_duration_ms": 2.0,
        }

        checks = evaluate_case(case, response)

        self.assertTrue(all(check["passed"] for check in checks), checks)

    def test_zero_result_sentinel_uses_public_state(self) -> None:
        case = {
            "id": "ZERO",
            "query": "Godrej Air 4BHK",
            "expected": {"state": "no_matches", "total_matches": 0, "zero_results": True},
        }
        response = {
            "query": case["query"],
            "resultSets": [],
            "totalMatches": 0,
            "state": "no_matches",
            "_request_duration_ms": 1.0,
        }

        checks = evaluate_case(case, response)

        self.assertTrue(all(check["passed"] for check in checks), checks)

    def test_guardrail_guidance_uses_the_public_response_contract(self) -> None:
        case = {
            "id": "GUIDANCE",
            "query": "find me something good",
            "expected": {"search_guidance_mode": "needs_more_specifics"},
        }
        response = {
            "query": case["query"],
            "resultSets": [],
            "totalMatches": 0,
            "state": "no_matches",
            "searchGuidance": {
                "mode": "needs_more_specifics",
                "title": "Tell us one thing that matters",
                "message": "Add a place, budget, or home size.",
                "suggestions": [],
            },
            "_request_duration_ms": 1.0,
        }

        checks = evaluate_case(case, response)

        self.assertTrue(all(check["passed"] for check in checks), checks)

    def test_budget_constraint_uses_the_public_listing_lower_bound(self) -> None:
        case = {
            "id": "BUDGET-BAND",
            "query": "3BHK under 2cr",
            "expected": {"result_budget_max": 20_000_000},
        }
        response = {
            "resultSets": [
                {
                    "branchId": "branch-1",
                    "label": "3 BHK",
                    "results": [
                        {
                            "id": "property:banded",
                            "price": 22_000_000,
                            "price_min": 19_000_000,
                            "price_max": 25_000_000,
                        }
                    ],
                }
            ],
            "_request_duration_ms": 1.0,
        }

        checks = evaluate_case(case, response)

        self.assertTrue(all(check["passed"] for check in checks), checks)

    def test_repeated_public_results_must_keep_the_same_order(self) -> None:
        case = {"id": "STABLE", "query": "3BHK in Whitefield", "expected": {}}
        response = {
            "resultSets": [],
            "_request_duration_ms": 1.0,
            "_ordered_result_ids_runs": [["one", "two"], ["two", "one"]],
        }

        checks = evaluate_case(case, response)

        self.assertIn(
            ("stability", "ordered_result_ids"),
            {(check["layer"], check["check"]) for check in checks if not check["passed"]},
        )

    def test_public_quality_summary_reports_recall_constraints_and_proof(self) -> None:
        results = [
            {
                "expected": {"result_ids_all": ["wanted"]},
                "ordered_result_ids": ["other", "wanted"],
                "checks": [
                    {"layer": "hard_constraint", "check": "result_bhks_all", "passed": True},
                    {"layer": "proof", "check": "proof_focus_any", "passed": True},
                    {"layer": "stability", "check": "ordered_result_ids", "passed": True},
                ],
            }
        ]

        summary = public_quality_summary(results)

        self.assertEqual(summary["recall_at_1_pct"], 0.0)
        self.assertEqual(summary["recall_at_3_pct"], 100.0)
        self.assertEqual(summary["mean_reciprocal_rank"], 0.5)
        self.assertEqual(summary["hard_constraint_violation_count"], 0)
        self.assertEqual(summary["proof_precision_pct"], 100.0)

    def test_required_serving_bundle_must_match_live_runtime(self) -> None:
        self.assertIsNone(serving_bundle_requirement_error("bundle-v1", "bundle-v1"))
        self.assertIn(
            "expected 'bundle-v1', got 'bundle-v2'",
            serving_bundle_requirement_error("bundle-v1", "bundle-v2") or "",
        )

    def test_declared_failure_bucket_overrides_generic_recall_classification(self) -> None:
        result = case_result(
            {
                "id": "SEMANTIC-MISS",
                "mode": "held_out",
                "failure_bucket": "embedding_gap",
                "query": "unseen paraphrase",
            },
            None,
            [
                {
                    "layer": "recall",
                    "check": "candidate_ids_any",
                    "passed": False,
                    "detail": "candidate missing",
                }
            ],
        )

        self.assertEqual(result["failure_bucket"], "embedding_gap")

    def test_fact_first_contract_checks_result_resolution_and_proof_handles(self) -> None:
        case = {
            "id": "FACT-TEST",
            "query": "3BHK near Kudlu Gate Metro Station",
            "expected": {
                "top_result_ids_any": ["property:purva-westend-3bhk"],
                "result_ids_any": ["property:purva-westend-3bhk"],
                "result_areas_all": ["Kudlu Gate"],
                "resolved_entity_matches_all": [
                    {
                        "entity_id": "place:metro:kudlu-gate",
                        "entity_type": "place",
                        "match_source": "serving_entity",
                        "polarity": "positive",
                    }
                ],
                "proof_focus_any": [
                    {
                        "surface_id": "around_this_home",
                        "layer_id": "metro",
                        "fact_key": "nearby_metro_stations",
                        "entity_id": "place:metro:kudlu-gate",
                    }
                ],
            },
        }
        response = {
            "intent": {},
            "results": [
                {
                    "id": "property:purva-westend-3bhk",
                    "area": "Kudlu Gate",
                    "match_explanation": {"reasons": []},
                    "proof_focuses": [
                        {
                            "surfaceId": "around_this_home",
                            "layerId": "metro",
                            "factKey": "nearby_metro_stations",
                            "entityId": "place:metro:kudlu-gate",
                            "reason": "matched named metro",
                        }
                    ],
                }
            ],
            "search_diagnostics": search_diagnostics(),
        }

        checks = evaluate_case(case, response)

        self.assertTrue(all(check["passed"] for check in checks), checks)

    def test_area_and_forbidden_focus_failures_are_visible(self) -> None:
        case = {
            "id": "FACT-FAIL",
            "query": "Whitefield near metro",
            "expected": {
                "result_areas_all": ["Whitefield"],
                "forbidden_proof_focus_fact_keys": ["nearby_metro_stations"],
            },
        }
        response = {
            "intent": {},
            "results": [
                {
                    "id": "property:remote",
                    "area": "Kanakapura Road",
                    "match_explanation": {"reasons": []},
                    "proof_focuses": [
                        {
                            "surfaceId": "around_this_home",
                            "layerId": "metro",
                            "factKey": "nearby_metro_stations",
                            "reason": "remote proof",
                        }
                    ],
                }
            ],
            "search_diagnostics": search_diagnostics(),
        }

        checks = evaluate_case(case, response)
        failures = {(check["layer"], check["check"]) for check in checks if not check["passed"]}

        self.assertIn(("ranking", "result_areas_all"), failures)
        self.assertIn(("safety", "forbidden_proof_focus_fact_keys"), failures)

    def test_multi_anchor_contract_requires_every_reason_and_focus(self) -> None:
        case = {
            "id": "MULTI-ANCHOR",
            "query": "3BHK near a hospital and near a tech park",
            "expected": {
                "reason_fact_keys_all": ["nearby_hospitals", "nearby_tech_parks"],
                "proof_focus_matches_all": [
                    {"fact_key": "nearby_hospitals", "entity_id": "place:hospital"},
                    {"fact_key": "nearby_tech_parks", "entity_id": "place:office"},
                ],
            },
        }
        response = {
            "intent": {},
            "results": [
                {
                    "id": "property:one",
                    "match_explanation": {
                        "reasons": [
                            {"fact_key": "nearby_hospitals"},
                            {"fact_key": "nearby_tech_parks"},
                        ]
                    },
                    "proof_focuses": [
                        {"factKey": "nearby_hospitals", "entityId": "place:hospital"},
                        {"factKey": "nearby_tech_parks", "entityId": "place:office"},
                    ],
                }
            ],
            "search_diagnostics": search_diagnostics(),
        }

        checks = evaluate_case(case, response)

        self.assertTrue(all(check["passed"] for check in checks), checks)

    def test_proof_handoff_preserves_focus_feature_receipt_and_version(self) -> None:
        search_focus = {
            "surfaceId": "around_this_home",
            "layerId": "metro",
            "factKey": "nearby_metro_stations",
            "entityId": "place:hoodi",
            "matchedLabel": "Hoodi",
            "distanceM": 100,
            "reason": "matched near Hoodi",
        }
        scene_focus = {
            **search_focus,
            "featureId": "feature:hoodi",
            "receiptId": "receipt:hoodi",
        }
        handoffs = [
            {
                "result_id": "property:godrej-air-3bhk",
                "search_focus": search_focus,
                "scene": {
                    "servingBundleVersion": "bundle-v1",
                    "proofFocus": scene_focus,
                    "features": [
                        {
                            "id": "feature:hoodi",
                            "entityId": "place:hoodi",
                            "layerId": "metro",
                            "receiptIds": ["receipt:hoodi"],
                        }
                    ],
                    "receipts": [
                        {
                            "id": "receipt:hoodi",
                            "factKey": "nearby_metro_stations",
                            "sourceType": "Google",
                            "sourceUrl": "https://maps.google.com/hoodi",
                            "learnedAt": "2026-08-01T00:00:00Z",
                        }
                    ],
                },
            }
        ]
        requirements = [
            {
                "result_id": "property:godrej-air-3bhk",
                "fact_key": "nearby_metro_stations",
                "entity_id": "place:hoodi",
                "distance_m": 100,
                "source_type": "Google",
                "serving_bundle_version": "bundle-v1",
            }
        ]

        checks = evaluate_proof_handoffs(requirements, handoffs)

        self.assertEqual(len(checks), 5)
        self.assertTrue(all(check["passed"] for check in checks), checks)

        handoffs[0]["scene"]["receipts"][0]["sourceUrl"] = ""
        failed = evaluate_proof_handoffs(requirements, handoffs)
        self.assertIn(
            "receipt_lineage_1",
            {check["check"] for check in failed if not check["passed"]},
        )

if __name__ == "__main__":
    unittest.main()
