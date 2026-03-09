#!/usr/bin/env python3
"""
OpenEstates Render Truth Check (Day 15)

A lightweight verification that answers: "Did the app shell render?"
before any deeper UX review is attempted.

This script does NOT use Playwright for the initial check. It uses plain
HTTP to fetch the HTML and verify app-shell markers. An optional Playwright
pass confirms the SPA actually hydrates.

Checks performed:
  1. HTTP GET on each route returns 200
  2. HTML contains expected app-shell markers (root div, script tags, meta)
  3. HTML does NOT contain known tooling error signatures
  4. (Optional) Playwright hydration check: body text length > threshold

Output: pipeline/feedback/day15/render_truth.summary.json

Usage:
  python3 pipeline/check_render_truth.py
  python3 pipeline/check_render_truth.py --base-url http://localhost:5173
  python3 pipeline/check_render_truth.py --base-url http://localhost:5173 --with-hydration
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path
from urllib.request import urlopen, Request
from urllib.error import HTTPError, URLError


# ---------------------------------------------------------------------------
# App-shell markers we expect in the raw HTML of a Vite+React SPA
# ---------------------------------------------------------------------------

APP_SHELL_MARKERS = [
    '<div id="root"',       # React mount point
    "<script",              # JS bundle must be present
]

# If any of these appear in the raw HTML, it's a server-side or build error
TOOLING_ERROR_SIGNATURES = [
    "playwright sync api",
    "traceback (most recent",
    "runtimeerror",
    "econnrefused",
    "internal server error",
    "502 bad gateway",
    "503 service unavailable",
]

# OpenEstates-owned text that confirms the app is ours (not a generic error page)
OWNERSHIP_MARKERS = [
    "openestates",
    "open estates",
    "transparency",
    "bengaluru",
    "property",
]


# ---------------------------------------------------------------------------
# HTTP-based render truth check (no browser needed)
# ---------------------------------------------------------------------------

def check_route_html(base_url: str, path: str) -> dict:
    """Fetch raw HTML for a route and check app-shell markers."""
    url = base_url.rstrip("/") + path
    result = {
        "route": path,
        "url": url,
        "http_status": None,
        "has_app_shell": False,
        "has_scripts": False,
        "has_ownership_marker": False,
        "has_tooling_error": False,
        "html_length": 0,
        "render_truth": "unknown",
        "error": None,
        "checked_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }

    try:
        req = Request(url, headers={
            "Accept": "text/html",
            "User-Agent": "OpenEstates-RenderTruthCheck/1.0",
        })
        with urlopen(req, timeout=15) as resp:
            html = resp.read().decode("utf-8", errors="replace")
            result["http_status"] = resp.status
            result["html_length"] = len(html)
    except HTTPError as e:
        result["http_status"] = e.code
        result["render_truth"] = "http_error"
        result["error"] = f"HTTP {e.code}: {e.reason}"
        return result
    except URLError as e:
        result["render_truth"] = "unreachable"
        result["error"] = f"Connection failed: {e.reason}"
        return result
    except Exception as e:
        result["render_truth"] = "unreachable"
        result["error"] = str(e)
        return result

    html_lower = html.lower()

    # Check app-shell markers
    result["has_app_shell"] = '<div id="root"' in html_lower or '<div id="root"' in html
    result["has_scripts"] = "<script" in html_lower

    # Check ownership
    result["has_ownership_marker"] = any(m in html_lower for m in OWNERSHIP_MARKERS)

    # Check for tooling errors
    result["has_tooling_error"] = any(sig in html_lower for sig in TOOLING_ERROR_SIGNATURES)

    # Classify
    if result["has_tooling_error"]:
        result["render_truth"] = "tooling_error"
    elif not result["has_app_shell"]:
        result["render_truth"] = "no_app_shell"
    elif not result["has_scripts"]:
        result["render_truth"] = "no_scripts"
    elif result["http_status"] == 200 and result["has_app_shell"] and result["has_scripts"]:
        result["render_truth"] = "app_shell_present"
    else:
        result["render_truth"] = "unknown"

    return result


# ---------------------------------------------------------------------------
# Optional Playwright hydration check
# ---------------------------------------------------------------------------

async def check_hydration(base_url: str, routes: list[str]) -> list[dict]:
    """Use Playwright to verify the SPA actually hydrates and renders content."""
    from playwright.async_api import async_playwright

    results = []

    async with async_playwright() as p:
        browser = await p.chromium.launch(headless=True)
        context = await browser.new_context(viewport={"width": 1280, "height": 900})

        for path in routes:
            url = base_url.rstrip("/") + path
            page = await context.new_page()
            result = {
                "route": path,
                "url": url,
                "hydrated": False,
                "body_text_length": 0,
                "has_react_root_content": False,
                "error": None,
            }

            try:
                await page.goto(url, wait_until="networkidle", timeout=20000)
                await page.wait_for_timeout(3000)

                # Check if React root has content
                root = await page.query_selector("#root")
                if root:
                    inner = await root.inner_text()
                    result["body_text_length"] = len(inner.strip())
                    result["has_react_root_content"] = len(inner.strip()) > 20
                    result["hydrated"] = result["has_react_root_content"]
                else:
                    result["error"] = "No #root element found after hydration"

            except Exception as e:
                result["error"] = str(e)

            results.append(result)
            await page.close()

        await browser.close()

    return results


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description="OpenEstates Render Truth Check (Day 15)")
    parser.add_argument("--base-url", default="http://localhost:5173", help="Frontend URL")
    parser.add_argument("--output-dir", default="pipeline/feedback/day15", help="Output directory")
    parser.add_argument("--with-hydration", action="store_true", help="Also run Playwright hydration check")
    args = parser.parse_args()

    routes = ["/", "/results", "/property/prop_w_001", "/shortlist"]
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    print("OpenEstates Render Truth Check (Day 15)")
    print(f"  Base URL: {args.base_url}")
    print(f"  Routes:   {routes}")
    print()

    # Phase 1: HTTP-based checks
    print("Phase 1: HTTP app-shell verification")
    print("-" * 50)
    route_results = []
    all_have_shell = True

    for path in routes:
        result = check_route_html(args.base_url, path)
        route_results.append(result)

        truth = result["render_truth"]
        icon = "OK" if truth == "app_shell_present" else "FAIL"
        print(f"  [{icon}] {path} -> {truth} ({result['html_length']} bytes)")
        if result["error"]:
            print(f"        Error: {result['error']}")

        if truth != "app_shell_present":
            all_have_shell = False

    # Phase 2: Optional hydration check
    hydration_results = []
    all_hydrated = None

    if args.with_hydration:
        import asyncio
        print()
        print("Phase 2: Playwright hydration verification")
        print("-" * 50)
        hydration_results = asyncio.run(check_hydration(args.base_url, routes))
        all_hydrated = True

        for hr in hydration_results:
            icon = "OK" if hr["hydrated"] else "FAIL"
            print(f"  [{icon}] {hr['route']} -> hydrated={hr['hydrated']} ({hr['body_text_length']} chars)")
            if hr["error"]:
                print(f"        Error: {hr['error']}")
            if not hr["hydrated"]:
                all_hydrated = False

    # Determine overall render truth
    if not all_have_shell:
        overall = "app_shell_missing"
    elif args.with_hydration and not all_hydrated:
        overall = "hydration_failed"
    elif all_have_shell:
        overall = "app_shell_verified"
    else:
        overall = "unknown"

    # Build summary
    summary = {
        "render_truth_status": overall,
        "base_url": args.base_url,
        "checked_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "routes_checked": len(routes),
        "routes_with_app_shell": sum(1 for r in route_results if r["render_truth"] == "app_shell_present"),
        "routes_with_tooling_error": sum(1 for r in route_results if r["has_tooling_error"]),
        "hydration_checked": args.with_hydration,
        "routes_hydrated": sum(1 for h in hydration_results if h["hydrated"]) if hydration_results else None,
        "route_details": route_results,
        "hydration_details": hydration_results if hydration_results else None,
    }

    summary_path = output_dir / "render_truth.summary.json"
    summary_path.write_text(json.dumps(summary, indent=2), encoding="utf-8")

    print()
    print(f"Render truth: {overall}")
    print(f"Summary saved to {summary_path}")

    if overall in ("app_shell_verified",):
        print("\nRender truth PASSED — app shell is present on all routes.")
        sys.exit(0)
    else:
        print(f"\nRender truth FAILED — {overall}")
        sys.exit(1)


if __name__ == "__main__":
    main()
