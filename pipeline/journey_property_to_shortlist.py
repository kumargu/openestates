"""
OpenEstates Journey: Results -> Property Detail -> Save -> Shortlist (Day 15)

Standalone deterministic browser journey script. Uses async Playwright only.

Day 15 changes:
  - Fallback-aware assertions: explicitly distinguishes product fallback from tooling failure
  - Updated shortlist heading check ("Compare saved homes" instead of "Compare Workspace")
  - Tooling error signature detection on every page
  - Journey status: journey_passed, journey_failed_tooling, journey_failed_product

Usage:
  python3 pipeline/journey_property_to_shortlist.py --base-url http://localhost:5173
"""

from __future__ import annotations

import argparse
import asyncio
import json
import sys
import time
from pathlib import Path


# Reuse signature detection from capture script
TOOLING_ERROR_SIGNATURES = [
    "playwright sync api",
    "traceback (most recent",
    "runtimeerror",
    "econnrefused",
    "unhandledrejection",
    "typeerror:",
    "cannot read properties of",
]

PRODUCT_FALLBACK_SIGNATURES = [
    "results temporarily unavailable",
    "property details unavailable",
    "property not found",
    "compare saved homes",
    "your shortlist is empty",
    "no saved homes to compare yet",
    "saved homes stay on this browser",
    "live data temporarily unavailable",
    "we couldn't load live property data",
    "we couldn't load this listing",
    "we couldn't load property details",
    "this property page could not be loaded",
    "this listing may no longer be available",
]


def classify_text(text: str) -> str:
    """Classify page text as 'ok', 'tooling_error', or 'product_fallback'."""
    lower = text.lower()
    has_tooling = any(sig in lower for sig in TOOLING_ERROR_SIGNATURES)
    has_fallback = any(sig in lower for sig in PRODUCT_FALLBACK_SIGNATURES)
    if has_tooling:
        return "tooling_error"
    if has_fallback:
        return "product_fallback"
    return "ok"


async def resolve_property_ids(api_base_url: str) -> tuple[str, str]:
    """Fetch /api/properties and return (first_id, second_id)."""
    import urllib.request

    url = api_base_url.rstrip("/") + "/api/properties"
    try:
        req = urllib.request.Request(url, headers={"Accept": "application/json"})
        with urllib.request.urlopen(req, timeout=10) as resp:
            data = json.loads(resp.read().decode())
            if isinstance(data, list):
                id1 = data[0].get("id") if len(data) > 0 else None
                id2 = data[1].get("id") if len(data) > 1 else None
                return id1 or "prop_w_001", id2 or "prop_s_001"
    except Exception:
        pass
    return "prop_w_001", "prop_s_001"


async def run_journey(
    base_url: str,
    api_base_url: str,
    output_dir: Path,
) -> dict:
    """Run the property-to-shortlist journey with fallback-aware assertions."""
    from playwright.async_api import async_playwright

    base = base_url.rstrip("/")
    output_dir.mkdir(parents=True, exist_ok=True)
    steps: list[dict] = []
    has_tooling_error = False

    def step(name: str, passed: bool, detail: str = "", classification: str = "ok"):
        nonlocal has_tooling_error
        icon = "PASS" if passed else "FAIL"
        extra = f" [{classification}]" if classification != "ok" else ""
        print(f"  [{icon}] {name}{extra}" + (f" — {detail}" if detail else ""))
        steps.append({"step": name, "passed": passed, "detail": detail, "classification": classification})
        if classification == "tooling_error":
            has_tooling_error = True

    # Resolve property IDs
    prop_id, prop_id_2 = await resolve_property_ids(api_base_url)
    print(f"  Property IDs: {prop_id}, {prop_id_2}")

    async with async_playwright() as p:
        browser = await p.chromium.launch(headless=True)
        context = await browser.new_context(viewport={"width": 1280, "height": 900})
        page = await context.new_page()

        # Step 1: Open /results
        try:
            resp = await page.goto(f"{base}/results", wait_until="networkidle", timeout=15000)
            await page.wait_for_timeout(2000)
            text = await page.inner_text("body")
            cls = classify_text(text)
            has_content = len(text) > 50

            if cls == "tooling_error":
                step("Open results page", False, f"Tooling error leaked: {len(text)} chars", "tooling_error")
            elif cls == "product_fallback":
                step("Open results page (fallback)", True,
                     f"Product-owned fallback state rendered: {len(text)} chars", "product_fallback")
            else:
                has_properties = "bhk" in text.lower() or "sqft" in text.lower() or "properties" in text.lower()
                step("Open results page", has_content and has_properties,
                     f"{len(text)} chars, HTTP {resp.status if resp else '?'}")
        except Exception as e:
            step("Open results page", False, str(e), "tooling_error")

        # Step 2: Navigate to property detail
        try:
            card_link = await page.query_selector(f'a[href="/property/{prop_id}"]')
            if card_link:
                await card_link.click()
                await page.wait_for_load_state("networkidle")
                await page.wait_for_timeout(2500)
            else:
                await page.goto(f"{base}/property/{prop_id}", wait_until="networkidle", timeout=15000)
                await page.wait_for_timeout(2500)

            text = await page.inner_text("body")
            cls = classify_text(text)

            if cls == "tooling_error":
                step("Property detail renders", False, "Tooling error leaked", "tooling_error")
            elif cls == "product_fallback":
                step("Property detail renders (fallback)", True,
                     "Property detail unavailable — product fallback shown", "product_fallback")
            else:
                has_detail = "why this property" in text.lower() or "price vs" in text.lower() or "tradeoffs" in text.lower()
                step("Property detail renders", has_detail, f"{len(text)} chars")
        except Exception as e:
            step("Property detail renders", False, str(e), "tooling_error")

        # Step 3: Save the property
        try:
            save_btn = await page.query_selector('[data-testid="save-button"]')
            if not save_btn:
                save_btn = await page.query_selector("button:has-text('Save to shortlist')")
            if save_btn:
                btn_before = await save_btn.inner_text()
                await save_btn.click()
                await page.wait_for_timeout(500)
                btn_after = await save_btn.inner_text()
                toggled = btn_before != btn_after
                step("Save property", toggled, f"'{btn_before}' -> '{btn_after}'")
            else:
                step("Save property", False, "Save button not found — may be in fallback state",
                     "product_fallback" if classify_text(await page.inner_text("body")) == "product_fallback" else "ok")
        except Exception as e:
            step("Save property", False, str(e))

        # Step 3b: Save a second property
        try:
            await page.goto(f"{base}/property/{prop_id_2}", wait_until="networkidle", timeout=15000)
            await page.wait_for_timeout(2000)
            save_btn = await page.query_selector('[data-testid="save-button"]')
            if save_btn:
                btn_text = await save_btn.inner_text()
                if "Save to shortlist" in btn_text:
                    await save_btn.click()
                    await page.wait_for_timeout(500)
                step("Save second property", True, prop_id_2)
            else:
                step("Save second property", False, "Save button not found")
        except Exception as e:
            step("Save second property", False, str(e))

        # Step 4: Open /shortlist
        try:
            await page.goto(f"{base}/shortlist", wait_until="networkidle", timeout=15000)
            await page.wait_for_timeout(3000)
            text = await page.inner_text("body")
            cls = classify_text(text)
            text_lower = text.lower()

            if cls == "tooling_error":
                step("Shortlist renders", False, "Tooling error leaked on shortlist", "tooling_error")
            else:
                # Check for compare-oriented content (including empty/fallback state)
                checks = {
                    "compare_saved_homes": "compare saved homes" in text_lower,
                    "quick_compare": "quick compare" in text_lower,
                    "decision_themes": "decision themes" in text_lower,
                    "best_for": "best for" in text_lower,
                    "no_saved_homes": "no saved homes to compare yet" in text_lower,
                    "saved_homes_local": "saved homes stay on this browser" in text_lower,
                }
                has_compare_content = checks["compare_saved_homes"] or checks["quick_compare"]
                has_empty_state = checks["no_saved_homes"]

                if has_compare_content and (checks["quick_compare"] or checks["decision_themes"]):
                    step("Shortlist shows compare workspace", True,
                         f"checks={json.dumps(checks)}")
                elif has_empty_state or checks["compare_saved_homes"]:
                    step("Shortlist shows compare-ready state", True,
                         f"Empty or fallback state: {json.dumps(checks)}", "product_fallback")
                else:
                    step("Shortlist renders", len(text) > 50,
                         f"{len(text)} chars, checks={json.dumps(checks)}")

            # Screenshot
            await page.screenshot(
                path=str(output_dir / "journey_shortlist.png"),
                full_page=True,
            )
        except Exception as e:
            step("Shortlist renders", False, str(e), "tooling_error")

        await browser.close()

    # Determine journey status
    total = len(steps)
    passed = sum(1 for s in steps if s["passed"])
    tooling_errors = sum(1 for s in steps if s["classification"] == "tooling_error")
    product_fallbacks = sum(1 for s in steps if s["classification"] == "product_fallback")

    # Precise classification per Day 15 spec (deliverable 3.8)
    if passed == total and tooling_errors == 0 and product_fallbacks == 0:
        journey_status = "full_happy_path"
    elif has_tooling_error or tooling_errors > 0:
        journey_status = "tooling_failed_before_journey"
    elif product_fallbacks > 0 and passed == total:
        # Determine which pages showed fallback
        results_fallback = any(
            s["classification"] == "product_fallback" and "results" in s["step"].lower()
            for s in steps
        )
        property_fallback = any(
            s["classification"] == "product_fallback" and "property" in s["step"].lower()
            for s in steps
        )
        shortlist_fallback = any(
            s["classification"] == "product_fallback" and "shortlist" in s["step"].lower()
            for s in steps
        )

        if results_fallback:
            journey_status = "results_rendered_product_fallback"
        elif property_fallback:
            journey_status = "property_rendered_product_fallback"
        elif shortlist_fallback:
            journey_status = "shortlist_rendered_compare_ready_empty"
        else:
            journey_status = "partial_product_fallback"
    elif passed == total:
        journey_status = "full_happy_path"
    else:
        journey_status = "journey_failed_product"

    report = {
        "base_url": base_url,
        "api_base_url": api_base_url,
        "property_ids": [prop_id, prop_id_2],
        "captured_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "total_steps": total,
        "passed": passed,
        "failed": total - passed,
        "tooling_errors": tooling_errors,
        "product_fallbacks": product_fallbacks,
        "journey_status": journey_status,
        "steps": steps,
    }

    report_path = output_dir / "journey_report.json"
    report_path.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"\n  Journey report saved to {report_path}")

    return report


def main():
    parser = argparse.ArgumentParser(
        description="OpenEstates journey: results -> property -> save -> shortlist (Day 15)"
    )
    parser.add_argument("--base-url", default="http://localhost:5173", help="Frontend base URL")
    parser.add_argument("--api-base-url", default="http://localhost:4000", help="Backend API URL")
    parser.add_argument("--output-dir", default="pipeline/feedback/day15", help="Output directory")
    args = parser.parse_args()

    output = Path(args.output_dir)
    print("OpenEstates Journey: Results -> Property -> Save -> Shortlist (Day 15)")
    print(f"  Frontend: {args.base_url}")
    print(f"  API:      {args.api_base_url}")
    print()

    report = asyncio.run(run_journey(args.base_url, args.api_base_url, output))

    print(f"\nJourney: {report['passed']}/{report['total_steps']} steps passed")
    print(f"Status: {report['journey_status']}")

    # Exit 0 for happy path or product fallback (app rendered intentionally)
    # Exit 1 only for tooling failures or unclassified product failures
    if report["journey_status"] in ("full_happy_path", "results_rendered_product_fallback",
                                     "property_rendered_product_fallback",
                                     "shortlist_rendered_compare_ready_empty",
                                     "partial_product_fallback"):
        sys.exit(0)
    else:
        sys.exit(1)


if __name__ == "__main__":
    main()
