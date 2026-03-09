"""
OpenEstates Standalone Page Capture (Day 15)

Minimal, standalone deployed-page capture script. Uses only async Playwright API
end-to-end to avoid the sync/async runtime conflict seen in Days 10-12.

Day 15 additions:
  - Explicit tooling error signature detection (Playwright Sync API, Traceback, RuntimeError)
  - Explicit product fallback signature detection (stable headings from frontend)
  - contains_tooling_error_signature and contains_product_fallback_heading in status JSON
  - Stronger classification logic that never confuses the two

For each target page, saves:
  - screenshot (.png)
  - rendered text (.txt)
  - status JSON (.status.json)

Usage:
  python3 pipeline/capture_deployed_pages.py --base-url http://localhost:5173
  python3 pipeline/capture_deployed_pages.py --base-url https://openestates-xxx.vercel.app
"""

from __future__ import annotations

import argparse
import asyncio
import json
import sys
import time
from pathlib import Path


# ---------------------------------------------------------------------------
# Signature detection
# ---------------------------------------------------------------------------

TOOLING_ERROR_SIGNATURES = [
    "playwright sync api",
    "traceback (most recent",
    "runtimeerror",
    "unexpected error",
    "econnrefused",
    "unhandledrejection",
    "module not found",
    "syntaxerror",
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


def detect_signatures(text: str) -> tuple[bool, bool]:
    """Return (has_tooling_error, has_product_fallback) from rendered text."""
    lower = text.lower()
    has_tooling = any(sig in lower for sig in TOOLING_ERROR_SIGNATURES)
    has_fallback = any(sig in lower for sig in PRODUCT_FALLBACK_SIGNATURES)
    return has_tooling, has_fallback


# ---------------------------------------------------------------------------
# Resolve a valid property ID dynamically from the API
# ---------------------------------------------------------------------------

async def resolve_valid_property_id(api_base_url: str) -> str | None:
    """Fetch /api/properties and return the first property ID, or None."""
    import urllib.request
    import urllib.error

    url = api_base_url.rstrip("/") + "/api/properties"
    try:
        req = urllib.request.Request(url, headers={"Accept": "application/json"})
        with urllib.request.urlopen(req, timeout=10) as resp:
            data = json.loads(resp.read().decode())
            if isinstance(data, list) and len(data) > 0:
                return data[0].get("id")
    except Exception as e:
        print(f"  WARNING: Could not resolve property ID from API ({api_base_url}): {e}")
    return None


# ---------------------------------------------------------------------------
# Capture a single page
# ---------------------------------------------------------------------------

async def capture_page(
    page_obj,
    name: str,
    url: str,
    output_dir: Path,
) -> dict:
    """Navigate to URL, wait for render, save screenshot + text + status."""
    screenshot_path = output_dir / f"{name}.png"
    text_path = output_dir / f"{name}.txt"
    status_path = output_dir / f"{name}.status.json"

    result = {
        "page_name": name,
        "url": url,
        "capture_status": "ok",
        "http_status": None,
        "text_length": 0,
        "has_screenshot": False,
        "has_rendered_text": False,
        "contains_tooling_error_signature": False,
        "contains_product_fallback_heading": False,
        "error": None,
        "captured_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }

    # --- Navigate ---
    try:
        response = await page_obj.goto(url, wait_until="networkidle", timeout=20000)
        result["http_status"] = response.status if response else None
    except Exception as e:
        result["capture_status"] = "navigation_error"
        result["error"] = f"Navigation failed: {e}"
        status_path.write_text(json.dumps(result, indent=2), encoding="utf-8")
        return result

    # Wait for React hydration
    await page_obj.wait_for_timeout(2500)

    # --- Preflight: body present? ---
    try:
        body = await page_obj.query_selector("body")
        if not body:
            result["capture_status"] = "render_error"
            result["error"] = "No <body> element found"
            status_path.write_text(json.dumps(result, indent=2), encoding="utf-8")
            return result
    except Exception as e:
        result["capture_status"] = "tooling_error"
        result["error"] = f"Preflight failed: {e}"
        status_path.write_text(json.dumps(result, indent=2), encoding="utf-8")
        return result

    # --- Extract text ---
    try:
        rendered_text = await page_obj.inner_text("body")
    except Exception as e:
        result["capture_status"] = "tooling_error"
        result["error"] = f"Text extraction failed: {e}"
        status_path.write_text(json.dumps(result, indent=2), encoding="utf-8")
        return result

    # --- Check for crash overlay ---
    try:
        crash = await page_obj.query_selector("vite-error-overlay, #react-error-overlay")
        if crash:
            result["capture_status"] = "render_error"
            result["error"] = "Runtime crash overlay detected"
    except Exception:
        pass

    # --- Classify using signature detection ---
    text_length = len(rendered_text.strip())
    result["text_length"] = text_length

    has_tooling, has_fallback = detect_signatures(rendered_text)
    result["contains_tooling_error_signature"] = has_tooling
    result["contains_product_fallback_heading"] = has_fallback

    if result["capture_status"] not in ("navigation_error", "render_error", "tooling_error"):
        if text_length < 10:
            result["capture_status"] = "render_error"
            result["error"] = f"Body text too short ({text_length} chars) — SPA may not have rendered"
        elif has_tooling:
            # Tooling error takes priority — this is NOT a product state
            result["capture_status"] = "tooling_error"
            result["error"] = "Leaked tooling error signature detected in rendered text"
        elif has_fallback:
            result["capture_status"] = "product_fallback"
        else:
            result["capture_status"] = "ok"

    # --- Save screenshot ---
    try:
        await page_obj.screenshot(path=str(screenshot_path), full_page=True)
        result["has_screenshot"] = True
    except Exception as e:
        result["error"] = result.get("error") or f"Screenshot failed: {e}"

    # --- Save rendered text ---
    text_path.write_text(rendered_text[:8000], encoding="utf-8")
    result["has_rendered_text"] = text_length > 0

    # --- Save status JSON ---
    status_path.write_text(json.dumps(result, indent=2), encoding="utf-8")

    return result


# ---------------------------------------------------------------------------
# Main capture runner
# ---------------------------------------------------------------------------

async def run_capture(base_url: str, output_dir: Path, api_base_url: str | None = None):
    """Launch browser, capture all target pages, save artifacts."""
    from playwright.async_api import async_playwright

    output_dir.mkdir(parents=True, exist_ok=True)

    # Resolve valid property ID dynamically
    api_url = api_base_url or "http://localhost:4000"
    print(f"  Resolving valid property ID from {api_url}...")
    valid_id = await resolve_valid_property_id(api_url)
    if valid_id:
        print(f"  Resolved: {valid_id}")
    else:
        valid_id = "prop_w_001"
        print(f"  Fallback to hardcoded: {valid_id}")

    # Build capture targets
    targets = [
        ("landing", "/"),
        ("results", "/results"),
        ("property_valid", f"/property/{valid_id}"),
        ("property_invalid", "/property/does-not-exist"),
        ("shortlist", "/shortlist"),
    ]

    results = []

    async with async_playwright() as p:
        browser = await p.chromium.launch(headless=True)
        context = await browser.new_context(viewport={"width": 1280, "height": 900})

        for name, path in targets:
            url = base_url.rstrip("/") + path
            page = await context.new_page()
            print(f"  Capturing: {name} -> {url}")

            result = await capture_page(page, name, url, output_dir)
            results.append(result)

            status = result["capture_status"]
            icon = "OK" if status == "ok" else ("FALLBACK" if status == "product_fallback" else "FAIL")
            extra = ""
            if result["contains_tooling_error_signature"]:
                extra += " [TOOLING_ERROR_LEAKED]"
            if result["contains_product_fallback_heading"]:
                extra += " [PRODUCT_FALLBACK]"
            print(f"    [{icon}] {result['text_length']} chars{extra}")
            if result["error"]:
                print(f"    Error: {result['error']}")

            await page.close()

        await browser.close()

    # Save summary
    summary = {
        "base_url": base_url,
        "api_base_url": api_url,
        "resolved_property_id": valid_id,
        "captured_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "total_pages": len(results),
        "ok_count": sum(1 for r in results if r["capture_status"] == "ok"),
        "fallback_count": sum(1 for r in results if r["capture_status"] == "product_fallback"),
        "tooling_error_count": sum(1 for r in results if r["capture_status"] == "tooling_error"),
        "navigation_error_count": sum(1 for r in results if r["capture_status"] == "navigation_error"),
        "render_error_count": sum(1 for r in results if r["capture_status"] == "render_error"),
        "pages": results,
    }
    summary_path = output_dir / "capture_summary.json"
    summary_path.write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(f"\n  Summary saved to {summary_path}")

    return summary


def main():
    parser = argparse.ArgumentParser(
        description="OpenEstates standalone page capture (Day 15)"
    )
    parser.add_argument(
        "--base-url",
        default="http://localhost:5173",
        help="Base URL of the frontend app (default: http://localhost:5173)",
    )
    parser.add_argument(
        "--api-base-url",
        default="http://localhost:4000",
        help="Backend API URL for resolving property IDs (default: http://localhost:4000)",
    )
    parser.add_argument(
        "--output-dir",
        default="pipeline/feedback/day15",
        help="Output directory for artifacts",
    )
    args = parser.parse_args()

    output = Path(args.output_dir)
    print("OpenEstates Standalone Page Capture (Day 15)")
    print(f"  Frontend: {args.base_url}")
    print(f"  API:      {args.api_base_url}")
    print(f"  Output:   {output}")
    print()

    summary = asyncio.run(run_capture(args.base_url, output, args.api_base_url))

    ok = summary["ok_count"]
    fb = summary["fallback_count"]
    total = summary["total_pages"]
    print(f"\nDone: {ok} ok, {fb} fallback, {total - ok - fb} failed out of {total} pages")

    if (ok + fb) < total:
        sys.exit(1)


if __name__ == "__main__":
    main()
