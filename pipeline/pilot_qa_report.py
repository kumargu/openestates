#!/usr/bin/env python3.11
"""
OpenEstates pilot QA report.

Runs the post-DAG promotion checks we care about for a scoped pilot:
- API coverage for expected property pages.
- Buyer evidence presence: RERA, reviews, map context, and water evidence.
- Media promotion checks: local hero only, no known thumbnail/watermark markers.
- Headless Chrome render checks against the frontend route.

No third-party Python packages are required.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


DEFAULT_SLUGS = [
    "discovered-assetz-marq-3bhk",
    "discovered-godrej-splendour-3bhk",
    "discovered-godrej-air-3bhk",
    "discovered-godrej-woodscapes-3bhk",
    "discovered-prestige-waterford-3bhk",
    "discovered-prestige-lakeside-habitat-3bhk",
    "discovered-sbr-one-residence-3bhk",
    "discovered-sumadhura-capitol-residences-3bhk",
    "discovered-godrej-united-3bhk",
    "discovered-candeur-signature-3bhk",
]

BAD_MEDIA_MARKERS = [
    "photo_h180_w240",
    "photo_h300_w450",
    "thumb",
    "thumbnail",
    "sprite",
    "logo",
    "qr",
    "squareyards",
    "square yards",
    "watermark",
]

RENDER_ERROR_MARKERS = [
    "application error",
    "cannot read properties of",
    "undefined is not",
    "null is not",
    "failed to fetch",
    "vite error",
    "stack trace",
    "traceback",
    "![image",
]


@dataclass
class ApiCheck:
    slug: str
    passed: bool
    status_code: int | None = None
    title: str | None = None
    hero_image: str | None = None
    hero_width: int | None = None
    hero_height: int | None = None
    has_rera: bool = False
    has_reviews: bool = False
    has_map_context: bool = False
    has_water_evidence: bool = False
    hero_is_local: bool = False
    has_bad_media_marker: bool = False
    failures: list[str] = field(default_factory=list)


@dataclass
class RenderCheck:
    slug: str
    passed: bool
    url: str
    dom_bytes: int = 0
    title_match: str | None = None
    failures: list[str] = field(default_factory=list)
    artifact_html: str | None = None
    artifact_stderr: str | None = None


def api_get_json(api_base_url: str, path: str) -> tuple[int, Any]:
    url = api_base_url.rstrip("/") + path
    req = Request(url, headers={"Accept": "application/json"})
    try:
        with urlopen(req, timeout=10) as resp:
            return resp.status, json.loads(resp.read().decode())
    except HTTPError as exc:
        try:
            body = json.loads(exc.read().decode())
        except Exception:
            body = None
        return exc.code, body
    except URLError as exc:
        return 0, {"error": str(exc.reason)}


def frontend_public_path(project_root: Path, image_url: str) -> Path | None:
    if not image_url.startswith("/"):
        return None
    relative = image_url.lstrip("/")
    if relative.startswith("media/"):
        return project_root / relative
    return project_root / "frontend" / "public" / relative


def image_dimensions(path: Path) -> tuple[int, int] | None:
    try:
        with path.open("rb") as fh:
            header = fh.read(64)
            if header.startswith(b"\x89PNG\r\n\x1a\n"):
                return int.from_bytes(header[16:20], "big"), int.from_bytes(header[20:24], "big")
            if header.startswith(b"RIFF") and header[8:12] == b"WEBP":
                if header[12:16] == b"VP8X":
                    width = int.from_bytes(header[24:27], "little") + 1
                    height = int.from_bytes(header[27:30], "little") + 1
                    return width, height
                if header[12:16] == b"VP8 ":
                    fh.seek(26)
                    raw = fh.read(4)
                    return int.from_bytes(raw[0:2], "little") & 0x3FFF, int.from_bytes(raw[2:4], "little") & 0x3FFF
            if header.startswith(b"\xff\xd8"):
                fh.seek(2)
                while True:
                    marker_start = fh.read(1)
                    if not marker_start:
                        return None
                    if marker_start != b"\xff":
                        continue
                    marker = fh.read(1)
                    while marker == b"\xff":
                        marker = fh.read(1)
                    if marker in {b"\xc0", b"\xc1", b"\xc2", b"\xc3", b"\xc5", b"\xc6", b"\xc7", b"\xc9", b"\xca", b"\xcb", b"\xcd", b"\xce", b"\xcf"}:
                        length = int.from_bytes(fh.read(2), "big")
                        segment = fh.read(length - 2)
                        return int.from_bytes(segment[3:5], "big"), int.from_bytes(segment[1:3], "big")
                    if marker in {b"\xd8", b"\xd9"}:
                        continue
                    length = int.from_bytes(fh.read(2), "big")
                    fh.seek(length - 2, 1)
    except OSError:
        return None
    return None


def run_api_check(project_root: Path, api_base_url: str, slug: str, min_hero_width: int) -> ApiCheck:
    status, payload = api_get_json(api_base_url, f"/api/properties/{slug}")
    check = ApiCheck(slug=slug, passed=False, status_code=status)
    if status != 200 or not isinstance(payload, dict):
        check.failures.append(f"property_api_status:{status}")
        return check

    prop = payload.get("property") or {}
    check.title = prop.get("title")
    check.hero_image = prop.get("hero_image") or prop.get("heroImage")
    check.has_rera = bool(payload.get("rera"))
    check.has_reviews = bool(payload.get("external_reviews"))
    check.has_map_context = bool(payload.get("map_context"))
    payload_text = json.dumps(payload).lower()
    check.has_water_evidence = "groundwater" in payload_text or "water" in payload_text

    media_text = json.dumps(
        {key: value for key, value in prop.items() if "image" in key.lower() or key in {"images", "gallery"}},
        sort_keys=True,
    ).lower()
    check.has_bad_media_marker = any(marker in media_text for marker in BAD_MEDIA_MARKERS)
    check.hero_is_local = bool(check.hero_image) and str(check.hero_image).startswith("/")

    if not check.hero_image:
        check.failures.append("missing_hero_image")
    elif not check.hero_is_local:
        check.failures.append("hero_not_local")
    else:
        local_path = frontend_public_path(project_root, str(check.hero_image))
        if local_path is None or not local_path.exists():
            check.failures.append("hero_file_missing")
        else:
            dims = image_dimensions(local_path)
            if dims is None:
                check.failures.append("hero_dimensions_unreadable")
            else:
                check.hero_width, check.hero_height = dims
                if check.hero_width < min_hero_width:
                    check.failures.append(f"hero_width_below_{min_hero_width}")

    if not check.has_rera:
        check.failures.append("missing_rera")
    if not check.has_reviews:
        check.failures.append("missing_external_reviews")
    if not check.has_map_context:
        check.failures.append("missing_map_context")
    if not check.has_water_evidence:
        check.failures.append("missing_water_evidence")
    if check.has_bad_media_marker:
        check.failures.append("bad_media_marker")

    check.passed = not check.failures
    return check


def find_chrome(explicit_path: str | None) -> str | None:
    if explicit_path:
        return explicit_path
    for candidate in ["google-chrome", "chromium", "chromium-browser"]:
        result = subprocess.run(["which", candidate], text=True, capture_output=True, check=False)
        if result.returncode == 0:
            return result.stdout.strip()
    return None


def run_render_check(
    frontend_base_url: str,
    slug: str,
    output_dir: Path,
    chrome_path: str,
    min_dom_bytes: int,
) -> RenderCheck:
    url = f"{frontend_base_url.rstrip('/')}/property/{slug}"
    html_path = output_dir / f"{slug}.html"
    err_path = output_dir / f"{slug}.err"
    result = subprocess.run(
        [
            chrome_path,
            "--headless=new",
            "--no-sandbox",
            "--disable-gpu",
            "--virtual-time-budget=7000",
            "--window-size=1440,1200",
            "--dump-dom",
            url,
        ],
        text=True,
        capture_output=True,
        check=False,
        timeout=25,
    )
    html_path.write_text(result.stdout, encoding="utf-8")
    err_path.write_text(result.stderr, encoding="utf-8")

    text = f"{result.stdout}\n{result.stderr}"
    check = RenderCheck(
        slug=slug,
        passed=False,
        url=url,
        dom_bytes=len(result.stdout.encode("utf-8")),
        artifact_html=str(html_path),
        artifact_stderr=str(err_path),
    )
    title_match = re.search(r"([1-4] BHK in [^<]+|Not Found)", result.stdout)
    check.title_match = title_match.group(1) if title_match else None

    if result.returncode != 0:
        check.failures.append(f"chrome_exit:{result.returncode}")
    if check.dom_bytes < min_dom_bytes:
        check.failures.append(f"dom_bytes_below_{min_dom_bytes}")
    if not check.title_match:
        check.failures.append("missing_expected_title")
    lower = text.lower()
    for marker in RENDER_ERROR_MARKERS:
        if marker in lower:
            check.failures.append(f"render_marker:{marker}")
            break

    check.passed = not check.failures
    return check


def load_slugs(args: argparse.Namespace) -> list[str]:
    if args.slugs:
        return [slug.strip() for slug in args.slugs.split(",") if slug.strip()]
    if args.slugs_file:
        payload = json.loads(Path(args.slugs_file).read_text(encoding="utf-8"))
        if isinstance(payload, list):
            return [str(item) for item in payload]
        if isinstance(payload, dict) and isinstance(payload.get("slugs"), list):
            return [str(item) for item in payload["slugs"]]
        raise ValueError("--slugs-file must contain a JSON array or {'slugs': [...]}")
    return DEFAULT_SLUGS


def write_markdown_report(path: Path, report: dict[str, Any]) -> None:
    lines = [
        f"# OpenEstates Pilot QA - {report['run_id']}",
        "",
        f"- API base: `{report['api_base_url']}`",
        f"- Frontend base: `{report['frontend_base_url']}`",
        f"- Properties checked: `{report['summary']['property_count']}`",
        f"- API pass: `{report['summary']['api_pass_count']}`",
        f"- Render pass: `{report['summary']['render_pass_count']}`",
        f"- Overall status: `{'PASS' if report['passed'] else 'FAIL'}`",
        "",
        "## Failures",
    ]
    failures = []
    for check in report["api_checks"]:
        for failure in check["failures"]:
            failures.append(f"- API `{check['slug']}`: {failure}")
    for check in report["render_checks"]:
        for failure in check["failures"]:
            failures.append(f"- Render `{check['slug']}`: {failure}")
    lines.extend(failures or ["- None"])
    lines.extend(["", "## Properties"])
    for api, render in zip(report["api_checks"], report["render_checks"]):
        lines.append(
            f"- `{api['slug']}`: api={'PASS' if api['passed'] else 'FAIL'}, "
            f"render={'PASS' if render['passed'] else 'FAIL'}, "
            f"hero=`{api.get('hero_image')}`, title=`{api.get('title')}`"
        )
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description="Run scoped OpenEstates pilot QA checks.")
    parser.add_argument("--api-base-url", default="http://localhost:4000")
    parser.add_argument("--frontend-base-url", default="http://localhost:5173")
    parser.add_argument("--project-root", default=str(Path(__file__).resolve().parents[1]))
    parser.add_argument("--output-dir", default="data/validation/pilot_qa")
    parser.add_argument("--run-id", default=time.strftime("pilot-qa-%Y%m%d-%H%M%S", time.gmtime()))
    parser.add_argument("--slugs", help="Comma-separated property slugs. Defaults to the 10-property pilot.")
    parser.add_argument("--slugs-file", help="JSON array or {'slugs': [...]} file.")
    parser.add_argument("--chrome-path", help="Path to google-chrome/chromium.")
    parser.add_argument("--min-hero-width", type=int, default=900)
    parser.add_argument("--min-dom-bytes", type=int, default=50_000)
    parser.add_argument("--skip-browser", action="store_true")
    args = parser.parse_args()

    project_root = Path(args.project_root).resolve()
    output_root = (project_root / args.output_dir).resolve()
    run_dir = output_root / args.run_id
    run_dir.mkdir(parents=True, exist_ok=True)
    slugs = load_slugs(args)

    chrome_path = None if args.skip_browser else find_chrome(args.chrome_path)
    if not args.skip_browser and not chrome_path:
        print("FAIL: google-chrome/chromium not found. Use --skip-browser to run API-only checks.", file=sys.stderr)
        return 2

    api_checks = [run_api_check(project_root, args.api_base_url, slug, args.min_hero_width) for slug in slugs]
    render_checks: list[RenderCheck] = []
    if not args.skip_browser:
        render_dir = run_dir / "render"
        render_dir.mkdir(parents=True, exist_ok=True)
        render_checks = [
            run_render_check(args.frontend_base_url, slug, render_dir, str(chrome_path), args.min_dom_bytes)
            for slug in slugs
        ]

    passed = all(check.passed for check in api_checks) and all(check.passed for check in render_checks)
    report = {
        "run_id": args.run_id,
        "created_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "api_base_url": args.api_base_url,
        "frontend_base_url": args.frontend_base_url,
        "slugs": slugs,
        "passed": passed,
        "summary": {
            "property_count": len(slugs),
            "api_pass_count": sum(1 for check in api_checks if check.passed),
            "api_fail_count": sum(1 for check in api_checks if not check.passed),
            "render_pass_count": sum(1 for check in render_checks if check.passed),
            "render_fail_count": sum(1 for check in render_checks if not check.passed),
        },
        "api_checks": [asdict(check) for check in api_checks],
        "render_checks": [asdict(check) for check in render_checks],
    }

    json_path = run_dir / "pilot_qa_report.json"
    md_path = run_dir / "pilot_qa_report.md"
    latest_path = output_root / "latest.json"
    json_path.write_text(json.dumps(report, indent=2), encoding="utf-8")
    latest_path.write_text(json.dumps(report, indent=2), encoding="utf-8")
    write_markdown_report(md_path, report)

    status = "PASS" if passed else "FAIL"
    print(f"{status}: {len(slugs)} properties checked")
    print(f"  API: {report['summary']['api_pass_count']} pass, {report['summary']['api_fail_count']} fail")
    if args.skip_browser:
        print("  Browser: skipped")
    else:
        print(f"  Browser: {report['summary']['render_pass_count']} pass, {report['summary']['render_fail_count']} fail")
    print(f"  JSON: {json_path}")
    print(f"  Markdown: {md_path}")
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
