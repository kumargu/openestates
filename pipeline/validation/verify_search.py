"""
Search verification for Phase 0 gate.

Reads the validation set, constructs NL queries per property, hits the search
API, and verifies each property appears in at least one query's results.

Requires the backend running on port 4000.

Usage:
    python3 pipeline/validation/verify_search.py
    python3 pipeline/validation/verify_search.py --port 4000
"""

from __future__ import annotations

import json
import sys
import urllib.request
import urllib.parse
from pathlib import Path
from typing import Optional

ROOT = Path(__file__).resolve().parent.parent.parent
DATA = ROOT / "data"


def load_json(path: Path) -> Optional[list | dict]:
    try:
        with open(path) as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return None


def search(query: str, base_url: str) -> list[dict]:
    """Hit GET /api/search?q=... and return the results array."""
    encoded = urllib.parse.quote(query, safe="")
    url = f"{base_url}/api/search?q={encoded}"
    try:
        req = urllib.request.Request(url, headers={"Accept": "application/json"})
        with urllib.request.urlopen(req, timeout=10) as resp:
            data = json.loads(resp.read().decode())
            return data.get("results", data) if isinstance(data, dict) else data
    except Exception as e:
        print(f"  ERROR querying '{query}': {e}")
        return []


def build_queries(prop: dict) -> list[str]:
    """Build 2-3 NL search queries from property metadata."""
    queries = []
    area = prop.get("area", "")
    bhk = prop.get("bhk")
    society_id = prop.get("society_id", "")

    # Derive society name from society_id (strip soc- prefix, replace hyphens with spaces)
    society_name = society_id.replace("soc-", "").replace("-", " ").title() if society_id else ""

    # Query 1: BHK + area (e.g. "3BHK Whitefield")
    if bhk and area:
        queries.append(f"{bhk}BHK {area}")

    # Query 2: society name (e.g. "Prestige Somerville")
    if society_name:
        queries.append(society_name)

    # Query 3: BHK + society name (e.g. "3BHK Prestige Somerville")
    if bhk and society_name:
        queries.append(f"{bhk}BHK {society_name}")

    return queries


def result_contains_property(results: list[dict], prop_id: str) -> bool:
    """Check if a property ID appears anywhere in search results."""
    for r in results:
        rid = r.get("id", "")
        if rid == prop_id:
            return True
        # Also check nested property if present
        if r.get("property", {}).get("id") == prop_id:
            return True
    return False


def main() -> int:
    import argparse
    parser = argparse.ArgumentParser(description="Phase 0 search verification")
    parser.add_argument("--port", type=int, default=4000, help="Backend port")
    args = parser.parse_args()

    base_url = f"http://localhost:{args.port}"

    # Check backend is running
    try:
        req = urllib.request.Request(f"{base_url}/api/health", headers={"Accept": "application/json"})
        with urllib.request.urlopen(req, timeout=5) as resp:
            pass
    except Exception:
        print(f"FATAL: Backend not reachable at {base_url}")
        print("Start the backend with: cd backend && cargo run")
        return 1

    # Load data
    val_ids = load_json(ROOT / "pipeline" / "validation" / "validation_set.json")
    if not val_ids:
        print("FATAL: Could not load validation_set.json")
        return 1

    properties = load_json(DATA / "seed" / "properties.json") or []
    props_by_id = {p["id"]: p for p in properties}

    print(f"Search Verification — {len(val_ids)} properties")
    print(f"Backend: {base_url}")
    print("=" * 70)

    total_pass = 0
    total_warn = 0
    results_detail = []

    for prop_id in val_ids:
        prop = props_by_id.get(prop_id)
        if not prop:
            print(f"\n{prop_id}")
            print(f"  [!] WARN: Property not found in seed data — cannot verify search")
            total_warn += 1
            results_detail.append({"id": prop_id, "status": "WARN", "reason": "not in seed data"})
            continue

        queries = build_queries(prop)
        found_in = []

        for query in queries:
            results = search(query, base_url)
            if result_contains_property(results, prop_id):
                found_in.append(query)

        area = prop.get("area", "?")
        bhk = prop.get("bhk", "?")

        print(f"\n{prop_id}")
        print(f"  Area: {area} | BHK: {bhk}")
        print(f"  Queries tested: {len(queries)}")

        if found_in:
            print(f"  [+] PASS: Found in {len(found_in)}/{len(queries)} queries")
            for q in found_in:
                print(f"       - \"{q}\"")
            total_pass += 1
            results_detail.append({
                "id": prop_id, "status": "PASS",
                "found_in": found_in, "queries_tested": queries
            })
        else:
            print(f"  [~] WARN: NOT found in any of {len(queries)} queries")
            for q in queries:
                print(f"       - \"{q}\"")
            total_warn += 1
            results_detail.append({
                "id": prop_id, "status": "WARN",
                "found_in": [], "queries_tested": queries
            })

    # Summary
    print("\n" + "=" * 70)
    print(f"TOTAL: {total_pass} PASS, {total_warn} WARN out of {len(val_ids)} properties")

    gate = "PASS" if total_warn <= 2 else "WARN"
    print(f"Gate: {gate} (threshold: each property found in at least 1 query, max 2 exceptions)")

    # Write JSON report
    output_dir = DATA / "validation"
    output_dir.mkdir(parents=True, exist_ok=True)
    report = {
        "test": "search_verification",
        "total_properties": len(val_ids),
        "pass": total_pass,
        "warn": total_warn,
        "gate": gate,
        "results": results_detail,
    }
    report_path = output_dir / "search_verification.json"
    with open(report_path, "w") as f:
        json.dump(report, f, indent=2)
    print(f"Report written to {report_path.relative_to(ROOT)}")

    return 0 if gate == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
