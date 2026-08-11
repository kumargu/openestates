"""Emit typed source inputs for the Rust asset DAG.

The Rust runner sends ``SourceInputRequest`` JSON on stdin. This module writes
only ``AssetSourceInputs`` JSON to stdout; progress and diagnostics go to
stderr. Durable Parquet writes, lineage, and promotion remain Rust-owned.
"""

import hashlib
import json
import logging
import math
import os
import re
import sys
import time
import xml.etree.ElementTree as ET
from datetime import datetime, timezone
from html import unescape
from pathlib import Path
from typing import Any, Callable, Dict, List, Optional, Tuple

from urllib.parse import urlencode
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen
from pipeline.skills.fetch_rera import (
    DETAIL_URL,
    LISTING_CACHE_PATH,
    LISTING_RAW_CACHE_PATH,
    LISTING_URL,
    RERA_BASE,
    ReraSession,
    search_rera_project,
    scrape_rera_listing,
)


logger = logging.getLogger(__name__)
PROJECT_ROOT = Path(__file__).resolve().parent.parent
KNOWLEDGE_DIR = PROJECT_ROOT / "data" / "knowledge" / "nodes"
DAG_ROOT = PROJECT_ROOT / "app" / "config" / "dag"
FACT_REGISTRY_PATH = DAG_ROOT / "fact_registry.json"
RESOLUTION_POLICIES_PATH = DAG_ROOT / "resolution_policies.json"
_FACT_REGISTRY_CACHE = None  # type: Optional[Dict[str, Any]]
_RESOLUTION_POLICIES_CACHE = None  # type: Optional[Dict[str, Any]]


def load_project_local_env() -> None:
    path = PROJECT_ROOT / ".env.local"
    if not path.exists():
        return
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("export "):
            line = line[len("export ") :].strip()
        key, separator, value = line.partition("=")
        if not separator or not key.strip():
            continue
        value = value.strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in ("'", '"'):
            value = value[1:-1]
        os.environ.setdefault(key.strip(), value)


load_project_local_env()


def load_fact_registry_index():
    # type: () -> Dict[str, Dict[str, Any]]
    """Load fact_key → registry entry map (includes legacy_key_map aliases)."""
    global _FACT_REGISTRY_CACHE
    if _FACT_REGISTRY_CACHE is not None:
        return _FACT_REGISTRY_CACHE

    if not FACT_REGISTRY_PATH.exists():
        _FACT_REGISTRY_CACHE = {}
        return _FACT_REGISTRY_CACHE

    payload = json.loads(FACT_REGISTRY_PATH.read_text(encoding="utf-8"))
    index = {}  # type: Dict[str, Dict[str, Any]]
    for entry in payload.get("facts", []):
        fact_key = entry.get("fact_key")
        if fact_key:
            index[str(fact_key)] = entry
    for legacy_key, canonical in (payload.get("legacy_key_map") or {}).items():
        if canonical in index and legacy_key not in index:
            index[str(legacy_key)] = index[str(canonical)]
    _FACT_REGISTRY_CACHE = index
    return index


def load_resolution_policies():
    # type: () -> Dict[str, Any]
    global _RESOLUTION_POLICIES_CACHE
    if _RESOLUTION_POLICIES_CACHE is not None:
        return _RESOLUTION_POLICIES_CACHE
    if not RESOLUTION_POLICIES_PATH.exists():
        _RESOLUTION_POLICIES_CACHE = {}
        return _RESOLUTION_POLICIES_CACHE
    _RESOLUTION_POLICIES_CACHE = json.loads(
        RESOLUTION_POLICIES_PATH.read_text(encoding="utf-8")
    )
    return _RESOLUTION_POLICIES_CACHE


def annotation_from_registry(fact_key, skill_scoring=None):
    # type: (str, Optional[Dict[str, Any]]) -> Optional[Dict[str, Any]]
    entry = load_fact_registry_index().get(fact_key)
    if not entry:
        return None

    scoring = entry.get("scoring_hint") or {}
    direction = scoring.get("direction")
    if direction == "text_match":
        direction = "TextMatch"
    elif direction == "numeric":
        direction = scoring.get("numeric_direction") or "LowerIsBetter"

    return {
        "display_template": entry.get("display_template"),
        "answers_preferences": entry.get("answers_preferences") or [],
        "scoring_direction": direction,
        "scoring_weight": scoring.get("weight"),
        "scoring_thresholds": scoring.get("thresholds") or [],
    }

RERA_REGISTRY_MONTHLY = "rera_registry_monthly"
RERA_RECEIPTS = "rera_receipts"
RERA_SOURCE_RECORDS = "rera_source_records"
GOOGLE_PLACES_WEEKLY = "google_places_weekly"
GOOGLE_NEARBY_PLACES_WEEKLY = "google_nearby_places_weekly"
EXTERNAL_LISTINGS_WEEKLY = "external_listings_weekly"
EXTERNAL_IMAGES_WEEKLY = "external_images_weekly"
SOCIETY_GROUNDWATER_POTENTIAL_FACTS = "society_groundwater_potential_facts"
BENGALURU_METRO_STATION_FACTS = "bengaluru_metro_station_facts"
OSM_POWER_LINE_FACTS = "osm_power_line_facts"
STORMWATER_DRAIN_FACTS = "stormwater_drain_facts"
RERA_DETAIL_RECEIPT_CACHE_DIR = PROJECT_ROOT / "data" / "cache" / "skills" / "rera_detail_receipts"
GROUNDWATER_KML_URL = (
    "https://data.opencity.in/dataset/035c1d40-8f4e-4780-90c5-ff1ce2281849/"
    "resource/d3ae3603-d786-4782-ae71-a034ad4ebc0b/download/"
    "1dda919d-ff28-4aa9-90ce-bd18e708927b.kml"
)
OVERPASS_API_URL = "https://overpass-api.de/api/interpreter"
BENGALURU_METRO_OVERPASS_QUERY = """
[out:json][timeout:25];
node["railway"="station"]["network"="Namma Metro"](12.75,77.35,13.20,77.95);
out body;
""".strip()
SUPPORTED_ASSETS = frozenset(
    (
        RERA_RECEIPTS,
        RERA_SOURCE_RECORDS,
        RERA_REGISTRY_MONTHLY,
        GOOGLE_PLACES_WEEKLY,
        GOOGLE_NEARBY_PLACES_WEEKLY,
        EXTERNAL_LISTINGS_WEEKLY,
        EXTERNAL_IMAGES_WEEKLY,
        SOCIETY_GROUNDWATER_POTENTIAL_FACTS,
        BENGALURU_METRO_STATION_FACTS,
        OSM_POWER_LINE_FACTS,
        STORMWATER_DRAIN_FACTS,
    )
)


def collect_asset_sources(
    request: Dict[str, Any],
    rera_fetch: Callable[[], Any] = None,
) -> Dict[str, Any]:
    requested = [asset_id for asset_id in request.get("requested_assets", []) if asset_id]
    unsupported = sorted(set(requested) - SUPPORTED_ASSETS)
    if unsupported:
        raise ValueError("unsupported source assets: {}".format(", ".join(unsupported)))

    output = {}  # type: Dict[str, Any]
    source_failures = {}  # type: Dict[str, str]
    planned_at = normalized_planned_at(request)
    partition = partition_values(request)
    snapshot_date = partition.get("dt") or planned_at[:10]
    if RERA_REGISTRY_MONTHLY in requested:
        try:
            output[RERA_REGISTRY_MONTHLY] = collect_rera_registry(request, rera_fetch)
        except Exception as error:
            record_source_failure(source_failures, [RERA_REGISTRY_MONTHLY], error)
    if RERA_RECEIPTS in requested:
        try:
            output[RERA_RECEIPTS] = collect_rera_receipts(request)
        except Exception as error:
            record_source_failure(source_failures, [RERA_RECEIPTS], error)
    if RERA_SOURCE_RECORDS in requested:
        try:
            output[RERA_SOURCE_RECORDS] = collect_rera_source_records(request)
        except Exception as error:
            record_source_failure(source_failures, [RERA_SOURCE_RECORDS], error)
    google_address_input = None
    google_requested = (
        GOOGLE_PLACES_WEEKLY in requested or GOOGLE_NEARBY_PLACES_WEEKLY in requested
    )
    if google_requested:
        google_address_input = output.get(RERA_REGISTRY_MONTHLY)
        if not google_address_input:
            google_address_input = rera_address_input_for_request(request)
    if GOOGLE_PLACES_WEEKLY in requested:
        try:
            google_inputs = google_society_inputs(request, google_address_input)
            if not google_inputs:
                raise ValueError(
                    "Google collection requires scoped source_entities or RERA projects"
                )
            output[GOOGLE_PLACES_WEEKLY] = collect_google_places(
                request,
                society_inputs=google_inputs,
            )
        except Exception as error:
            record_source_failure(source_failures, [GOOGLE_PLACES_WEEKLY], error)
    if GOOGLE_NEARBY_PLACES_WEEKLY in requested:
        try:
            google_inputs = google_society_inputs(request, google_address_input)
            apply_google_origin_locations(
                google_inputs, output.get(GOOGLE_PLACES_WEEKLY)
            )
            if not google_inputs:
                raise ValueError(
                    "Google nearby collection requires scoped source_entities or RERA projects"
                )
            output[GOOGLE_NEARBY_PLACES_WEEKLY] = collect_google_nearby_places(
                request,
                society_inputs=google_inputs,
            )
        except Exception as error:
            record_source_failure(source_failures, [GOOGLE_NEARBY_PLACES_WEEKLY], error)
    if EXTERNAL_LISTINGS_WEEKLY in requested:
        try:
            from pipeline.sources.external_listings import collect_external_listings

            output[EXTERNAL_LISTINGS_WEEKLY] = collect_external_listings(
                request_with_rera_detail_facts(request, output.get(RERA_REGISTRY_MONTHLY))
            )
        except Exception as error:
            record_source_failure(source_failures, [EXTERNAL_LISTINGS_WEEKLY], error)
    if EXTERNAL_IMAGES_WEEKLY in requested:
        try:
            from pipeline.sources.external_images import collect_external_images

            output[EXTERNAL_IMAGES_WEEKLY] = collect_external_images(
                request_with_rera_detail_facts(request, output.get(RERA_REGISTRY_MONTHLY))
            )
        except Exception as error:
            record_source_failure(source_failures, [EXTERNAL_IMAGES_WEEKLY], error)
    if SOCIETY_GROUNDWATER_POTENTIAL_FACTS in requested:
        try:
            output["environment_groundwater_potential"] = (
                collect_environment_groundwater_potential(request)
            )
        except Exception as error:
            record_source_failure(
                source_failures, [SOCIETY_GROUNDWATER_POTENTIAL_FACTS], error
            )
    if BENGALURU_METRO_STATION_FACTS in requested:
        try:
            output["bengaluru_metro_stations"] = collect_bengaluru_metro_stations(
                request
            )
        except Exception as error:
            record_source_failure(
                source_failures, [BENGALURU_METRO_STATION_FACTS], error
            )
    if OSM_POWER_LINE_FACTS in requested:
        try:
            output["osm_power_infrastructure"] = collect_osm_power_infrastructure(
                request,
                output.get(RERA_REGISTRY_MONTHLY),
                output.get(GOOGLE_PLACES_WEEKLY),
            )
        except Exception as error:
            record_source_failure(source_failures, [OSM_POWER_LINE_FACTS], error)
    if STORMWATER_DRAIN_FACTS in requested:
        try:
            output["stormwater_drains"] = collect_stormwater_drains(
                request,
                output.get(RERA_REGISTRY_MONTHLY),
                output.get(GOOGLE_PLACES_WEEKLY),
            )
        except Exception as error:
            record_source_failure(source_failures, [STORMWATER_DRAIN_FACTS], error)
    if source_failures:
        output["source_failures"] = source_failures
    return output


def collect_environment_groundwater_potential(
    request: Dict[str, Any],
    fetch: Callable[[str], bytes] = None,
) -> Dict[str, Any]:
    planned_at = normalized_planned_at(request)
    snapshot_date = partition_values(request).get("dt") or planned_at[:10]
    source_url = os.environ.get("OPENESTATES_GROUNDWATER_KML_URL") or GROUNDWATER_KML_URL
    kml_bytes = (fetch or fetch_url_bytes)(source_url)
    zones = groundwater_zones_from_kml(kml_bytes)
    if not zones:
        raise ValueError("groundwater KML produced zero usable polygon zones")
    return {
        "snapshot_date": snapshot_date,
        "source_url": source_url,
        "zones": zones,
        "source_watermarks": [
            {
                "source": "opencity_groundwater_potential_kml",
                "high_watermark": "{}#sha256={}".format(
                    source_url, hashlib.sha256(kml_bytes).hexdigest()
                ),
            },
            {
                "source": "opencity_groundwater_potential_zone_count",
                "high_watermark": str(len(zones)),
            },
        ],
    }


def fetch_url_bytes(url: str) -> bytes:
    with urlopen(url, timeout=30) as response:
        return response.read()


def collect_bengaluru_metro_stations(
    request: Dict[str, Any],
    fetch: Callable[[str, str], Dict[str, Any]] = None,
) -> Dict[str, Any]:
    planned_at = normalized_planned_at(request)
    snapshot_date = partition_values(request).get("dt") or planned_at[:10]
    source_url = os.environ.get("OPENESTATES_OVERPASS_API_URL") or OVERPASS_API_URL
    query = (
        os.environ.get("OPENESTATES_BENGALURU_METRO_OVERPASS_QUERY")
        or BENGALURU_METRO_OVERPASS_QUERY
    )
    payload = (fetch or fetch_overpass_json)(source_url, query)
    stations = bengaluru_metro_stations_from_overpass(payload)
    if not stations:
        raise ValueError("Overpass payload produced zero usable Bengaluru metro stations")
    return {
        "snapshot_date": snapshot_date,
        "source_url": source_url,
        "stations": stations,
        "source_watermarks": [
            {
                "source": "openstreetmap_overpass_query",
                "high_watermark": "sha256:{}".format(
                    hashlib.sha256(query.encode("utf-8")).hexdigest()
                ),
            },
            {
                "source": "openstreetmap_bengaluru_metro_station_count",
                "high_watermark": str(len(stations)),
            },
        ],
    }


def fetch_overpass_json(url: str, query: str) -> Dict[str, Any]:
    for attempt in range(1, 4):
        request = Request(
            url,
            data=urlencode({"data": query}).encode("utf-8"),
            headers={
                "Content-Type": "application/x-www-form-urlencoded; charset=utf-8",
                "User-Agent": "OpenEstates DAG source collector",
            },
            method="POST",
        )
        try:
            with urlopen(request, timeout=45) as response:
                return json.loads(response.read().decode("utf-8"))
        except HTTPError as error:
            if attempt >= 3 or error.code not in (429, 500, 502, 503, 504):
                raise
        except URLError:
            if attempt >= 3:
                raise
        time.sleep(float(attempt * 2))
    raise RuntimeError("Overpass request exhausted retries")


def collect_osm_power_infrastructure(
    request: Dict[str, Any],
    rera_input: Dict[str, Any] = None,
    google_places_input: Dict[str, Any] = None,
    fetch: Callable[[str, str], Dict[str, Any]] = None,
) -> Dict[str, Any]:
    planned_at = normalized_planned_at(request)
    snapshot_date = partition_values(request).get("dt") or planned_at[:10]
    config = load_dag_config("osm_power_infrastructure.json")
    policy = config.get("transmission_lines") or {}
    collector = config.get("collector") or {}
    subjects = geospatial_society_inputs(request, rera_input, google_places_input)
    if not subjects:
        raise ValueError("OSM power collection requires society coordinates")

    max_distance = float(policy.get("max_distance_meters") or 1000.0)
    accepted_power_values = optional_string_list(policy.get("accepted_power_values")) or ["line"]
    voltage_values = optional_string_list(collector.get("voltage_query_values"))
    source_url = collector_url(collector)
    records, query_hashes = collect_osm_power_records_from_overpass(
        fetch or fetch_overpass_json,
        source_url,
        subjects,
        max_distance,
        accepted_power_values,
        voltage_values,
        collector,
        planned_at,
    )
    watermark_source = str(collector.get("source_id") or "openstreetmap_power")
    if not records:
        watermark_source = "{}_empty".format(watermark_source)
    return {
        "snapshot_date": snapshot_date,
        "collection_status": "complete" if records else "complete_empty",
        "records": records,
        "source_watermarks": [
            {
                "source": watermark_source,
                "high_watermark": "query_sha256:{};records={}".format(
                    hashlib.sha256(";".join(query_hashes).encode("utf-8")).hexdigest(),
                    len(records),
                ),
            }
        ],
    }


def collect_stormwater_drains(
    request: Dict[str, Any],
    rera_input: Dict[str, Any] = None,
    google_places_input: Dict[str, Any] = None,
    fetch: Callable[[str, str], Dict[str, Any]] = None,
) -> Dict[str, Any]:
    planned_at = normalized_planned_at(request)
    snapshot_date = partition_values(request).get("dt") or planned_at[:10]
    config = load_dag_config("stormwater_drain_risk.json")
    policy = config.get("drains") or {}
    collector = config.get("collector") or {}
    subjects = geospatial_society_inputs(request, rera_input, google_places_input)
    if not subjects:
        raise ValueError("stormwater drain collection requires society coordinates")

    max_distance = float(policy.get("max_distance_meters") or 250.0)
    source_url = collector_url(collector)
    records = []
    query_hashes = []
    overpass_failures = []
    waterway_values = optional_string_list(collector.get("waterway_values")) or [
        "drain",
        "ditch",
        "canal",
    ]
    for subject in subjects:
        bbox = padded_bbox(
            [subject],
            max_distance + float(collector.get("bbox_padding_meters") or 0.0),
        )
        query = stormwater_overpass_query(
            bbox,
            waterway_values,
            int(collector.get("query_timeout_seconds") or 60),
        )
        query_hashes.append(hashlib.sha256(query.encode("utf-8")).hexdigest())
        try:
            payload = (fetch or fetch_overpass_json)(source_url, query)
            records.extend(
                stormwater_records_from_overpass(
                    payload,
                    [subject],
                    max_distance,
                    query,
                    collector,
                    planned_at,
                )
            )
        except Exception as error:
            overpass_failures.append("{}: {}".format(subject["entity_id"], error))
            logger.warning(
                "Stormwater Overpass collection failed for %s: %s",
                subject["entity_id"],
                error,
            )
    if overpass_failures:
        raise ValueError(
            "stormwater Overpass was unavailable for {} of {} subjects: {}".format(
                len(overpass_failures), len(subjects), "; ".join(overpass_failures[:5])
            )
        )
    records = dedupe_spatial_records(records, "drain_id")
    watermark_source = str(collector.get("source_id") or "openstreetmap_stormwater")
    if not records:
        watermark_source = "{}_empty".format(watermark_source)
    watermarks = [
        {
            "source": watermark_source,
            "high_watermark": "query_sha256:{};records={}".format(
                hashlib.sha256(";".join(query_hashes).encode("utf-8")).hexdigest(),
                len(records),
            ),
        }
    ]
    return {
        "snapshot_date": snapshot_date,
        "collection_status": "complete" if records else "complete_empty",
        "records": records,
        "source_watermarks": watermarks,
    }


def load_dag_config(filename: str) -> Dict[str, Any]:
    return json.loads((DAG_ROOT / filename).read_text(encoding="utf-8"))


def collector_url(collector: Dict[str, Any]) -> str:
    env_key = optional_string(collector.get("overpass_url_env"))
    if env_key:
        value = optional_string(os.environ.get(env_key))
        if value:
            return value
    return str(collector.get("default_overpass_url") or OVERPASS_API_URL)


def collect_osm_power_records_from_overpass(
    fetch: Callable[[str, str], Dict[str, Any]],
    source_url: str,
    subjects: List[Dict[str, Any]],
    max_distance_meters: float,
    accepted_power_values: List[str],
    voltage_values: List[str],
    collector: Dict[str, Any],
    planned_at: str,
) -> Tuple[List[Dict[str, Any]], List[str]]:
    query_timeout = int(collector.get("query_timeout_seconds") or 60)
    bbox_padding = float(collector.get("bbox_padding_meters") or 0.0)
    query_hashes = []
    combined_query = osm_power_overpass_query(
        padded_bbox(subjects, max_distance_meters + bbox_padding),
        accepted_power_values,
        voltage_values,
        query_timeout,
    )
    query_hashes.append(hashlib.sha256(combined_query.encode("utf-8")).hexdigest())
    try:
        payload = fetch(source_url, combined_query)
        records = osm_power_records_from_overpass(
            payload,
            subjects,
            max_distance_meters,
            combined_query,
            collector,
            planned_at,
        )
        return records, query_hashes
    except Exception as error:
        if not bool(collector.get("fallback_to_subject_queries", True)):
            raise
        logger.warning("OSM power combined Overpass collection failed: %s", error)

    records = []
    failures = []
    for subject in subjects:
        query = osm_power_overpass_query(
            padded_bbox([subject], max_distance_meters + bbox_padding),
            accepted_power_values,
            voltage_values,
            query_timeout,
        )
        query_hashes.append(hashlib.sha256(query.encode("utf-8")).hexdigest())
        try:
            payload = fetch_overpass_with_retries(fetch, source_url, query, collector)
            records.extend(
                osm_power_records_from_overpass(
                    payload,
                    [subject],
                    max_distance_meters,
                    query,
                    collector,
                    planned_at,
                )
            )
        except Exception as error:
            failures.append("{}: {}".format(subject["entity_id"], error))
            logger.warning(
                "OSM power Overpass collection failed for %s: %s",
                subject["entity_id"],
                error,
            )
    if failures and (
        len(failures) == len(subjects)
        or not bool(collector.get("allow_partial_subject_failures", False))
    ):
        raise ValueError(
            "OSM power Overpass failed for {} of {} subjects: {}".format(
                len(failures),
                len(subjects),
                "; ".join(failures[:5])
            )
        )
    return dedupe_spatial_records(records, "osm_id"), query_hashes


def fetch_overpass_with_retries(
    fetch: Callable[[str, str], Dict[str, Any]],
    source_url: str,
    query: str,
    collector: Dict[str, Any],
) -> Dict[str, Any]:
    retry_count = max(0, int(collector.get("subject_query_retry_count") or 0))
    retry_delay_seconds = max(0.0, float(collector.get("subject_query_retry_delay_seconds") or 0.0))
    retry_status_codes = {
        int(code)
        for code in (collector.get("retry_status_codes") or [])
        if str(code).strip().isdigit()
    }
    attempt = 0
    while True:
        try:
            return fetch(source_url, query)
        except Exception as error:
            if attempt >= retry_count or not overpass_error_is_retryable(error, retry_status_codes):
                raise
            attempt += 1
            delay = retry_after_seconds(error) or retry_delay_seconds
            if delay > 0.0:
                time.sleep(delay)


def overpass_error_is_retryable(error: Exception, retry_status_codes: set) -> bool:
    if isinstance(error, HTTPError):
        return error.code in retry_status_codes
    return False


def retry_after_seconds(error: Exception) -> Optional[float]:
    if not isinstance(error, HTTPError):
        return None
    header = error.headers.get("Retry-After") if error.headers else None
    try:
        value = float(header) if header else None
    except ValueError:
        return None
    if value is None or value < 0.0:
        return None
    return value


def osm_power_overpass_query(
    bbox: Tuple[float, float, float, float],
    accepted_power_values: List[str],
    voltage_values: List[str],
    timeout_seconds: int,
) -> str:
    pattern = "|".join(sorted({value for value in accepted_power_values if value}))
    voltage_pattern = "|".join(sorted({value for value in voltage_values if value}))
    south, west, north, east = bbox
    voltage_filter = (
        '["voltage"~"(^|;)({})(;|$)"]'.format(voltage_pattern)
        if voltage_pattern
        else '["voltage"]'
    )
    return """
[out:json][timeout:{timeout}];
(
  way["power"~"^({pattern})$"]{voltage_filter}({south:.7f},{west:.7f},{north:.7f},{east:.7f});
);
out tags geom;
""".format(
        timeout=timeout_seconds,
        pattern=pattern or "line",
        voltage_filter=voltage_filter,
        south=south,
        west=west,
        north=north,
        east=east,
    ).strip()


def stormwater_overpass_query(
    bbox: Tuple[float, float, float, float],
    waterway_values: List[str],
    timeout_seconds: int,
) -> str:
    pattern = "|".join(sorted({value for value in waterway_values if value}))
    south, west, north, east = bbox
    return """
[out:json][timeout:{timeout}];
(
  way["waterway"~"^({pattern})$"]({south:.7f},{west:.7f},{north:.7f},{east:.7f});
);
out tags geom;
""".format(
        timeout=timeout_seconds,
        pattern=pattern or "drain|ditch|canal",
        south=south,
        west=west,
        north=north,
        east=east,
    ).strip()


def osm_power_records_from_overpass(
    payload: Dict[str, Any],
    subjects: List[Dict[str, Any]],
    max_distance_meters: float,
    query: str,
    collector: Dict[str, Any],
    planned_at: str,
) -> List[Dict[str, Any]]:
    records = []
    for element in overpass_way_elements(payload):
        tags = element_tags(element)
        points = element_geometry_points(element)
        if len(points) < 2:
            continue
        geometry_geojson = line_geojson(points)
        osm_id = osm_element_id(element)
        voltage_kv = voltage_kv_from_tag(tags.get("voltage"))
        for subject in subjects:
            distance_meters, closest = distance_from_subject_to_line(subject, points)
            if distance_meters > max_distance_meters:
                continue
            records.append(
                {
                    "entity_id": subject["entity_id"],
                    "project_key": optional_string(subject.get("project_key")),
                    "query": subject_query(subject, "power=line"),
                    "osm_id": osm_id,
                    "name": optional_string(tags.get("name") or tags.get("operator")),
                    "power": optional_string(tags.get("power")) or "line",
                    "voltage_kv": voltage_kv,
                    "distance_meters": distance_meters,
                    "subject_latitude": subject["latitude"],
                    "subject_longitude": subject["longitude"],
                    "latitude": closest["latitude"],
                    "longitude": closest["longitude"],
                    "geometry_geojson": geometry_geojson,
                    "source_tags": tags,
                    "source_url": osm_source_url(element),
                    "confidence": float(collector.get("confidence") or 0.82),
                    "fetched_at": planned_at,
                    "fetch_source": str(collector.get("fetch_source") or "overpass_power_snapshot"),
                }
            )
    return dedupe_spatial_records(records, "osm_id")


def stormwater_records_from_overpass(
    payload: Dict[str, Any],
    subjects: List[Dict[str, Any]],
    max_distance_meters: float,
    query: str,
    collector: Dict[str, Any],
    planned_at: str,
) -> List[Dict[str, Any]]:
    records = []
    for element in overpass_way_elements(payload):
        tags = element_tags(element)
        points = element_geometry_points(element)
        if len(points) < 2:
            continue
        drain_type = stormwater_drain_type(tags, collector)
        geometry_geojson = line_geojson(points)
        drain_id = osm_element_id(element)
        for subject in subjects:
            distance_meters, closest = distance_from_subject_to_line(subject, points)
            if distance_meters > max_distance_meters:
                continue
            records.append(
                {
                    "entity_id": subject["entity_id"],
                    "project_key": optional_string(subject.get("project_key")),
                    "query": subject_query(subject, "stormwater drain"),
                    "drain_id": drain_id,
                    "name": optional_string(tags.get("name") or tags.get("waterway")),
                    "drain_type": drain_type,
                    "hierarchy": stormwater_hierarchy(tags, collector),
                    "distance_meters": distance_meters,
                    "intersects_property": distance_meters <= 1.0,
                    "subject_latitude": subject["latitude"],
                    "subject_longitude": subject["longitude"],
                    "latitude": closest["latitude"],
                    "longitude": closest["longitude"],
                    "geometry_geojson": geometry_geojson,
                    "encroachment_record": optional_string(tags.get("encroachment")),
                    "source_tags": tags,
                    "source_url": osm_source_url(element),
                    "source_type": str(collector.get("source_type") or "OpenStreetMap"),
                    "confidence": float(collector.get("confidence") or 0.74),
                    "fetched_at": planned_at,
                    "fetch_source": str(collector.get("fetch_source") or "overpass_stormwater_snapshot"),
                }
            )
    return dedupe_spatial_records(records, "drain_id")


def geospatial_society_inputs(
    request: Dict[str, Any],
    rera_input: Dict[str, Any] = None,
    google_places_input: Dict[str, Any] = None,
) -> List[Dict[str, Any]]:
    by_entity = {}
    google_coordinates = google_place_coordinates_by_entity(google_places_input)
    for seed in request.get("source_entities", []):
        if not isinstance(seed, dict):
            continue
        entity_id = optional_string(seed.get("entity_id"))
        name = optional_string(seed.get("name"))
        if not entity_id or not name:
            continue
        google_point = google_coordinates.get(entity_id, {})
        latitude, longitude = select_coordinate_pair(
            [
                {
                    "source_type": "source_entity_seed",
                    "latitude": seed.get("latitude"),
                    "longitude": seed.get("longitude"),
                },
                {
                    "source_type": "google",
                    "latitude": google_point.get("latitude"),
                    "longitude": google_point.get("longitude"),
                },
            ]
        )
        if latitude is None or longitude is None:
            continue
        by_entity[entity_id] = {
            "entity_id": entity_id,
            "project_key": optional_string(seed.get("project_key")),
            "name": name,
            "area": optional_string(seed.get("area")),
            "city": optional_string(seed.get("city")) or "Bengaluru",
            "latitude": latitude,
            "longitude": longitude,
        }
    return sorted(by_entity.values(), key=lambda row: row["entity_id"])


def google_place_coordinates_by_entity(
    google_places_input: Dict[str, Any] = None,
) -> Dict[str, Dict[str, Any]]:
    coordinates = {}
    if not google_places_input:
        return coordinates
    for record in google_places_input.get("records") or []:
        entity_id = optional_string(record.get("entity_id"))
        latitude = optional_float(record.get("latitude"))
        longitude = optional_float(record.get("longitude"))
        if not entity_id or latitude is None or longitude is None:
            continue
        coordinates[entity_id] = {
            "latitude": latitude,
            "longitude": longitude,
        }
    return coordinates


def select_coordinate_pair(
    candidates: List[Dict[str, Any]], entity_scope: str = "society"
) -> Tuple[Optional[float], Optional[float]]:
    policy = (load_resolution_policies().get("coordinate_sources") or {}).get(
        entity_scope
    ) or {}
    allowed = {
        normalize_source_type(source) for source in policy.get("allowed_sources") or []
    }
    denied = {
        normalize_source_type(source) for source in policy.get("denied_sources") or []
    }
    priority = policy.get("source_priority") or []
    ranked = {
        normalize_source_type(source): index
        for index, source in enumerate(priority)
    }
    best = None
    best_rank = len(ranked) + len(candidates)
    for index, candidate in enumerate(candidates):
        latitude = optional_float(candidate.get("latitude"))
        longitude = optional_float(candidate.get("longitude"))
        if latitude is None or longitude is None:
            continue
        source_type = normalize_source_type(candidate.get("source_type"))
        if not source_type or source_type in denied or source_type not in allowed:
            continue
        if not (
            math.isfinite(latitude)
            and math.isfinite(longitude)
            and -90.0 <= latitude <= 90.0
            and -180.0 <= longitude <= 180.0
        ):
            continue
        rank = ranked.get(source_type, len(ranked) + index)
        if best is None or rank < best_rank:
            best = (latitude, longitude)
            best_rank = rank
    if best is None:
        return None, None
    return best


def normalize_source_type(value: Any) -> str:
    return (
        optional_string(value)
        .replace("_", "")
        .replace("-", "")
        .replace(" ", "")
        .lower()
    )


def parse_lat_lng_text(value: Any) -> Dict[str, float]:
    text = optional_string(value)
    if not text:
        return {}
    parts = [part.strip() for part in text.split(",")]
    if len(parts) != 2:
        return {}
    latitude = optional_float(parts[0])
    longitude = optional_float(parts[1])
    if latitude is None or longitude is None:
        return {}
    if not (-90.0 <= latitude <= 90.0 and -180.0 <= longitude <= 180.0):
        return {}
    return {"latitude": latitude, "longitude": longitude}


def rera_detail_facts_by_entity_for_keys(
    rera_input: Dict[str, Any], fact_keys: set
) -> Dict[str, Dict[str, Any]]:
    by_entity = {}
    if not rera_input:
        return by_entity
    for fact in rera_input.get("detail_facts") or []:
        entity_id = optional_string(fact.get("entity_id"))
        fact_key = optional_string(fact.get("fact_key"))
        if not entity_id or fact_key not in fact_keys:
            continue
        value = fact_value_data(fact)
        if value is not None:
            by_entity.setdefault(entity_id, {})[fact_key] = value
    return by_entity


def overpass_way_elements(payload: Dict[str, Any]) -> List[Dict[str, Any]]:
    elements = payload.get("elements") if isinstance(payload, dict) else None
    if not isinstance(elements, list):
        raise ValueError("Overpass response must contain an elements list")
    return [
        element
        for element in elements
        if isinstance(element, dict) and element.get("type") == "way"
    ]


def element_tags(element: Dict[str, Any]) -> Dict[str, str]:
    return {
        str(key): str(value)
        for key, value in (element.get("tags") or {}).items()
        if optional_string(value)
    }


def element_geometry_points(element: Dict[str, Any]) -> List[Dict[str, float]]:
    points = []
    for point in element.get("geometry") or []:
        latitude = optional_float(point.get("lat")) if isinstance(point, dict) else None
        longitude = optional_float(point.get("lon")) if isinstance(point, dict) else None
        if latitude is None or longitude is None:
            continue
        points.append({"latitude": latitude, "longitude": longitude})
    return points


def line_geojson(points: List[Dict[str, float]]) -> str:
    return json.dumps(
        {
            "type": "LineString",
            "coordinates": [
                [point["longitude"], point["latitude"]]
                for point in points
            ],
        },
        separators=(",", ":"),
    )


def osm_element_id(element: Dict[str, Any]) -> str:
    return "{}/{}".format(element.get("type") or "way", element.get("id"))


def osm_source_url(element: Dict[str, Any]) -> str:
    return "https://www.openstreetmap.org/{}".format(osm_element_id(element))


def voltage_kv_from_tag(value: Any) -> Optional[float]:
    text = optional_string(value)
    if not text:
        return None
    voltages = []
    for token in split_tag_list(text):
        normalized = token.lower().replace("kv", "").strip()
        try:
            voltage = float(normalized)
        except ValueError:
            continue
        if voltage > 1000:
            voltage /= 1000.0
        voltages.append(voltage)
    return max(voltages) if voltages else None


def stormwater_drain_type(tags: Dict[str, str], collector: Dict[str, Any]) -> str:
    text = " ".join(
        optional_string(tags.get(key)) or ""
        for key in ("name", "waterway", "description", "local_name")
    ).lower()
    if any(marker in text for marker in optional_string_list(collector.get("rajakaluve_name_markers"))):
        return "rajakaluve"
    waterway = (optional_string(tags.get("waterway")) or "").lower()
    if waterway in ("drain", "ditch"):
        return "stormwater_drain"
    if waterway == "canal":
        return "primary_swd"
    return "stormwater_drain"


def stormwater_hierarchy(tags: Dict[str, str], collector: Dict[str, Any]) -> Optional[str]:
    text = " ".join(
        optional_string(tags.get(key)) or ""
        for key in ("name", "description", "local_name")
    ).lower()
    for rule in collector.get("hierarchy_name_markers") or []:
        if not isinstance(rule, dict):
            continue
        hierarchy = optional_string(rule.get("hierarchy"))
        markers = optional_string_list(rule.get("markers"))
        if hierarchy and any(marker in text for marker in markers):
            return hierarchy
    return None


def padded_bbox(
    subjects: List[Dict[str, Any]], padding_meters: float
) -> Tuple[float, float, float, float]:
    latitudes = [float(subject["latitude"]) for subject in subjects]
    longitudes = [float(subject["longitude"]) for subject in subjects]
    center_latitude = sum(latitudes) / len(latitudes)
    latitude_delta = padding_meters / 111_320.0
    longitude_delta = padding_meters / max(1.0, 111_320.0 * math.cos(math.radians(center_latitude)))
    return (
        min(latitudes) - latitude_delta,
        min(longitudes) - longitude_delta,
        max(latitudes) + latitude_delta,
        max(longitudes) + longitude_delta,
    )


def distance_from_subject_to_line(
    subject: Dict[str, Any],
    points: List[Dict[str, float]],
) -> Tuple[float, Dict[str, float]]:
    origin_latitude = float(subject["latitude"])
    origin_longitude = float(subject["longitude"])
    projected = [
        project_point(point["latitude"], point["longitude"], origin_latitude, origin_longitude)
        for point in points
    ]
    best_distance = None
    best_projected = None
    for start, end in zip(projected, projected[1:]):
        distance, closest = point_segment_distance((0.0, 0.0), start, end)
        if best_distance is None or distance < best_distance:
            best_distance = distance
            best_projected = closest
    if best_distance is None or best_projected is None:
        first = points[0]
        return haversine_meters(origin_latitude, origin_longitude, first["latitude"], first["longitude"]), first
    return best_distance, unproject_point(best_projected[0], best_projected[1], origin_latitude, origin_longitude)


def project_point(
    latitude: float,
    longitude: float,
    origin_latitude: float,
    origin_longitude: float,
) -> Tuple[float, float]:
    meters_per_degree_latitude = 111_320.0
    meters_per_degree_longitude = meters_per_degree_latitude * math.cos(math.radians(origin_latitude))
    return (
        (longitude - origin_longitude) * meters_per_degree_longitude,
        (latitude - origin_latitude) * meters_per_degree_latitude,
    )


def unproject_point(
    x: float,
    y: float,
    origin_latitude: float,
    origin_longitude: float,
) -> Dict[str, float]:
    meters_per_degree_latitude = 111_320.0
    meters_per_degree_longitude = meters_per_degree_latitude * math.cos(math.radians(origin_latitude))
    return {
        "latitude": origin_latitude + y / meters_per_degree_latitude,
        "longitude": origin_longitude + x / meters_per_degree_longitude,
    }


def point_segment_distance(
    point: Tuple[float, float],
    start: Tuple[float, float],
    end: Tuple[float, float],
) -> Tuple[float, Tuple[float, float]]:
    px, py = point
    sx, sy = start
    ex, ey = end
    dx = ex - sx
    dy = ey - sy
    length_sq = dx * dx + dy * dy
    if length_sq == 0:
        closest = start
    else:
        t = max(0.0, min(1.0, ((px - sx) * dx + (py - sy) * dy) / length_sq))
        closest = (sx + t * dx, sy + t * dy)
    distance = math.hypot(px - closest[0], py - closest[1])
    return distance, closest


def haversine_meters(lat1: float, lon1: float, lat2: float, lon2: float) -> float:
    radius_meters = 6_371_000.0
    phi1 = math.radians(lat1)
    phi2 = math.radians(lat2)
    delta_phi = math.radians(lat2 - lat1)
    delta_lambda = math.radians(lon2 - lon1)
    a = (
        math.sin(delta_phi / 2) ** 2
        + math.cos(phi1) * math.cos(phi2) * math.sin(delta_lambda / 2) ** 2
    )
    return radius_meters * 2 * math.atan2(math.sqrt(a), math.sqrt(1 - a))


def subject_query(subject: Dict[str, Any], target: str) -> str:
    parts = []
    if optional_string(target):
        parts.append(target)
    parts.extend(["around", str(subject.get("name") or subject.get("entity_id"))])
    for key in ("area", "city"):
        value = optional_string(subject.get(key))
        if value:
            parts.append(value)
    return " ".join(parts)


def dedupe_spatial_records(records: List[Dict[str, Any]], feature_key: str) -> List[Dict[str, Any]]:
    deduped = {}
    for record in records:
        key = (record.get("entity_id"), record.get(feature_key))
        existing = deduped.get(key)
        if existing is None or record["distance_meters"] < existing["distance_meters"]:
            deduped[key] = record
    return sorted(
        deduped.values(),
        key=lambda record: (
            str(record.get("entity_id") or ""),
            float(record.get("distance_meters") or 0.0),
            str(record.get(feature_key) or ""),
        ),
    )


def bengaluru_metro_stations_from_overpass(
    payload: Dict[str, Any]
) -> List[Dict[str, Any]]:
    elements = payload.get("elements") if isinstance(payload, dict) else None
    if not isinstance(elements, list):
        raise ValueError("Overpass response must contain an elements list")

    stations_by_key = {}  # type: Dict[Tuple[str, str], Dict[str, Any]]
    for element in elements:
        if not isinstance(element, dict):
            continue
        tags = {
            str(key): str(value)
            for key, value in (element.get("tags") or {}).items()
            if optional_string(value)
        }
        name = optional_string(tags.get("name:en")) or optional_string(tags.get("name"))
        latitude, longitude = element_coordinates(element)
        if not name or latitude is None or longitude is None:
            continue
        station = {
            "station_id": "{}/{}".format(element.get("type") or "element", element.get("id")),
            "name": name,
            "latitude": latitude,
            "longitude": longitude,
            "lines": station_lines(tags),
            "network": optional_string(tags.get("network")) or "Namma Metro",
            "operator": optional_string(tags.get("operator")),
            "operational_status": station_operational_status(tags),
            "source_url": osm_element_url(element),
            "source_tags": tags,
        }
        key = (
            normalize_selector(name),
            "{:.5f},{:.5f}".format(latitude, longitude),
        )
        existing = stations_by_key.get(key)
        if existing is None or len(station["source_tags"]) > len(existing["source_tags"]):
            stations_by_key[key] = station

    return sorted(stations_by_key.values(), key=lambda station: station["name"].lower())


def element_coordinates(element: Dict[str, Any]) -> Tuple[Optional[float], Optional[float]]:
    latitude = optional_float(element.get("lat"))
    longitude = optional_float(element.get("lon"))
    center = element.get("center")
    if (latitude is None or longitude is None) and isinstance(center, dict):
        latitude = optional_float(center.get("lat"))
        longitude = optional_float(center.get("lon"))
    if latitude is None or longitude is None:
        return None, None
    if not (-90 <= latitude <= 90 and -180 <= longitude <= 180):
        return None, None
    return latitude, longitude


# Namma Metro colour → buyer-facing trunk line. OSM stations often carry
# colour/ref instead of a clean `line=Purple Line` tag.
METRO_LINE_COLOR_HEX = {
    "e542de": "Purple Line",
    "800080": "Purple Line",
    "9b59b6": "Purple Line",
    "6b2d5c": "Purple Line",
    "2ecc71": "Green Line",
    "008000": "Green Line",
    "00a651": "Green Line",
    "39b54a": "Green Line",
    "f1c40f": "Yellow Line",
    "ffd100": "Yellow Line",
    "ffcc00": "Yellow Line",
    "ffeb3b": "Yellow Line",
    "e91e63": "Pink Line",
    "ff69b4": "Pink Line",
    "ec407a": "Pink Line",
    "3498db": "Blue Line",
    "0077c8": "Blue Line",
}


def station_lines(tags: Dict[str, str]) -> List[str]:
    """Normalize OSM station tags into clean trunk-line labels.

    Prefer explicit `line`/`lines`. Accept colour hex or colour names. Ignore
    short station refs such as `BYPH` that are not line identities.
    """
    values = []  # type: List[str]

    def push(raw):
        normalized = normalize_metro_line_label(raw)
        if normalized and normalized not in values:
            values.append(normalized)

    for key in ("line", "lines", "route_ref"):
        value = optional_string(tags.get(key))
        if not value:
            continue
        for part in split_tag_list(value):
            push(part)

    # `ref` is often a station code; only keep it when it already looks like a line.
    ref = optional_string(tags.get("ref"))
    if ref:
        for part in split_tag_list(ref):
            if looks_like_metro_line_name(part):
                push(part)

    for key in ("colour", "color", "colour:en", "color:en"):
        value = optional_string(tags.get(key))
        if value:
            push(value)

    return values


def normalize_metro_line_label(value: str) -> Optional[str]:
    text = optional_string(value)
    if not text:
        return None
    lower = text.lower().strip()
    hex_key = lower.lstrip("#")
    if hex_key in METRO_LINE_COLOR_HEX:
        return METRO_LINE_COLOR_HEX[hex_key]
    for color in ("purple", "green", "yellow", "pink", "blue", "orange", "red"):
        if color in lower and "line" in lower:
            return "{} Line".format(color.capitalize())
        if lower == color:
            return "{} Line".format(color.capitalize())
    if looks_like_metro_line_name(text):
        # Preserve already-clean labels such as "Purple Line".
        if lower.endswith(" line"):
            color = lower[: -len(" line")].strip()
            if color:
                return "{} Line".format(color.capitalize())
        return text
    return None


def looks_like_metro_line_name(value: str) -> bool:
    text = optional_string(value)
    if not text:
        return False
    lower = text.lower()
    if any(color in lower for color in ("purple", "green", "yellow", "pink", "blue")):
        return True
    if "line" in lower and len(text) >= 8:
        return True
    return False


def split_tag_list(value: str) -> List[str]:
    parts = [value]
    for separator in (";", ",", "|", "/"):
        next_parts = []  # type: List[str]
        for part in parts:
            next_parts.extend(part.split(separator))
        parts = next_parts
    return [part.strip() for part in parts if part.strip()]


def station_operational_status(tags: Dict[str, str]) -> str:
    if truthy_tag(tags.get("construction")) or tags.get("railway") == "construction":
        return "under_construction"
    if truthy_tag(tags.get("proposed")) or tags.get("railway") == "proposed":
        return "proposed"
    if truthy_tag(tags.get("disused")) or tags.get("railway") == "disused":
        return "disused"
    return "operational"


def truthy_tag(value: Any) -> bool:
    text = optional_string(value)
    return text is not None and text.lower() not in ("0", "false", "no")


def osm_element_url(element: Dict[str, Any]) -> Optional[str]:
    element_type = optional_string(element.get("type"))
    element_id = optional_string(element.get("id"))
    if not element_type or not element_id:
        return None
    return "https://www.openstreetmap.org/{}/{}".format(element_type, element_id)


def groundwater_zones_from_kml(kml_bytes: bytes) -> List[Dict[str, Any]]:
    root = ET.fromstring(kml_bytes)
    zones = []  # type: List[Dict[str, Any]]
    for index, placemark in enumerate(
        element for element in root.iter() if local_name(element.tag) == "Placemark"
    ):
        fields = placemark_fields(placemark)
        potential = optional_string(fields.get("GW_PROS"))
        if not potential:
            continue
        rings = [
            ring
            for ring in (
                coordinates_from_text(element.text or "")
                for element in placemark.iter()
                if local_name(element.tag) == "coordinates"
            )
            if len(ring) >= 3
        ]
        if not rings:
            continue
        zone_id = (
            optional_string(fields.get("GWATER_ID"))
            or optional_string(fields.get("OBJECTID"))
            or str(index)
        )
        zones.append(
            {
                "zone_id": zone_id,
                "groundwater_potential_class": potential,
                "rings": rings,
                "source_fields": fields,
            }
        )
    return zones


def placemark_fields(placemark: ET.Element) -> Dict[str, str]:
    fields = {}  # type: Dict[str, str]
    for data in placemark.iter():
        if local_name(data.tag) != "Data":
            continue
        key = data.attrib.get("name")
        if not key:
            continue
        for child in data:
            if local_name(child.tag) == "value" and child.text:
                fields[str(key)] = child.text.strip()
    for simple in placemark.iter():
        if local_name(simple.tag) != "SimpleData":
            continue
        key = simple.attrib.get("name")
        if key and simple.text:
            fields[str(key)] = simple.text.strip()
    return fields


def coordinates_from_text(text: str) -> List[Dict[str, float]]:
    coordinates = []  # type: List[Dict[str, float]]
    for part in text.split():
        pieces = part.split(",")
        if len(pieces) < 2:
            continue
        try:
            longitude = float(pieces[0])
            latitude = float(pieces[1])
        except ValueError:
            continue
        coordinates.append({"latitude": latitude, "longitude": longitude})
    return coordinates


def local_name(tag: str) -> str:
    return tag.rsplit("}", 1)[-1]


def record_source_failure(
    failures: Dict[str, str], asset_ids: List[str], error: Exception
) -> None:
    reason = "{}: {}".format(type(error).__name__, error)
    for asset_id in asset_ids:
        failures[asset_id] = reason
    logger.error("Source collection failed for %s: %s", ", ".join(asset_ids), reason)


def load_crawl_policy(policy_id: str) -> Optional[Dict[str, Any]]:
    path = DAG_ROOT / "crawl_policies" / f"{policy_id}.json"
    if not path.exists():
        return None
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def skip_reddit_collection() -> bool:
    env = os.environ.get("OPENESTATES_SKIP_REDDIT")
    if env is not None:
        return str(env).lower() in (
            "1",
            "true",
            "yes",
        )

    policy = load_crawl_policy("reddit_threads_daily")
    if policy is not None:
        return not bool(policy.get("enabled", True))

    # Legacy default when no crawl policy file exists.
    return True


def empty_reddit_assets(request: Dict[str, Any]) -> Tuple[Dict[str, Any], Dict[str, Any]]:
    from pipeline.skills.reddit_poc_import import collect_reddit_poc_fact_rows

    planned_at = normalized_planned_at(request)
    partition = partition_values(request)
    snapshot_date = partition.get("dt") or planned_at[:10]
    subreddit = partition.get("subreddit") or "BangaloreRealEstates"
    watermark = {"source": "reddit_skipped", "high_watermark": planned_at}
    poc_facts, poc_annotations = collect_reddit_poc_fact_rows(snapshot_date)
    return (
        {
            "snapshot_date": snapshot_date,
            "subreddit": subreddit,
            "records": [],
            "source_watermarks": [watermark],
        },
        {
            "source": "reddit",
            "snapshot_date": snapshot_date,
            "facts": poc_facts,
            "fact_annotations": poc_annotations,
            "source_watermarks": [watermark],
        },
    )


def get_skill_instance(skill_id: str) -> Any:
    if skill_id == "fetch_google_review_links":
        from pipeline.skills.fetch_google_review_links import FetchGoogleReviewLinksSkill

        return FetchGoogleReviewLinksSkill()
    if skill_id == "fetch_rera":
        from pipeline.skills.fetch_rera import FetchReraSkill

        return FetchReraSkill()
    raise ValueError("Unknown collector skill: {}".format(skill_id))


def load_society_inputs_for_reddit() -> Dict[str, Dict[str, Any]]:
    inputs: Dict[str, Dict[str, Any]] = {}
    kg_dir = KNOWLEDGE_DIR / "society"
    if not kg_dir.exists():
        return inputs

    for path in sorted(kg_dir.glob("*.json")):
        try:
            node = json.loads(path.read_text())
        except (json.JSONDecodeError, OSError):
            continue
        slug = path.stem
        name = node.get("name") or slug.replace("-", " ").title()
        area = node.get("area") or node_fact_text(node, "area")
        inputs[slug] = {
            "query": "{} {}".format(name, area).strip(),
            "subreddit": "bangalore",
            "entity_id": "society:{}".format(slug),
        }
    return inputs


def apply_google_origin_locations(
    inputs: Dict[str, Dict[str, Any]], google_places_input: Dict[str, Any] = None
) -> None:
    if not google_places_input:
        return
    for record in google_places_input.get("records") or []:
        entity_id = optional_string(record.get("entity_id"))
        input_data = inputs.get(entity_id)
        if input_data is None:
            continue
        existing_latitude, existing_longitude = select_coordinate_pair(
            [
                {
                    "source_type": "source_entity_seed",
                    "latitude": input_data.get("latitude"),
                    "longitude": input_data.get("longitude"),
                }
            ]
        )
        if existing_latitude is not None and existing_longitude is not None:
            continue
        latitude, longitude = select_coordinate_pair(
            [
                {
                    "source_type": "google",
                    "latitude": record.get("latitude"),
                    "longitude": record.get("longitude"),
                }
            ]
        )
        if latitude is None or longitude is None:
            continue
        input_data["latitude"] = latitude
        input_data["longitude"] = longitude
        input_data["google_place_id"] = optional_string(record.get("place_id"))


def node_fact_text(node: Dict[str, Any], key: str) -> str:
    for fact in node.get("facts", []):
        if fact.get("key") != key:
            continue
        value = fact.get("value", {})
        data = value.get("data") if isinstance(value, dict) else value
        return str(data or "").strip()
    return ""


def collect_google_places(
    request: Dict[str, Any],
    society_inputs: Dict[str, Dict[str, Any]] = None,
    skill: Any = None,
) -> Dict[str, Any]:
    from pipeline.skills.fetch_google_review_links import build_place_query

    planned_at = normalized_planned_at(request)
    snapshot_date = partition_values(request).get("dt") or planned_at[:10]
    inputs = society_inputs or {}
    google_skill = skill or get_skill_instance("fetch_google_review_links")
    force_refresh = GOOGLE_PLACES_WEEKLY in request.get("force_refresh_assets", [])
    records = []  # type: List[Dict[str, Any]]

    for slug, input_data in sorted(inputs.items()):
        query = build_place_query(input_data)
        result = google_skill.run(input_data, force=force_refresh)
        values = {fact.key: fact for fact in result.facts}
        url_fact = values.get("google_reviews_url")
        if not query or url_fact is None:
            logger.warning("Skipping Google place row without query/link for %s", slug)
            continue
        reviews_url = fact_data(url_fact)
        if not isinstance(reviews_url, str) or not reviews_url.strip():
            logger.warning("Skipping Google place row with invalid link for %s", slug)
            continue
        learned_at = max(
            (fact.learned_at for fact in result.facts if fact.learned_at),
            default=planned_at,
        )
        records.append(
            {
                "entity_id": str(input_data.get("entity_id") or ""),
                "project_key": optional_string(input_data.get("project_key")),
                "query": query,
                "place_name": optional_string(
                    input_data.get("society_name")
                    or input_data.get("name")
                    or input_data.get("project_name")
                ),
                "place_id": optional_string(fact_data(values.get("google_place_id"))),
                "reviews_url": reviews_url,
                "rating": optional_float(fact_data(values.get("google_rating"))),
                "review_count": optional_int(fact_data(values.get("google_review_count"))),
                "review_snippets": optional_string_list(
                    fact_data(values.get("google_review_snippets"))
                ),
                "latitude": optional_float(fact_data(values.get("geo.latitude"))),
                "longitude": optional_float(fact_data(values.get("geo.longitude"))),
                "address": optional_string(fact_data(values.get("google_place_address")))
                or optional_string(input_data.get("address")),
                "confidence": float(result.confidence),
                "fetched_at": learned_at,
                "fetch_source": google_fetch_source(result),
            }
        )

    logger.info("Collected %d Google place snapshots", len(records))
    return {
        "snapshot_date": snapshot_date,
        "records": records,
        "source_watermarks": [
            {
                "source": "fetch_google_review_links",
                "high_watermark": max(
                    (record["fetched_at"] for record in records), default=planned_at
                ),
            }
        ],
    }


def collect_google_nearby_places(
    request: Dict[str, Any],
    society_inputs: Dict[str, Dict[str, Any]] = None,
    nearby_fetch: Callable[[Dict[str, Any], str], List[Dict[str, Any]]] = None,
) -> Dict[str, Any]:
    from pipeline.skills.fetch_google_review_links import fetch_google_places_nearby_text

    planned_at = normalized_planned_at(request)
    snapshot_date = partition_values(request).get("dt") or planned_at[:10]
    inputs = society_inputs or {}
    fetch = nearby_fetch or fetch_google_places_nearby_text
    records = []  # type: List[Dict[str, Any]]
    categories = google_nearby_collection_categories()

    for slug, input_data in sorted(inputs.items()):
        for category in categories:
            query = nearby_query(input_data, category)
            try:
                nearby_places = fetch(input_data, category)
            except ValueError as exc:
                if str(exc) == "Google nearby collection requires an accepted origin coordinate pair":
                    logger.warning(
                        "Skipping Google nearby collection for %s: missing accepted origin coordinates",
                        slug,
                    )
                    break
                raise
            for place in nearby_places:
                name = optional_string(place.get("place_name") or place.get("name"))
                url = optional_string(place.get("place_url") or place.get("url"))
                if not name or not url:
                    logger.warning(
                        "Skipping Google nearby row without name/link for %s %s",
                        slug,
                        category,
                    )
                    continue
                records.append(
                    {
                        "entity_id": str(input_data.get("entity_id") or ""),
                        "project_key": optional_string(input_data.get("project_key")),
                        "query": optional_string(place.get("query")) or query,
                        "category": category,
                        "place_name": name,
                        "place_id": optional_string(place.get("place_id")),
                        "place_url": url,
                        "distance_km": optional_float(place.get("distance_km")),
                        "latitude": optional_float(place.get("latitude")),
                        "longitude": optional_float(place.get("longitude")),
                        "rating": optional_float(place.get("rating")),
                        "review_count": optional_int(place.get("review_count")),
                        "primary_type": optional_string(place.get("primary_type")),
                        "place_types": optional_string_list(place.get("place_types")),
                        "confidence": float(place.get("confidence") or 0.7),
                        "fetched_at": optional_string(place.get("fetched_at")) or planned_at,
                        "fetch_source": optional_string(place.get("fetch_source"))
                        or "google_nearby_places",
                    }
                )

    logger.info("Collected %d Google nearby place snapshots", len(records))
    return {
        "snapshot_date": snapshot_date,
        "records": records,
        "source_watermarks": [
            {
                "source": "google_nearby_places",
                "high_watermark": max(
                    (record["fetched_at"] for record in records), default=planned_at
                ),
            }
        ],
    }


def unavailable_google_nearby_fetch(
    _input_data: Dict[str, Any], _category: str
) -> List[Dict[str, Any]]:
    raise ValueError(
        "Google nearby collection needs a real Places nearby provider or local source input"
    )


def nearby_query(input_data: Dict[str, Any], category: str) -> str:
    base = society_query(input_data)
    label = nearby_category_label(category)
    if not base:
        return ""
    return "{} near {}".format(label, base)


def nearby_category_label(category: str) -> str:
    config = nearby_category_config(category)
    if config:
        return str(config.get("display_label") or category).replace("Nearby ", "").lower()
    return category.replace("-", "_").strip().lower().replace("_", " ")


def google_nearby_collection_categories() -> Tuple[str, ...]:
    categories = []
    for category in nearby_category_configs():
        if not nearby_category_supports_collection_source(category, "google"):
            continue
        aliases = category.get("category_aliases") or []
        if aliases:
            categories.append(str(aliases[0]))
    return tuple(categories)


def nearby_category_supports_collection_source(
    category: Dict[str, Any], source: str
) -> bool:
    sources = category.get("collection_sources") or []
    normalized_source = source.replace("-", "_").strip().lower()
    return not sources or normalized_source in {
        str(value).replace("-", "_").strip().lower() for value in sources
    }


def nearby_category_config(category: str) -> Optional[Dict[str, Any]]:
    normalized = category.replace("-", "_").strip().lower()
    for config in nearby_category_configs():
        aliases = {
            str(alias).replace("-", "_").strip().lower()
            for alias in config.get("category_aliases") or []
        }
        if normalized in aliases:
            return config
    return None


def nearby_category_configs() -> List[Dict[str, Any]]:
    try:
        payload = json.loads(
            (DAG_ROOT / "nearby_place_categories.json").read_text(encoding="utf-8")
        )
    except (OSError, json.JSONDecodeError):
        return []
    return [
        category
        for category in payload.get("categories", [])
        if isinstance(category, dict) and category.get("category_aliases")
    ]


def google_society_inputs(
    request: Dict[str, Any], rera_input: Dict[str, Any] = None
) -> Dict[str, Dict[str, Any]]:
    """Build Google resolver inputs with RERA addresses when available.

    Google Places can return only a route for society-name queries such as
    "Godrej Air". RERA project address is used only as resolver evidence; RERA
    coordinates are intentionally not copied into Google inputs.
    """
    address_input = rera_input or rera_address_input_for_request(request)
    return source_society_inputs(request, address_input)


def rera_address_input_for_request(request: Dict[str, Any]) -> Dict[str, Any]:
    """Hydrate RERA address facts for Google-only scoped source collection."""
    if not needs_rera_address_hydration(request):
        return {}
    detail_facts, _annotations, _watermark = collect_rera_project_details(request)
    if not detail_facts:
        return {}
    return {"detail_facts": detail_facts}


def needs_rera_address_hydration(request: Dict[str, Any]) -> bool:
    """Return true when scoped Google inputs lack address evidence."""
    return any(
        isinstance(seed, dict) and not optional_string(seed.get("address"))
        for seed in request.get("source_entities", [])
    )


def reddit_society_inputs(
    request: Dict[str, Any], rera_input: Dict[str, Any] = None
) -> Dict[str, Dict[str, Any]]:
    subreddit = partition_values(request).get("subreddit") or "bangalore"
    inputs = source_society_inputs(request, rera_input)
    for input_data in inputs.values():
        input_data["query"] = society_query(input_data)
        input_data["subreddit"] = subreddit
    return inputs


def source_society_inputs(
    request: Dict[str, Any], rera_input: Dict[str, Any] = None
) -> Dict[str, Dict[str, Any]]:
    inputs = {}  # type: Dict[str, Dict[str, Any]]
    rera_addresses = rera_detail_facts_by_entity_for_keys(
        rera_input or {}, {"rera_project_address"}
    )
    for seed in request.get("source_entities", []):
        entity_id = str(seed.get("entity_id") or "").strip()
        name = str(seed.get("name") or "").strip()
        if not entity_id or not name:
            continue
        address = source_entity_address(seed, rera_addresses)
        inputs[entity_id] = {
            "entity_id": entity_id,
            "alias_entity_id": optional_string(seed.get("alias_entity_id")),
            "project_key": optional_string(seed.get("project_key")),
            "society_name": name,
            "area": optional_string(seed.get("area")),
            "city": optional_string(seed.get("city")) or "Bengaluru",
            "address": address,
            "latitude": optional_float(seed.get("latitude")),
            "longitude": optional_float(seed.get("longitude")),
        }
    if inputs or not rera_input:
        return inputs

    known_project_keys = {
        input_data.get("project_key")
        for input_data in inputs.values()
        if input_data.get("project_key")
    }
    for project in rera_input.get("projects", []):
        project_key = optional_string(project.get("registration_number")) or optional_string(
            project.get("ack_number")
        )
        name = optional_string(project.get("project_name"))
        if not project_key or not name or project_key in known_project_keys:
            continue
        inputs[project_key] = {
            "entity_id": "",
            "project_key": project_key,
            "society_name": name,
            "area": optional_string(project.get("area_name")),
            "city": "Bengaluru",
            "address": optional_string(project.get("project_address")),
        }
        known_project_keys.add(project_key)
    return inputs


def source_entity_address(
    seed: Dict[str, Any], rera_addresses: Dict[str, Dict[str, Any]]
) -> Optional[str]:
    address = optional_string(seed.get("address"))
    if address:
        return address
    for candidate_id in (
        optional_string(seed.get("entity_id")),
        optional_string(seed.get("alias_entity_id")),
    ):
        if not candidate_id:
            continue
        rera_facts = rera_addresses.get(candidate_id, {})
        address = optional_string(rera_facts.get("rera_project_address"))
        if address:
            return address
    return None


def request_with_rera_detail_facts(
    request: Dict[str, Any], rera_input: Dict[str, Any] = None
) -> Dict[str, Any]:
    if not rera_input:
        return request
    facts_by_entity = rera_detail_facts_by_entity(rera_input)
    if not facts_by_entity:
        return request
    enriched = dict(request)
    source_entities = []
    changed = False
    for seed in request.get("source_entities", []):
        if not isinstance(seed, dict):
            source_entities.append(seed)
            continue
        entity_ids = [
            optional_string(seed.get("entity_id")),
            optional_string(seed.get("alias_entity_id")),
        ]
        rera_facts = {}
        for entity_id in entity_ids:
            if entity_id and entity_id in facts_by_entity:
                rera_facts.update(facts_by_entity[entity_id])
        if not rera_facts:
            source_entities.append(seed)
            continue
        updated = dict(seed)
        if "rera_configurations" not in updated and rera_facts.get("available_configurations"):
            updated["rera_configurations"] = rera_facts["available_configurations"]
            changed = True
        if "rera_plan_artifact_manifest" not in updated and rera_facts.get("rera_plan_artifact_manifest"):
            updated["rera_plan_artifact_manifest"] = rera_facts["rera_plan_artifact_manifest"]
            changed = True
        source_entities.append(updated)
    if not changed:
        return request
    enriched["source_entities"] = source_entities
    return enriched


def rera_detail_facts_by_entity(rera_input: Dict[str, Any]) -> Dict[str, Dict[str, Any]]:
    by_entity = {}  # type: Dict[str, Dict[str, Any]]
    for fact in rera_input.get("detail_facts") or []:
        entity_id = optional_string(fact.get("entity_id"))
        fact_key = optional_string(fact.get("fact_key"))
        if not entity_id or fact_key not in (
            "available_configurations",
            "rera_plan_artifact_manifest",
        ):
            continue
        value = fact_value_data(fact)
        if value is None:
            continue
        by_entity.setdefault(entity_id, {})[fact_key] = value
    return by_entity


def fact_value_data(fact: Dict[str, Any]) -> Any:
    try:
        value = json.loads(str(fact.get("value_json") or "{}"))
    except (TypeError, ValueError):
        return None
    if not isinstance(value, dict):
        return None
    return value.get("data")


def society_query(input_data: Dict[str, Any]) -> str:
    name = optional_string(
        input_data.get("society_name")
        or input_data.get("name")
        or input_data.get("project_name")
    )
    if not name:
        return ""
    parts = [name]
    for key in ("area", "city"):
        value = optional_string(input_data.get(key))
        if value and value.lower() not in name.lower():
            parts.append(value)
    return " ".join(parts)


def fact_data(fact: Any) -> Any:
    if fact is None or not isinstance(fact.value, dict):
        return None
    return fact.value.get("data")


def google_fetch_source(result: Any) -> str:
    if result.cached:
        return "fetch_google_review_links_cache"
    for fact in getattr(result, "facts", []):
        source = getattr(fact, "source", None)
        triggered_by = getattr(source, "triggered_by", None)
        if isinstance(triggered_by, str) and ":" in triggered_by:
            return triggered_by.split(":", 1)[0]
    if result.cost.api_calls:
        return "serpapi_google_maps"
    return "google_maps_search_fallback"


def collect_rera_registry(
    request: Dict[str, Any],
    rera_fetch: Callable[[], Any] = None,
    detail_skill: Any = None,
) -> Dict[str, Any]:
    entries, observed_at = (rera_fetch or fetch_rera_listing_snapshot)()
    entries = list(entries)
    selected_keys, selected_names = rera_project_selectors(request)
    projects = []  # type: List[Dict[str, Any]]
    skipped = 0
    for entry in entries:
        project_name = optional_text(entry, "project_name")
        if not project_name:
            skipped += 1
            continue
        ack_number = optional_text(entry, "ack_number")
        registration_number = optional_text(entry, "registration_number")
        if selected_keys or selected_names:
            identifiers = [
                normalize_selector(ack_number),
                normalize_selector(registration_number),
                normalize_selector(project_name),
            ]
            if not any(identifier in selected_keys for identifier in identifiers) and (
                normalize_selector(project_name) not in selected_names
            ):
                continue
        projects.append(
            {
                "ack_number": ack_number,
                "registration_number": registration_number,
                "project_name": project_name,
                "promoter_name": optional_text(entry, "promoter_name"),
                "status": None,
                "project_type": None,
                "project_address": None,
                "area_name": None,
                "district": None,
                "taluk": None,
                "total_land_area_sqm": None,
                "land_litigation": None,
                "source_url": LISTING_URL,
                "fetched_at": observed_at,
            }
        )

    if skipped:
        logger.warning("Skipped %d RERA rows without a project name", skipped)
    if selected_keys or selected_names:
        logger.info("Collected %d scoped RERA listing rows", len(projects))
    else:
        logger.info("Collected %d valid RERA listing rows", len(projects))
    detail_facts, detail_annotations, detail_watermark = collect_rera_project_details(
        request, detail_skill
    )
    source_watermarks = [
        {"source": "karnataka_rera_listing", "high_watermark": observed_at}
    ]
    if detail_watermark:
        source_watermarks.append(
            {
                "source": "karnataka_rera_project_details",
                "high_watermark": detail_watermark,
            }
        )
    return {
        "snapshot_date": observed_at[:7],
        "projects": projects,
        "detail_facts": detail_facts,
        "detail_fact_annotations": detail_annotations,
        "source_watermarks": source_watermarks,
    }


def rera_project_selectors(request: Dict[str, Any]) -> Tuple[set, set]:
    source_entities = request.get("source_entities", [])
    keys = set()
    names = set()
    for seed in source_entities:
        for key in ("project_key", "registration_number", "ack_number"):
            value = normalize_selector(seed.get(key))
            if value:
                keys.add(value)
        name = normalize_selector(seed.get("name") or seed.get("project_name"))
        if name:
            names.add(name)
            keys.add(name)
    return keys, names


def normalize_selector(value: Any) -> str:
    text = optional_string(value)
    if not text:
        return ""
    return " ".join(text.lower().split())


def collect_rera_project_details(
    request: Dict[str, Any], skill: Any = None
) -> Tuple[List[Dict[str, Any]], List[Dict[str, Any]], Any]:
    source_entities = request.get("source_entities", [])
    if not source_entities:
        return [], [], None

    if skill is None:
        skill = get_skill_instance("fetch_rera")

    snapshot_date = partition_values(request).get("dt") or normalized_planned_at(request)[:10]
    force_refresh = RERA_REGISTRY_MONTHLY in request.get("force_refresh_assets", [])
    facts = []  # type: List[Dict[str, Any]]
    annotations = []  # type: List[Dict[str, Any]]
    latest_learned_at = None

    for seed in source_entities:
        entity_id = optional_string(seed.get("entity_id"))
        project_name = optional_string(seed.get("name"))
        if not entity_id or not project_name:
            logger.warning("Skipping RERA detail input without entity ID or project name")
            continue
        input_data = {
            "entity_id": entity_id,
            "project_name": project_name,
            "project_key": optional_string(seed.get("project_key")),
            "area": optional_string(seed.get("area")),
            "city": optional_string(seed.get("city")) or "Bengaluru",
            "latitude": optional_float(seed.get("latitude")),
            "longitude": optional_float(seed.get("longitude")),
            "triggered_by": "asset_dag",
        }
        profile_facts, profile_annotations = source_entity_profile_rows(
            entity_id, snapshot_date, input_data
        )
        facts.extend(profile_facts)
        annotations.extend(profile_annotations)
        try:
            result = skill.run(input_data, force=force_refresh)
        except Exception as error:
            logger.error("RERA detail collection failed for %s: %s", project_name, error)
            continue
        if not result.facts:
            logger.warning("RERA detail collection returned no facts for %s", project_name)
            continue

        result_facts, result_annotations = skill_result_rows(
            entity_id,
            "fetch_rera",
            snapshot_date,
            input_data,
            result,
        )
        facts.extend(result_facts)
        annotations.extend(result_annotations)
        alias_entity_id = optional_string(seed.get("alias_entity_id"))
        if alias_entity_id and alias_entity_id != entity_id:
            alias_facts, alias_annotations = skill_result_rows(
                alias_entity_id,
                "fetch_rera",
                snapshot_date,
                input_data,
                result,
            )
            facts.extend(alias_facts)
            annotations.extend(alias_annotations)
        for fact in result.facts:
            if fact.learned_at:
                latest_learned_at = max(latest_learned_at or fact.learned_at, fact.learned_at)

    logger.info(
        "Collected %d detailed RERA facts for %d selected societies",
        len(facts),
        len(source_entities),
    )
    return facts, annotations, latest_learned_at


def source_entity_profile_rows(
    entity_id: str, snapshot_date: str, input_data: Dict[str, Any]
) -> Tuple[List[Dict[str, Any]], List[Dict[str, Any]]]:
    learned_at = datetime.now(timezone.utc).isoformat()
    input_hash = "sha256:{}".format(
        hashlib.sha256(
            json.dumps(input_data, sort_keys=True, separators=(",", ":")).encode("utf-8")
        ).hexdigest()
    )
    rows = []  # type: List[Tuple[str, str, str, str, List[str], float]]
    for key, value, template, preferences in (
        (
            "title",
            optional_string(input_data.get("project_name")),
            "Project: {value}",
            ["project name", "society name"],
        ),
        (
            "area",
            optional_string(input_data.get("area")),
            "Area: {value}",
            ["area", "location", "neighbourhood"],
        ),
        (
            "city",
            optional_string(input_data.get("city")),
            "City: {value}",
            ["city", "location"],
        ),
    ):
        if value:
            rows.append((key, "text", value, template, preferences, 0.95))
    latitude = optional_float(input_data.get("latitude"))
    longitude = optional_float(input_data.get("longitude"))
    if latitude is not None and longitude is not None:
        rows.extend(
            (
                (
                    "geo.latitude",
                    "numeric",
                    latitude,
                    "Latitude: {value}",
                    ["coordinates", "location", "latitude"],
                    0.9,
                ),
                (
                    "geo.longitude",
                    "numeric",
                    longitude,
                    "Longitude: {value}",
                    ["coordinates", "location", "longitude"],
                    0.9,
                ),
            )
        )
    rows.append(
        (
            "source_scan_selected",
            "bool",
            True,
            "Selected for fresh area scan: {value}",
            ["fresh scan", "area tracker scan"],
            1.0,
        )
    )

    facts = []
    annotations = []
    run_id = "collector-source_entity_profile-{}".format(snapshot_date)
    for key, value_type, value, template, preferences, confidence in rows:
        if value_type == "bool":
            value_json = json.dumps({"type": "Bool", "data": bool(value)}, separators=(",", ":"))
        elif value_type == "numeric":
            value_json = json.dumps(
                {"type": "Numeric", "data": float(value)}, separators=(",", ":")
            )
        else:
            value_json = json.dumps({"type": "Text", "data": str(value)}, separators=(",", ":"))
        facts.append(
            {
                "entity_id": entity_id,
                "fact_key": key,
                "value_type": value_type,
                "value_json": value_json,
                "confidence": confidence,
                "source_type": "SourceEntitySeed",
                "source_url": None,
                "model": None,
                "skill_id": "source_entity_profile",
                "triggered_by": "asset_dag",
                "learned_at": learned_at,
                "run_id": run_id,
                "input_hash": input_hash,
            }
        )
        annotations.append(
            {
                "entity_id": entity_id,
                "fact_key": key,
                "display_template": template,
                "answers_preferences_json": json.dumps(preferences, separators=(",", ":")),
                "scoring_direction": "TextMatch",
                "scoring_weight": 2.0,
                "scoring_thresholds_json": "[]",
                "updated_at": learned_at,
            }
        )
    return facts, annotations


def fetch_rera_listing_snapshot():
    entries = scrape_rera_listing()
    observed_at = datetime.now(timezone.utc).isoformat()
    if LISTING_CACHE_PATH.exists():
        try:
            cache = json.loads(LISTING_CACHE_PATH.read_text())
            observed_at = str(cache.get("cached_at") or observed_at)
        except (OSError, ValueError, TypeError):
            logger.warning("Could not read RERA cache observation timestamp")
    return entries, observed_at


def collect_rera_receipts(request: Dict[str, Any]) -> Dict[str, Any]:
    """Emit raw K-RERA listing and explicitly scoped project-detail receipts.

    Project details are fetched again from K-RERA before entering L0. Old HTML
    cache files without their URL and capture metadata are deliberately not
    admitted as evidence receipts.
    """
    if not LISTING_RAW_CACHE_PATH.exists():
        scrape_rera_listing(force=True)
    if not LISTING_RAW_CACHE_PATH.exists():
        raise ValueError("K-RERA listing raw receipt was not captured")

    observed_at = datetime.now(timezone.utc).isoformat()
    if LISTING_CACHE_PATH.exists():
        try:
            cached = json.loads(LISTING_CACHE_PATH.read_text())
            observed_at = str(cached.get("cached_at") or observed_at)
        except (OSError, ValueError, TypeError):
            logger.warning("Could not read K-RERA listing receipt timestamp")
    body = LISTING_RAW_CACHE_PATH.read_bytes()
    if not body:
        raise ValueError("K-RERA listing raw receipt is empty")
    listing_receipt_id = "rera_receipt:sha256:{}".format(hashlib.sha256(body).hexdigest())
    receipts = [
        {
            "kind": "registry_listing",
            "source_url": LISTING_URL,
            "content_type": "text/html",
            "body_hex": body.hex(),
            "captured_at": observed_at,
            "crawl_run_id": "rera-listing-{}".format(observed_at[:10]),
        }
    ]
    force_refresh = RERA_RECEIPTS in set(request.get("force_refresh_assets") or [])
    if force_refresh:
        detail_snapshots = capture_scoped_rera_detail_receipts(
            request, listing_receipt_id
        )
    else:
        try:
            detail_snapshots = load_scoped_rera_detail_receipts(request)
        except ValueError:
            detail_snapshots = capture_scoped_rera_detail_receipts(
                request, listing_receipt_id
            )
    for snapshot in detail_snapshots:
        receipts.append(
            {
                "kind": "project_detail",
                "source_url": snapshot["source_url"],
                "content_type": "text/html",
                "body_hex": snapshot["body_hex"],
                "captured_at": snapshot["captured_at"],
                "registration_number": snapshot["registration_number"],
                "parent_receipt_id": snapshot["parent_receipt_id"],
                "crawl_run_id": snapshot["crawl_run_id"],
            }
        )
    return {
        "snapshot_date": observed_at[:10],
        "receipts": receipts,
        "source_watermarks": [
            {"source": "karnataka_rera_listing_receipt", "high_watermark": observed_at}
        ] + [
            {
                "source": "karnataka_rera_project_detail_receipt",
                "high_watermark": snapshot["captured_at"],
            }
            for snapshot in detail_snapshots
        ],
    }


def collect_rera_source_records(request: Dict[str, Any]) -> Dict[str, Any]:
    """Normalize L0 listing and scoped project-detail receipts into L1 rows."""
    if not LISTING_RAW_CACHE_PATH.exists():
        scrape_rera_listing(force=True)
    if not LISTING_RAW_CACHE_PATH.exists():
        raise ValueError("K-RERA listing raw receipt was not captured")

    observed_at = datetime.now(timezone.utc).isoformat()
    if LISTING_CACHE_PATH.exists():
        try:
            cached = json.loads(LISTING_CACHE_PATH.read_text())
            observed_at = str(cached.get("cached_at") or observed_at)
        except (OSError, ValueError, TypeError):
            logger.warning("Could not read K-RERA listing source-record timestamp")
    body = LISTING_RAW_CACHE_PATH.read_bytes()
    if not body:
        raise ValueError("K-RERA listing raw receipt is empty")

    receipt_id = "rera_receipt:sha256:{}".format(hashlib.sha256(body).hexdigest())
    capture_observed_at = observed_at[:-1] + "+00:00" if observed_at.endswith("Z") else observed_at
    capture_material = "rera_capture.v1\n{}\n{}\n{}".format(
        receipt_id, LISTING_URL, capture_observed_at
    )
    capture_id = "rera_capture:sha256:{}".format(
        hashlib.sha256(capture_material.encode("utf-8")).hexdigest()
    )
    listing_text = body.decode("utf-8", errors="replace")
    arrays = {}
    for suffix in ("", "2", "3", "4"):
        name = "applicationNameList{}".format(suffix)
        arrays[name] = re.findall(r"{}\s*\.push\('([^']*)'\)".format(name), listing_text)
    counts = {name: len(values) for name, values in arrays.items()}
    if len(set(counts.values())) != 1 or not next(iter(counts.values()), 0):
        raise ValueError("K-RERA listing arrays are incomplete: {}".format(counts))

    records = []
    scoped_entities = scoped_rera_entities(request)
    scoped_by_registration = {
        entity["registration_number"]: entity for entity in scoped_entities
    }
    scoped_registrations = set(scoped_by_registration)
    for index, registration_number in enumerate(arrays["applicationNameList2"]):
        normalized_registration = normalized_registration_number(registration_number)
        if scoped_registrations and normalized_registration not in scoped_registrations:
            continue
        if not registration_number.strip():
            raise ValueError("K-RERA listing row {} has no registration number".format(index))
        records.append(
            {
                "kind": "registration_summary",
                "registration_number": registration_number,
                "receipt_id": receipt_id,
                "capture_id": capture_id,
                "source_locator": "applicationNameList[{}]".format(index),
                "raw_label": "K-RERA listing row",
                "raw_value": json.dumps(
                    {
                        "acknowledgement_number": arrays["applicationNameList"][index],
                        "registration_number": registration_number,
                        "project_name": arrays["applicationNameList3"][index],
                        "promoter_name": arrays["applicationNameList4"][index],
                    },
                    ensure_ascii=False,
                    separators=(",", ":"),
                ),
                "observed_at": observed_at,
                "parser_version": "rera_listing_source_records.v1",
            }
        )
        scoped_entity = scoped_by_registration.get(normalized_registration)
        if scoped_entity:
            records.append(
                {
                    "kind": "registration_relation",
                    "registration_number": registration_number,
                    "receipt_id": receipt_id,
                    "capture_id": capture_id,
                    "source_locator": "applicationNameList[{}]".format(index),
                    "raw_label": "Catalog project key exact registration match",
                    "raw_value": json.dumps(
                        {
                            "entity_id": scoped_entity["entity_id"],
                            "entity_type": "society",
                            "resolution_method": "catalog_project_key_exact",
                            "resolution_confidence": 1.0,
                        },
                        ensure_ascii=False,
                        separators=(",", ":"),
                    ),
                    "observed_at": observed_at,
                    "parser_version": "rera_registration_relation.v1",
                }
            )
    if scoped_registrations:
        found_registrations = {
            normalized_registration_number(record["registration_number"])
            for record in records
        }
        missing_registrations = sorted(scoped_registrations - found_registrations)
        if missing_registrations:
            raise ValueError(
                "K-RERA listing is missing scoped registration(s): {}".format(
                    ", ".join(missing_registrations)
                )
            )
    detail_snapshots = load_scoped_rera_detail_receipts(request)
    for snapshot in detail_snapshots:
        records.extend(rera_project_detail_source_records(snapshot))
    return {
        "snapshot_date": observed_at[:10],
        "records": records,
        "source_watermarks": [
            {"source": "karnataka_rera_listing_source_records", "high_watermark": observed_at}
        ] + [
            {
                "source": "karnataka_rera_project_detail_source_records",
                "high_watermark": snapshot["captured_at"],
            }
            for snapshot in detail_snapshots
        ],
    }


def normalized_registration_number(value: str) -> str:
    return " ".join(str(value or "").strip().upper().split())


def rera_receipt_id(body: bytes) -> str:
    return "rera_receipt:sha256:{}".format(hashlib.sha256(body).hexdigest())


def rera_capture_id(receipt_id: str, source_url: str, captured_at: str) -> str:
    timestamp = captured_at[:-1] + "+00:00" if captured_at.endswith("Z") else captured_at
    material = "rera_capture.v1\n{}\n{}\n{}".format(receipt_id, source_url, timestamp)
    return "rera_capture:sha256:{}".format(
        hashlib.sha256(material.encode("utf-8")).hexdigest()
    )


def scoped_rera_entities(request: Dict[str, Any]) -> List[Dict[str, str]]:
    entities = []
    seen = set()
    for entity in request.get("source_entities") or []:
        registration_number = normalized_registration_number(entity.get("project_key"))
        if not registration_number:
            continue
        if registration_number in seen:
            continue
        name = str(entity.get("name") or "").strip()
        entity_id = str(entity.get("entity_id") or "").strip()
        if not name:
            raise ValueError(
                "RERA detail collection requires a project name for {}".format(
                    registration_number
                )
            )
        if not entity_id:
            raise ValueError(
                "RERA detail collection requires an entity ID for {}".format(
                    registration_number
                )
            )
        seen.add(registration_number)
        entities.append(
            {
                "registration_number": registration_number,
                "project_name": name,
                "entity_id": entity_id,
            }
        )
    return entities


def detail_receipt_metadata_path(registration_number: str) -> Path:
    digest = hashlib.sha256(registration_number.encode("utf-8")).hexdigest()
    return RERA_DETAIL_RECEIPT_CACHE_DIR / "{}.json".format(digest)


def capture_scoped_rera_detail_receipts(
    request: Dict[str, Any], parent_receipt_id: str
) -> List[Dict[str, Any]]:
    """Fetch only registration-scoped project-detail pages and persist capture metadata."""
    snapshots = []
    for entity in scoped_rera_entities(request):
        session = ReraSession()
        search_result = search_rera_project(session, entity["project_name"])
        if search_result is None:
            raise ValueError(
                "K-RERA did not return a project detail for {}".format(
                    entity["registration_number"]
                )
            )
        found_registration = normalized_registration_number(search_result.registration_number)
        if found_registration != entity["registration_number"]:
            raise ValueError(
                "K-RERA project search for {!r} returned {}, not requested {}".format(
                    entity["project_name"],
                    found_registration or "no registration number",
                    entity["registration_number"],
                )
            )
        body = session.post_bytes(
            DETAIL_URL, {"action": search_result.numeric_id}, ajax=True, timeout=120
        )
        if not body:
            raise ValueError(
                "K-RERA returned an empty project-detail receipt for {}".format(
                    entity["registration_number"]
                )
            )
        captured_at = datetime.now(timezone.utc).isoformat()
        source_url = "{}?action={}".format(DETAIL_URL, search_result.numeric_id)
        snapshot = {
            "registration_number": entity["registration_number"],
            "source_url": source_url,
            "captured_at": captured_at,
            "parent_receipt_id": parent_receipt_id,
            "crawl_run_id": "rera-project-detail-{}".format(captured_at[:10]),
            "body_hex": body.hex(),
            "body_sha256": hashlib.sha256(body).hexdigest(),
        }
        metadata_path = detail_receipt_metadata_path(entity["registration_number"])
        metadata_path.parent.mkdir(parents=True, exist_ok=True)
        metadata_path.write_text(json.dumps(snapshot, indent=2), encoding="utf-8")
        snapshots.append(snapshot)
    return snapshots


def load_scoped_rera_detail_receipts(request: Dict[str, Any]) -> List[Dict[str, Any]]:
    """Load only captures whose provenance and body hash are intact."""
    snapshots = []
    for entity in scoped_rera_entities(request):
        metadata_path = detail_receipt_metadata_path(entity["registration_number"])
        if not metadata_path.exists():
            raise ValueError(
                "No provenance-complete K-RERA detail receipt exists for {}; "
                "run rera_receipts first".format(entity["registration_number"])
            )
        snapshot = json.loads(metadata_path.read_text(encoding="utf-8"))
        required = (
            "registration_number",
            "source_url",
            "captured_at",
            "parent_receipt_id",
            "crawl_run_id",
            "body_hex",
            "body_sha256",
        )
        if any(not snapshot.get(key) for key in required):
            raise ValueError("K-RERA detail capture metadata is incomplete: {}".format(metadata_path))
        if normalized_registration_number(snapshot["registration_number"]) != entity["registration_number"]:
            raise ValueError("K-RERA detail capture registration scope mismatch: {}".format(metadata_path))
        try:
            body = bytes.fromhex(snapshot["body_hex"])
        except ValueError as error:
            raise ValueError("K-RERA detail capture has invalid body encoding: {}".format(metadata_path)) from error
        if hashlib.sha256(body).hexdigest() != snapshot["body_sha256"]:
            raise ValueError("K-RERA detail capture body hash does not match metadata: {}".format(metadata_path))
        snapshots.append(snapshot)
    return snapshots


def clean_html_fragment(value: str) -> str:
    return re.sub(r"\s+", " ", unescape(re.sub(r"<[^>]+>", " ", value or ""))).strip()


def rera_detail_labeled_value(detail_html: str, label: str) -> Optional[str]:
    pattern = re.compile(
        re.escape(label)
        + r".{0,300}?</p>\s*</div>\s*<div[^>]*>\s*<p[^>]*>\s*(.*?)\s*</p>",
        re.IGNORECASE | re.DOTALL,
    )
    match = pattern.search(detail_html)
    return clean_html_fragment(match.group(1)) if match else None


def iso_rera_date(value: str) -> Optional[str]:
    for fmt in ("%d-%m-%Y", "%d/%m/%Y", "%Y-%m-%d"):
        try:
            return datetime.strptime(value.strip(), fmt).date().isoformat()
        except (TypeError, ValueError):
            continue
    return None


def rera_square_metres(value: str) -> Optional[float]:
    """Parse a structured K-RERA square-metre value without guessing text."""
    match = re.fullmatch(
        r"\s*([0-9][0-9,]*(?:\.[0-9]+)?)\s*(?:sq\.?\s*m(?:tr|etre)?s?)?\s*",
        value,
        re.IGNORECASE,
    )
    if not match:
        return None
    normalized = match.group(1).replace(",", "")
    try:
        parsed = float(normalized)
    except (TypeError, ValueError):
        return None
    return parsed if parsed >= 0 else None


def project_detail_receipt_ids(snapshot: Dict[str, Any]) -> Tuple[str, str]:
    body = bytes.fromhex(snapshot["body_hex"])
    receipt_id = rera_receipt_id(body)
    return receipt_id, rera_capture_id(receipt_id, snapshot["source_url"], snapshot["captured_at"])


def rera_declared_inventory_rows(detail_html: str) -> List[Dict[str, Any]]:
    """Read the project-level inventory table without confusing it with QPR tables."""
    heading = re.search(
        r"Development\s*<span>\s*Details\s*\(\s*Bifurcation of Type of Inventories",
        detail_html,
        re.IGNORECASE | re.DOTALL,
    )
    if not heading:
        return rera_legacy_declared_inventory_rows(detail_html)
    table_start = detail_html.find("<table", heading.end())
    if table_start < 0:
        return []
    table_end = detail_html.find("</table>", table_start)
    if table_end < 0:
        return []
    table_html = detail_html[table_start : table_end + len("</table>")]
    header = clean_html_fragment(table_html[: table_html.find("</thead>")])
    if "Type of Inventory" not in header or "Carpet Area" not in header:
        return []
    rows = []
    for row_html in re.findall(r"<tr[^>]*>(.*?)</tr>", table_html, re.IGNORECASE | re.DOTALL):
        cells = [
            clean_html_fragment(cell)
            for cell in re.findall(r"<td[^>]*>(.*?)</td>", row_html, re.IGNORECASE | re.DOTALL)
        ]
        if len(cells) != 6:
            continue
        label = cells[1].strip()
        if not label:
            continue
        unit_count_value = cells[2].replace(",", "").strip()
        area_values = [rera_square_metres(cell) for cell in cells[3:]]
        rows.append(
            {
                "row_number": cells[0].strip(),
                "inventory_type": label,
                "unit_count": int(unit_count_value) if unit_count_value.isdigit() else None,
                "total_carpet_area_sqm": area_values[0],
                "total_balcony_verandah_area_sqm": area_values[1],
                "total_open_terrace_area_sqm": area_values[2],
            }
        )
    return rows


def rera_legacy_declared_inventory_rows(detail_html: str) -> List[Dict[str, Any]]:
    """Read older K-RERA inventory label/value blocks and retain filed values."""
    matches = list(re.finditer(r"Type of Inventory\s*<span[^>]*>.*?</p>", detail_html, re.I | re.S))
    rows = []
    seen = set()
    for index, match in enumerate(matches):
        end = matches[index + 1].start() if index + 1 < len(matches) else min(
            len(detail_html), match.start() + 5_000
        )
        block = detail_html[match.start() : end]

        def value_after(label: str) -> Optional[str]:
            found = re.search(
                re.escape(label)
                + r"\s*<span[^>]*>.*?</p>\s*</div>\s*<div[^>]*>\s*<p[^>]*>(.*?)</p>",
                block,
                re.I | re.S,
            )
            return clean_html_fragment(found.group(1)) if found else None

        inventory_type = value_after("Type of Inventory")
        unit_text = value_after("No of Inventory")
        carpet_text = value_after("Carpet Area (Sq Mtr)")
        if not inventory_type or not unit_text or not unit_text.replace(",", "").isdigit():
            continue
        balcony_text = value_after("Area of exclusive balcony/verandah (Sq Mtr)")
        terrace_text = value_after("Area of exclusive open Terrace (Sq Mtr)")
        row = {
            "row_number": str(index + 1),
            "inventory_type": inventory_type,
            "unit_count": int(unit_text.replace(",", "")),
            "filed_carpet_area_sqm": rera_square_metres(carpet_text) if carpet_text else None,
            "filed_balcony_verandah_area_sqm": rera_square_metres(balcony_text) if balcony_text else None,
            "filed_open_terrace_area_sqm": rera_square_metres(terrace_text) if terrace_text else None,
            "area_scope": "source_unspecified",
            "filed_values": {
                "unit_count": unit_text,
                "carpet_area": carpet_text,
                "balcony_verandah_area": balcony_text,
                "open_terrace_area": terrace_text,
            },
        }
        identity = json.dumps(
            {
                key: value
                for key, value in row.items()
                if key not in {"row_number", "filed_values"}
            },
            sort_keys=True,
        )
        if identity not in seen:
            seen.add(identity)
            rows.append(row)
    return rows


def rera_project_detail_source_records(snapshot: Dict[str, Any]) -> List[Dict[str, Any]]:
    """Parse source-preserving RERA detail records needed by the first report slice.

    This deliberately covers the project summary, water declaration, QPR
    inventory totals, and document metadata. Other page sections remain in the
    immutable receipt and are reported as partial parser coverage, never as
    silent absence.
    """
    body = bytes.fromhex(snapshot["body_hex"])
    detail_html = body.decode("utf-8", errors="replace")
    receipt_id, capture_id = project_detail_receipt_ids(snapshot)
    registration_number = snapshot["registration_number"]
    observed_at = snapshot["captured_at"]
    base = {
        "registration_number": registration_number,
        "receipt_id": receipt_id,
        "capture_id": capture_id,
        "observed_at": observed_at,
        "parser_version": "rera_project_detail_source_records.v1",
    }
    records = []

    def add(kind: str, locator: str, label: str, value: Any, effective_at=None, filing_at=None):
        record = dict(base)
        record.update(
            {
                "kind": kind,
                "source_locator": locator,
                "raw_label": label,
                "raw_value": value if isinstance(value, str) else json.dumps(value, separators=(",", ":")),
            }
        )
        if effective_at:
            record["effective_at"] = effective_at
        if filing_at:
            record["filing_at"] = filing_at
        records.append(record)

    declared_units = rera_detail_labeled_value(
        detail_html, "Total Number of Inventories/Flats/Villas"
    )
    if declared_units and declared_units.replace(",", "").strip().isdigit():
        add(
            "promoter_declaration",
            "#menu2/project-summary/total-inventories",
            "Total Number of Inventories/Flats/Villas",
            {"unit_count": int(declared_units.replace(",", "").strip())},
        )

    declared_total_carpet_area = rera_detail_labeled_value(
        detail_html, "Total Carpet Area of all the Floors (Sq Mtr)"
    )
    total_carpet_area_sqm = (
        rera_square_metres(declared_total_carpet_area)
        if declared_total_carpet_area
        else None
    )
    if total_carpet_area_sqm is not None:
        add(
            "promoter_declaration",
            "#menu2/project-summary/total-carpet-area",
            "Total Carpet Area of all the Floors (Sq Mtr)",
            {"total_carpet_area_sqm": total_carpet_area_sqm},
        )

    for inventory in rera_declared_inventory_rows(detail_html):
        row_number = inventory.pop("row_number") or "unknown"
        if inventory["inventory_type"].strip().upper() == "TOTAL":
            add(
                "inventory",
                "#menu2/development-inventory/total",
                "Declared inventory aggregate",
                inventory,
            )
            continue
        add(
            "inventory",
            "#menu2/development-inventory/row-{}".format(row_number),
            "Declared inventory configuration",
            inventory,
        )

    water_source = rera_detail_labeled_value(detail_html, "Source of Water")
    if water_source:
        add(
            "water_service_declaration",
            "#menu2/source-of-water/source",
            "Source of Water",
            {"source": water_source.rstrip(", ")},
        )
    water_authority = rera_detail_labeled_value(detail_html, "Local Authority")
    if water_authority:
        add(
            "water_service_declaration",
            "#menu2/source-of-water/local-authority",
            "Local Authority",
            {"authority": water_authority.rstrip(", ")},
        )

    registration_row = re.search(
        r"At the time of Registration.*?(\d{2}-\d{2}-\d{4}).*?(\d{2}-\d{2}-\d{4})",
        detail_html,
        re.IGNORECASE | re.DOTALL,
    )
    if registration_row:
        start_date = iso_rera_date(registration_row.group(1))
        completion_date = iso_rera_date(registration_row.group(2))
        if start_date:
            add(
                "completion",
                "#menu2/registration-schedule/at-registration/start-date",
                "Registration start date",
                {"date": start_date},
                effective_at=start_date,
            )
        if completion_date:
            add(
                "completion",
                "#menu2/registration-schedule/at-registration/proposed-completion-date",
                "Proposed completion date",
                {"date": completion_date},
                effective_at=completion_date,
            )

    quarter_headers = list(
        re.finditer(
            r"<b[^>]*>\s*Quarter\s+(Q[1-4])\s*\(\s*(\d{4}-\d{2})\s*\)"
            r".*?Submitted on\s+(\d{2}-\d{2}-\d{4})",
            detail_html,
            re.IGNORECASE | re.DOTALL,
        )
    )
    for index, header in enumerate(quarter_headers):
        block_end = quarter_headers[index + 1].start() if index + 1 < len(quarter_headers) else len(detail_html)
        block = detail_html[header.start() : block_end]
        totals = []
        for table in re.findall(r"<table\b[^>]*>(.*?)</table>", block, re.IGNORECASE | re.DOTALL):
            if "Total No of Units Booked" not in clean_html_fragment(table):
                continue
            total_row = re.search(
                r"<tr[^>]*>\s*<td[^>]*>\s*Total\s*</td>\s*"
                r"<td[^>]*>\s*(\d+)\s*</td>\s*<td[^>]*>\s*(\d+)\s*</td>\s*"
                r"<td[^>]*>\s*(\d+)\s*</td>",
                table,
                re.IGNORECASE | re.DOTALL,
            )
            if total_row:
                totals.append(tuple(int(value) for value in total_row.groups()))
        if not totals:
            continue
        filing_at = iso_rera_date(header.group(3))
        quarter = header.group(1).upper()
        financial_year = header.group(2)
        add(
            "quarterly_progress",
            "#quarterly-update/{}-{}".format(quarter.lower(), financial_year),
            "Quarterly inventory totals",
            {
                "quarter": quarter,
                "financial_year": financial_year,
                "tower_count": len(totals),
                "total_units": sum(row[0] for row in totals),
                "booked_units": sum(row[1] for row in totals),
                "unsold_units": sum(row[2] for row in totals),
            },
            filing_at=filing_at,
        )
        for href, label in re.findall(
            r"<a[^>]+href=['\"]([^'\"]*download_jc\?DOC_ID=[^'\"]+)['\"][^>]*>\s*(.*?)\s*</a>",
            block,
            re.IGNORECASE | re.DOTALL,
        ):
            document_label = clean_html_fragment(label)
            if not re.search(r"\bForm[- ]?[456]\b", document_label, re.IGNORECASE):
                continue
            document_url = href if href.startswith("http") else "{}{}".format(RERA_BASE, href)
            add(
                "document_approval",
                "#quarterly-update/{}/{}/{}".format(
                    quarter.lower(), financial_year, hashlib.sha256(document_url.encode("utf-8")).hexdigest()[:12]
                ),
                "Quarterly supporting document",
                {
                    "quarter": quarter,
                    "financial_year": financial_year,
                    "label": document_label,
                    "official_url": document_url,
                },
                filing_at=filing_at,
            )

    add(
        "source_warning",
        "#project-detail/parser-coverage",
        "Parser coverage",
        "Partial parser coverage: project summary, declared inventory table, schedule, water declaration, QPR inventory totals, and QPR document metadata only.",
    )
    return records


def collect_reddit_assets(
    request: Dict[str, Any],
    society_inputs: Dict[str, Dict[str, Any]] = None,
    thread_fetch: Callable[[str, str], List[Dict[str, Any]]] = None,
    result_builder: Callable[[Dict[str, Any], List[Dict[str, Any]]], Any] = None,
) -> Tuple[Dict[str, Any], Dict[str, Any]]:
    from pipeline.skills.reddit_resident_facts import threads_to_concern_facts
    from pipeline.skills.search_reddit import fetch_reddit_threads_with_retry

    planned_at = normalized_planned_at(request)
    partition = partition_values(request)
    snapshot_date = partition.get("dt") or planned_at[:10]
    subreddit = partition.get("subreddit")
    if not subreddit:
        raise ValueError("reddit_threads_daily requires a subreddit partition")

    records = []  # type: List[Dict[str, Any]]
    facts = []  # type: List[Dict[str, Any]]
    annotations = []  # type: List[Dict[str, Any]]
    latest_created = None
    if society_inputs is None:
        society_inputs = load_society_inputs_for_reddit()
    fetch_threads = thread_fetch or fetch_reddit_threads_with_retry
    build_result = result_builder or threads_to_concern_facts
    for slug, input_data in sorted(society_inputs.items()):
        query = input_data.get("query") or ""
        query_subreddit = input_data.get("subreddit") or subreddit
        threads = fetch_threads(query, query_subreddit)
        result = build_result(input_data, threads)
        entity_id = optional_string(input_data.get("entity_id")) or "society:{}".format(
            slug
        )
        result_facts, result_annotations = skill_result_rows(
            entity_id,
            "search_reddit",
            snapshot_date,
            input_data,
            result,
        )
        facts.extend(result_facts)
        annotations.extend(result_annotations)
        alias_entity_id = optional_string(input_data.get("alias_entity_id"))
        if alias_entity_id and alias_entity_id != entity_id:
            alias_facts, alias_annotations = skill_result_rows(
                alias_entity_id,
                "search_reddit",
                snapshot_date,
                input_data,
                result,
            )
            facts.extend(alias_facts)
            annotations.extend(alias_annotations)

        for thread in threads:
            thread_id = str(thread.get("id") or "").strip()
            title = str(thread.get("title") or "").strip()
            if not thread_id or not title:
                continue
            created_utc = optional_int(thread.get("created_utc"))
            if created_utc is not None:
                latest_created = max(latest_created or created_utc, created_utc)
            records.append(
                {
                    "thread_id": thread_id,
                    "subreddit": str(thread.get("subreddit") or query_subreddit),
                    "query": query,
                    "title": title,
                    "url": optional_string(thread.get("url")),
                    "score": int(thread.get("score") or 0),
                    "num_comments": int(thread.get("num_comments") or 0),
                    "created_utc": created_utc,
                    "selftext": optional_string(thread.get("selftext")),
                    "fetched_at": planned_at,
                    "fetch_source": "reddit_public_json_search",
                }
            )

    logger.info(
        "Collected %d Reddit threads and %d facts for %d societies",
        len(records),
        len(facts),
        len(society_inputs),
    )
    thread_input = {
        "snapshot_date": snapshot_date,
        "subreddit": subreddit,
        "records": records,
        "source_watermarks": [
            {
                "source": "reddit_public_json",
                "high_watermark": str(latest_created) if latest_created else planned_at,
            }
        ],
    }
    fact_input = {
        "source": "reddit",
        "snapshot_date": snapshot_date,
        "facts": facts,
        "fact_annotations": annotations,
        "source_watermarks": [
            {
                "source": "reddit_public_json_search",
                "high_watermark": str(latest_created) if latest_created else planned_at,
            }
        ],
    }
    return thread_input, fact_input


def skill_result_rows(
    entity_id: str,
    skill_id: str,
    snapshot_date: str,
    input_data: Dict[str, Any],
    result: Any,
):
    input_json = json.dumps(input_data, sort_keys=True, separators=(",", ":"))
    input_hash = "sha256:{}".format(hashlib.sha256(input_json.encode("utf-8")).hexdigest())
    run_id = "collector-{}-{}".format(skill_id, snapshot_date)
    facts = []  # type: List[Dict[str, Any]]
    annotations = []  # type: List[Dict[str, Any]]

    for fact in result.facts:
        if should_skip_skill_fact(skill_id, fact):
            continue
        value = fact.value or {}
        source = fact.source
        scoring = fact.scoring_hint or {}
        facts.append(
            {
                "entity_id": entity_id,
                "fact_key": fact.key,
                "value_type": str(value.get("type") or "text").lower(),
                "value_json": json.dumps(value, separators=(",", ":")),
                "confidence": float(fact.confidence),
                "source_type": source.source_type,
                "source_url": source.url,
                "model": source.model,
                "skill_id": source.skill_id or skill_id,
                "triggered_by": source.triggered_by or input_data.get("triggered_by"),
                "learned_at": fact.learned_at,
                "run_id": run_id,
                "input_hash": input_hash,
            }
        )
        registry_annotation = annotation_from_registry(fact.key, scoring)
        if registry_annotation:
            display_template = registry_annotation["display_template"]
            preferences = registry_annotation["answers_preferences"]
            scoring_direction = registry_annotation["scoring_direction"]
            scoring_weight = registry_annotation["scoring_weight"]
            scoring_thresholds = registry_annotation["scoring_thresholds"]
        else:
            display_template = fact.display_template
            preferences = fact.answers_preferences or []
            scoring_direction = scoring.get("direction")
            scoring_weight = scoring.get("weight")
            scoring_thresholds = scoring.get("thresholds") or []
        annotations.append(
            {
                "entity_id": entity_id,
                "fact_key": fact.key,
                "display_template": display_template,
                "answers_preferences_json": json.dumps(
                    preferences, separators=(",", ":")
                ),
                "scoring_direction": scoring_direction,
                "scoring_weight": scoring_weight,
                "scoring_thresholds_json": json.dumps(
                    scoring_thresholds, separators=(",", ":")
                ),
            }
        )

    return facts, annotations


def should_skip_skill_fact(skill_id: str, fact: Any) -> bool:
    if skill_id == "fetch_rera" and getattr(fact, "key", None) in (
        "geo.latitude",
        "geo.longitude",
    ):
        return True
    return False


def partition_values(request: Dict[str, Any]) -> Dict[str, str]:
    partition = request.get("partition", {})
    return {str(key): str(value) for key, value in partition.get("parts", [])}


def normalized_planned_at(request: Dict[str, Any]) -> str:
    value = str(request.get("planned_at") or "").strip()
    if not value:
        return datetime.now(timezone.utc).isoformat()
    return value


def optional_text(value: Any, field: str):
    if isinstance(value, dict):
        raw = value.get(field)
    else:
        raw = getattr(value, field, None)
    return optional_string(raw)


def optional_string(value: Any):
    if value is None:
        return None
    text = str(value).strip()
    return text or None


def optional_int(value: Any):
    if value is None or value == "":
        return None
    return int(float(value))


def optional_float(value: Any):
    if isinstance(value, (int, float)):
        return float(value)
    if isinstance(value, str):
        try:
            return float(value.strip())
        except ValueError:
            return None
    return None


def optional_string_list(value: Any) -> List[str]:
    if not isinstance(value, list):
        return []
    values = []
    for item in value:
        text = optional_string(item)
        if text and text not in values:
            values.append(text)
    return values


def main() -> int:
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(name)s] %(levelname)s: %(message)s",
        stream=sys.stderr,
    )
    try:
        request = json.load(sys.stdin)
        output = collect_asset_sources(request)
        json.dump(output, sys.stdout, separators=(",", ":"))
        sys.stdout.write("\n")
        return 0
    except Exception as error:
        logger.error("Asset source collection failed: %s", error)
        return 1


if __name__ == "__main__":
    sys.exit(main())
