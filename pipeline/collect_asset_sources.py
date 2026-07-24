"""Emit typed source inputs for the Rust asset DAG.

The Rust runner sends ``SourceInputRequest`` JSON on stdin. This module writes
only ``AssetSourceInputs`` JSON to stdout; progress and diagnostics go to
stderr. Durable Parquet writes, lineage, and promotion remain Rust-owned.
"""

import hashlib
import json
import logging
import os
import sys
import xml.etree.ElementTree as ET
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable, Dict, List, Optional, Tuple

from urllib.request import urlopen
from pipeline.skills.fetch_rera import LISTING_CACHE_PATH, LISTING_URL, scrape_rera_listing


logger = logging.getLogger(__name__)
PROJECT_ROOT = Path(__file__).resolve().parent.parent
KNOWLEDGE_DIR = PROJECT_ROOT / "data" / "knowledge" / "nodes"
DAG_ROOT = PROJECT_ROOT / "app" / "config" / "dag"
FACT_REGISTRY_PATH = DAG_ROOT / "fact_registry.json"
_FACT_REGISTRY_CACHE = None  # type: Optional[Dict[str, Any]]


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
GOOGLE_PLACES_WEEKLY = "google_places_weekly"
GOOGLE_NEARBY_PLACES_WEEKLY = "google_nearby_places_weekly"
EXTERNAL_LISTINGS_WEEKLY = "external_listings_weekly"
EXTERNAL_IMAGES_WEEKLY = "external_images_weekly"
SOCIETY_GROUNDWATER_POTENTIAL_FACTS = "society_groundwater_potential_facts"
GROUNDWATER_KML_URL = (
    "https://data.opencity.in/dataset/035c1d40-8f4e-4780-90c5-ff1ce2281849/"
    "resource/d3ae3603-d786-4782-ae71-a034ad4ebc0b/download/"
    "1dda919d-ff28-4aa9-90ce-bd18e708927b.kml"
)
SUPPORTED_ASSETS = frozenset(
    (
        RERA_REGISTRY_MONTHLY,
        GOOGLE_PLACES_WEEKLY,
        GOOGLE_NEARBY_PLACES_WEEKLY,
        EXTERNAL_LISTINGS_WEEKLY,
        EXTERNAL_IMAGES_WEEKLY,
        SOCIETY_GROUNDWATER_POTENTIAL_FACTS,
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
    if GOOGLE_PLACES_WEEKLY in requested:
        try:
            google_inputs = google_society_inputs(
                request, output.get(RERA_REGISTRY_MONTHLY)
            )
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
            google_inputs = google_society_inputs(
                request, output.get(RERA_REGISTRY_MONTHLY)
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

            output[EXTERNAL_LISTINGS_WEEKLY] = collect_external_listings(request)
        except Exception as error:
            record_source_failure(source_failures, [EXTERNAL_LISTINGS_WEEKLY], error)
    if EXTERNAL_IMAGES_WEEKLY in requested:
        try:
            from pipeline.sources.external_images import collect_external_images

            output[EXTERNAL_IMAGES_WEEKLY] = collect_external_images(request)
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
    categories = ("school", "metro", "hospital", "fitness", "tech_park")

    for slug, input_data in sorted(inputs.items()):
        for category in categories:
            query = nearby_query(input_data, category)
            for place in fetch(input_data, category):
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
    normalized = category.replace("-", "_").strip().lower()
    labels = {
        "school": "school",
        "metro": "metro station",
        "hospital": "hospital",
        "fitness": "gym fitness",
        "tech_park": "tech park office",
    }
    return labels.get(normalized, normalized.replace("_", " "))


def google_society_inputs(
    request: Dict[str, Any], rera_input: Dict[str, Any] = None
) -> Dict[str, Dict[str, Any]]:
    return source_society_inputs(request, rera_input)


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
    for seed in request.get("source_entities", []):
        entity_id = str(seed.get("entity_id") or "").strip()
        name = str(seed.get("name") or "").strip()
        if not entity_id or not name:
            continue
        inputs[entity_id] = {
            "entity_id": entity_id,
            "alias_entity_id": optional_string(seed.get("alias_entity_id")),
            "project_key": optional_string(seed.get("project_key")),
            "society_name": name,
            "area": optional_string(seed.get("area")),
            "city": optional_string(seed.get("city")) or "Bengaluru",
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
                "source_type": "Manual",
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
