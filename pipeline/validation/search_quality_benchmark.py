"""
Search quality benchmark for the proof-first OpenEstates engine.

This is an end-to-end product benchmark against the local API. It deliberately
does not share ranking code with the backend. The goal is to pressure the
engine with real buyer queries and verify that results are backed by current,
sourced facts instead of only legacy/seed scoring.

Usage:
    python3 pipeline/validation/search_quality_benchmark.py
    python3 pipeline/validation/search_quality_benchmark.py --base-url http://127.0.0.1:4000
"""

import argparse
import json
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional, Set

ROOT = Path(__file__).resolve().parent.parent.parent
DATA = ROOT / "data"
DEFAULT_CASES = ROOT / "pipeline" / "validation" / "search_quality_cases.json"
DEFAULT_PROOF_SET = ROOT / "pipeline" / "validation" / "product_proof_set.json"
DEFAULT_JSON_OUTPUT = DATA / "validation" / "search_quality_benchmark.json"
DEFAULT_MD_OUTPUT = ROOT / "docs" / "search_quality_benchmark_report.md"

CORE_PROOF_FACTS = {
    "rera_number",
    "rera_status",
    "rera_completion_date",
    "rera_total_land_area_sqm",
}
SUPPORT_FACTS = {
    "market_project_status",
    "market_starting_price_inr",
    "market_bhk_options",
    "google_reviews_url",
    "google_top_positives",
    "google_sentiment",
    "metro_distance_km",
    "nearest_operational_metro_station",
    "official_project_url",
}
SEARCH_P95_GATE_MS = 50.0
DETAIL_P95_GATE_MS = 30.0


class ApiResponse:
    def __init__(self, payload, latency_ms):
        self.payload = payload
        self.latency_ms = latency_ms


def load_json(path: Path) -> Any:
    with open(path) as f:
        return json.load(f)


def fetch_json(base_url: str, path: str, timeout: int = 10) -> ApiResponse:
    url = f"{base_url.rstrip('/')}{path}"
    req = urllib.request.Request(url, headers={"Accept": "application/json"})
    started = time.perf_counter()
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        payload = json.loads(resp.read().decode())
    return ApiResponse(payload, (time.perf_counter() - started) * 1000.0)


def search(base_url: str, query: str) -> ApiResponse:
    encoded = urllib.parse.quote(query, safe="")
    return fetch_json(base_url, f"/api/search?q={encoded}")


def property_detail(base_url: str, property_id: str) -> ApiResponse:
    encoded = urllib.parse.quote(property_id, safe="")
    return fetch_json(base_url, f"/api/properties/{encoded}")


def warm_local_serving_path(base_url: str) -> None:
    """Warm in-memory routing/search code before measuring benchmark latency."""
    for path in (
        "/api/properties",
        "/api/search?q=3bhk%20whitefield",
    ):
        try:
            fetch_json(base_url, path, timeout=5)
        except Exception:
            # The real benchmark checks below will report reachability/shape failures.
            pass


def normalize(value: Any) -> str:
    return str(value or "").strip().lower().replace("_", " ")


def intent_preferences(intent: Dict[str, Any]) -> Set[str]:
    values = {normalize(v) for v in intent.get("preferences", [])}
    for key in ("positive_preferences", "negative_preferences"):
        for pref in intent.get(key, []) or []:
            values.add(normalize(pref.get("raw_text")))
            for expanded in pref.get("expanded_keys", []) or []:
                values.add(normalize(expanded))
    return {v for v in values if v}


def result_id(result: Dict[str, Any]) -> str:
    return result.get("id") or result.get("property", {}).get("id") or ""


def result_title(result: Dict[str, Any]) -> str:
    return result.get("title") or result.get("property", {}).get("title") or result_id(result)


def explanation(result: Dict[str, Any]) -> Dict[str, Any]:
    return result.get("match_explanation") or result.get("explanationCard") or {}


def explanation_reasons(result: Dict[str, Any]) -> List[Dict[str, Any]]:
    return explanation(result).get("reasons", []) or []


def explanation_coverage(result: Dict[str, Any]) -> List[Dict[str, Any]]:
    return explanation(result).get("preference_coverage", []) or []


def graph_driven_pct(result: Dict[str, Any]) -> float:
    value = explanation(result).get("graph_driven_pct")
    if value is None:
        return 0.0
    return float(value)


def flatten_source_items(detail: Dict[str, Any]) -> List[Dict[str, Any]]:
    items = []
    for panel in detail.get("source_panels", []) or []:
        for item in panel.get("items", []) or []:
            enriched = dict(item)
            enriched["panel_kind"] = panel.get("kind")
            enriched["panel_title"] = panel.get("title")
            items.append(enriched)
    return items


def source_item_keys(detail: Dict[str, Any]) -> Set[str]:
    return {item.get("key") for item in flatten_source_items(detail) if item.get("key")}


def source_item_sources(detail: Dict[str, Any]) -> Set[str]:
    return {item.get("source_type") for item in flatten_source_items(detail) if item.get("source_type")}


def source_item_value(detail: Dict[str, Any], key: str) -> Optional[str]:
    for item in flatten_source_items(detail):
        if item.get("key") == key:
            return item.get("value")
    return None


def has_reason(result: Dict[str, Any], expected: Dict[str, Any]) -> bool:
    for reason in explanation_reasons(result):
        if expected.get("fact_key") and reason.get("fact_key") != expected["fact_key"]:
            continue
        if expected.get("source_type") and reason.get("source_type") != expected["source_type"]:
            continue
        if expected.get("scoring_method") and reason.get("scoring_method") != expected["scoring_method"]:
            continue
        if expected.get("preference") and normalize(reason.get("preference")) != normalize(expected["preference"]):
            continue
        return True
    return False


def has_coverage(result: Dict[str, Any], expected: Dict[str, Any]) -> bool:
    for coverage in explanation_coverage(result):
        if expected.get("preference") and normalize(coverage.get("preference")) != normalize(expected["preference"]):
            continue
        if expected.get("status") and coverage.get("status") != expected["status"]:
            continue
        if expected.get("fact_key") and coverage.get("fact_key") != expected["fact_key"]:
            continue
        return True
    return False


def add_check(checks: List[Dict[str, Any]], status: str, name: str, message: str, details: Optional[Dict[str, Any]] = None) -> None:
    checks.append({
        "status": status,
        "check": name,
        "message": message,
        "details": details or {},
    })


def worst_status(checks: List[Dict[str, Any]]) -> str:
    statuses = {check["status"] for check in checks}
    if "FAIL" in statuses:
        return "FAIL"
    if "WARN" in statuses:
        return "WARN"
    return "PASS"


def check_expected_intent(case: Dict[str, Any], intent: Dict[str, Any], checks: List[Dict[str, Any]]) -> None:
    expected = case.get("expected_intent", {})
    if not expected:
        return

    if "area" in expected:
        actual = intent.get("area")
        status = "PASS" if normalize(actual) == normalize(expected["area"]) else "FAIL"
        add_check(checks, status, "intent_area", f"expected area {expected['area']}, got {actual}")

    if "bhk" in expected:
        actual = intent.get("bhk")
        status = "PASS" if actual == expected["bhk"] else "FAIL"
        add_check(checks, status, "intent_bhk", f"expected BHK {expected['bhk']}, got {actual}")

    if "budget_max" in expected:
        actual = intent.get("budget_max")
        status = "PASS" if actual == expected["budget_max"] else "FAIL"
        add_check(checks, status, "intent_budget", f"expected budget {expected['budget_max']}, got {actual}")

    prefs = intent_preferences(intent)
    for pref in expected.get("preferences", []):
        wanted = normalize(pref)
        matched = wanted in prefs or any(wanted in candidate or candidate in wanted for candidate in prefs)
        add_check(
            checks,
            "PASS" if matched else "WARN",
            "intent_preference",
            f"expected preference {pref}",
            {"actual_preferences": sorted(prefs)},
        )

    neg_prefs = {
        normalize(pref.get("raw_text"))
        for pref in intent.get("negative_preferences", []) or []
        if pref.get("raw_text")
    }
    for pref in expected.get("negative_preferences", []):
        wanted = normalize(pref)
        matched = wanted in neg_prefs or any(wanted in candidate or candidate in wanted for candidate in neg_prefs)
        add_check(
            checks,
            "PASS" if matched else "WARN",
            "intent_negative_preference",
            f"expected negative preference {pref}",
            {"actual_negative_preferences": sorted(neg_prefs)},
        )


def check_query_case(base_url: str, case: Dict[str, Any]) -> Dict[str, Any]:
    checks = []
    response = search(base_url, case["query"])
    payload = response.payload
    results = payload.get("results", []) or []
    top_n = int(case.get("top_n", 5))
    top_results = results[:top_n]
    top_ids = [result_id(result) for result in top_results]

    check_expected_intent(case, payload.get("intent", {}), checks)

    min_results = int(case.get("min_results", 1))
    add_check(
        checks,
        "PASS" if len(results) >= min_results else "FAIL",
        "result_count",
        f"expected at least {min_results} results, got {len(results)}",
    )

    expected_top = case.get("expected_top_any", [])
    if expected_top:
        found = [property_id for property_id in expected_top if property_id in top_ids]
        add_check(
            checks,
            "PASS" if found else "FAIL",
            "expected_top_any",
            f"expected one target in top {top_n}",
            {"expected": expected_top, "actual_top": top_ids, "found": found},
        )

    top = results[0] if results else {}
    if top and "min_graph_driven_pct" in case:
        actual = graph_driven_pct(top)
        expected = float(case["min_graph_driven_pct"])
        add_check(
            checks,
            "PASS" if actual >= expected else "FAIL",
            "top_graph_driven_pct",
            f"expected top graph-driven pct >= {expected:.0f}, got {actual:.0f}",
        )

    for expected in case.get("required_reasons", []) or []:
        add_check(
            checks,
            "PASS" if top and has_reason(top, expected) else "FAIL",
            "required_reason",
            f"top result should include reason {expected}",
            {"top_id": result_id(top), "reasons": explanation_reasons(top)},
        )

    any_reasons = case.get("required_reasons_any", []) or []
    if any_reasons:
        matched = [expected for expected in any_reasons if top and has_reason(top, expected)]
        add_check(
            checks,
            "PASS" if matched else ("WARN" if case.get("allow_gap") else "FAIL"),
            "required_reason_any",
            "top result should include at least one expected reason",
            {"expected_any": any_reasons, "matched": matched, "top_reasons": explanation_reasons(top)},
        )

    any_coverage = case.get("required_coverage_any", []) or []
    if any_coverage:
        matched = [expected for expected in any_coverage if top and has_coverage(top, expected)]
        add_check(
            checks,
            "PASS" if matched else ("WARN" if case.get("allow_gap") else "FAIL"),
            "required_coverage_any",
            "top result should cover at least one expected preference",
            {"expected_any": any_coverage, "matched": matched, "top_coverage": explanation_coverage(top)},
        )

    if "max_legacy_reasons_top3" in case:
        legacy_reasons = []
        for result in results[:3]:
            for reason in explanation_reasons(result):
                if normalize(reason.get("scoring_method")) == "legacy":
                    legacy_reasons.append({
                        "id": result_id(result),
                        "fact_key": reason.get("fact_key"),
                        "preference": reason.get("preference"),
                    })
        maximum = int(case["max_legacy_reasons_top3"])
        add_check(
            checks,
            "PASS" if len(legacy_reasons) <= maximum else "FAIL",
            "legacy_reason_budget",
            f"expected <= {maximum} legacy reasons in top 3, got {len(legacy_reasons)}",
            {"legacy_reasons": legacy_reasons},
        )

    detail_payload = None
    detail_latency_ms = None
    detail_id = result_id(top) if top else None
    if detail_id and (case.get("required_detail_fact_keys") or case.get("forbid_status_conflict")):
        detail_response = property_detail(base_url, detail_id)
        detail_payload = detail_response.payload
        detail_latency_ms = detail_response.latency_ms
        keys = source_item_keys(detail_payload)
        for key in case.get("required_detail_fact_keys", []) or []:
            add_check(
                checks,
                "PASS" if key in keys else "FAIL",
                "detail_fact_key",
                f"detail should expose source-backed fact {key}",
                {"detail_id": detail_id, "available_keys": sorted(keys)},
            )
        if case.get("forbid_status_conflict"):
            status_display = detail_payload.get("project_status_display")
            inventory_status = source_item_value(detail_payload, "market_project_status")
            conflict = (
                inventory_status
                and "sold out" in inventory_status.lower()
                and status_display
                and "under construction" in status_display.lower()
                and "sold out" not in status_display.lower()
            )
            add_check(
                checks,
                "FAIL" if conflict else "PASS",
                "detail_status_consistency",
                "builder inventory status should not be collapsed into construction status",
                {"status_display": status_display, "market_project_status": inventory_status},
            )

    top_summary = [
        {
            "id": result_id(result),
            "title": result_title(result),
            "area": result.get("area"),
            "match_score": result.get("match_score"),
            "match_reason": result.get("match_reason"),
            "graph_driven_pct": graph_driven_pct(result),
            "reason_sources": sorted({reason.get("source_type") for reason in explanation_reasons(result) if reason.get("source_type")}),
            "reason_methods": sorted({reason.get("scoring_method") for reason in explanation_reasons(result) if reason.get("scoring_method")}),
        }
        for result in top_results
    ]

    return {
        "id": case["id"],
        "query": case["query"],
        "category": case.get("category"),
        "status": worst_status(checks),
        "latency_ms": round(response.latency_ms, 2),
        "detail_latency_ms": round(detail_latency_ms, 2) if detail_latency_ms is not None else None,
        "total_results": len(results),
        "top": top_summary,
        "knowledge_gaps": (payload.get("knowledge_context") or {}).get("learning_gaps", []),
        "checks": checks,
    }


def check_product_proof_case(base_url: str, case: Dict[str, Any]) -> Dict[str, Any]:
    checks = []
    response = property_detail(base_url, case["id"])
    detail = response.payload
    property_info = detail.get("property") or {}
    keys = source_item_keys(detail)
    sources = source_item_sources(detail)
    source_items = flatten_source_items(detail)

    if case.get("expected_area"):
        actual = property_info.get("area") or detail.get("area", {}).get("name")
        add_check(
            checks,
            "PASS" if normalize(actual) == normalize(case["expected_area"]) else "FAIL",
            "detail_area",
            f"expected area {case['expected_area']}, got {actual}",
        )

    if case.get("expected_bhk"):
        actual = property_info.get("bhk")
        add_check(
            checks,
            "PASS" if actual == case["expected_bhk"] else "FAIL",
            "detail_bhk",
            f"expected BHK {case['expected_bhk']}, got {actual}",
        )

    core_hits = sorted(keys & CORE_PROOF_FACTS)
    support_hits = sorted(keys & SUPPORT_FACTS)
    add_check(
        checks,
        "PASS" if len(core_hits) >= 3 else "FAIL",
        "core_proof_coverage",
        f"expected at least 3 core RERA proof facts, got {len(core_hits)}",
        {"core_hits": core_hits},
    )
    add_check(
        checks,
        "PASS" if len(support_hits) >= 4 else "WARN",
        "support_fact_coverage",
        f"expected at least 4 support facts, got {len(support_hits)}",
        {"support_hits": support_hits},
    )
    add_check(
        checks,
        "PASS" if len(source_items) >= 15 else "WARN",
        "source_item_depth",
        f"expected at least 15 visible source items, got {len(source_items)}",
    )
    add_check(
        checks,
        "PASS" if "Rera" in sources else "FAIL",
        "rera_source_present",
        "detail should expose RERA as source",
        {"sources": sorted(sources)},
    )
    add_check(
        checks,
        "PASS" if len(sources - {"Rera", "Seed", "Manual"}) >= 2 else "WARN",
        "support_source_diversity",
        "detail should expose at least two non-RERA support source families",
        {"sources": sorted(sources)},
    )

    status_display = detail.get("project_status_display")
    inventory_status = source_item_value(detail, "market_project_status")
    conflict = (
        inventory_status
        and "sold out" in inventory_status.lower()
        and status_display
        and "under construction" in status_display.lower()
        and "sold out" not in status_display.lower()
    )
    add_check(
        checks,
        "FAIL" if conflict else "PASS",
        "status_consistency",
        "top-level status should not contradict builder inventory status",
        {"status_display": status_display, "market_project_status": inventory_status},
    )

    proof_query = f"{case.get('expected_bhk', '')}bhk {case['label']}".strip()
    search_response = search(base_url, proof_query)
    search_results = search_response.payload.get("results", []) or []
    top5_ids = [result_id(result) for result in search_results[:5]]
    add_check(
        checks,
        "PASS" if case["id"] in top5_ids else "WARN",
        "search_recall_by_name",
        f"property should appear in top 5 for {proof_query}",
        {"top5": top5_ids},
    )

    return {
        "id": case["id"],
        "label": case["label"],
        "segment": case.get("segment"),
        "status": worst_status(checks),
        "detail_latency_ms": round(response.latency_ms, 2),
        "search_latency_ms": round(search_response.latency_ms, 2),
        "source_item_count": len(source_items),
        "source_keys": sorted(keys),
        "source_types": sorted(sources),
        "checks": checks,
    }


def percentile(values: List[float], pct: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = int(round((len(ordered) - 1) * pct))
    return ordered[index]


def summarize(results: List[Dict[str, Any]]) -> Dict[str, Any]:
    statuses = {status: sum(1 for result in results if result["status"] == status) for status in ("PASS", "WARN", "FAIL")}
    checks = [check for result in results for check in result.get("checks", [])]
    check_statuses = {status: sum(1 for check in checks if check["status"] == status) for status in ("PASS", "WARN", "FAIL")}
    return {
        "statuses": statuses,
        "checks": check_statuses,
        "gate": "FAIL" if statuses["FAIL"] else ("WARN" if statuses["WARN"] else "PASS"),
    }


def latency_summary(search_latencies: List[float], detail_latencies: List[float]) -> Dict[str, Any]:
    search_p95 = round(percentile(search_latencies, 0.95), 2)
    detail_p95 = round(percentile(detail_latencies, 0.95), 2)
    checks = [
        {
            "status": "PASS" if search_p95 <= SEARCH_P95_GATE_MS else "WARN",
            "check": "search_p95",
            "message": f"search p95 should be <= {SEARCH_P95_GATE_MS:.0f} ms, got {search_p95:.2f} ms",
        },
        {
            "status": "PASS" if detail_p95 <= DETAIL_P95_GATE_MS else "WARN",
            "check": "detail_p95",
            "message": f"detail p95 should be <= {DETAIL_P95_GATE_MS:.0f} ms, got {detail_p95:.2f} ms",
        },
    ]
    return {
        "search_p50_ms": round(percentile(search_latencies, 0.50), 2),
        "search_p95_ms": search_p95,
        "search_max_ms": round(max(search_latencies) if search_latencies else 0.0, 2),
        "detail_p50_ms": round(percentile(detail_latencies, 0.50), 2),
        "detail_p95_ms": detail_p95,
        "detail_max_ms": round(max(detail_latencies) if detail_latencies else 0.0, 2),
        "gate": "WARN" if any(check["status"] == "WARN" for check in checks) else "PASS",
        "checks": checks,
    }


def build_markdown(report: Dict[str, Any]) -> str:
    lines = []
    lines.append("# Search Quality Benchmark")
    lines.append("")
    lines.append(f"Generated: {report['generated_at']}")
    lines.append(f"Backend: `{report['base_url']}`")
    lines.append("")

    product = report["product_proof"]
    search_summary = report["search_quality"]
    latency = report["latency"]

    lines.append("## Summary")
    lines.append("")
    lines.append("| Surface | Gate | PASS | WARN | FAIL |")
    lines.append("|---|---:|---:|---:|---:|")
    for name, summary in (
        ("10-society product proof", product["summary"]),
        ("Search quality cases", search_summary["summary"]),
    ):
        statuses = summary["statuses"]
        lines.append(f"| {name} | {summary['gate']} | {statuses['PASS']} | {statuses['WARN']} | {statuses['FAIL']} |")
    lines.append("")
    lines.append("| Latency | p50 | p95 | max |")
    lines.append("|---|---:|---:|---:|")
    lines.append(f"| Search | {latency['search_p50_ms']:.2f} ms | {latency['search_p95_ms']:.2f} ms | {latency['search_max_ms']:.2f} ms |")
    lines.append(f"| Detail | {latency['detail_p50_ms']:.2f} ms | {latency['detail_p95_ms']:.2f} ms | {latency['detail_max_ms']:.2f} ms |")
    lines.append("")
    lines.append(f"Latency gate: **{latency['gate']}**")
    lines.append("")

    lines.append("## Product Proof")
    lines.append("")
    lines.append("| Society | Segment | Status | Source items | Sources | Key failures |")
    lines.append("|---|---|---:|---:|---|---|")
    for result in product["results"]:
        failures = [check["check"] for check in result["checks"] if check["status"] == "FAIL"]
        sources = ", ".join(result.get("source_types", [])[:5])
        lines.append(
            f"| {result['label']} | {result.get('segment', '')} | {result['status']} | "
            f"{result['source_item_count']} | {sources} | {', '.join(failures) or '-'} |"
        )
    lines.append("")

    lines.append("## Search Cases")
    lines.append("")
    lines.append("| Case | Query | Status | Top result | Graph % | Key failures |")
    lines.append("|---|---|---:|---|---:|---|")
    for result in search_summary["results"]:
        top = result["top"][0] if result["top"] else {}
        failures = [check["check"] for check in result["checks"] if check["status"] == "FAIL"]
        lines.append(
            f"| {result['id']} | `{result['query']}` | {result['status']} | "
            f"{top.get('title', '-')} | {top.get('graph_driven_pct', 0):.0f} | {', '.join(failures) or '-'} |"
        )
    lines.append("")

    lines.append("## Failure Details")
    lines.append("")
    for result in product["results"] + search_summary["results"]:
        failed = [check for check in result["checks"] if check["status"] == "FAIL"]
        if not failed:
            continue
        lines.append(f"### {result.get('label') or result['id']}")
        lines.append("")
        for check in failed:
            lines.append(f"- `{check['check']}`: {check['message']}")
        lines.append("")

    lines.append("## Product Reading")
    lines.append("")
    lines.append("- PASS means the current local product can explain the result with sourced evidence.")
    lines.append("- WARN means the user-facing path works but data is sparse or support evidence is weak.")
    lines.append("- FAIL means the current engine is likely misleading, stale, or too legacy-driven for that query.")
    lines.append("")
    return "\n".join(lines)


def run(args: argparse.Namespace) -> int:
    base_url = args.base_url.rstrip("/")
    try:
        fetch_json(base_url, "/api/health", timeout=5)
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as err:
        print(f"FATAL: backend is not reachable at {base_url}: {err}", file=sys.stderr)
        return 1
    warm_local_serving_path(base_url)

    cases = load_json(args.cases)
    proof_cases = load_json(args.proof_set)

    product_results = [check_product_proof_case(base_url, case) for case in proof_cases]
    search_results = [check_query_case(base_url, case) for case in cases]

    search_latencies = [result["latency_ms"] for result in search_results]
    detail_latencies = [
        result["detail_latency_ms"]
        for result in product_results
        if result.get("detail_latency_ms") is not None
    ] + [
        result["detail_latency_ms"]
        for result in search_results
        if result.get("detail_latency_ms") is not None
    ]

    latency = latency_summary(search_latencies, detail_latencies)

    report = {
        "benchmark": "search_quality_v2",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "base_url": base_url,
        "product_proof": {
            "summary": summarize(product_results),
            "results": product_results,
        },
        "search_quality": {
            "summary": summarize(search_results),
            "results": search_results,
        },
        "latency": latency,
    }

    args.json_output.parent.mkdir(parents=True, exist_ok=True)
    with open(args.json_output, "w") as f:
        json.dump(report, f, indent=2)

    args.markdown_output.parent.mkdir(parents=True, exist_ok=True)
    with open(args.markdown_output, "w") as f:
        f.write(build_markdown(report))

    product_gate = report["product_proof"]["summary"]["gate"]
    search_gate = report["search_quality"]["summary"]["gate"]
    print(f"Product proof gate: {product_gate}")
    print(f"Search quality gate: {search_gate}")
    print(f"Search p95: {report['latency']['search_p95_ms']:.2f} ms")
    print(f"Detail p95: {report['latency']['detail_p95_ms']:.2f} ms")
    print(f"JSON report: {args.json_output.relative_to(ROOT)}")
    print(f"Markdown report: {args.markdown_output.relative_to(ROOT)}")

    if args.fail_on_gate and (product_gate == "FAIL" or search_gate == "FAIL"):
        return 1
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Run the OpenEstates search quality benchmark")
    parser.add_argument("--base-url", default="http://127.0.0.1:4000")
    parser.add_argument("--cases", type=Path, default=DEFAULT_CASES)
    parser.add_argument("--proof-set", type=Path, default=DEFAULT_PROOF_SET)
    parser.add_argument("--json-output", type=Path, default=DEFAULT_JSON_OUTPUT)
    parser.add_argument("--markdown-output", type=Path, default=DEFAULT_MD_OUTPUT)
    parser.add_argument("--fail-on-gate", action="store_true")
    return run(parser.parse_args())


if __name__ == "__main__":
    sys.exit(main())
