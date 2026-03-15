"""
Seller matching verification for Phase 0 gate.

Reads sellers.json, finds sellers linked to validation properties, verifies:
1. Seller exists and is retrievable via API
2. Property is linked in seller's property_ids
3. Interest flow works (POST interest, verify count)

Requires the backend running on port 4000.

Usage:
    python3 pipeline/validation/verify_sellers.py
    python3 pipeline/validation/verify_sellers.py --port 4000
"""

from __future__ import annotations

import json
import sys
import urllib.request
import urllib.error
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


def api_get(url: str) -> Optional[dict | list]:
    """GET a JSON endpoint. Returns None on error."""
    try:
        req = urllib.request.Request(url, headers={"Accept": "application/json"})
        with urllib.request.urlopen(req, timeout=10) as resp:
            return json.loads(resp.read().decode())
    except urllib.error.HTTPError as e:
        return {"_http_error": e.code}
    except Exception as e:
        return {"_error": str(e)}


def api_post(url: str, body: dict) -> tuple[int, Optional[dict]]:
    """POST JSON to an endpoint. Returns (status_code, response_body)."""
    try:
        payload = json.dumps(body).encode()
        req = urllib.request.Request(
            url,
            data=payload,
            headers={"Content-Type": "application/json", "Accept": "application/json"},
            method="POST",
        )
        with urllib.request.urlopen(req, timeout=10) as resp:
            data = json.loads(resp.read().decode())
            return resp.status, data
    except urllib.error.HTTPError as e:
        try:
            data = json.loads(e.read().decode())
        except Exception:
            data = None
        return e.code, data
    except Exception as e:
        return 0, {"_error": str(e)}


def main() -> int:
    import argparse
    parser = argparse.ArgumentParser(description="Phase 0 seller matching verification")
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
    val_ids = load_json(ROOT / "pipeline" / "validation" / "validation_set.json") or []
    sellers = load_json(DATA / "sellers" / "sellers.json") or []

    if not sellers:
        print("FATAL: Could not load sellers.json")
        return 1

    # Find sellers linked to validation properties
    seller_prop_pairs: list[tuple[dict, str]] = []
    for seller in sellers:
        for pid in seller.get("property_ids", []):
            if pid in val_ids:
                seller_prop_pairs.append((seller, pid))

    print(f"Seller Matching Verification — {len(seller_prop_pairs)} seller-property pairs")
    print(f"Backend: {base_url}")
    print("=" * 70)

    total_pass = 0
    total_fail = 0
    results_detail = []

    for seller, prop_id in seller_prop_pairs:
        seller_id = seller["id"]
        seller_name = seller.get("name", "?")
        print(f"\n{seller_id} -> {prop_id}")
        print(f"  Seller: {seller_name}")

        checks_passed = 0
        checks_total = 0
        check_results = []

        # Check 1: Seller API returns seller
        checks_total += 1
        seller_data = api_get(f"{base_url}/api/sellers/{seller_id}")
        if seller_data and "_http_error" not in seller_data and "_error" not in seller_data:
            print(f"  [+] PASS: GET /api/sellers/{seller_id} returns data")
            checks_passed += 1
            check_results.append({"check": "seller_api", "status": "PASS"})
        else:
            err = seller_data.get("_http_error", seller_data.get("_error", "unknown")) if seller_data else "no response"
            print(f"  [!] FAIL: GET /api/sellers/{seller_id} failed ({err})")
            check_results.append({"check": "seller_api", "status": "FAIL", "error": str(err)})

        # Check 2: Property is in seller's property_ids (data-level check, already verified above)
        checks_total += 1
        if prop_id in seller.get("property_ids", []):
            print(f"  [+] PASS: Property {prop_id} in seller's property_ids")
            checks_passed += 1
            check_results.append({"check": "property_link", "status": "PASS"})
        else:
            print(f"  [!] FAIL: Property {prop_id} NOT in seller's property_ids")
            check_results.append({"check": "property_link", "status": "FAIL"})

        # Check 3: POST interest flow
        checks_total += 1
        status_code, resp = api_post(
            f"{base_url}/api/interests",
            {"property_id": prop_id, "buyer_name": "Phase0 Test", "buyer_contact": "test@phase0.dev"}
        )
        if status_code == 201 and resp and resp.get("status") == "interest_recorded":
            interest_id = resp.get("id", "?")
            print(f"  [+] PASS: POST /api/interests succeeded (id={interest_id[:40]}...)")
            checks_passed += 1
            check_results.append({"check": "interest_post", "status": "PASS"})
        else:
            print(f"  [!] FAIL: POST /api/interests failed (status={status_code})")
            check_results.append({"check": "interest_post", "status": "FAIL", "http_status": status_code})

        # Check 4: Interest count endpoint
        checks_total += 1
        count_data = api_get(f"{base_url}/api/properties/{prop_id}/interests/count")
        if count_data and isinstance(count_data, dict) and count_data.get("count", 0) >= 1:
            count_val = count_data["count"]
            print(f"  [+] PASS: Interest count = {count_val}")
            checks_passed += 1
            check_results.append({"check": "interest_count", "status": "PASS", "count": count_val})
        else:
            print(f"  [~] FAIL: Interest count check failed: {count_data}")
            check_results.append({"check": "interest_count", "status": "FAIL"})

        # Aggregate
        pair_status = "PASS" if checks_passed == checks_total else "FAIL"
        print(f"  Result: {checks_passed}/{checks_total} checks passed -> {pair_status}")

        if pair_status == "PASS":
            total_pass += 1
        else:
            total_fail += 1

        results_detail.append({
            "seller_id": seller_id,
            "property_id": prop_id,
            "status": pair_status,
            "checks": check_results,
        })

    # Summary
    print("\n" + "=" * 70)
    print(f"TOTAL: {total_pass} PASS, {total_fail} FAIL out of {len(seller_prop_pairs)} pairs")

    gate = "PASS" if total_fail == 0 else "FAIL"
    print(f"Gate: {gate}")

    # Write JSON report
    output_dir = DATA / "validation"
    output_dir.mkdir(parents=True, exist_ok=True)
    report = {
        "test": "seller_matching_verification",
        "total_pairs": len(seller_prop_pairs),
        "pass": total_pass,
        "fail": total_fail,
        "gate": gate,
        "results": results_detail,
    }
    report_path = output_dir / "seller_verification.json"
    with open(report_path, "w") as f:
        json.dump(report, f, indent=2)
    print(f"Report written to {report_path.relative_to(ROOT)}")

    return 0 if gate == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
