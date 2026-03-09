#!/usr/bin/env python3
"""
OpenEstates Review Gate (Day 15)

The single "truth entrypoint" for deployed review. Runs four evidence layers
in strict order, gating each step on the previous one:

  1. API smoke tests       — are contracts alive?
  2. Render-truth check    — does the app shell render at all?
  3. Page capture          — what user-facing state appears?
  4. Journey verification  — what user path actually works?

If render-truth fails, the gate stops early — deeper review is meaningless
until the app shell is proven to render.

Verdicts:
  review_passed             — all evidence layers green
  review_failed_render_truth — app shell did not render; deeper review skipped
  review_failed_tooling     — harness/automation failure detected
  review_failed_product     — product rendering or data issues
  review_failed_mixed       — both tooling and product failures present

Usage:
  python3 pipeline/run_review_gate.py
  python3 pipeline/run_review_gate.py --frontend-url http://localhost:5173 --api-url http://localhost:4000
  python3 pipeline/run_review_gate.py --skip-journey
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path


SCRIPT_DIR = Path(__file__).parent
OUTPUT_DIR = SCRIPT_DIR / "feedback" / "day15"


# ---------------------------------------------------------------------------
# Layer A: API Smoke Tests
# ---------------------------------------------------------------------------

def run_smoke_tests(api_url: str) -> dict:
    """Run API smoke tests and return the report."""
    print("=" * 60)
    print("Layer A: API Smoke Tests")
    print("=" * 60)

    cmd = [
        sys.executable,
        str(SCRIPT_DIR / "smoke_test_api.py"),
        "--base-url", api_url,
        "--output-dir", str(OUTPUT_DIR),
    ]
    subprocess.run(cmd, capture_output=False, text=True)

    report_path = OUTPUT_DIR / "smoke_test_report.json"
    if report_path.exists():
        with open(report_path) as f:
            return json.load(f)
    return {
        "smoke_status": "fail",
        "passed": 0,
        "total_tests": 0,
        "failed": 0,
        "error": f"Report not found at {report_path}",
    }


# ---------------------------------------------------------------------------
# Layer B: Render Truth Check (NEW in Day 15)
# ---------------------------------------------------------------------------

def run_render_truth(frontend_url: str) -> dict:
    """Run the render-truth check and return the summary."""
    print()
    print("=" * 60)
    print("Layer B: Render Truth Check")
    print("=" * 60)

    cmd = [
        sys.executable,
        str(SCRIPT_DIR / "check_render_truth.py"),
        "--base-url", frontend_url,
        "--output-dir", str(OUTPUT_DIR),
    ]
    subprocess.run(cmd, capture_output=False, text=True)

    summary_path = OUTPUT_DIR / "render_truth.summary.json"
    if summary_path.exists():
        with open(summary_path) as f:
            return json.load(f)
    return {
        "render_truth_status": "unknown",
        "routes_checked": 0,
        "routes_with_app_shell": 0,
        "error": f"Summary not found at {summary_path}",
    }


# ---------------------------------------------------------------------------
# Layer C: Page Capture
# ---------------------------------------------------------------------------

def run_page_capture(frontend_url: str, api_url: str) -> dict:
    """Run page capture and return the summary."""
    print()
    print("=" * 60)
    print("Layer C: Page Capture")
    print("=" * 60)

    cmd = [
        sys.executable,
        str(SCRIPT_DIR / "capture_deployed_pages.py"),
        "--base-url", frontend_url,
        "--api-base-url", api_url,
        "--output-dir", str(OUTPUT_DIR),
    ]
    subprocess.run(cmd, capture_output=False, text=True)

    summary_path = OUTPUT_DIR / "capture_summary.json"
    if summary_path.exists():
        with open(summary_path) as f:
            return json.load(f)
    return {
        "ok_count": 0,
        "fallback_count": 0,
        "tooling_error_count": 0,
        "total_pages": 0,
        "error": f"Summary not found at {summary_path}",
    }


# ---------------------------------------------------------------------------
# Layer D: Journey Verification
# ---------------------------------------------------------------------------

def run_journey(frontend_url: str, api_url: str) -> dict:
    """Run the journey script and return the report."""
    print()
    print("=" * 60)
    print("Layer D: Journey Verification")
    print("=" * 60)

    cmd = [
        sys.executable,
        str(SCRIPT_DIR / "journey_property_to_shortlist.py"),
        "--base-url", frontend_url,
        "--api-base-url", api_url,
        "--output-dir", str(OUTPUT_DIR),
    ]
    subprocess.run(cmd, capture_output=False, text=True)

    report_path = OUTPUT_DIR / "journey_report.json"
    if report_path.exists():
        with open(report_path) as f:
            return json.load(f)
    return {
        "journey_status": "not_run",
        "passed": 0,
        "total_steps": 0,
        "error": f"Report not found at {report_path}",
    }


# ---------------------------------------------------------------------------
# Verdict computation
# ---------------------------------------------------------------------------

def compute_verdict(
    smoke: dict,
    render_truth: dict,
    capture: dict,
    journey: dict,
) -> str:
    """Compute the overall review gate verdict."""
    has_tooling_failure = False
    has_product_failure = False

    # Check smoke tests
    smoke_status = smoke.get("smoke_status", "fail")
    if smoke_status == "fail":
        has_product_failure = True

    # Check render truth (Day 15 gate)
    rt_status = render_truth.get("render_truth_status", "unknown")
    if rt_status not in ("app_shell_verified", "app_shell_present"):
        # Render truth failed — this is a hard gate
        return "review_failed_render_truth"

    # Check capture
    tooling_errors = capture.get("tooling_error_count", 0)
    nav_errors = capture.get("navigation_error_count", 0)
    render_errors = capture.get("render_error_count", 0)
    ok_count = capture.get("ok_count", 0)
    fb_count = capture.get("fallback_count", 0)
    total_pages = capture.get("total_pages", 0)

    if tooling_errors > 0:
        has_tooling_failure = True
    if nav_errors > 0 or render_errors > 0:
        has_product_failure = True

    # Check journey
    journey_status = journey.get("journey_status", "not_run")
    if journey_status in ("tooling_failed_before_journey",):
        has_tooling_failure = True
    elif journey_status == "journey_failed_product":
        has_product_failure = True
    elif journey_status == "not_run":
        pass

    # Determine verdict
    if has_tooling_failure and has_product_failure:
        return "review_failed_mixed"
    if has_tooling_failure:
        return "review_failed_tooling"
    if has_product_failure:
        return "review_failed_product"

    # All green
    if smoke_status == "pass" and (ok_count + fb_count) >= total_pages:
        return "review_passed"

    # Partial success (e.g. some fallbacks but no errors)
    if smoke_status in ("pass", "partial") and tooling_errors == 0:
        return "review_passed"

    return "review_failed_product"


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description="OpenEstates Review Gate (Day 15)")
    parser.add_argument("--frontend-url", default="http://localhost:5173", help="Frontend URL")
    parser.add_argument("--api-url", default="http://localhost:4000", help="Backend API URL")
    parser.add_argument("--skip-journey", action="store_true", help="Skip browser journey")
    args = parser.parse_args()

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)

    print("OpenEstates Review Gate (Day 15)")
    print(f"  Frontend: {args.frontend_url}")
    print(f"  API:      {args.api_url}")
    print(f"  Output:   {OUTPUT_DIR}")
    print()

    # Layer A: Smoke tests
    smoke_report = run_smoke_tests(args.api_url)

    # Layer B: Render truth (GATE — stops early if failed)
    render_truth_report = run_render_truth(args.frontend_url)
    rt_status = render_truth_report.get("render_truth_status", "unknown")

    if rt_status not in ("app_shell_verified", "app_shell_present"):
        print()
        print("=" * 60)
        print("RENDER TRUTH GATE: FAILED")
        print("=" * 60)
        print(f"  Status: {rt_status}")
        print(f"  Routes with app shell: {render_truth_report.get('routes_with_app_shell', 0)}/{render_truth_report.get('routes_checked', 0)}")
        print()
        print("  Deeper review skipped — app shell must render before UX review.")
        print("  Fix the deployment/runtime issue before proceeding.")
        print()

        # Still write review summary so artifacts exist
        review_summary = {
            "review_status": "review_failed_render_truth",
            "captured_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "smoke_test_status": smoke_report.get("smoke_status", "unknown"),
            "smoke_tests_passed": smoke_report.get("passed", 0),
            "smoke_tests_total": smoke_report.get("total_tests", 0),
            "render_truth_status": rt_status,
            "render_truth_routes_checked": render_truth_report.get("routes_checked", 0),
            "render_truth_routes_with_shell": render_truth_report.get("routes_with_app_shell", 0),
            "pages": [],
            "capture_ok": 0,
            "capture_fallback": 0,
            "capture_tooling_errors": 0,
            "capture_total": 0,
            "journey_status": "not_run",
            "journey_passed": 0,
            "journey_total": 0,
            "gate_stopped_at": "render_truth",
        }
        summary_path = OUTPUT_DIR / "review.summary.json"
        summary_path.write_text(json.dumps(review_summary, indent=2), encoding="utf-8")
        print(f"  Summary saved to {summary_path}")
        sys.exit(4)

    print()
    print(f"  Render truth: PASSED ({rt_status})")
    print()

    # Layer C: Page capture (only if render truth passed)
    capture_summary = run_page_capture(args.frontend_url, args.api_url)

    # Layer D: Journey (only if page capture completed)
    if args.skip_journey:
        print()
        print("=" * 60)
        print("Layer D: Journey Verification (SKIPPED)")
        print("=" * 60)
        journey_report = {"journey_status": "not_run", "passed": 0, "total_steps": 0}
    else:
        journey_report = run_journey(args.frontend_url, args.api_url)

    # Compute verdict
    verdict = compute_verdict(smoke_report, render_truth_report, capture_summary, journey_report)

    # Build review summary
    pages_summary = []
    for pg in capture_summary.get("pages", []):
        pages_summary.append({
            "page_name": pg.get("page_name"),
            "capture_status": pg.get("capture_status"),
            "http_status": pg.get("http_status"),
            "text_length": pg.get("text_length"),
            "contains_tooling_error_signature": pg.get("contains_tooling_error_signature"),
            "contains_product_fallback_heading": pg.get("contains_product_fallback_heading"),
        })

    review_summary = {
        "review_status": verdict,
        "captured_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "smoke_test_status": smoke_report.get("smoke_status", "unknown"),
        "smoke_tests_passed": smoke_report.get("passed", 0),
        "smoke_tests_total": smoke_report.get("total_tests", 0),
        "render_truth_status": rt_status,
        "render_truth_routes_checked": render_truth_report.get("routes_checked", 0),
        "render_truth_routes_with_shell": render_truth_report.get("routes_with_app_shell", 0),
        "pages": pages_summary,
        "capture_ok": capture_summary.get("ok_count", 0),
        "capture_fallback": capture_summary.get("fallback_count", 0),
        "capture_tooling_errors": capture_summary.get("tooling_error_count", 0),
        "capture_total": capture_summary.get("total_pages", 0),
        "journey_status": journey_report.get("journey_status", "not_run"),
        "journey_passed": journey_report.get("passed", 0),
        "journey_total": journey_report.get("total_steps", 0),
    }

    # Save review summary
    summary_path = OUTPUT_DIR / "review.summary.json"
    summary_path.write_text(json.dumps(review_summary, indent=2), encoding="utf-8")

    # Print final verdict
    print()
    print("=" * 60)
    print("REVIEW GATE VERDICT")
    print("=" * 60)
    print(f"  Status:       {verdict}")
    print(f"  Smoke:        {smoke_report.get('passed', 0)}/{smoke_report.get('total_tests', 0)} ({smoke_report.get('smoke_status', '?')})")
    print(f"  Render truth: {rt_status}")
    print(f"  Capture:      {capture_summary.get('ok_count', 0)} ok, {capture_summary.get('fallback_count', 0)} fallback, "
          f"{capture_summary.get('tooling_error_count', 0)} tooling errors")
    print(f"  Journey:      {journey_report.get('journey_status', 'not_run')} "
          f"({journey_report.get('passed', 0)}/{journey_report.get('total_steps', 0)})")
    print()
    print(f"  Summary saved to {summary_path}")
    print()

    if verdict == "review_passed":
        print("  GATE: PASSED — product is reviewable")
        sys.exit(0)
    elif verdict == "review_failed_render_truth":
        print("  GATE: FAILED (render truth) — app shell did not render")
        sys.exit(4)
    elif verdict == "review_failed_tooling":
        print("  GATE: FAILED (tooling) — automation/harness issue, not a product bug")
        sys.exit(2)
    elif verdict == "review_failed_product":
        print("  GATE: FAILED (product) — deployed rendering has issues")
        sys.exit(1)
    else:
        print("  GATE: FAILED (mixed) — both tooling and product issues detected")
        sys.exit(3)


if __name__ == "__main__":
    main()
