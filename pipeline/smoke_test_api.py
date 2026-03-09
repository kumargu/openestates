#!/usr/bin/env python3
"""
OpenEstates Backend Smoke Tests (Day 15)

Expanded API smoke tests covering:
  - Health check
  - Property listing + card shape
  - Property detail (valid ID) with conviction, market, greenery, compare, transparency fields
  - Property detail (invalid ID) -> 404
  - Area listing + card shape
  - Area detail (valid ID) with full signal fields
  - Area detail (invalid ID) -> 404
  - Society livability fields in property detail
  - Shortlist placeholder

Dynamic ID resolution: fetches /api/properties and /api/areas to get valid IDs.

Usage:
  python3 pipeline/smoke_test_api.py
  python3 pipeline/smoke_test_api.py --base-url http://localhost:4000
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from urllib.request import urlopen, Request
from urllib.error import HTTPError, URLError


def api_get(base_url: str, path: str) -> tuple[int, dict | list | None]:
    """GET request returning (status_code, parsed_json)."""
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


def run_tests(base_url: str) -> list[dict]:
    results: list[dict] = []

    def test(name: str, passed: bool, detail: str = ""):
        icon = "PASS" if passed else "FAIL"
        print(f"  [{icon}] {name}" + (f" — {detail}" if detail else ""))
        results.append({"test": name, "passed": passed, "detail": detail})

    # ---------------------------------------------------------------
    # 1. Health check
    # ---------------------------------------------------------------
    code, body = api_get(base_url, "/api/health")
    test("GET /api/health -> 200", code == 200, f"status={code}")
    if code != 200:
        print("\n  Backend unreachable — aborting remaining tests.")
        return results

    # ---------------------------------------------------------------
    # 2. List properties
    # ---------------------------------------------------------------
    code, body = api_get(base_url, "/api/properties")
    is_list = isinstance(body, list)
    prop_count = len(body) if is_list else 0
    test("GET /api/properties -> non-empty list", code == 200 and prop_count > 0,
         f"status={code}, count={prop_count}")

    # Resolve valid ID dynamically
    valid_id = body[0]["id"] if is_list and prop_count > 0 and "id" in body[0] else None
    invalid_id = "NONEXISTENT_ID_12345"

    # Card shape check
    if is_list and prop_count > 0:
        card = body[0]
        required_card_fields = ["id", "title", "area", "price", "price_per_sqft", "bhk", "sqft", "society_name", "transparency_tags"]
        missing = [f for f in required_card_fields if f not in card]
        test("Property card has required fields",
             len(missing) == 0,
             f"missing={missing}" if missing else "all present")

    # ---------------------------------------------------------------
    # 3. Valid property detail
    # ---------------------------------------------------------------
    if valid_id:
        code, detail = api_get(base_url, f"/api/properties/{valid_id}")
        has_property = isinstance(detail, dict) and "property" in detail
        has_society = isinstance(detail, dict) and "society" in detail
        has_area = isinstance(detail, dict) and "area" in detail
        test(f"GET /api/properties/{valid_id} -> 200",
             code == 200 and has_property,
             f"property={has_property}, society={has_society}, area={has_area}")

        if code == 200 and has_property:
            prop = detail["property"]

            # 3a. Conviction fields
            conviction_fields = [
                "society_quality_score", "builder_quality_score",
                "document_completeness_score", "litigation_risk",
                "metro_distance_mins", "noise_score", "traffic_score",
                "waterlogging_risk_score", "sunlight_score",
            ]
            missing_c = [f for f in conviction_fields if f not in prop]
            test("Property has conviction fields",
                 len(missing_c) == 0,
                 f"missing={missing_c}" if missing_c else "all present")

            # 3b. Market activity fields
            market_fields = [
                "days_on_market", "saves_last_7d", "interest_level",
            ]
            missing_m = [f for f in market_fields if f not in prop]
            test("Property has market activity fields",
                 len(missing_m) == 0,
                 f"missing={missing_m}" if missing_m else "all present")

            # 3c. Greenery/open space fields
            greenery_fields = ["greenery_score", "open_space_score", "resale_strength_score"]
            present_g = [f for f in greenery_fields if f in prop]
            test("Property has greenery/resale fields",
                 len(present_g) >= 2,
                 f"{len(present_g)}/{len(greenery_fields)}: {present_g}")

            # 3d. Shortlist compare fields
            compare_fields = ["price", "price_per_sqft", "bhk", "area", "carpet_area_sqft"]
            missing_cmp = [f for f in compare_fields if f not in prop]
            test("Property has shortlist compare fields",
                 len(missing_cmp) == 0,
                 f"missing={missing_cmp}" if missing_cmp else "all present")

            # 3e. Transparency tags
            test("Property has transparency_tags",
                 "transparency_tags" in prop and isinstance(prop["transparency_tags"], list),
                 f"count={len(prop.get('transparency_tags', []))}")

            # 3f. Detail-page render fields
            render_fields = [
                "title", "price", "price_per_sqft", "bhk",
                "carpet_area_sqft", "super_builtup_sqft",
                "floor", "total_floors", "facing", "possession_status",
                "hero_image", "description_summary", "area", "city", "builder_name",
            ]
            missing_r = [f for f in render_fields if f not in prop]
            test("Property has all render fields for detail page",
                 len(missing_r) == 0,
                 f"missing={missing_r}" if missing_r else "all present")

            # 3g. Society livability fields
            soc = detail.get("society")
            if soc:
                soc_fields = ["name", "summary", "common_positives",
                              "common_complaints", "livability_sentiment",
                              "maintenance_sentiment",
                              "builder_name", "year_built", "total_units"]
                soc_missing = [f for f in soc_fields if f not in soc]
                test("Society has livability fields",
                     len(soc_missing) == 0,
                     f"missing={soc_missing}" if soc_missing else "all present")
            else:
                test("Society data present in detail response", False, "society is null")

            # 3h. Area signal fields in property detail
            area = detail.get("area")
            if area:
                area_fields = ["name", "median_price_per_sqft", "trend_direction",
                               "metro_access_summary", "livability_summary",
                               "traffic_summary", "waterlogging_summary",
                               "externality_tags", "community_notes"]
                area_missing = [f for f in area_fields if f not in area]
                test("Area has signal fields in property detail",
                     len(area_missing) == 0,
                     f"missing={area_missing}" if area_missing else "all present")
            else:
                test("Area data present in detail response", False, "area is null")
    else:
        test("GET /api/properties/:id (valid)", False, "No valid ID resolved")

    # ---------------------------------------------------------------
    # 4. Invalid property detail -> 404
    # ---------------------------------------------------------------
    code, body = api_get(base_url, f"/api/properties/{invalid_id}")
    test(f"GET /api/properties/{invalid_id} -> 404",
         code == 404,
         f"status={code}")

    # ---------------------------------------------------------------
    # 5. List areas
    # ---------------------------------------------------------------
    code, body = api_get(base_url, "/api/areas")
    is_list = isinstance(body, list)
    area_count = len(body) if is_list else 0
    test("GET /api/areas -> non-empty list",
         code == 200 and area_count > 0,
         f"status={code}, count={area_count}")

    # Area card shape check
    if is_list and area_count > 0:
        acard = body[0]
        area_card_fields = ["id", "name", "median_price_per_sqft", "trend_direction"]
        missing_ac = [f for f in area_card_fields if f not in acard]
        test("Area card has required fields",
             len(missing_ac) == 0,
             f"missing={missing_ac}" if missing_ac else "all present")

    # ---------------------------------------------------------------
    # 6. Valid area detail
    # ---------------------------------------------------------------
    area_id = None
    if is_list and area_count > 0:
        area_id = body[0].get("id", "")
    if area_id:
        code, area_body = api_get(base_url, f"/api/areas/{area_id}")
        test(f"GET /api/areas/{area_id} -> 200",
             code == 200 and area_body is not None,
             f"status={code}")

        if code == 200 and isinstance(area_body, dict):
            area_full_fields = ["name", "median_price_per_sqft", "trend_direction",
                                "metro_access_summary", "livability_summary",
                                "traffic_summary", "waterlogging_summary",
                                "externality_tags", "community_notes"]
            present_af = [f for f in area_full_fields if f in area_body]
            missing_af = [f for f in area_full_fields if f not in area_body]
            test("Area detail has full signal fields",
                 len(missing_af) == 0,
                 f"missing={missing_af}" if missing_af else "all present")

    # ---------------------------------------------------------------
    # 7. Invalid area detail -> 404
    # ---------------------------------------------------------------
    code, body = api_get(base_url, "/api/areas/INVALID_AREA_999")
    test("GET /api/areas/INVALID_AREA_999 -> 404",
         code == 404,
         f"status={code}")

    # ---------------------------------------------------------------
    # 8. Shortlist (placeholder)
    # ---------------------------------------------------------------
    code, body = api_get(base_url, "/api/shortlist")
    test("GET /api/shortlist -> 200",
         code == 200,
         "Shortlist is frontend-local (localStorage); server returns placeholder")

    return results


def main():
    parser = argparse.ArgumentParser(description="OpenEstates API smoke tests (Day 15)")
    parser.add_argument("--base-url", default="http://localhost:4000", help="Backend API URL")
    parser.add_argument("--output-dir", default=None, help="Output directory for report")
    args = parser.parse_args()

    print("OpenEstates API Smoke Tests (Day 15)")
    print(f"  Base URL: {args.base_url}")
    print()

    results = run_tests(args.base_url)

    total = len(results)
    passed = sum(1 for r in results if r["passed"])
    failed = total - passed

    print(f"\nResults: {passed}/{total} passed" + (f" ({failed} failed)" if failed else ""))

    smoke_status = "pass" if failed == 0 else ("partial" if passed > 0 else "fail")

    # Save report
    report = {
        "base_url": args.base_url,
        "captured_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "total_tests": total,
        "passed": passed,
        "failed": failed,
        "smoke_status": smoke_status,
        "tests": results,
    }
    report_dir = args.output_dir or os.path.join(os.path.dirname(os.path.abspath(__file__)), "feedback", "day15")
    os.makedirs(report_dir, exist_ok=True)
    report_path = os.path.join(report_dir, "smoke_test_report.json")
    with open(report_path, "w") as f:
        json.dump(report, f, indent=2)
    print(f"Report saved to {report_path}")

    if failed > 0:
        sys.exit(1)


if __name__ == "__main__":
    main()
