import unittest

from pipeline.benchmark_search_quality import (
    case_result,
    evaluate_case,
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

if __name__ == "__main__":
    unittest.main()
