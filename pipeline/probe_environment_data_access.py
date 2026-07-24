"""Probe public water and terrain data access for OpenEstates.

This is an access check only. It discovers candidate datasets, tests whether
their metadata and resource URLs are reachable, and writes a small JSON report.
It does not ingest data into the DAG or treat any source as approved truth.
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import Request, urlopen


PROJECT_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_OUTPUT = PROJECT_ROOT / "tmp" / "environment_data_access_probe.json"
OPEN_CITY_PACKAGE_SEARCH = "https://data.opencity.in/api/3/action/package_search"
USER_AGENT = "OpenEstates data-access-probe/0.1"


THEME_QUERIES = {
    "groundwater_potential": [
        "bengaluru groundwater potential",
        "groundwater bengaluru",
    ],
    "stormwater_drains": [
        "bengaluru stormwater drains",
        "rajakaluve bengaluru",
    ],
    "lakes_wetlands": [
        "bengaluru lakes wetlands",
        "bengaluru lake boundary",
    ],
    "flood_waterlogging": [
        "bengaluru flood waterlogging",
        "bengaluru flood prone",
    ],
    "rainwater_harvesting": [
        "bengaluru rainwater harvesting",
        "bwssb rainwater harvesting",
    ],
    "terrain_elevation": [
        "bengaluru elevation",
        "bengaluru dem",
    ],
}

THEME_FILTERS = {
    "groundwater_potential": {
        "include_any": ["groundwater", "aquifer", "borewell"],
    },
    "stormwater_drains": {
        "include_any": ["stormwater", "storm water", "drain", "rajakaluve", "nalla"],
    },
    "lakes_wetlands": {
        "include_any": ["lake", "wetland", "water body", "waterbody"],
    },
    "flood_waterlogging": {
        "include_any": ["flood", "flooding", "waterlogging", "underpass"],
    },
    "rainwater_harvesting": {
        "include_any": ["rainwater", "rain water", "harvesting", "recharge"],
    },
    "terrain_elevation": {
        "include_any": ["terrain", "elevation", "dem", "srtm", "digital elevation"],
        "exclude_any": ["elevated corridor", "elevated road", "road corridor"],
    },
}
PLACE_TERMS = ["bengaluru", "bangalore", "bbmp", "karnataka"]


LANDING_PROBES = {
    "india_wris": "https://indiawris.gov.in/",
    "cgwb": "https://cgwb.gov.in/",
    "bhuvan": "https://bhuvan.nrsc.gov.in/",
}


@dataclass
class UrlProbe:
    url: str
    ok: bool
    status: int | None
    content_type: str | None
    content_length: int | None
    final_url: str | None
    elapsed_ms: int
    error: str | None = None
    sample_kind: str | None = None


@dataclass
class ResourceProbe:
    name: str
    format: str | None
    url: str | None
    size: int | None
    license_id: str | None
    source: str | None
    access: UrlProbe | None


@dataclass
class DatasetProbe:
    theme: str
    query: str
    package_name: str
    title: str
    organization: str | None
    license_id: str | None
    license_title: str | None
    notes: str | None
    resources: list[ResourceProbe]


def fetch_json(url: str, timeout: float) -> dict[str, Any]:
    request = Request(url, headers={"User-Agent": USER_AGENT})
    with urlopen(request, timeout=timeout) as response:
        return json.loads(response.read().decode("utf-8"))


def probe_url(url: str, timeout: float) -> UrlProbe:
    started = time.monotonic()
    status = None
    headers = None
    final_url = None
    sample = b""
    error = None

    try:
        request = Request(
            url,
            method="HEAD",
            headers={"User-Agent": USER_AGENT},
        )
        with urlopen(request, timeout=timeout) as response:
            status = response.status
            headers = response.headers
            final_url = response.geturl()
    except (HTTPError, URLError, TimeoutError, OSError) as head_error:
        error = "{}: {}".format(type(head_error).__name__, head_error)
        try:
            request = Request(
                url,
                headers={
                    "User-Agent": USER_AGENT,
                    "Range": "bytes=0-2047",
                },
            )
            with urlopen(request, timeout=timeout) as response:
                status = response.status
                headers = response.headers
                final_url = response.geturl()
                sample = response.read(2048)
                error = None
        except (HTTPError, URLError, TimeoutError, OSError) as get_error:
            status = getattr(get_error, "code", status)
            if hasattr(get_error, "headers"):
                headers = get_error.headers
            error = "{}: {}".format(type(get_error).__name__, get_error)

    elapsed_ms = int((time.monotonic() - started) * 1000)
    content_length = header_int(headers, "Content-Length") if headers else None
    content_type = headers.get("Content-Type") if headers else None
    ok = status is not None and 200 <= status < 400 and error is None
    return UrlProbe(
        url=url,
        ok=ok,
        status=status,
        content_type=content_type,
        content_length=content_length,
        final_url=final_url,
        elapsed_ms=elapsed_ms,
        error=error,
        sample_kind=sample_kind(sample, content_type),
    )


def header_int(headers: Any, key: str) -> int | None:
    value = headers.get(key)
    if value is None:
        return None
    try:
        return int(value)
    except ValueError:
        return None


def sample_kind(sample: bytes, content_type: str | None) -> str | None:
    if not sample:
        return None
    stripped = sample.lstrip()
    if stripped.startswith(b"<?xml") or b"<kml" in stripped[:300].lower():
        return "xml_or_kml"
    if stripped.startswith(b"PK"):
        return "zip"
    if stripped.startswith(b"%PDF"):
        return "pdf"
    if stripped.startswith((b"{", b"[")):
        return "json"
    if content_type:
        return content_type.split(";")[0]
    return "unknown"


def search_open_city(
    theme: str,
    query: str,
    max_packages: int,
    max_resources: int,
    timeout: float,
) -> list[DatasetProbe]:
    params = urlencode({"q": query, "rows": max_packages})
    payload = fetch_json("{}?{}".format(OPEN_CITY_PACKAGE_SEARCH, params), timeout)
    if not payload.get("success"):
        raise RuntimeError("OpenCity package_search failed for query {}".format(query))

    datasets = []
    for package in payload.get("result", {}).get("results", []):
        if not is_relevant_package(theme, package):
            continue
        resources = []
        for resource in package.get("resources", [])[:max_resources]:
            resource_url = resource.get("url")
            access = probe_url(resource_url, timeout) if resource_url else None
            resources.append(
                ResourceProbe(
                    name=resource.get("name") or "",
                    format=resource.get("format"),
                    url=resource_url,
                    size=resource.get("size"),
                    license_id=resource.get("license_id") or package.get("license_id"),
                    source=resource.get("source") or package.get("url"),
                    access=access,
                )
            )
        organization = package.get("organization") or {}
        datasets.append(
            DatasetProbe(
                theme=theme,
                query=query,
                package_name=package.get("name") or "",
                title=package.get("title") or "",
                organization=organization.get("title") or organization.get("name"),
                license_id=package.get("license_id"),
                license_title=package.get("license_title"),
                notes=package.get("notes"),
                resources=resources,
            )
        )
    return datasets


def is_relevant_package(theme: str, package: dict[str, Any]) -> bool:
    filters = THEME_FILTERS.get(theme)
    if not filters:
        return True
    haystack_parts = [
        package.get("name") or "",
        package.get("title") or "",
        package.get("notes") or "",
    ]
    for resource in package.get("resources", []):
        haystack_parts.extend(
            [
                resource.get("name") or "",
                resource.get("description") or "",
                resource.get("format") or "",
            ]
        )
    haystack = " ".join(haystack_parts).lower()
    if not any(term in haystack for term in PLACE_TERMS):
        return False
    include_any = [term.lower() for term in filters.get("include_any", [])]
    exclude_any = [term.lower() for term in filters.get("exclude_any", [])]
    if include_any and not any(term in haystack for term in include_any):
        return False
    if exclude_any and any(term in haystack for term in exclude_any):
        return False
    return True


def run_probe(args: argparse.Namespace) -> dict[str, Any]:
    datasets = []
    failures = []
    for theme, queries in THEME_QUERIES.items():
        if args.theme and theme not in args.theme:
            continue
        for query in queries[: args.queries_per_theme]:
            try:
                datasets.extend(
                    search_open_city(
                        theme=theme,
                        query=query,
                        max_packages=args.max_packages,
                        max_resources=args.max_resources,
                        timeout=args.timeout,
                    )
                )
            except Exception as error:
                failures.append(
                    {
                        "theme": theme,
                        "query": query,
                        "error": "{}: {}".format(type(error).__name__, error),
                    }
                )

    landing_pages = {
        name: asdict(probe_url(url, args.timeout))
        for name, url in LANDING_PROBES.items()
        if not args.theme
    }
    return {
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "open_city_api": OPEN_CITY_PACKAGE_SEARCH,
        "datasets": [asdict(dataset) for dataset in datasets],
        "landing_pages": landing_pages,
        "failures": failures,
        "summary": summarize(datasets, failures, landing_pages),
    }


def summarize(
    datasets: list[DatasetProbe],
    failures: list[dict[str, str]],
    landing_pages: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    theme_counts = {}
    reachable_resources = 0
    blocked_resources = 0
    direct_formats = {}
    for dataset in datasets:
        theme_counts[dataset.theme] = theme_counts.get(dataset.theme, 0) + 1
        for resource in dataset.resources:
            fmt = (resource.format or "unknown").upper()
            direct_formats[fmt] = direct_formats.get(fmt, 0) + 1
            if resource.access and resource.access.ok:
                reachable_resources += 1
            else:
                blocked_resources += 1
    return {
        "dataset_count": len(datasets),
        "theme_counts": theme_counts,
        "reachable_resources": reachable_resources,
        "blocked_resources": blocked_resources,
        "resource_formats": direct_formats,
        "landing_pages_ok": {
            name: probe.get("ok") for name, probe in landing_pages.items()
        },
        "failure_count": len(failures),
    }


def print_human_report(report: dict[str, Any]) -> None:
    summary = report["summary"]
    print("Environment data access probe")
    print("Datasets discovered: {}".format(summary["dataset_count"]))
    print("Reachable resources: {}".format(summary["reachable_resources"]))
    print("Blocked resources: {}".format(summary["blocked_resources"]))
    print("Formats: {}".format(json.dumps(summary["resource_formats"], sort_keys=True)))
    print("")

    by_theme = {}
    for dataset in report["datasets"]:
        by_theme.setdefault(dataset["theme"], []).append(dataset)

    for theme, datasets in sorted(by_theme.items()):
        print("[{}]".format(theme))
        for dataset in datasets[:3]:
            ok_count = sum(
                1
                for resource in dataset["resources"]
                if resource.get("access") and resource["access"].get("ok")
            )
            print(
                "- {} | {} | resources ok {}/{}".format(
                    dataset["title"],
                    dataset.get("license_title") or dataset.get("license_id") or "unknown license",
                    ok_count,
                    len(dataset["resources"]),
                )
            )
            for resource in dataset["resources"][:2]:
                access = resource.get("access") or {}
                print(
                    "  - {} ({}) -> {} {}".format(
                        resource.get("name"),
                        resource.get("format") or "unknown",
                        access.get("status"),
                        "ok" if access.get("ok") else access.get("error"),
                    )
                )
        print("")

    if report["failures"]:
        print("Failures:")
        for failure in report["failures"]:
            print("- {theme} / {query}: {error}".format(**failure))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Probe access to candidate environment datasets."
    )
    parser.add_argument(
        "--theme",
        action="append",
        choices=sorted(THEME_QUERIES),
        help="Limit to one theme. May be repeated.",
    )
    parser.add_argument("--max-packages", type=int, default=3)
    parser.add_argument("--max-resources", type=int, default=3)
    parser.add_argument("--queries-per-theme", type=int, default=1)
    parser.add_argument("--timeout", type=float, default=12.0)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--json", action="store_true", help="Print JSON to stdout.")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    report = run_probe(args)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True), encoding="utf-8")
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print_human_report(report)
        print("Wrote {}".format(args.output))
    return 1 if report["failures"] else 0


if __name__ == "__main__":
    sys.exit(main())
