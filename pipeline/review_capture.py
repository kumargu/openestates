"""
OpenEstates Review Capture Pipeline (Day 12)

Async Playwright-based page capture for deployed SPA pages.
Captures screenshots, rendered text, and status metadata for each page.
Classifies each capture into: ok, product_fallback, tooling_error,
navigation_error, or render_error.

Usage:
  python3 pipeline/review_capture.py <base_url> [--output-dir pipeline/feedback/day12]

Example:
  python3 pipeline/review_capture.py http://localhost:5173
  python3 pipeline/review_capture.py https://openestates-xxx.vercel.app
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys
import time
from dataclasses import dataclass, asdict
from enum import Enum
from pathlib import Path


class CaptureStatus(str, Enum):
    OK = "ok"
    PRODUCT_FALLBACK = "product_fallback"
    TOOLING_ERROR = "tooling_error"
    NAVIGATION_ERROR = "navigation_error"
    RENDER_ERROR = "render_error"


@dataclass
class PageCapture:
    page: str
    url: str
    capture_status: str
    http_status: int | None
    text_length: int
    screenshot_saved: bool
    error: str | None
    rendered_text: str
    captured_at: str = ""


# Patterns that indicate an intentional product fallback state (not a bug)
PRODUCT_FALLBACK_PATTERNS = [
    "was not found",
    "not found",
    "no properties",
    "save homes to compare",
    "browse properties",
]


CAPTURE_TARGETS = [
    ("landing", "/"),
    ("results", "/results"),
    ("property_prop_w_001", "/property/prop_w_001"),
    ("property_invalid", "/property/NONEXISTENT_ID"),
    ("shortlist", "/shortlist"),
]


async def capture_page(
    page_obj,
    name: str,
    url: str,
    output_dir: Path,
) -> PageCapture:
    """Capture a single page: navigate, wait for render, save screenshot + text."""
    screenshot_path = output_dir / f"{name}.png"
    text_path = output_dir / f"{name}.txt"

    try:
        response = await page_obj.goto(url, wait_until="networkidle", timeout=20000)
        http_status = response.status if response else None
    except Exception as e:
        return PageCapture(
            page=name,
            url=url,
            capture_status=CaptureStatus.NAVIGATION_ERROR,
            http_status=None,
            text_length=0,
            screenshot_saved=False,
            error=f"Navigation failed: {e}",
            rendered_text="",
        )

    # Wait for React hydration
    await page_obj.wait_for_timeout(2500)

    # Preflight: check DOM is present and body has content
    try:
        body_exists = await page_obj.query_selector("body")
        if not body_exists:
            return PageCapture(
                page=name,
                url=url,
                capture_status=CaptureStatus.RENDER_ERROR,
                http_status=http_status,
                text_length=0,
                screenshot_saved=False,
                error="No <body> element found",
                rendered_text="",
            )

        rendered_text = await page_obj.inner_text("body")
    except Exception as e:
        return PageCapture(
            page=name,
            url=url,
            capture_status=CaptureStatus.TOOLING_ERROR,
            http_status=http_status,
            text_length=0,
            screenshot_saved=False,
            error=f"Text extraction failed: {e}",
            rendered_text="",
        )

    # Check for runtime crash overlay (Vite/React error overlay)
    has_crash_overlay = False
    try:
        crash_el = await page_obj.query_selector("vite-error-overlay, #react-error-overlay")
        if crash_el:
            has_crash_overlay = True
    except Exception:
        pass

    # Classify the capture status
    text_length = len(rendered_text.strip())
    if has_crash_overlay:
        capture_status = CaptureStatus.RENDER_ERROR
        error_msg = "Runtime crash overlay detected"
    elif text_length < 10:
        capture_status = CaptureStatus.RENDER_ERROR
        error_msg = f"Body text too short ({text_length} chars) — SPA may not have rendered"
    else:
        # Check if this is an intentional product fallback state
        text_lower = rendered_text.lower()
        is_fallback = any(pattern in text_lower for pattern in PRODUCT_FALLBACK_PATTERNS)
        if is_fallback and name in ("property_invalid", "shortlist"):
            capture_status = CaptureStatus.PRODUCT_FALLBACK
        else:
            capture_status = CaptureStatus.OK
        error_msg = None

    # Save screenshot
    screenshot_saved = False
    try:
        await page_obj.screenshot(path=str(screenshot_path), full_page=True)
        screenshot_saved = True
    except Exception as e:
        error_msg = error_msg or f"Screenshot failed: {e}"

    # Save rendered text
    text_path.write_text(rendered_text[:8000], encoding="utf-8")

    return PageCapture(
        page=name,
        url=url,
        capture_status=capture_status,
        http_status=http_status,
        text_length=text_length,
        screenshot_saved=screenshot_saved,
        error=error_msg,
        rendered_text=rendered_text[:5000],
        captured_at=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    )


async def run_capture(
    base_url: str,
    output_dir: Path,
    targets: list[tuple[str, str]] | None = None,
) -> list[PageCapture]:
    """Run the full capture pipeline using async Playwright."""
    from playwright.async_api import async_playwright

    if targets is None:
        targets = CAPTURE_TARGETS

    output_dir.mkdir(parents=True, exist_ok=True)
    results: list[PageCapture] = []

    async with async_playwright() as p:
        browser = await p.chromium.launch(headless=True)
        context = await browser.new_context(viewport={"width": 1280, "height": 900})

        for name, path in targets:
            url = base_url.rstrip("/") + path
            page = await context.new_page()
            print(f"  Capturing: {name} -> {url}")

            capture = await capture_page(page, name, url, output_dir)
            results.append(capture)

            if capture.capture_status == CaptureStatus.OK:
                status_icon = "OK"
            elif capture.capture_status == CaptureStatus.PRODUCT_FALLBACK:
                status_icon = "FALLBACK"
            else:
                status_icon = "FAIL"
            print(f"    [{status_icon}] {capture.text_length} chars, screenshot={capture.screenshot_saved}")
            if capture.error:
                print(f"    Error: {capture.error}")

            await page.close()

        await browser.close()

    # Save per-page status JSON
    for cap in results:
        status_path = output_dir / f"{cap.page}.status.json"
        status_data = {
            "page_name": cap.page,
            "url": cap.url,
            "capture_status": cap.capture_status,
            "http_status": cap.http_status,
            "text_length": cap.text_length,
            "has_screenshot": cap.screenshot_saved,
            "has_rendered_text": cap.text_length > 0,
            "error": cap.error,
            "captured_at": cap.captured_at,
        }
        status_path.write_text(json.dumps(status_data, indent=2), encoding="utf-8")

    # Save summary JSON
    summary = {
        "base_url": base_url,
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "total_pages": len(results),
        "ok_count": sum(1 for r in results if r.capture_status in (CaptureStatus.OK, CaptureStatus.PRODUCT_FALLBACK)),
        "fallback_count": sum(1 for r in results if r.capture_status == CaptureStatus.PRODUCT_FALLBACK),
        "failed_count": sum(1 for r in results if r.capture_status not in (CaptureStatus.OK, CaptureStatus.PRODUCT_FALLBACK)),
        "pages": [
            {
                "page": r.page,
                "capture_status": r.capture_status,
                "http_status": r.http_status,
                "text_length": r.text_length,
                "screenshot_saved": r.screenshot_saved,
                "error": r.error,
            }
            for r in results
        ],
    }
    summary_path = output_dir / "capture_summary.json"
    summary_path.write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(f"\n  Summary saved to {summary_path}")

    return results


def main():
    parser = argparse.ArgumentParser(description="OpenEstates page capture pipeline")
    parser.add_argument("base_url", help="Base URL of the deployed app")
    parser.add_argument("--output-dir", default="pipeline/feedback/day12", help="Output directory for artifacts")
    args = parser.parse_args()

    output_dir = Path(args.output_dir)
    print(f"OpenEstates Review Capture")
    print(f"  Base URL: {args.base_url}")
    print(f"  Output:   {output_dir}")
    print()

    results = asyncio.run(run_capture(args.base_url, output_dir))

    ok = sum(1 for r in results if r.capture_status in (CaptureStatus.OK, CaptureStatus.PRODUCT_FALLBACK))
    fallbacks = sum(1 for r in results if r.capture_status == CaptureStatus.PRODUCT_FALLBACK)
    total = len(results)
    print(f"\nDone: {ok}/{total} pages captured successfully ({fallbacks} product fallback states)")

    if ok < total:
        sys.exit(1)


if __name__ == "__main__":
    main()
