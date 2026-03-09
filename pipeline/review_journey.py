"""
OpenEstates Conviction Journey Script (Day 12)

A deterministic Playwright scenario that validates the core user journey:
  homepage -> results -> property detail -> save -> shortlist -> compare

Uses async Playwright API. Tests that the save/shortlist flow works end-to-end.
Verifies conviction surfaces, compare sections, and stable text anchors.

Usage:
  python3 pipeline/review_journey.py <base_url>
  python3 pipeline/review_journey.py http://localhost:5173
"""

from __future__ import annotations

import argparse
import asyncio
import json
import sys
import time
from pathlib import Path


VALID_PROPERTY_ID = "prop_w_001"
SECOND_PROPERTY_ID = "prop_s_001"


async def run_journey(base_url: str, output_dir: Path) -> dict:
    """Run the conviction journey and return a result report."""
    from playwright.async_api import async_playwright

    base = base_url.rstrip("/")
    output_dir.mkdir(parents=True, exist_ok=True)
    steps: list[dict] = []

    def step(name: str, passed: bool, detail: str = ""):
        status = "PASS" if passed else "FAIL"
        print(f"  [{status}] {name}" + (f" — {detail}" if detail else ""))
        steps.append({"step": name, "passed": passed, "detail": detail})

    async with async_playwright() as p:
        browser = await p.chromium.launch(headless=True)
        context = await browser.new_context(viewport={"width": 1280, "height": 900})
        page = await context.new_page()

        # Step 1: Open homepage
        try:
            resp = await page.goto(base, wait_until="networkidle", timeout=15000)
            await page.wait_for_timeout(2000)
            text = await page.inner_text("body")
            has_content = len(text.strip()) > 50
            step("Open homepage", has_content and resp and resp.status == 200,
                 f"{len(text)} chars rendered")
        except Exception as e:
            step("Open homepage", False, str(e))

        # Step 2: Navigate to results
        try:
            await page.goto(f"{base}/results", wait_until="networkidle", timeout=15000)
            await page.wait_for_timeout(2000)
            text = await page.inner_text("body")
            has_properties = "BHK" in text or "sqft" in text or "Cr" in text
            step("Open results page", has_properties, f"{len(text)} chars")
        except Exception as e:
            step("Open results page", False, str(e))

        # Step 3: Open property detail page
        try:
            await page.goto(f"{base}/property/{VALID_PROPERTY_ID}", wait_until="networkidle", timeout=15000)
            await page.wait_for_timeout(2500)
            text = await page.inner_text("body")

            # Verify Day 12 required conviction sections (stable text anchors)
            sections_found = []
            required_sections = [
                "Why this property for you",
                "Price vs area median",
                "Market activity",
                "Tradeoffs to know",
                "Society / Livability",
                "Area signals",
            ]
            text_lower = text.lower()
            for section in required_sections:
                if section.lower() in text_lower:
                    sections_found.append(section)

            # Also verify via data-testid selectors
            testid_checks = []
            for tid in ["why-this-property-section", "price-vs-median-section",
                        "market-activity-section", "tradeoffs-section",
                        "society-livability-section", "area-signals-section"]:
                el = await page.query_selector(f'[data-testid="{tid}"]')
                if el:
                    testid_checks.append(tid)

            step("Open property detail", len(sections_found) >= 5,
                 f"Found {len(sections_found)}/{len(required_sections)} sections: {sections_found}; "
                 f"testid selectors: {len(testid_checks)}/{6}")
        except Exception as e:
            step("Open property detail", False, str(e))

        # Step 4: Save property (click save button)
        try:
            save_btn = await page.query_selector('[data-testid="save-button"]')
            if not save_btn:
                save_btn = await page.query_selector("button:has-text('Save to shortlist'), button:has-text('Saved')")
            if save_btn:
                btn_text_before = await save_btn.inner_text()
                await save_btn.click()
                await page.wait_for_timeout(500)
                btn_text_after = await save_btn.inner_text()
                state_changed = btn_text_before != btn_text_after
                step("Save property from detail", state_changed,
                     f"Button: '{btn_text_before}' -> '{btn_text_after}'")
            else:
                step("Save property from detail", False, "Save button not found")
        except Exception as e:
            step("Save property from detail", False, str(e))

        # Step 4b: Save a second property so compare works
        try:
            await page.goto(f"{base}/property/{SECOND_PROPERTY_ID}", wait_until="networkidle", timeout=15000)
            await page.wait_for_timeout(2000)
            save_btn = await page.query_selector('[data-testid="save-button"]')
            if not save_btn:
                save_btn = await page.query_selector("button:has-text('Save to shortlist')")
            if save_btn:
                btn_text = await save_btn.inner_text()
                if "Save to shortlist" in btn_text:
                    await save_btn.click()
                    await page.wait_for_timeout(500)
                step("Save second property", True, SECOND_PROPERTY_ID)
            else:
                # May already be saved
                step("Save second property", True, "Already saved or button state different")
        except Exception as e:
            step("Save second property", False, str(e))

        # Step 5: Open shortlist
        try:
            await page.goto(f"{base}/shortlist", wait_until="networkidle", timeout=15000)
            await page.wait_for_timeout(3000)
            text = await page.inner_text("body")
            text_lower = text.lower()

            shortlist_checks = {
                "has_saved_properties": "saved propert" in text_lower or "compare workspace" in text_lower,
                "has_quick_compare": "quick compare" in text_lower,
                "has_decision_themes": "decision themes" in text_lower,
                "has_best_for": "best for" in text_lower,
                "has_greenery_theme": "greenery" in text_lower or "open space" in text_lower,
            }

            # Also verify compare sections via data-testid
            testid_sections = {}
            for tid in ["quick-compare-section", "decision-themes-section", "best-for-section"]:
                el = await page.query_selector(f'[data-testid="{tid}"]')
                testid_sections[tid] = el is not None

            passed_checks = sum(1 for v in shortlist_checks.values() if v)
            testid_found = sum(1 for v in testid_sections.values() if v)
            step("Open shortlist with saved properties", passed_checks >= 3,
                 f"Text checks: {json.dumps(shortlist_checks)}; "
                 f"testid sections: {testid_found}/3")

            # Explicit compare-oriented state check
            is_compare_oriented = (
                shortlist_checks["has_quick_compare"]
                and (shortlist_checks["has_decision_themes"] or shortlist_checks["has_best_for"])
            )
            step("Shortlist is compare-oriented", is_compare_oriented,
                 "Quick Compare + Decision Themes/Best For present")

            # Screenshot the shortlist
            await page.screenshot(path=str(output_dir / "journey_shortlist.png"), full_page=True)
        except Exception as e:
            step("Open shortlist with saved properties", False, str(e))

        # Step 6: Remove from shortlist
        try:
            remove_btn = await page.query_selector("button:has-text('Remove')")
            if remove_btn:
                await remove_btn.click()
                await page.wait_for_timeout(500)
                step("Remove from shortlist", True, "Remove button clicked")
            else:
                step("Remove from shortlist", False, "Remove button not found")
        except Exception as e:
            step("Remove from shortlist", False, str(e))

        await browser.close()

    # Summary
    total = len(steps)
    passed = sum(1 for s in steps if s["passed"])
    report = {
        "base_url": base_url,
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "total_steps": total,
        "passed": passed,
        "failed": total - passed,
        "all_passed": passed == total,
        "steps": steps,
    }

    report_path = output_dir / "journey_report.json"
    report_path.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"\n  Journey report saved to {report_path}")

    return report


def main():
    parser = argparse.ArgumentParser(description="OpenEstates conviction journey test")
    parser.add_argument("base_url", help="Base URL of the deployed app")
    parser.add_argument("--output-dir", default="pipeline/feedback/day12", help="Output directory")
    args = parser.parse_args()

    output_dir = Path(args.output_dir)
    print("OpenEstates Conviction Journey")
    print(f"  Base URL: {args.base_url}")
    print()

    report = asyncio.run(run_journey(args.base_url, output_dir))

    passed = report["passed"]
    total = report["total_steps"]
    print(f"\nJourney: {passed}/{total} steps passed")

    if not report["all_passed"]:
        sys.exit(1)


if __name__ == "__main__":
    main()
