#!/usr/bin/env python3
"""Capture and compare property API richness across catalog releases.

This is intentionally API-level: the DAG can change internals, but a candidate
release should keep property detail, evidence, RERA, map/surface, search, and
recommendation payloads at least as rich as the baseline release.
"""

from __future__ import annotations

import argparse
import json
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any


DEFAULT_PROPERTY_IDS = [
    "discovered-prestige-waterford-3bhk",
    "discovered-prestige-falcon-city-3bhk",
    "discovered-prestige-southern-star-3bhk",
]

DEFAULT_SEARCH_QUERIES = [
    "Prestige Waterford 3BHK",
    "Prestige Falcon City",
    "3BHK near metro with proof",
]


@dataclass(frozen=True)
class ApiTarget:
    base_url: str

    def url(self, path: str, query: dict[str, str] | None = None) -> str:
        url = self.base_url.rstrip("/") + path
        if query:
            url += "?" + urllib.parse.urlencode(query)
        return url


def main() -> int:
    args = parse_args()
    if args.command == "capture":
        report = capture_release(
            ApiTarget(args.base_url),
            args.property_id or DEFAULT_PROPERTY_IDS,
            args.query or DEFAULT_SEARCH_QUERIES,
        )
        write_json(args.output, report)
        print(json.dumps(summary(report), indent=2))
        return 0

    baseline = read_json(args.baseline)
    candidate = capture_release(
        ApiTarget(args.base_url),
        args.property_id or list(baseline["properties"].keys()),
        args.query or [entry["query"] for entry in baseline["searches"]],
    )
    comparison = compare_reports(baseline, candidate, args.min_ratio)
    write_json(args.output, comparison)
    print(json.dumps(comparison["summary"], indent=2))
    return 0 if comparison["summary"]["status"] == "passed" else 1


def capture_release(
    target: ApiTarget, property_ids: list[str], queries: list[str]
) -> dict[str, Any]:
    properties: dict[str, Any] = {}
    for property_id in property_ids:
        detail = get_json(target.url(f"/api/properties/{property_id}"))
        evidence = get_json(target.url(f"/api/properties/{property_id}/evidence"))
        rera = get_json(target.url(f"/api/properties/{property_id}/rera"))
        recommendations = get_json(
            target.url(f"/api/properties/{property_id}/recommendations")
        )
        surfaces = get_json(target.url(f"/api/properties/{property_id}/surfaces"))
        properties[property_id] = {
            "metrics": property_metrics(detail, evidence, rera, recommendations, surfaces),
            "samples": {
                "title": detail.get("title"),
                "society": detail.get("society", {}).get("name")
                if isinstance(detail.get("society"), dict)
                else None,
                "evidence_sections": [
                    section.get("title")
                    for section in evidence.get("sections", [])
                    if isinstance(section, dict)
                ],
            },
        }

    searches = []
    for query in queries:
        response = get_json(target.url("/api/search", {"q": query}))
        searches.append(
            {
                "query": query,
                "metrics": search_metrics(response),
                "top_ids": [
                    result.get("card", {}).get("id")
                    for result in response.get("results", [])[:5]
                    if isinstance(result, dict)
                ],
            }
        )

    return {
        "captured_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "base_url": target.base_url,
        "properties": properties,
        "searches": searches,
    }


def property_metrics(
    detail: dict[str, Any],
    evidence: dict[str, Any],
    rera: dict[str, Any],
    recommendations: dict[str, Any],
    surfaces: dict[str, Any],
) -> dict[str, int]:
    evidence_sections = list_values(evidence.get("sections"))
    evidence_items = [
        item
        for section in evidence_sections
        for item in list_values(section.get("items"))
        if isinstance(item, dict)
    ]
    source_count = sum(
        len(list_values(item.get("sources")))
        + int(bool(item.get("source_url")))
        + int(bool(item.get("source_type")))
        for item in evidence_items
    )
    rera_sections = list_values(rera.get("fact_sections")) or list_values(rera.get("sections"))
    rera_facts = [
        fact
        for section in rera_sections
        for fact in list_values(section.get("facts"))
        if isinstance(fact, dict)
    ]
    surface_items = list_values(surfaces.get("surfaces")) or list_values(surfaces.get("items"))
    recommendation_items = list_values(recommendations.get("items")) or list_values(
        recommendations.get("recommendations")
    )
    images = list_values(detail.get("images"))
    hero_images = 1 if detail.get("hero_image") else 0
    similar = list_values(detail.get("similar_properties"))

    return {
        "evidence_sections": len(evidence_sections),
        "evidence_items": len(evidence_items),
        "evidence_sources": source_count,
        "rera_sections": len(rera_sections),
        "rera_facts": len(rera_facts),
        "surface_count": len(surface_items),
        "recommendations": len(recommendation_items),
        "similar_properties": len(similar),
        "images": len(images) + hero_images,
        "detail_keys": len(detail.keys()),
    }


def search_metrics(response: dict[str, Any]) -> dict[str, int]:
    results = list_values(response.get("results"))
    proof_keys = set()
    reasons = 0
    for result in results:
        if not isinstance(result, dict):
            continue
        if result.get("match_reason"):
            reasons += 1
        for claim in list_values(result.get("sourced_claims")):
            if isinstance(claim, dict) and claim.get("fact_key"):
                proof_keys.add(str(claim["fact_key"]))
    return {
        "results": len(results),
        "match_reasons": reasons,
        "proof_keys": len(proof_keys),
    }


def compare_reports(
    baseline: dict[str, Any], candidate: dict[str, Any], min_ratio: float
) -> dict[str, Any]:
    failures: list[dict[str, Any]] = []
    property_reports: dict[str, Any] = {}
    for property_id, baseline_entry in baseline["properties"].items():
        candidate_entry = candidate["properties"].get(property_id)
        if candidate_entry is None:
            failures.append({"property_id": property_id, "reason": "missing property"})
            continue
        checks = compare_metric_group(
            baseline_entry["metrics"],
            candidate_entry["metrics"],
            min_ratio,
        )
        property_reports[property_id] = checks
        failures.extend(
            {"property_id": property_id, **check}
            for check in checks
            if check["status"] == "failed"
        )

    search_reports = []
    for index, baseline_search in enumerate(baseline["searches"]):
        candidate_search = candidate["searches"][index]
        checks = compare_metric_group(
            baseline_search["metrics"], candidate_search["metrics"], min_ratio
        )
        search_reports.append(
            {
                "query": baseline_search["query"],
                "checks": checks,
                "baseline_top_ids": baseline_search["top_ids"],
                "candidate_top_ids": candidate_search["top_ids"],
            }
        )
        failures.extend(
            {"query": baseline_search["query"], **check}
            for check in checks
            if check["status"] == "failed"
        )

    return {
        "summary": {
            "status": "passed" if not failures else "failed",
            "failure_count": len(failures),
            "property_count": len(baseline["properties"]),
            "search_count": len(baseline["searches"]),
            "min_ratio": min_ratio,
        },
        "failures": failures,
        "properties": property_reports,
        "searches": search_reports,
        "candidate": candidate,
    }


def compare_metric_group(
    baseline: dict[str, int], candidate: dict[str, int], min_ratio: float
) -> list[dict[str, Any]]:
    checks = []
    for key, baseline_value in baseline.items():
        candidate_value = candidate.get(key, 0)
        required = int(baseline_value * min_ratio)
        status = "passed" if candidate_value >= required else "failed"
        checks.append(
            {
                "metric": key,
                "baseline": baseline_value,
                "candidate": candidate_value,
                "required": required,
                "status": status,
            }
        )
    return checks


def summary(report: dict[str, Any]) -> dict[str, Any]:
    return {
        "captured_at": report["captured_at"],
        "property_count": len(report["properties"]),
        "search_count": len(report["searches"]),
        "properties": {
            property_id: entry["metrics"]
            for property_id, entry in report["properties"].items()
        },
        "searches": {
            entry["query"]: entry["metrics"] for entry in report["searches"]
        },
    }


def get_json(url: str) -> dict[str, Any]:
    request = urllib.request.Request(url, headers={"Accept": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            return json.loads(response.read())
    except urllib.error.HTTPError as error:
        body = error.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"{url} returned HTTP {error.code}: {body}") from error


def list_values(value: Any) -> list[Any]:
    return value if isinstance(value, list) else []


def read_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text())


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    capture = subparsers.add_parser("capture")
    add_common_args(capture)

    compare = subparsers.add_parser("compare")
    add_common_args(compare)
    compare.add_argument("--baseline", type=Path, required=True)
    compare.add_argument("--min-ratio", type=float, default=0.85)

    return parser.parse_args()


def add_common_args(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--base-url", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--property-id", action="append", default=[])
    parser.add_argument("--query", action="append", default=[])


if __name__ == "__main__":
    sys.exit(main())
