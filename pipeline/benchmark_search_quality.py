"""Run the buyer-language search quality benchmark against a live backend.

The benchmark is intentionally layered. A case can fail intent parsing while
recall still works, or recall can work while proof is missing. Keeping those
signals separate prevents us from collapsing search quality into one magic score.

Usage:
    python3.10 -m pipeline.benchmark_search_quality \
      --base-url http://127.0.0.1:4000 \
      --spec data/validation/search_quality_queries_v1.json \
      --output tmp/search_quality_benchmark_v1.json \
      --markdown-output tmp/search_quality_benchmark_v1.md
"""

import argparse
import json
import math
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from collections import Counter, defaultdict
from datetime import datetime
from pathlib import Path
from typing import Any, Dict, Iterable, List, Optional, Tuple


DEFAULT_BASE_URL = "http://127.0.0.1:4000"
DEFAULT_SPEC = "data/validation/search_quality_queries_v1.json"


PREFERENCE_ALIASES: Dict[str, set[str]] = {
    "approach_road": {"approach_road"},
    "approach_road_condition": {"approach_road"},
    "builder_quality_score": {"builder_trust"},
    "builder_reputation": {"builder_trust"},
    "builder_trust": {"builder_trust"},
    "commute": {"commute"},
    "commute_reality": {"commute"},
    "density_risk": {"open_space"},
    "family_friendly": {"family_friendly"},
    "good_maintenance": {"good_maintenance"},
    "good_society": {"good_maintenance"},
    "green_cover": {"greenery"},
    "greenery": {"greenery", "open_space"},
    "legal_safety": {"legal_safety"},
    "litigation_risk": {"legal_safety"},
    "liveability": {"livability"},
    "livability_sentiment": {"livability"},
    "maintenance": {"good_maintenance"},
    "maintenance_quality": {"good_maintenance"},
    "maintenance_sentiment": {"good_maintenance"},
    "metro_access": {"commute"},
    "metro_distance_mins": {"commute"},
    "nearby_hospitals": {"family_friendly", "social_infrastructure"},
    "nearby_metro_stations": {"commute"},
    "nearby_schools": {"family_friendly"},
    "noise": {"quiet"},
    "noise_score": {"quiet"},
    "open_space": {"open_space"},
    "open_space_score": {"open_space"},
    "operating.summer_water_shortage": {"water_issues"},
    "operating.tanker_dependence": {"water_issues"},
    "premium": {"premium"},
    "price_per_sqft": {"value_for_money"},
    "pricing_insight": {"investment", "value_for_money"},
    "project_status": {"ready_to_move"},
    "ready_to_move": {"ready_to_move"},
    "reliable_builder": {"builder_trust"},
    "rental_and_resale_demand": {"investment"},
    "resale_potential": {"investment"},
    "resale_strength_score": {"investment"},
    "review_quality": {"review_quality"},
    "rera_number": {"legal_safety"},
    "rera_status": {"legal_safety"},
    "school_access": {"family_friendly"},
    "social_infra_score": {"family_friendly", "social_infrastructure"},
    "society_quality_score": {"good_maintenance", "family_friendly"},
    "tanker_dependence": {"water_issues"},
    "tanker_dependency": {"water_issues"},
    "traffic": {"commute"},
    "traffic_reality": {"commute"},
    "trusted_builder": {"builder_trust"},
    "value_for_money": {"value_for_money"},
    "water_issues": {"water_issues"},
    "water_supply": {"water_issues"},
    "water_supply_risk": {"water_issues"},
    "waterlogging_risk": {"water_issues"},
    "waterlogging_risk_score": {"water_issues"},
}


def main() -> None:
    args = parse_args()
    spec_path = Path(args.spec)
    spec = load_json(spec_path)
    cases, query_sources = load_cases(spec, spec_path.parent)
    if not cases:
        raise SystemExit("benchmark spec has no cases")

    results = []
    for case in cases:
        print(f"[{case['id']}] {first_line(case['query'])}")
        for _ in range(args.warmup_runs):
            call_search(args.base_url, case["query"], args.timeout_seconds)
        responses = [
            call_search(args.base_url, case["query"], args.timeout_seconds)
            for _ in range(args.repeat_runs)
        ]
        response = next((item for item in responses if item is not None), None)
        if response is None:
            checks = [
                check(
                    "request",
                    "api_reachable",
                    False,
                    f"GET /api/search failed for {case['id']}",
                )
            ]
            results.append(case_result(case, None, checks))
            continue

        successful_responses = [item for item in responses if item is not None]
        response["_request_durations_ms"] = [
            item["_request_duration_ms"] for item in successful_responses
        ]
        response["_ordered_result_ids_runs"] = [
            [result.get("id") for result in flattened_results(item)]
            for item in successful_responses
        ]

        checks = evaluate_case(case, response)
        passed = sum(1 for item in checks if item["passed"])
        print(f"  checks={passed}/{len(checks)} results={len(flattened_results(response))}")
        results.append(case_result(case, response, checks))

    scoreable_modes = spec.get("scoreable_modes") or inferred_scoreable_modes(cases)
    search_runtime = search_runtime_summary(results)
    health = call_health(args.base_url, args.timeout_seconds)
    if not runtime_serving_bundle_version(search_runtime) and health:
        search_runtime = {
            "serving_bundle_version": health.get("serving_bundle_version")
            or health.get("servingBundleVersion")
        }
    runtime_bundle_version = runtime_serving_bundle_version(search_runtime)
    required_bundle_version = spec.get("required_serving_bundle_version")
    bundle_requirement_error = serving_bundle_requirement_error(
        required_bundle_version, runtime_bundle_version
    )
    runtime_materialization = materialization_for_bundle_version(Path.cwd(), runtime_bundle_version)
    local_current_materialization = current_search_bundle_materialization(Path.cwd())
    serving_materialization = runtime_materialization or local_current_materialization
    output = {
        "benchmark": spec.get("benchmark"),
        "version": spec.get("version"),
        "generated_at": datetime.utcnow().isoformat(timespec="seconds") + "Z",
        "base_url": args.base_url,
        "scoreable_modes": scoreable_modes,
        "query_sources": query_sources,
        "search_runtime": search_runtime,
        "required_serving_bundle_version": required_bundle_version,
        "serving_bundle_requirement_satisfied": bundle_requirement_error is None,
        "serving_bundle_materialization": serving_materialization,
        "runtime_serving_bundle_materialization": runtime_materialization,
        "local_current_serving_bundle_materialization": local_current_materialization,
        "runtime_serving_bundle_manifest": search_bundle_manifest_summary(
            Path.cwd(), runtime_bundle_version
        ),
        "provenance_warnings": provenance_warnings(
            runtime_bundle_version,
            runtime_materialization,
            local_current_materialization,
        ),
        "summary": summarize(results, scoreable_modes),
        "results": results,
    }
    write_json(Path(args.output), output)
    print(f"\nWrote JSON: {args.output}")
    if args.markdown_output:
        Path(args.markdown_output).write_text(markdown_report(output), encoding="utf-8")
        print(f"Wrote Markdown: {args.markdown_output}")

    summary = output["summary"]
    print(
        "Overall: "
        f"{summary['passed_checks']}/{summary['total_checks']} checks passed "
        f"({summary['pass_rate_pct']}%)"
    )
    if args.max_endpoint_p95_ms is not None:
        endpoint_p95 = (summary.get("latency") or {}).get("endpoint_p95_ms")
        if endpoint_p95 is None or endpoint_p95 > args.max_endpoint_p95_ms:
            raise SystemExit(
                f"endpoint p95 latency gate failed: {endpoint_p95}ms > {args.max_endpoint_p95_ms}ms"
            )
    if bundle_requirement_error:
        raise SystemExit(bundle_requirement_error)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run buyer-language search quality benchmark")
    parser.add_argument("--base-url", default=DEFAULT_BASE_URL)
    parser.add_argument("--spec", default=DEFAULT_SPEC)
    parser.add_argument("--output", default="tmp/search_quality_benchmark_v1.json")
    parser.add_argument("--markdown-output")
    parser.add_argument("--timeout-seconds", type=int, default=15)
    parser.add_argument(
        "--warmup-runs",
        type=non_negative_int,
        default=0,
        help="discard this many requests per case before measurement",
    )
    parser.add_argument(
        "--repeat-runs",
        type=positive_int,
        default=1,
        help="measure this many requests per case and verify ordered-result stability",
    )
    parser.add_argument("--max-endpoint-p95-ms", type=float)
    return parser.parse_args()


def non_negative_int(value: str) -> int:
    parsed = int(value)
    if parsed < 0:
        raise argparse.ArgumentTypeError("must be at least 0")
    return parsed


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed < 1:
        raise argparse.ArgumentTypeError("must be at least 1")
    return parsed


def call_search(base_url: str, query: str, timeout_seconds: int) -> Optional[Dict[str, Any]]:
    url = f"{base_url}/api/search?q={urllib.parse.quote(query)}"
    try:
        started_at = time.perf_counter()
        with urllib.request.urlopen(url, timeout=timeout_seconds) as response:
            payload = json.loads(response.read())
            payload["_request_duration_ms"] = (time.perf_counter() - started_at) * 1000
            return payload
    except (urllib.error.URLError, json.JSONDecodeError) as err:
        print(f"  ERROR: {err}", file=sys.stderr)
        return None


def call_health(base_url: str, timeout_seconds: int) -> Optional[Dict[str, Any]]:
    try:
        with urllib.request.urlopen(f"{base_url}/api/health", timeout=timeout_seconds) as response:
            payload = json.loads(response.read())
            return payload if isinstance(payload, dict) else None
    except (urllib.error.URLError, json.JSONDecodeError) as err:
        print(f"  ERROR: health check failed: {err}", file=sys.stderr)
        return None


def load_cases(spec: Dict[str, Any], spec_dir: Path) -> Tuple[List[Dict[str, Any]], List[str]]:
    cases: List[Dict[str, Any]] = []
    sources: List[str] = []

    inline_cases = spec.get("cases") or []
    if inline_cases:
        cases.extend(inline_cases)
        sources.append("<inline>")

    for case_file in spec.get("case_files") or []:
        path = Path(str(case_file))
        if not path.is_absolute():
            path = spec_dir / path
        bank = load_json(path)
        bank_cases = bank.get("cases") or []
        if not bank_cases:
            raise SystemExit(f"query bank has no cases: {path}")
        cases.extend(bank_cases)
        sources.append(str(path))

    duplicate_ids = sorted(
        case_id for case_id, count in Counter(case["id"] for case in cases).items() if count > 1
    )
    if duplicate_ids:
        raise SystemExit(f"duplicate benchmark case ids: {', '.join(duplicate_ids)}")

    return cases, sources


def evaluate_case(case: Dict[str, Any], response: Dict[str, Any]) -> List[Dict[str, Any]]:
    expected = case.get("expected") or {}
    intent = response.get("intent") or {}
    checks: List[Dict[str, Any]] = []
    diagnostics = search_diagnostics(response)
    timing_layers = {
        str(item.get("layer"))
        for item in diagnostics.get("layerTimings", diagnostics.get("layer_timings", []))
        if isinstance(item, dict)
    }
    required_timing_layers = {
        "intent_parse",
        "structured_recall",
        "tantivy_recall",
        "ranking",
        "total",
    }
    if timing_layers:
        missing_timing_layers = sorted(required_timing_layers - timing_layers)
        checks.append(
            check(
                "latency",
                "search_diagnostics",
                not missing_timing_layers,
                f"missing timing layers {missing_timing_layers}; got {sorted(timing_layers)}",
            )
        )
    else:
        endpoint_duration = response.get("_request_duration_ms")
        checks.append(
            check(
                "latency",
                "endpoint_timing",
                isinstance(endpoint_duration, (int, float)) and endpoint_duration >= 0,
                f"endpoint request duration was {endpoint_duration!r}",
            )
        )

    recall = diagnostics.get("recall") or {}
    structured_ids = set(recall.get("structuredSample") or recall.get("structured_sample") or [])
    tantivy_ids = set(recall.get("tantivySample") or recall.get("tantivy_sample") or [])
    if "candidate_ids_any" in expected:
        wanted = {str(value) for value in expected["candidate_ids_any"]}
        candidates = structured_ids | tantivy_ids
        checks.append(
            check(
                "recall",
                "candidate_ids_any",
                bool(candidates.intersection(wanted)),
                f"expected one candidate from {sorted(wanted)}, got {sorted(candidates)}",
            )
        )
    if "area" in expected:
        got = intent.get("area")
        checks.append(
            check(
                "intent",
                "area",
                equal_text(got, expected["area"]),
                f"expected area {expected['area']!r}, got {got!r}",
            )
        )
    if "bhk" in expected:
        got = intent.get("bhk")
        checks.append(
            check("intent", "bhk", got == expected["bhk"], f"expected BHK {expected['bhk']}, got {got}")
        )
    if "budget_max" in expected:
        got = intent.get("budget_max") or intent.get("budgetMax")
        checks.append(
            check(
                "intent",
                "budget_max",
                got == expected["budget_max"],
                f"expected budget_max {expected['budget_max']}, got {got}",
            )
        )
    if "buyer_archetype" in expected:
        got = intent.get("buyer_archetype") or intent.get("buyerArchetype")
        checks.append(
            check(
                "intent",
                "buyer_archetype",
                normalize_token(got) == normalize_token(expected["buyer_archetype"]),
                f"expected archetype {expected['buyer_archetype']!r}, got {got!r}",
            )
        )
    if "excluded_areas" in expected:
        got = {normalize_token(value) for value in intent.get("excluded_areas", [])}
        want = {normalize_token(value) for value in expected["excluded_areas"]}
        checks.append(
            check(
                "intent",
                "excluded_areas",
                want.issubset(got),
                f"expected excluded areas {sorted(want)}, got {sorted(got)}",
            )
        )
    if "accepted_tradeoffs" in expected:
        got = normalized_string_values(intent, "accepted_tradeoffs")
        want = {normalize_token(value) for value in expected["accepted_tradeoffs"]}
        checks.append(
            check(
                "intent",
                "accepted_tradeoffs",
                want.issubset(got),
                f"expected accepted tradeoffs {sorted(want)}, got {sorted(got)}",
            )
        )
    if "unsupported_inventory_types" in expected:
        got = normalized_string_values(intent, "unsupported_inventory_types")
        want = {normalize_token(value) for value in expected["unsupported_inventory_types"]}
        checks.append(
            check(
                "intent",
                "unsupported_inventory_types",
                want.issubset(got),
                f"expected unsupported inventory {sorted(want)}, got {sorted(got)}",
            )
        )

    positive_values = preference_values(intent, "positive_preferences")
    negative_values = preference_values(intent, "negative_preferences")
    if "positive_preferences" in expected:
        missing = missing_values(expected["positive_preferences"], positive_values)
        checks.append(
            check(
                "intent",
                "positive_preferences",
                not missing,
                f"missing positive preferences {missing}; got {sorted(positive_values)}",
            )
        )
    if "negative_preferences" in expected:
        missing = missing_values(expected["negative_preferences"], negative_values)
        checks.append(
            check(
                "intent",
                "negative_preferences",
                not missing,
                f"missing negative preferences {missing}; got {sorted(negative_values)}",
            )
        )

    result_sets = public_result_sets(response)
    results = flattened_results(response)
    orderings = response.get("_ordered_result_ids_runs") or []
    if len(orderings) > 1:
        checks.append(
            check(
                "stability",
                "ordered_result_ids",
                all(ordering == orderings[0] for ordering in orderings[1:]),
                f"ordered result ids changed across repeats: {orderings}",
            )
        )
    if "state" in expected:
        got_state = normalize_token(response.get("state"))
        wanted_state = normalize_token(expected["state"])
        checks.append(
            check(
                "result_count",
                "state",
                got_state == wanted_state,
                f"expected state {wanted_state!r}, got {got_state!r}",
            )
        )
    if "total_matches" in expected:
        got_total = response.get("totalMatches", response.get("total_matches", len(results)))
        checks.append(
            check(
                "result_count",
                "total_matches",
                got_total == expected["total_matches"],
                f"expected {expected['total_matches']} total matches, got {got_total}",
            )
        )
    if expected.get("zero_results") is True:
        checks.append(
            check(
                "result_count",
                "zero_results",
                not results,
                f"expected zero results, got {len(results)}",
            )
        )
    if "branch_labels" in expected:
        got_labels = [str(result_set.get("label", "")) for result_set in result_sets]
        wanted_labels = [str(label) for label in expected["branch_labels"]]
        checks.append(
            check(
                "branching",
                "branch_labels",
                got_labels == wanted_labels,
                f"expected branch labels {wanted_labels}, got {got_labels}",
            )
        )
    if "branch_result_ids" in expected:
        got_branch_ids = [
            [str(result.get("id", "")) for result in result_set.get("results") or []]
            for result_set in result_sets
        ]
        wanted_branch_ids = [
            [str(result_id) for result_id in branch]
            for branch in expected["branch_result_ids"]
        ]
        checks.append(
            check(
                "branching",
                "branch_result_ids",
                got_branch_ids == wanted_branch_ids,
                f"expected branch result ids {wanted_branch_ids}, got {got_branch_ids}",
            )
        )
    if "min_results" in expected:
        checks.append(
            check(
                "result_count",
                "min_results",
                len(results) >= expected["min_results"],
                f"expected at least {expected['min_results']} results, got {len(results)}",
            )
        )
    if "top_title_any" in expected:
        top_titles = [str(result.get("title", "")) for result in results[:3]]
        checks.append(
            check(
                "ranking",
                "top_title_any",
                any_title_contains(top_titles, expected["top_title_any"]),
                f"expected one of {expected['top_title_any']} in top 3, got {top_titles}",
            )
        )
    if "result_title_any" in expected:
        titles = [str(result.get("title", "")) for result in results[:10]]
        checks.append(
            check(
                "recall",
                "result_title_any",
                any_title_contains(titles, expected["result_title_any"]),
                f"expected one of {expected['result_title_any']} in top 10, got {titles}",
            )
        )
    if "top_result_ids_any" in expected:
        top_ids = {str(result.get("id", "")) for result in results[:3]}
        wanted_ids = {str(value) for value in expected["top_result_ids_any"]}
        checks.append(
            check(
                "ranking",
                "top_result_ids_any",
                bool(top_ids.intersection(wanted_ids)),
                f"expected one of {sorted(wanted_ids)} in top 3, got {sorted(top_ids)}",
            )
        )
    if "result_ids_any" in expected:
        result_ids = {str(result.get("id", "")) for result in results[:10]}
        wanted_ids = {str(value) for value in expected["result_ids_any"]}
        checks.append(
            check(
                "recall",
                "result_ids_any",
                bool(result_ids.intersection(wanted_ids)),
                f"expected one of {sorted(wanted_ids)} in top 10, got {sorted(result_ids)}",
            )
        )
    if "result_ids_all" in expected:
        result_ids = {str(result.get("id", "")) for result in results}
        wanted_ids = {str(value) for value in expected["result_ids_all"]}
        missing_ids = sorted(wanted_ids - result_ids)
        checks.append(
            check(
                "recall",
                "result_ids_all",
                not missing_ids,
                f"missing expected result ids {missing_ids}; got {sorted(result_ids)}",
            )
        )
    if "forbidden_result_ids" in expected:
        result_ids = {str(result.get("id", "")) for result in results}
        forbidden_ids = {str(value) for value in expected["forbidden_result_ids"]}
        leaked_ids = sorted(result_ids.intersection(forbidden_ids))
        checks.append(
            check(
                "safety",
                "forbidden_result_ids",
                not leaked_ids,
                f"forbidden result ids leaked: {leaked_ids}",
            )
        )
    if "ordered_result_ids_prefix" in expected:
        got_ids = [str(result.get("id", "")) for result in results]
        wanted_ids = [str(value) for value in expected["ordered_result_ids_prefix"]]
        checks.append(
            check(
                "ranking",
                "ordered_result_ids_prefix",
                got_ids[: len(wanted_ids)] == wanted_ids,
                f"expected ordered prefix {wanted_ids}, got {got_ids[:len(wanted_ids)]}",
            )
        )
    if "result_bhks_all" in expected:
        allowed_bhks = {int(value) for value in expected["result_bhks_all"]}
        unexpected = sorted(
            {
                result.get("bhk")
                for result in results
                if result.get("bhk") not in allowed_bhks
            },
            key=lambda value: str(value),
        )
        checks.append(
            check(
                "hard_constraint",
                "result_bhks_all",
                not unexpected,
                f"expected only BHK values {sorted(allowed_bhks)}, got unexpected {unexpected}",
            )
        )
    if "result_price_max" in expected:
        over_budget = [
            {"id": result.get("id"), "price": result.get("price")}
            for result in results
            if not isinstance(result.get("price"), (int, float))
            or result["price"] > expected["result_price_max"]
        ]
        checks.append(
            check(
                "hard_constraint",
                "result_price_max",
                not over_budget,
                f"results above {expected['result_price_max']}: {over_budget}",
            )
        )
    if "result_match_tiers_all" in expected:
        allowed_tiers = {normalize_token(value) for value in expected["result_match_tiers_all"]}
        unexpected_tiers = sorted(
            {
                normalize_token(result.get("match_tier") or result.get("matchTier"))
                for result in results
            }
            - allowed_tiers
        )
        checks.append(
            check(
                "ranking",
                "result_match_tiers_all",
                not unexpected_tiers,
                f"expected tiers {sorted(allowed_tiers)}, got unexpected {unexpected_tiers}",
            )
        )
    if "result_areas_all" in expected:
        allowed_areas = {normalize_token(value) for value in expected["result_areas_all"]}
        actual_areas = {
            normalize_token(result.get("area"))
            for result in results
            if normalize_token(result.get("area"))
        }
        unexpected_areas = sorted(actual_areas - allowed_areas)
        checks.append(
            check(
                "ranking",
                "result_areas_all",
                not unexpected_areas,
                f"expected only areas {sorted(allowed_areas)}, got unexpected {unexpected_areas}",
            )
        )

    reasons = top_reasons(results, limit=3)
    reason_keys = {normalize_fact_key(reason.get("fact_key")) for reason in reasons}
    if "reason_fact_keys_any" in expected:
        wanted = {normalize_fact_key(key) for key in expected["reason_fact_keys_any"]}
        checks.append(
            check(
                "proof",
                "reason_fact_keys_any",
                bool(reason_keys.intersection(wanted)),
                f"expected one proof key from {sorted(wanted)}, got {sorted(reason_keys)}",
            )
        )
    if "reason_fact_keys_all" in expected:
        top_reason_keys = {
            normalize_fact_key(reason.get("fact_key"))
            for reason in top_reasons(results, limit=1)
        }
        wanted = {normalize_fact_key(key) for key in expected["reason_fact_keys_all"]}
        missing = sorted(wanted - top_reason_keys)
        checks.append(
            check(
                "proof",
                "reason_fact_keys_all",
                not missing,
                f"missing proof keys {missing}; top result got {sorted(top_reason_keys)}",
            )
        )
    if "reason_scoring_methods_any" in expected:
        got_methods = {
            normalize_fact_key(reason.get("scoring_method")) for reason in reasons
        }
        wanted_methods = {
            normalize_fact_key(method) for method in expected["reason_scoring_methods_any"]
        }
        checks.append(
            check(
                "proof",
                "reason_scoring_methods_any",
                bool(got_methods.intersection(wanted_methods)),
                f"expected one scoring method from {sorted(wanted_methods)}, got {sorted(got_methods)}",
            )
        )
    if "resolved_place_any" in expected:
        resolved_names = {
            normalize_token(entity.get("name"))
            for entity in (diagnostics.get("resolved") or {}).get("entities", [])
            if isinstance(entity, dict)
            and normalize_token(entity.get("entityType") or entity.get("entity_type")) == "place"
        }
        wanted_places = {normalize_token(name) for name in expected["resolved_place_any"]}
        checks.append(
            check(
                "resolution",
                "resolved_place_any",
                bool(resolved_names.intersection(wanted_places)),
                f"expected one resolved place from {sorted(wanted_places)}, got {sorted(resolved_names)}",
            )
        )
    if "resolved_entity_matches_all" in expected:
        resolved_entities = [
            entity
            for entity in (diagnostics.get("resolved") or {}).get("entities", [])
            if isinstance(entity, dict)
        ]
        missing_matches = [
            requirement
            for requirement in expected["resolved_entity_matches_all"]
            if not any(record_matches(entity, requirement) for entity in resolved_entities)
        ]
        checks.append(
            check(
                "resolution",
                "resolved_entity_matches_all",
                not missing_matches,
                f"missing resolved entity matches {missing_matches}; got {resolved_entities}",
            )
        )
    if "forbidden_resolved_entity_matches" in expected:
        resolved_entities = [
            entity
            for entity in (diagnostics.get("resolved") or {}).get("entities", [])
            if isinstance(entity, dict)
        ]
        leaked_matches = [
            requirement
            for requirement in expected["forbidden_resolved_entity_matches"]
            if any(record_matches(entity, requirement) for entity in resolved_entities)
        ]
        checks.append(
            check(
                "resolution",
                "forbidden_resolved_entity_matches",
                not leaked_matches,
                f"forbidden resolved entity matches leaked {leaked_matches}; got {resolved_entities}",
            )
        )

    proof_focuses = top_proof_focuses(results, limit=3)
    if "proof_focus_any" in expected:
        wanted_focuses = expected["proof_focus_any"]
        checks.append(
            check(
                "proof",
                "proof_focus_any",
                any(
                    record_matches(focus, requirement)
                    for focus in proof_focuses
                    for requirement in wanted_focuses
                ),
                f"expected one proof focus matching {wanted_focuses}, got {proof_focuses}",
            )
        )
    if "proof_focus_matches_all" in expected:
        top_result_focuses = top_proof_focuses(results, limit=1)
        missing_focuses = [
            requirement
            for requirement in expected["proof_focus_matches_all"]
            if not any(record_matches(focus, requirement) for focus in top_result_focuses)
        ]
        checks.append(
            check(
                "proof",
                "proof_focus_matches_all",
                not missing_focuses,
                f"missing proof focus matches {missing_focuses}; top result got {top_result_focuses}",
            )
        )
    if "forbidden_proof_focus_fact_keys" in expected:
        forbidden_focus_keys = {
            normalize_fact_key(key) for key in expected["forbidden_proof_focus_fact_keys"]
        }
        leaked_focus_keys = sorted(
            normalize_fact_key(field_value(focus, "fact_key"))
            for focus in proof_focuses
            if normalize_fact_key(field_value(focus, "fact_key")) in forbidden_focus_keys
        )
        checks.append(
            check(
                "safety",
                "forbidden_proof_focus_fact_keys",
                not leaked_focus_keys,
                f"forbidden proof focus keys leaked: {leaked_focus_keys}",
            )
        )
    if "search_guidance_mode" in expected:
        guidance = search_guidance(response)
        got_mode = normalize_token(guidance.get("mode"))
        wanted_mode = normalize_token(expected["search_guidance_mode"])
        checks.append(
            check(
                "guardrail",
                "search_guidance_mode",
                got_mode == wanted_mode,
                f"expected guidance mode {wanted_mode!r}, got {got_mode!r}",
            )
        )
    if "max_results" in expected:
        checks.append(
            check(
                "guardrail",
                "max_results",
                len(results) <= expected["max_results"],
                f"expected at most {expected['max_results']} results, got {len(results)}",
            )
        )
    if "forbidden_reason_fact_keys" in expected:
        forbidden = {normalize_fact_key(key) for key in expected["forbidden_reason_fact_keys"]}
        leaked = sorted(reason_keys.intersection(forbidden))
        checks.append(
            check(
                "safety",
                "forbidden_reason_fact_keys",
                not leaked,
                f"forbidden proof keys leaked: {leaked}",
            )
        )

    if "gap_keys" in expected:
        gaps = learning_gaps(response)
        evidence_keys = intent_evidence_keys(intent)
        gap_text = "\n".join(gaps).lower()
        missing = [
            key
            for key in expected["gap_keys"]
            if key.lower() not in evidence_keys and key.lower() not in gap_text
        ]
        checks.append(
            check(
                "gap",
                "expected_gap_keys",
                not missing,
                f"missing gap keys {missing}; intent evidence keys were {sorted(evidence_keys)}; gaps were {gaps}",
            )
        )
    if "learning_gap_keys" in expected:
        gaps = learning_gaps(response)
        gap_text = "\n".join(gaps).lower()
        missing = [key for key in expected["learning_gap_keys"] if key.lower() not in gap_text]
        checks.append(
            check(
                "gap",
                "learning_gap_keys",
                not missing,
                f"missing recorded learning gap keys {missing}; gaps were {gaps}",
            )
        )

    return checks


def case_result(
    case: Dict[str, Any],
    response: Optional[Dict[str, Any]],
    checks: List[Dict[str, Any]],
) -> Dict[str, Any]:
    result: Dict[str, Any] = {
        "id": case["id"],
        "mode": case.get("mode", "data_backed"),
        "category": case.get("category"),
        "query": case["query"],
        "expected": case.get("expected") or {},
        "oracle": case.get("oracle") or {},
        "declared_failure_bucket": case.get("failure_bucket"),
        "known_missing_fact_keys": (case.get("expected") or {}).get("gap_keys", []),
        "checks": checks,
        "status": "PASS" if all(item["passed"] for item in checks) else "FAIL",
    }
    result["failure_bucket"] = primary_failure_bucket(result)
    if response is None:
        result.update({"num_results": 0, "intent": None, "top_results": [], "learning_gaps": []})
        return result

    results = flattened_results(response)
    result.update(
        {
            "num_results": len(results),
            "result_sets": result_set_summaries(response),
            "ordered_result_ids": [result.get("id") for result in results],
            "intent": response.get("intent") or {},
            "top_results": result_summaries(results[:5]),
            "learning_gaps": learning_gaps(response),
            "search_diagnostics": search_diagnostics(response),
            "request_duration_ms": response.get("_request_duration_ms"),
            "request_durations_ms": response.get("_request_durations_ms") or [],
        }
    )
    return result


def result_summaries(results: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
    summaries = []
    for result in results:
        explanation = get_explanation(result)
        summaries.append(
            {
                "id": result.get("id"),
                "title": result.get("title"),
                "area": result.get("area"),
                "branch_id": result.get("_benchmark_branch_id"),
                "branch_label": result.get("_benchmark_branch_label"),
                "match_tier": result.get("match_tier") or result.get("matchTier"),
                "tradeoff_label": result.get("tradeoff_label") or result.get("tradeoffLabel"),
                "match_score": result.get("match_score") or result.get("matchScore"),
                "reason_keys": [
                    reason.get("fact_key") for reason in (explanation.get("reasons") or [])
                ],
                "coverage": explanation.get("preference_coverage")
                or explanation.get("preferenceCoverage")
                or [],
                "proof_focuses": result.get("proof_focuses")
                or result.get("proofFocuses")
                or [],
            }
        )
    return summaries


def public_result_sets(response: Dict[str, Any]) -> List[Dict[str, Any]]:
    """Return buyer-visible result sets, with a legacy flat-response adapter."""
    result_sets = response.get("resultSets") or response.get("result_sets") or []
    if isinstance(result_sets, list):
        normalized = [item for item in result_sets if isinstance(item, dict)]
        if normalized:
            return normalized

    legacy_results = response.get("results") or []
    if not isinstance(legacy_results, list) or not legacy_results:
        return []
    return [{"branchId": "legacy", "label": "Results", "results": legacy_results}]


def flattened_results(response: Dict[str, Any]) -> List[Dict[str, Any]]:
    """Flatten public result sets while retaining branch provenance for checks."""
    flattened: List[Dict[str, Any]] = []
    for branch_rank, result_set in enumerate(public_result_sets(response)):
        branch_id = result_set.get("branchId") or result_set.get("branch_id")
        branch_label = result_set.get("label")
        for result_rank, raw_result in enumerate(result_set.get("results") or []):
            if not isinstance(raw_result, dict):
                continue
            result = dict(raw_result)
            result["_benchmark_branch_id"] = branch_id
            result["_benchmark_branch_label"] = branch_label
            result["_benchmark_branch_rank"] = branch_rank
            result["_benchmark_result_rank"] = result_rank
            flattened.append(result)
    return flattened


def result_set_summaries(response: Dict[str, Any]) -> List[Dict[str, Any]]:
    return [
        {
            "branch_id": result_set.get("branchId") or result_set.get("branch_id"),
            "label": result_set.get("label"),
            "result_ids": [
                result.get("id")
                for result in result_set.get("results") or []
                if isinstance(result, dict)
            ],
        }
        for result_set in public_result_sets(response)
    ]


def summarize(results: List[Dict[str, Any]], scoreable_modes: Iterable[str]) -> Dict[str, Any]:
    checks = [item for result in results for item in result["checks"]]
    scoreable_mode_set = {str(mode) for mode in scoreable_modes}
    scoreable_results = [
        result
        for result in results
        if result.get("mode", "data_backed") in scoreable_mode_set
    ]
    scoreable_checks = [item for result in scoreable_results for item in result["checks"]]
    by_layer: Dict[str, Counter] = defaultdict(Counter)
    by_category: Dict[str, Counter] = defaultdict(Counter)
    by_mode: Dict[str, Counter] = defaultdict(Counter)
    failure_buckets: Counter = Counter()
    for result in results:
        category = result.get("category") or "unknown"
        mode = result.get("mode") or "data_backed"
        if result["status"] != "PASS":
            failure_buckets[result.get("failure_bucket") or "architecture_gap"] += 1
        for item in result["checks"]:
            status = "passed" if item["passed"] else "failed"
            by_layer[item["layer"]][status] += 1
            by_category[category][status] += 1
            by_mode[mode][status] += 1

    passed = sum(1 for item in checks if item["passed"])
    total = len(checks)
    scoreable_passed = sum(1 for item in scoreable_checks if item["passed"])
    scoreable_total = len(scoreable_checks)
    summary = {
        "cases": len(results),
        "passed_cases": sum(1 for result in results if result["status"] == "PASS"),
        "failed_cases": sum(1 for result in results if result["status"] != "PASS"),
        "scoreable_cases": len(scoreable_results),
        "scoreable_passed_cases": sum(1 for result in scoreable_results if result["status"] == "PASS"),
        "scoreable_failed_cases": sum(1 for result in scoreable_results if result["status"] != "PASS"),
        "scoreable_passed_checks": scoreable_passed,
        "scoreable_total_checks": scoreable_total,
        "scoreable_pass_rate_pct": round(100 * scoreable_passed / scoreable_total, 1)
        if scoreable_total
        else 0.0,
        "passed_checks": passed,
        "total_checks": total,
        "pass_rate_pct": round(100 * passed / total, 1) if total else 0.0,
        "by_layer": counter_map(by_layer),
        "by_category": counter_map(by_category),
        "by_mode": counter_map(by_mode),
        "failure_buckets": dict(failure_buckets),
        "data_gap_cases": failure_buckets.get("data_gap", 0),
        "latency": latency_summary(results),
        "quality": public_quality_summary(scoreable_results),
    }
    summary["proof_loop_decision"] = proof_loop_decision(summary, results)
    return summary


def public_quality_summary(results: List[Dict[str, Any]]) -> Dict[str, Any]:
    ranked_oracles: List[Tuple[List[str], List[str]]] = []
    for result in results:
        expected = result.get("expected") or {}
        wanted = expected.get("top_result_ids_any") or expected.get("result_ids_all") or []
        if wanted:
            ranked_oracles.append(
                (
                    [str(result_id) for result_id in wanted],
                    [str(result_id) for result_id in result.get("ordered_result_ids") or []],
                )
            )

    recall = {}
    for limit in (1, 3, 5):
        hits = sum(
            1
            for wanted, ordered in ranked_oracles
            if set(wanted).intersection(ordered[:limit])
        )
        recall[f"recall_at_{limit}_pct"] = (
            round(100 * hits / len(ranked_oracles), 1) if ranked_oracles else 0.0
        )

    reciprocal_ranks = []
    for wanted, ordered in ranked_oracles:
        wanted_set = set(wanted)
        rank = next(
            (index for index, result_id in enumerate(ordered, start=1) if result_id in wanted_set),
            None,
        )
        reciprocal_ranks.append(0.0 if rank is None else 1.0 / rank)

    checks = [check for result in results for check in result.get("checks") or []]
    proof_checks = [check for check in checks if check.get("layer") == "proof"]
    unsupported_claim_checks = [
        check
        for check in checks
        if check.get("layer") == "safety"
        and ("reason" in str(check.get("check")) or "proof_focus" in str(check.get("check")))
    ]
    return {
        "oracle_case_count": len(ranked_oracles),
        **recall,
        "mean_reciprocal_rank": round(
            sum(reciprocal_ranks) / len(reciprocal_ranks), 4
        )
        if reciprocal_ranks
        else 0.0,
        "hard_constraint_violation_count": sum(
            1
            for check in checks
            if not check.get("passed")
            and (
                check.get("layer") == "hard_constraint"
                or check.get("check") == "forbidden_result_ids"
            )
        ),
        "proof_precision_pct": round(
            100 * sum(1 for check in proof_checks if check.get("passed")) / len(proof_checks),
            1,
        )
        if proof_checks
        else 0.0,
        "unsupported_claim_count": sum(
            1 for check in unsupported_claim_checks if not check.get("passed")
        ),
        "ordering_stability_failure_count": sum(
            1
            for check in checks
            if check.get("layer") == "stability" and not check.get("passed")
        ),
    }


def markdown_report(output: Dict[str, Any]) -> str:
    summary = output["summary"]
    lines = [
        "# Buyer-Language Search Quality Benchmark",
        "",
        f"Generated: {output['generated_at']}",
        f"Base URL: `{output['base_url']}`",
        "",
        "## Summary",
        "",
        f"- Cases: {summary['cases']} ({summary['passed_cases']} pass, {summary['failed_cases']} fail)",
        "- Scoreable quality checks: "
        f"{summary['scoreable_passed_checks']}/{summary['scoreable_total_checks']} "
        f"({summary['scoreable_pass_rate_pct']}%) across data-backed cases",
        f"- Overall checks including data-gap sentinels: {summary['passed_checks']}/{summary['total_checks']} ({summary['pass_rate_pct']}%)",
        f"- Failure buckets: {summary.get('failure_buckets') or {}}",
        f"- Proof-loop decision: `{summary.get('proof_loop_decision')}`",
        "",
        "### By Layer",
        "",
        "| Layer | Passed | Failed |",
        "|---|---:|---:|",
    ]
    for layer, counts in sorted(summary["by_layer"].items()):
        lines.append(f"| {layer} | {counts.get('passed', 0)} | {counts.get('failed', 0)} |")

    runtime = output.get("search_runtime") or {}
    if runtime:
        lines.extend(
            [
                "",
                "### Runtime",
                "",
                f"- Serving bundle: `{runtime.get('servingBundleVersion') or runtime.get('serving_bundle_version')}`",
            ]
        )
    materialization = output.get("serving_bundle_materialization") or {}
    if materialization:
        lines.extend(
            [
                f"- Runtime materialization id: `{materialization.get('materialization_id')}`",
                f"- Runtime materialization version: `{materialization.get('version')}`",
            ]
        )
    manifest = output.get("runtime_serving_bundle_manifest") or {}
    if manifest:
        lines.extend(
            [
                f"- Runtime entities/facts/search rows: {manifest.get('entity_count')} / "
                f"{manifest.get('fact_count')} / {manifest.get('search_metadata_count')}",
            ]
        )
    local_current = output.get("local_current_serving_bundle_materialization") or {}
    if local_current and local_current.get("version") != materialization.get("version"):
        lines.append(
            f"- Local current pointer: `{local_current.get('version')}` "
            f"(`{local_current.get('materialization_id')}`)"
        )
    for warning in output.get("provenance_warnings") or []:
        lines.append(f"- Provenance warning: {warning}")

    lines.extend(["", "### By Mode", "", "| Mode | Passed | Failed |", "|---|---:|---:|"])
    for mode, counts in sorted(summary["by_mode"].items()):
        lines.append(f"| {mode} | {counts.get('passed', 0)} | {counts.get('failed', 0)} |")

    latency = summary.get("latency") or {}
    if latency:
        lines.extend(
            [
                "",
                "### Latency",
                "",
                f"- Total p50: {latency.get('total_p50_ms')}ms",
                f"- Total p95: {latency.get('total_p95_ms')}ms",
                f"- Endpoint p50: {latency.get('endpoint_p50_ms')}ms",
                f"- Endpoint p95: {latency.get('endpoint_p95_ms')}ms",
                "",
                "| Layer | p50 ms | p95 ms |",
                "|---|---:|---:|",
            ]
        )
        for layer, values in sorted((latency.get("by_layer") or {}).items()):
            lines.append(f"| {layer} | {values.get('p50_ms')} | {values.get('p95_ms')} |")

    quality = summary.get("quality") or {}
    if quality:
        lines.extend(
            [
                "",
                "### Public Outcome Quality",
                "",
                f"- Recall @1 / @3 / @5: {quality.get('recall_at_1_pct')}% / "
                f"{quality.get('recall_at_3_pct')}% / {quality.get('recall_at_5_pct')}%",
                f"- Mean reciprocal rank: {quality.get('mean_reciprocal_rank')}",
                f"- Hard-constraint violations: {quality.get('hard_constraint_violation_count')}",
                f"- Proof precision: {quality.get('proof_precision_pct')}%",
                f"- Unsupported claims: {quality.get('unsupported_claim_count')}",
                f"- Ordering stability failures: {quality.get('ordering_stability_failure_count')}",
            ]
        )

    lines.extend(["", "## Failed Cases", ""])
    failed = [result for result in output["results"] if result["status"] != "PASS"]
    if not failed:
        lines.append("All cases passed.")
    for result in failed:
        lines.append(f"### {result['id']} ({result.get('mode')}, {result.get('category')})")
        lines.append("")
        lines.append(result["query"].replace("\n", " / "))
        lines.append("")
        for item in result["checks"]:
            if not item["passed"]:
                lines.append(f"- `{item['layer']}.{item['check']}`: {item['detail']}")
        if result.get("failure_bucket"):
            lines.append(f"- Failure bucket: `{result['failure_bucket']}`")
        if result.get("top_results"):
            top = result["top_results"][0]
            lines.append(
                f"- Top result: {top.get('title')} (score={top.get('match_score')})"
            )
        if result.get("learning_gaps"):
            lines.append(f"- Gaps: {result['learning_gaps']}")
        if result.get("known_missing_fact_keys"):
            lines.append(f"- Known missing/low-coverage keys: {result['known_missing_fact_keys']}")
        diagnostics = result.get("search_diagnostics") or {}
        total_ms = timing_value_ms(diagnostics, "total")
        if total_ms is not None:
            lines.append(f"- Total latency: {round(total_ms, 2)}ms")
        lines.append("")

    lines.extend(
        [
            "## All Cases",
            "",
            "| Case | Mode | Category | Status | Results | Failed Checks |",
            "|---|---|---|---|---:|---|",
        ]
    )
    for result in output["results"]:
        failed_checks = [
            f"{item['layer']}.{item['check']}" for item in result["checks"] if not item["passed"]
        ]
        lines.append(
            f"| {result['id']} | {result.get('mode')} | {result.get('category')} | {result['status']} | "
            f"{result.get('num_results', 0)} | {', '.join(failed_checks)} |"
        )
    lines.append("")
    return "\n".join(lines)


def preference_values(intent: Dict[str, Any], field: str) -> set[str]:
    prefs = intent.get(field) or intent.get(camelize(field)) or []
    values: set[str] = set()
    for pref in prefs:
        if not isinstance(pref, dict):
            continue
        for key in ("canonicalKey", "canonical_key", "rawText", "raw_text"):
            add_preference_value(values, pref.get(key))
        for key in ("expandedKeys", "expanded_keys"):
            for expanded in pref.get(key) or []:
                add_preference_value(values, expanded)
    return values


def normalized_string_values(intent: Dict[str, Any], field: str) -> set[str]:
    values = intent.get(field) or intent.get(camelize(field)) or []
    return {
        normalize_token(value)
        for value in values
        if isinstance(value, str) and value.strip()
    }


def add_preference_value(values: set[str], value: Any) -> None:
    if not isinstance(value, str) or not value.strip():
        return
    normalized = normalize_token(value)
    values.add(normalized)
    values.update(PREFERENCE_ALIASES.get(normalized, set()))


def missing_values(expected: Iterable[str], actual: set[str]) -> List[str]:
    missing = []
    for value in expected:
        normalized = normalize_token(value)
        acceptable = {normalized}
        acceptable.update(PREFERENCE_ALIASES.get(normalized, set()))
        if not acceptable.intersection(actual):
            missing.append(value)
    return missing


def top_reasons(results: List[Dict[str, Any]], limit: int) -> List[Dict[str, Any]]:
    reasons = []
    for result in results[:limit]:
        explanation = get_explanation(result)
        for reason in explanation.get("reasons") or []:
            if isinstance(reason, dict):
                reasons.append(reason)
    return reasons


def top_proof_focuses(results: List[Dict[str, Any]], limit: int) -> List[Dict[str, Any]]:
    focuses: List[Dict[str, Any]] = []
    for result in results[:limit]:
        for focus in result.get("proof_focuses") or result.get("proofFocuses") or []:
            if isinstance(focus, dict):
                focuses.append(focus)
    return focuses


def record_matches(record: Dict[str, Any], requirement: Dict[str, Any]) -> bool:
    for key, expected in requirement.items():
        actual = field_value(record, key)
        if isinstance(expected, str):
            if normalize_token(actual) != normalize_token(expected):
                return False
        elif actual != expected:
            return False
    return True


def field_value(record: Dict[str, Any], field: str) -> Any:
    if field in record:
        return record[field]
    return record.get(camelize(field))


def get_explanation(result: Dict[str, Any]) -> Dict[str, Any]:
    explanation = result.get("match_explanation") or result.get("matchExplanation") or {}
    return explanation if isinstance(explanation, dict) else {}


def learning_gaps(response: Dict[str, Any]) -> List[str]:
    context = response.get("knowledge_context") or response.get("knowledgeContext") or {}
    gaps = context.get("learning_gaps") or context.get("learningGaps") or []
    return [str(gap) for gap in gaps]


def intent_evidence_keys(intent: Dict[str, Any]) -> set[str]:
    keys: set[str] = set()
    for field in (
        "positive_preferences",
        "positivePreferences",
        "negative_preferences",
        "negativePreferences",
    ):
        for signal in intent.get(field) or []:
            if not isinstance(signal, dict):
                continue
            for key_field in ("expanded_keys", "expandedKeys", "gap_keys", "gapKeys"):
                for value in signal.get(key_field) or []:
                    keys.add(str(value).lower())
    for constraint in intent.get("hard_constraints") or intent.get("hardConstraints") or []:
        if not isinstance(constraint, dict):
            continue
        field = constraint.get("field")
        if field:
            keys.add(str(field).lower())
    return keys


def search_diagnostics(response: Dict[str, Any]) -> Dict[str, Any]:
    diagnostics = response.get("search_diagnostics") or response.get("searchDiagnostics") or {}
    return diagnostics if isinstance(diagnostics, dict) else {}


def search_guidance(response: Dict[str, Any]) -> Dict[str, Any]:
    guidance = response.get("search_guidance") or response.get("searchGuidance") or {}
    return guidance if isinstance(guidance, dict) else {}


def search_runtime_summary(results: List[Dict[str, Any]]) -> Dict[str, Any]:
    for result in results:
        diagnostics = result.get("search_diagnostics") or {}
        runtime = diagnostics.get("runtime")
        if isinstance(runtime, dict):
            return runtime
    return {}


def runtime_serving_bundle_version(runtime: Dict[str, Any]) -> Optional[str]:
    value = runtime.get("servingBundleVersion") or runtime.get("serving_bundle_version")
    return str(value) if isinstance(value, str) and value.strip() else None


def serving_bundle_requirement_error(
    required_bundle_version: Any, runtime_bundle_version: Optional[str]
) -> Optional[str]:
    if not isinstance(required_bundle_version, str) or not required_bundle_version.strip():
        return None
    if runtime_bundle_version == required_bundle_version:
        return None
    return (
        "serving bundle requirement failed: expected "
        f"{required_bundle_version!r}, got {runtime_bundle_version!r}"
    )


def inferred_scoreable_modes(cases: List[Dict[str, Any]]) -> List[str]:
    modes = sorted({str(case.get("mode", "data_backed")) for case in cases})
    non_scoreable = {"data_gap", "search_guardrail"}
    scoreable = [mode for mode in modes if mode not in non_scoreable]
    return scoreable or ["data_backed"]


def materialization_summary(data: Dict[str, Any]) -> Dict[str, Any]:
    return {
        "asset_id": data.get("asset_id"),
        "materialization_id": data.get("materialization_id"),
        "materialization_key": data.get("materialization_key"),
        "version": data.get("version"),
        "run_id": data.get("run_id"),
        "updated_at": data.get("updated_at"),
        "created_at": data.get("created_at"),
        "status": data.get("status"),
        "row_count": data.get("row_count"),
        "parent_materializations": data.get("parent_materializations") or [],
    }


def current_search_bundle_materialization(root: Path) -> Dict[str, Any]:
    current = (
        root
        / "data"
        / "lake"
        / "manifests"
        / "assets"
        / "search_serving_bundle"
        / "partition=global"
        / "current.json"
    )
    if not current.exists():
        return {}
    try:
        data = load_json(current)
    except (OSError, json.JSONDecodeError):
        return {}
    return materialization_summary(data)


def materialization_for_bundle_version(root: Path, bundle_version: Optional[str]) -> Dict[str, Any]:
    if not bundle_version:
        return {}
    materialization_dir = (
        root
        / "data"
        / "lake"
        / "manifests"
        / "assets"
        / "search_serving_bundle"
        / "partition=global"
        / "materializations"
    )
    if not materialization_dir.exists():
        return {}
    for path in sorted(materialization_dir.glob("*.json")):
        try:
            data = load_json(path)
        except (OSError, json.JSONDecodeError):
            continue
        if data.get("version") == bundle_version:
            summary = materialization_summary(data)
            summary["materialization_key"] = str(path.relative_to(root / "data" / "lake"))
            return summary
    return {}


def search_bundle_manifest_summary(root: Path, bundle_version: Optional[str]) -> Dict[str, Any]:
    if not bundle_version:
        return {}
    manifest = root / "data" / "lake" / "serving" / "search_bundle" / f"version={bundle_version}" / "manifest.json"
    if not manifest.exists():
        return {}
    try:
        data = load_json(manifest)
    except (OSError, json.JSONDecodeError):
        return {}
    return {
        "bundle_version": data.get("bundle_version"),
        "path": str(manifest.parent),
        "entity_count": data.get("entity_count"),
        "fact_count": data.get("fact_count"),
        "search_metadata_count": data.get("search_metadata_count"),
        "edge_count": data.get("edge_count"),
    }


def provenance_warnings(
    runtime_bundle_version: Optional[str],
    runtime_materialization: Dict[str, Any],
    local_current_materialization: Dict[str, Any],
) -> List[str]:
    warnings: List[str] = []
    if runtime_bundle_version and not runtime_materialization:
        warnings.append(
            f"live runtime bundle {runtime_bundle_version!r} has no matching local materialization record"
        )
    current_version = local_current_materialization.get("version")
    if runtime_bundle_version and current_version and runtime_bundle_version != current_version:
        warnings.append(
            "live runtime bundle differs from data/lake current pointer; "
            "compare benchmark runs by runtime bundle, not by the repo pointer"
        )
    return warnings


def primary_failure_bucket(result: Dict[str, Any]) -> Optional[str]:
    failed = [item for item in result["checks"] if not item["passed"]]
    if not failed:
        return None
    if result.get("declared_failure_bucket"):
        return str(result["declared_failure_bucket"])
    failed_layers = {str(item.get("layer")) for item in failed}
    if result.get("mode") == "data_gap":
        return "data_gap"

    layer_to_bucket = {
        "request": "architecture_gap",
        "latency": "architecture_gap",
        "intent": "intent_gap",
        "guardrail": "intent_gap",
        "resolution": "resolver_gap",
        "proof": "proof_gap",
        "ranking": "ranking_gap",
        "recall": "data_gap",
        "result_count": "data_gap",
        "gap": "data_gap",
        "safety": "architecture_gap",
    }
    priority = [
        "request",
        "intent",
        "guardrail",
        "resolution",
        "proof",
        "ranking",
        "recall",
        "result_count",
        "gap",
        "latency",
        "safety",
    ]
    for layer in priority:
        if layer in failed_layers:
            return layer_to_bucket[layer]
    return "architecture_gap"


def proof_loop_decision(summary: Dict[str, Any], results: List[Dict[str, Any]]) -> str:
    score = summary["scoreable_pass_rate_pct"] or summary["pass_rate_pct"]
    if score >= 80.0:
        return "keep"
    if summary.get("failure_buckets", {}).get("data_gap", 0) > 0:
        return "needs_more_data"
    return "needs_search_work"


def timing_value_ms(diagnostics: Dict[str, Any], layer: str) -> Optional[float]:
    timings = diagnostics.get("layerTimings") or diagnostics.get("layer_timings") or []
    for item in timings:
        if not isinstance(item, dict):
            continue
        if item.get("layer") != layer:
            continue
        value = item.get("durationMs", item.get("duration_ms"))
        if isinstance(value, (int, float)):
            return float(value)
    return None


def latency_summary(results: List[Dict[str, Any]]) -> Dict[str, Any]:
    totals: List[float] = []
    endpoint_totals: List[float] = []
    by_layer: Dict[str, List[float]] = defaultdict(list)
    for result in results:
        request_durations_ms = result.get("request_durations_ms") or []
        if request_durations_ms:
            endpoint_totals.extend(
                float(duration)
                for duration in request_durations_ms
                if isinstance(duration, (int, float))
            )
        else:
            request_duration_ms = result.get("request_duration_ms")
            if isinstance(request_duration_ms, (int, float)):
                endpoint_totals.append(float(request_duration_ms))
        diagnostics = result.get("search_diagnostics") or {}
        timings = diagnostics.get("layerTimings") or diagnostics.get("layer_timings") or []
        for item in timings:
            if not isinstance(item, dict):
                continue
            layer = item.get("layer")
            value = item.get("durationMs", item.get("duration_ms"))
            if not isinstance(layer, str) or not isinstance(value, (int, float)):
                continue
            value = float(value)
            by_layer[layer].append(value)
            if layer == "total":
                totals.append(value)

    return {
        "total_p50_ms": percentile(totals, 50),
        "total_p95_ms": percentile(totals, 95),
        "endpoint_p50_ms": percentile(endpoint_totals, 50),
        "endpoint_p95_ms": percentile(endpoint_totals, 95),
        "by_layer": {
            layer: {"p50_ms": percentile(values, 50), "p95_ms": percentile(values, 95)}
            for layer, values in sorted(by_layer.items())
        },
    }


def percentile(values: List[float], pct: int) -> Optional[float]:
    if not values:
        return None
    values = sorted(values)
    index = math.ceil((len(values) - 1) * pct / 100)
    return round(values[index], 2)


def check(layer: str, name: str, passed: bool, detail: str) -> Dict[str, Any]:
    return {"layer": layer, "check": name, "passed": bool(passed), "detail": detail}


def any_title_contains(actual_titles: List[str], expected_fragments: List[str]) -> bool:
    lower_titles = [title.lower() for title in actual_titles]
    for fragment in expected_fragments:
        fragment = fragment.lower()
        if any(fragment in title for title in lower_titles):
            return True
    return False


def equal_text(left: Any, right: Any) -> bool:
    return isinstance(left, str) and isinstance(right, str) and left.lower() == right.lower()


def normalize_token(value: Any) -> str:
    if not isinstance(value, str):
        return ""
    normalized = []
    for index, char in enumerate(value.strip()):
        if char in {"-", " ", "."}:
            normalized.append("_")
        elif char.isupper() and index > 0:
            normalized.append("_")
            normalized.append(char.lower())
        else:
            normalized.append(char.lower())
    return "".join(normalized).strip("_")


def normalize_fact_key(value: Any) -> str:
    return str(value or "").strip().lower()


def camelize(field: str) -> str:
    head, *tail = field.split("_")
    return head + "".join(part[:1].upper() + part[1:] for part in tail)


def counter_map(values: Dict[str, Counter]) -> Dict[str, Dict[str, int]]:
    return {key: dict(counter) for key, counter in values.items()}


def load_json(path: Path) -> Dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path: Path, value: Dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2), encoding="utf-8")


def first_line(value: str) -> str:
    return value.splitlines()[0][:96]


if __name__ == "__main__":
    main()
