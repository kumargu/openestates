"""
OpenEstates Backend Smoke Tests (Day 12)

Tests the Rust+Axum API endpoints to verify data is being served correctly.
Covers property detail (valid + invalid), property listing, areas, shortlist,
and deeper response shape verification for conviction surfaces.

Note: Shortlist state is frontend-local (localStorage). The backend /api/shortlist
endpoint returns an empty placeholder. Browser automation must set localStorage
to seed shortlist state.

Usage:
  python3 pipeline/smoke_test.py [--base-url http://localhost:4000]
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from urllib.request import urlopen, Request
from urllib.error import HTTPError, URLError


VALID_PROPERTY_ID = "prop_w_001"
INVALID_PROPERTY_ID = "NONEXISTENT_ID"


def api_get(base_url: str, path: str) -> tuple[int, dict | None]:
    """Make a GET request and return (status_code, json_body)."""
    url = base_url.rstrip("/") + path
    req = Request(url, headers={"Accept": "application/json"})
    try:
        with urlopen(req, timeout=10) as resp:
            body = json.loads(resp.read().decode())
            return resp.status, body
    except HTTPError as e:
        try:
            body = json.loads(e.read().decode())
        except Exception:
            body = None
        return e.code, body
    except URLError as e:
        print(f"  CONNECTION ERROR: {e.reason}")
        return 0, None


def run_smoke_tests(base_url: str) -> list[dict]:
    results: list[dict] = []

    def test(name: str, passed: bool, detail: str = ""):
        status = "PASS" if passed else "FAIL"
        print(f"  [{status}] {name}" + (f" — {detail}" if detail else ""))
        results.append({"test": name, "passed": passed, "detail": detail})

    # 1. Health check
    code, body = api_get(base_url, "/api/health")
    test("GET /api/health", code == 200, f"status={code}")

    # 2. List properties
    code, body = api_get(base_url, "/api/properties")
    if code == 200 and isinstance(body, list):
        test("GET /api/properties", len(body) > 0, f"{len(body)} properties returned")
    else:
        test("GET /api/properties", False, f"status={code}")

    # 3. Valid property detail
    code, body = api_get(base_url, f"/api/properties/{VALID_PROPERTY_ID}")
    if code == 200 and body:
        has_property = "property" in body
        has_society = "society" in body
        has_area = "area" in body
        prop = body.get("property", {})
        has_scores = all(k in prop for k in ["society_quality_score", "document_completeness_score", "greenery_score"])
        test("GET /api/properties/:id (valid)", has_property and has_society and has_area,
             f"property={has_property}, society={has_society}, area={has_area}, scores={has_scores}")

        # Verify conviction-surface fields exist
        conviction_fields = ["days_on_market", "saves_last_7d", "interest_level", "greenery_score", "open_space_score", "resale_strength_score"]
        present = [f for f in conviction_fields if f in prop]
        test("Property has conviction fields", len(present) >= 4,
             f"Found {len(present)}/{len(conviction_fields)}: {present}")
    else:
        test("GET /api/properties/:id (valid)", False, f"status={code}")
        test("Property has conviction fields", False, "skipped — property not loaded")

    # 4. Response shape: detail page supporting fields
    if code == 200 and body:
        prop = body.get("property", {})
        # Fields needed for transparency surfaces
        detail_fields = [
            "title", "price", "price_per_sqft", "bhk", "carpet_area_sqft",
            "society_quality_score", "document_completeness_score", "builder_quality_score",
            "litigation_risk", "waterlogging_risk_score", "traffic_score", "noise_score",
            "sunlight_score", "metro_distance_mins", "maintenance_cost_monthly",
            "days_on_market", "interest_level", "transparency_tags",
        ]
        present = [f for f in detail_fields if f in prop]
        missing = [f for f in detail_fields if f not in prop]
        test("Property detail has all transparency fields",
             len(missing) == 0,
             f"Present: {len(present)}/{len(detail_fields)}" + (f", missing: {missing}" if missing else ""))

        # Verify society and area have expected fields
        soc = body.get("society", {})
        if soc:
            soc_fields = ["name", "summary", "common_positives", "common_complaints", "livability_sentiment"]
            soc_present = [f for f in soc_fields if f in soc]
            test("Society has livability fields", len(soc_present) >= 4,
                 f"Found {len(soc_present)}/{len(soc_fields)}: {soc_present}")

        area_data = body.get("area", {})
        if area_data:
            area_fields = ["name", "median_price_per_sqft", "trend_direction", "metro_access_summary", "livability_summary"]
            area_present = [f for f in area_fields if f in area_data]
            test("Area has signal fields", len(area_present) >= 4,
                 f"Found {len(area_present)}/{len(area_fields)}: {area_present}")

    # 5. Invalid property detail (should 404)
    code, body = api_get(base_url, f"/api/properties/{INVALID_PROPERTY_ID}")
    test("GET /api/properties/:id (invalid) returns 404", code == 404, f"status={code}")

    # 6. List areas
    code, body = api_get(base_url, "/api/areas")
    if code == 200 and isinstance(body, list):
        test("GET /api/areas", len(body) > 0, f"{len(body)} areas returned")
    else:
        test("GET /api/areas", False, f"status={code}")

    # 7. Valid area detail
    if code == 200 and isinstance(body, list) and len(body) > 0:
        area_id = body[0].get("id", "")
        if area_id:
            a_code, a_body = api_get(base_url, f"/api/areas/{area_id}")
            test("GET /api/areas/:id (valid)", a_code == 200 and a_body is not None,
                 f"status={a_code}, area={area_id}")

    # 8. Shortlist endpoint (frontend-local, server returns placeholder)
    code, body = api_get(base_url, "/api/shortlist")
    test("GET /api/shortlist exists", code == 200,
         f"status={code} — shortlist is frontend-local (localStorage), server returns empty placeholder")

    return results


def main():
    parser = argparse.ArgumentParser(description="OpenEstates backend smoke tests")
    parser.add_argument("--base-url", default="http://localhost:4000", help="Backend API base URL")
    args = parser.parse_args()

    print("OpenEstates Backend Smoke Tests")
    print(f"  Base URL: {args.base_url}")
    print()

    results = run_smoke_tests(args.base_url)

    total = len(results)
    passed = sum(1 for r in results if r["passed"])
    failed = total - passed

    print(f"\nResults: {passed}/{total} passed")

    # Save report
    report = {
        "base_url": args.base_url,
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "total": total,
        "passed": passed,
        "failed": failed,
        "tests": results,
    }
    report_path = "pipeline/feedback/day12/smoke_test_report.json"
    import os
    os.makedirs(os.path.dirname(report_path), exist_ok=True)
    with open(report_path, "w") as f:
        json.dump(report, f, indent=2)
    print(f"Report saved to {report_path}")

    if failed > 0:
        sys.exit(1)


if __name__ == "__main__":
    main()
