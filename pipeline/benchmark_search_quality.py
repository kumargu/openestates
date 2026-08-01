"""Run the buyer-language search quality benchmark against a live backend.

The benchmark is intentionally layered. A case can fail intent parsing while
recall still works, or recall can work while proof is missing. Keeping those
signals separate prevents us from treating embeddings as a magic quality score.

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
        response = call_search(args.base_url, case["query"], args.timeout_seconds)
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

        checks = evaluate_case(case, response)
        passed = sum(1 for item in checks if item["passed"])
        print(f"  checks={passed}/{len(checks)} results={len(response.get('results') or [])}")
        results.append(case_result(case, response, checks))

    scoreable_modes = spec.get("scoreable_modes") or ["data_backed"]
    output = {
        "benchmark": spec.get("benchmark"),
        "version": spec.get("version"),
        "generated_at": datetime.utcnow().isoformat(timespec="seconds") + "Z",
        "base_url": args.base_url,
        "scoreable_modes": scoreable_modes,
        "query_sources": query_sources,
        "search_runtime": search_runtime_summary(results),
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


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run buyer-language search quality benchmark")
    parser.add_argument("--base-url", default=DEFAULT_BASE_URL)
    parser.add_argument("--spec", default=DEFAULT_SPEC)
    parser.add_argument("--output", default="tmp/search_quality_benchmark_v1.json")
    parser.add_argument("--markdown-output")
    parser.add_argument("--timeout-seconds", type=int, default=15)
    parser.add_argument("--max-endpoint-p95-ms", type=float)
    return parser.parse_args()


def call_search(base_url: str, query: str, timeout_seconds: int) -> Optional[Dict[str, Any]]:
    url = f"{base_url}/api/search?q={urllib.parse.quote(query)}&debug=true"
    try:
        started_at = time.perf_counter()
        with urllib.request.urlopen(url, timeout=timeout_seconds) as response:
            payload = json.loads(response.read())
            payload["_request_duration_ms"] = (time.perf_counter() - started_at) * 1000
            return payload
    except (urllib.error.URLError, json.JSONDecodeError) as err:
        print(f"  ERROR: {err}", file=sys.stderr)
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
        "semantic_recall",
        "ranking",
        "total",
    }
    missing_timing_layers = sorted(required_timing_layers - timing_layers)
    checks.append(
        check(
            "latency",
            "search_diagnostics",
            not missing_timing_layers,
            f"missing timing layers {missing_timing_layers}; got {sorted(timing_layers)}",
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

    results = response.get("results") or []
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
    if "relaxation_kinds_any" in expected:
        got_kinds = {normalize_token(item.get("kind")) for item in search_relaxations(response)}
        wanted_kinds = {normalize_token(kind) for kind in expected["relaxation_kinds_any"]}
        checks.append(
            check(
                "relaxation",
                "relaxation_kinds_any",
                bool(got_kinds.intersection(wanted_kinds)),
                f"expected one relaxation from {sorted(wanted_kinds)}, got {sorted(got_kinds)}",
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

    semantic_reason_methods = [
        reason.get("scoring_method")
        for reason in reasons
        if "semantic" in str(reason.get("scoring_method", "")).lower()
    ]
    checks.append(
        check(
            "safety",
            "no_semantic_proof_reason",
            not semantic_reason_methods,
            f"semantic scoring methods in proof reasons: {semantic_reason_methods}",
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
        "known_missing_fact_keys": (case.get("expected") or {}).get("gap_keys", []),
        "checks": checks,
        "status": "PASS" if all(item["passed"] for item in checks) else "FAIL",
    }
    if response is None:
        result.update({"num_results": 0, "intent": None, "top_results": [], "learning_gaps": []})
        return result

    results = response.get("results") or []
    result.update(
        {
            "num_results": len(results),
            "intent": response.get("intent") or {},
            "top_results": result_summaries(results[:5]),
            "learning_gaps": learning_gaps(response),
            "search_diagnostics": search_diagnostics(response),
            "request_duration_ms": response.get("_request_duration_ms"),
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
                "match_score": result.get("match_score") or result.get("matchScore"),
                "semantic_score": result.get("semantic_score") or result.get("semanticScore"),
                "reason_keys": [
                    reason.get("fact_key") for reason in (explanation.get("reasons") or [])
                ],
                "coverage": explanation.get("preference_coverage")
                or explanation.get("preferenceCoverage")
                or [],
            }
        )
    return summaries


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
    for result in results:
        category = result.get("category") or "unknown"
        mode = result.get("mode") or "data_backed"
        for item in result["checks"]:
            status = "passed" if item["passed"] else "failed"
            by_layer[item["layer"]][status] += 1
            by_category[category][status] += 1
            by_mode[mode][status] += 1

    passed = sum(1 for item in checks if item["passed"])
    total = len(checks)
    scoreable_passed = sum(1 for item in scoreable_checks if item["passed"])
    scoreable_total = len(scoreable_checks)
    return {
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
        "latency": latency_summary(results),
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
                f"- Semantic embedder: `{runtime.get('semanticEmbedderModelId') or runtime.get('semantic_embedder_model_id')}`",
                f"- Semantic index model: `{runtime.get('semanticIndexModelId') or runtime.get('semantic_index_model_id')}`",
                f"- Semantic index documents: {runtime.get('semanticIndexDocumentCount') or runtime.get('semantic_index_document_count')}",
                f"- Semantic index empty: {runtime.get('semanticIndexEmpty', runtime.get('semantic_index_empty'))}",
            ]
        )

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
        if result.get("top_results"):
            top = result["top_results"][0]
            lines.append(
                f"- Top result: {top.get('title')} "
                f"(score={top.get('match_score')}, semantic={top.get('semantic_score')})"
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


def search_relaxations(response: Dict[str, Any]) -> List[Dict[str, Any]]:
    relaxations = response.get("relaxations") or []
    if not relaxations:
        diagnostics = search_diagnostics(response)
        relaxations = diagnostics.get("relaxations") or []
    return [item for item in relaxations if isinstance(item, dict)]


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
