#!/usr/bin/env python3.11
"""API-level fuzzy search quality checks for OpenEstates.

This is a two-layer contract:
1. Truth layer: search may return internal evidence gaps such as `no_data` in
   structured coverage fields so the DAG knows what to enrich next.
2. UI layer: normal user-facing strings must not expose raw placeholders or
   unsupported claims.

The script intentionally tests the Rust API from the outside. It does not crawl,
enrich, mutate data, or call external services.
"""

from __future__ import annotations

import argparse
import json
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass, field
from typing import Any


DEFAULT_BASE_URL = "http://127.0.0.1:4000"
LATENCY_WARN_MS = 200.0
LATENCY_FAIL_MS = 750.0
FORBIDDEN_UI_TERMS = (
    "unknown",
    "n/a",
    "no data",
    "no_data",
    "missing",
    "gap",
    "not specified",
)
VISIBLE_RESULT_FIELDS = (
    "match_label",
    "match_reason",
    "title",
    "area",
    "society_name",
    "builder_name",
    "description_summary",
    "possession_status",
    "facing",
    "project_status_display",
    "builder_delivery_display",
)
VISIBLE_AREA_CONTEXT_FIELDS = (
    "name",
    "summary",
    "livability_summary",
    "traffic_summary",
    "metro_access_summary",
)


@dataclass(frozen=True)
class SearchCase:
    name: str
    query: str
    expected_area: str | None = None
    expected_bhk: int | None = None
    expected_budget_max: int | None = None
    expected_positive: tuple[str, ...] = ()
    expected_negative: tuple[str, ...] = ()
    expected_tradeoff: tuple[str, ...] = ()
    forbidden_visible_terms: tuple[str, ...] = ()
    watch_area: str | None = None
    watch_bhk: int | None = None
    watch_positive: tuple[str, ...] = ()
    strict_watch: bool = False
    notes: str = ""


@dataclass
class CaseResult:
    case: SearchCase
    passed: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    failures: list[str] = field(default_factory=list)
    latency_ms: float = 0.0
    result_count: int = 0


CASES = (
    SearchCase(
        name="plain budget metro query",
        query="whitefield 3 bed below 2 cr near metro",
        expected_area="Whitefield",
        expected_budget_max=20_000_000,
        expected_positive=("metro",),
        watch_bhk=3,
        notes="'3 bed' is currently a fuzzy parser improvement target.",
    ),
    SearchCase(
        name="typo area query",
        query="whitfield 3bhk under 2cr",
        expected_bhk=3,
        expected_budget_max=20_000_000,
        watch_area="Whitefield",
        notes="Typo correction should become a hard gate once backed by local area aliases.",
    ),
    SearchCase(
        name="family typo query",
        query="sarjapur family frendly 3 bhk",
        expected_area="Sarjapur Road",
        expected_bhk=3,
        watch_positive=("family",),
    ),
    SearchCase(
        name="traffic caution query",
        query="bellandur less traffic",
        expected_area="Bellandur",
        expected_negative=("traffic",),
        forbidden_visible_terms=("low traffic", "less traffic", "traffic-free"),
    ),
    SearchCase(
        name="waterlogging caution query",
        query="avoid waterlogging whitefield",
        expected_area="Whitefield",
        expected_negative=("waterlogging",),
        forbidden_visible_terms=("no waterlogging", "waterlogging-free"),
    ),
    SearchCase(
        name="accepted traffic tradeoff",
        query="ok with traffic if price is good whitefield",
        expected_area="Whitefield",
        expected_tradeoff=("traffic",),
    ),
    SearchCase(
        name="unsupported promise query",
        query="vaastu perfect guaranteed appreciation whitefield",
        expected_area="Whitefield",
        forbidden_visible_terms=("vaastu", "guaranteed", "guaranteed appreciation"),
    ),
    SearchCase(
        name="unsupported inventory type query",
        query="only owner listings whitefield",
        expected_area="Whitefield",
        forbidden_visible_terms=("owner listing", "owner-only", "owner only"),
    ),
)


def main() -> int:
    parser = argparse.ArgumentParser(description="Run fuzzy search quality checks against /api/search.")
    parser.add_argument("--base-url", default=DEFAULT_BASE_URL)
    parser.add_argument(
        "--strict-fuzzy",
        action="store_true",
        help="Treat known fuzzy parser watch items as failures.",
    )
    args = parser.parse_args()

    base_url = args.base_url.rstrip("/")
    results = [run_case(base_url, case, args.strict_fuzzy) for case in CASES]
    print_report(results)
    return 1 if any(result.failures for result in results) else 0


def run_case(base_url: str, case: SearchCase, strict_fuzzy: bool) -> CaseResult:
    result = CaseResult(case=case)
    try:
        payload, latency_ms = fetch_search(base_url, case.query)
    except RuntimeError as exc:
        result.failures.append(str(exc))
        return result

    result.latency_ms = latency_ms
    result.result_count = len(payload.get("results") or [])
    intent = payload.get("intent") or {}

    assert_latency(result, latency_ms)
    assert_basic_shape(result, payload)
    assert_intent(result, intent, case)
    assert_watch_items(result, intent, case, strict_fuzzy)
    assert_visible_copy_is_clean(result, payload)
    assert_no_unsupported_visible_claims(result, payload, case)
    assert_no_contradictory_tradeoff(result, intent, case)
    return result


def fetch_search(base_url: str, query: str) -> tuple[dict[str, Any], float]:
    params = urllib.parse.urlencode({"q": query})
    url = f"{base_url}/api/search?{params}"
    request = urllib.request.Request(url, headers={"Accept": "application/json"})
    started = time.perf_counter()
    try:
        with urllib.request.urlopen(request, timeout=5.0) as response:
            body = response.read()
            status = response.status
    except urllib.error.URLError as exc:
        raise RuntimeError(f"request failed for {query!r}: {exc}") from exc
    latency_ms = (time.perf_counter() - started) * 1000.0

    if status != 200:
        raise RuntimeError(f"expected HTTP 200 for {query!r}, got {status}")
    try:
        payload = json.loads(body)
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"invalid JSON for {query!r}: {exc}") from exc
    if not isinstance(payload, dict):
        raise RuntimeError(f"expected JSON object for {query!r}")
    return payload, latency_ms


def assert_latency(result: CaseResult, latency_ms: float) -> None:
    if latency_ms > LATENCY_FAIL_MS:
        result.failures.append(f"latency {latency_ms:.1f}ms exceeded {LATENCY_FAIL_MS:.0f}ms")
    elif latency_ms > LATENCY_WARN_MS:
        result.warnings.append(f"latency {latency_ms:.1f}ms exceeded {LATENCY_WARN_MS:.0f}ms")
    else:
        result.passed.append(f"latency {latency_ms:.1f}ms")


def assert_basic_shape(result: CaseResult, payload: dict[str, Any]) -> None:
    if payload.get("query") != result.case.query:
        result.failures.append("query echo did not match")
    if not isinstance(payload.get("intent"), dict):
        result.failures.append("intent is missing or not an object")
    if not isinstance(payload.get("results"), list):
        result.failures.append("results is missing or not an array")
    elif payload["results"]:
        first = payload["results"][0]
        required = ("match_score", "match_label", "match_reason", "title", "area")
        missing = [field_name for field_name in required if field_name not in first]
        if missing:
            result.failures.append(f"top result missing fields: {', '.join(missing)}")
        else:
            result.passed.append("result shape")


def assert_intent(result: CaseResult, intent: dict[str, Any], case: SearchCase) -> None:
    if case.expected_area is not None:
        assert_equal(result, "area", intent.get("area"), case.expected_area)
    if case.expected_bhk is not None:
        assert_equal(result, "bhk", intent.get("bhk"), case.expected_bhk)
    if case.expected_budget_max is not None:
        budget = intent.get("budget_max")
        if budget is None or int(budget) > case.expected_budget_max:
            result.failures.append(f"budget_max expected <= {case.expected_budget_max}, got {budget}")
        else:
            result.passed.append("budget intent")
    for expected in case.expected_positive:
        assert_collection_contains(result, "positive preference", intent_preferences(intent, "positive_preferences"), expected)
    for expected in case.expected_negative:
        assert_collection_contains(result, "negative preference", intent_preferences(intent, "negative_preferences"), expected)
    for expected in case.expected_tradeoff:
        assert_collection_contains(result, "accepted tradeoff", intent_list(intent, "accepted_tradeoffs"), expected)


def assert_watch_items(
    result: CaseResult,
    intent: dict[str, Any],
    case: SearchCase,
    strict_fuzzy: bool,
) -> None:
    watch_failures: list[str] = []
    if case.watch_area is not None and intent.get("area") != case.watch_area:
        watch_failures.append(f"watch: area expected {case.watch_area!r}, got {intent.get('area')!r}")
    if case.watch_bhk is not None and intent.get("bhk") != case.watch_bhk:
        watch_failures.append(f"watch: bhk expected {case.watch_bhk}, got {intent.get('bhk')!r}")
    for expected in case.watch_positive:
        values = intent_preferences(intent, "positive_preferences") + intent_list(intent, "preferences")
        if not any(expected in value.lower() for value in values):
            watch_failures.append(f"watch: positive preference should mention {expected!r}")

    if strict_fuzzy or case.strict_watch:
        result.failures.extend(watch_failures)
    else:
        result.warnings.extend(watch_failures)


def assert_visible_copy_is_clean(result: CaseResult, payload: dict[str, Any]) -> None:
    leaks: list[str] = []
    for path, value in visible_strings(payload):
        lower = value.lower()
        for term in FORBIDDEN_UI_TERMS:
            if term in lower:
                leaks.append(f"{path} contains {term!r}: {value!r}")
    if leaks:
        result.failures.extend(leaks)
    else:
        result.passed.append("visible copy has no raw placeholders")


def assert_no_unsupported_visible_claims(
    result: CaseResult,
    payload: dict[str, Any],
    case: SearchCase,
) -> None:
    if not case.forbidden_visible_terms:
        return
    visible = "\n".join(value.lower() for _, value in visible_strings(payload))
    leaked = [term for term in case.forbidden_visible_terms if term.lower() in visible]
    if leaked:
        result.failures.append(f"visible copy made unsupported claim(s): {', '.join(leaked)}")
    else:
        result.passed.append("unsupported query claims were not echoed")


def assert_no_contradictory_tradeoff(
    result: CaseResult,
    intent: dict[str, Any],
    case: SearchCase,
) -> None:
    if "traffic" not in case.expected_tradeoff:
        return
    negatives = intent_preferences(intent, "negative_preferences")
    if any("traffic" in value.lower() for value in negatives):
        result.failures.append("accepted traffic tradeoff was also parsed as an avoid-traffic preference")
    else:
        result.passed.append("accepted tradeoff did not become a negative preference")


def assert_equal(result: CaseResult, label: str, actual: Any, expected: Any) -> None:
    if actual != expected:
        result.failures.append(f"{label} expected {expected!r}, got {actual!r}")
    else:
        result.passed.append(f"{label} intent")


def assert_collection_contains(
    result: CaseResult,
    label: str,
    values: list[str],
    expected_substring: str,
) -> None:
    if any(expected_substring.lower() in value.lower() for value in values):
        result.passed.append(label)
    else:
        result.failures.append(f"{label} expected to contain {expected_substring!r}, got {values!r}")


def intent_preferences(intent: dict[str, Any], key: str) -> list[str]:
    values = intent.get(key)
    if not isinstance(values, list):
        return []
    preferences = []
    for value in values:
        if isinstance(value, str):
            preferences.append(value)
        elif isinstance(value, dict) and isinstance(value.get("raw_text"), str):
            preferences.append(value["raw_text"])
    return preferences


def intent_list(intent: dict[str, Any], key: str) -> list[str]:
    values = intent.get(key)
    if not isinstance(values, list):
        return []
    return [value for value in values if isinstance(value, str)]


def visible_strings(payload: dict[str, Any]) -> list[tuple[str, str]]:
    strings: list[tuple[str, str]] = []
    results = payload.get("results") or []
    if isinstance(results, list):
        for index, item in enumerate(results):
            if isinstance(item, dict):
                strings.extend(strings_for_result(index, item))

    area_context = payload.get("area_context")
    if isinstance(area_context, dict):
        for field_name in VISIBLE_AREA_CONTEXT_FIELDS:
            value = area_context.get(field_name)
            if isinstance(value, str) and value:
                strings.append((f"area_context.{field_name}", value))
    return strings


def strings_for_result(index: int, item: dict[str, Any]) -> list[tuple[str, str]]:
    strings: list[tuple[str, str]] = []
    for field_name in VISIBLE_RESULT_FIELDS:
        value = item.get(field_name)
        if isinstance(value, str) and value:
            strings.append((f"results[{index}].{field_name}", value))
    tags = item.get("transparency_tags")
    if isinstance(tags, list):
        for tag_index, tag in enumerate(tags):
            if isinstance(tag, str) and tag:
                strings.append((f"results[{index}].transparency_tags[{tag_index}]", tag))
    card = item.get("card")
    if isinstance(card, dict):
        for field_name in VISIBLE_RESULT_FIELDS:
            value = card.get(field_name)
            if isinstance(value, str) and value:
                strings.append((f"results[{index}].card.{field_name}", value))
    return strings


def print_report(results: list[CaseResult]) -> None:
    passed = sum(1 for result in results if not result.failures)
    warned = sum(1 for result in results if result.warnings and not result.failures)
    failed = sum(1 for result in results if result.failures)

    print("Fuzzy Search Quality")
    print(f"Base cases: {len(results)}  pass: {passed}  warn: {warned}  fail: {failed}")
    print("")
    for result in results:
        if result.failures:
            status = "FAIL"
        elif result.warnings:
            status = "WARN"
        else:
            status = "PASS"
        print(f"[{status}] {result.case.name}: {result.case.query!r}")
        print(f"  latency={result.latency_ms:.1f}ms results={result.result_count}")
        for failure in result.failures:
            print(f"  fail: {failure}")
        for warning in result.warnings:
            print(f"  warn: {warning}")
        if result.case.notes:
            print(f"  note: {result.case.notes}")
    print("")
    if failed:
        print("Result: FAIL")
    elif warned:
        print("Result: PASS with fuzzy-parser warnings")
    else:
        print("Result: PASS")


if __name__ == "__main__":
    sys.exit(main())
