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
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable, Dict, List, Tuple

from pipeline.skills.fetch_rera import LISTING_CACHE_PATH, LISTING_URL, scrape_rera_listing


logger = logging.getLogger(__name__)
PROJECT_ROOT = Path(__file__).resolve().parent.parent
KNOWLEDGE_DIR = PROJECT_ROOT / "data" / "knowledge" / "nodes"

RERA_REGISTRY_MONTHLY = "rera_registry_monthly"
REDDIT_THREADS_DAILY = "reddit_threads_daily"
REDDIT_RESIDENT_FACTS = "reddit_resident_facts"
GOOGLE_PLACES_WEEKLY = "google_places_weekly"
GOOGLE_NEARBY_PLACES_WEEKLY = "google_nearby_places_weekly"
PRESTIGE_INVENTORY_WEEKLY = "prestige_inventory_weekly"
EXTERNAL_LISTINGS_WEEKLY = "external_listings_weekly"
EXTERNAL_IMAGES_WEEKLY = "external_images_weekly"
METRO_STATIONS_MONTHLY = "metro_stations_monthly"
SUPPORTED_ASSETS = frozenset(
    (
        RERA_REGISTRY_MONTHLY,
        REDDIT_THREADS_DAILY,
        REDDIT_RESIDENT_FACTS,
        GOOGLE_PLACES_WEEKLY,
        GOOGLE_NEARBY_PLACES_WEEKLY,
        PRESTIGE_INVENTORY_WEEKLY,
        EXTERNAL_LISTINGS_WEEKLY,
        EXTERNAL_IMAGES_WEEKLY,
        METRO_STATIONS_MONTHLY,
    )
)


def collect_asset_sources(
    request: Dict[str, Any],
    rera_fetch: Callable[[], Any] = None,
    reddit_collect: Callable[..., Tuple[Dict[str, Any], Dict[str, Any]]] = None,
) -> Dict[str, Any]:
    requested = [asset_id for asset_id in request.get("requested_assets", []) if asset_id]
    unsupported = sorted(set(requested) - SUPPORTED_ASSETS)
    if unsupported:
        raise ValueError("unsupported source assets: {}".format(", ".join(unsupported)))

    output = {}  # type: Dict[str, Any]
    source_failures = {}  # type: Dict[str, str]
    if RERA_REGISTRY_MONTHLY in requested:
        try:
            output[RERA_REGISTRY_MONTHLY] = collect_rera_registry(request, rera_fetch)
        except Exception as error:
            record_source_failure(source_failures, [RERA_REGISTRY_MONTHLY], error)
    if REDDIT_THREADS_DAILY in requested or REDDIT_RESIDENT_FACTS in requested:
        reddit_assets = [
            asset_id
            for asset_id in (REDDIT_THREADS_DAILY, REDDIT_RESIDENT_FACTS)
            if asset_id in requested
        ]
        try:
            reddit_inputs = reddit_society_inputs(
                request, output.get(RERA_REGISTRY_MONTHLY)
            )
            if not reddit_inputs:
                raise ValueError(
                    "Reddit collection requires scoped source_entities or RERA projects"
                )
            if skip_reddit_collection():
                reddit_threads, reddit_facts = empty_reddit_assets(request)
            else:
                collect_reddit = reddit_collect or collect_reddit_assets
                reddit_threads, reddit_facts = collect_reddit(
                    request,
                    society_inputs=reddit_inputs,
                )
            if REDDIT_THREADS_DAILY in requested:
                output[REDDIT_THREADS_DAILY] = reddit_threads
            if REDDIT_RESIDENT_FACTS in requested:
                output[REDDIT_RESIDENT_FACTS] = reddit_facts
        except Exception as error:
            record_source_failure(source_failures, reddit_assets, error)
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
    if PRESTIGE_INVENTORY_WEEKLY in requested:
        try:
            from pipeline.sources.project_enrichment import collect_prestige_inventory

            prestige_inputs = source_society_inputs(
                request, output.get(RERA_REGISTRY_MONTHLY)
            )
            if not prestige_inputs:
                raise ValueError(
                    "Prestige collection requires scoped source_entities or RERA projects"
                )
            output[PRESTIGE_INVENTORY_WEEKLY] = collect_prestige_inventory(
                request, prestige_inputs
            )
        except Exception as error:
            record_source_failure(
                source_failures, [PRESTIGE_INVENTORY_WEEKLY], error
            )
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
    if METRO_STATIONS_MONTHLY in requested:
        try:
            from pipeline.sources.project_enrichment import collect_metro_stations

            output[METRO_STATIONS_MONTHLY] = collect_metro_stations(request)
        except Exception as error:
            record_source_failure(source_failures, [METRO_STATIONS_MONTHLY], error)
    if source_failures:
        output["source_failures"] = source_failures
    return output


def record_source_failure(
    failures: Dict[str, str], asset_ids: List[str], error: Exception
) -> None:
    reason = "{}: {}".format(type(error).__name__, error)
    for asset_id in asset_ids:
        failures[asset_id] = reason
    logger.error("Source collection failed for %s: %s", ", ".join(asset_ids), reason)


def skip_reddit_collection() -> bool:
    return str(os.environ.get("OPENESTATES_SKIP_REDDIT") or "").lower() in (
        "1",
        "true",
        "yes",
    )


def empty_reddit_assets(request: Dict[str, Any]) -> Tuple[Dict[str, Any], Dict[str, Any]]:
    planned_at = normalized_planned_at(request)
    partition = partition_values(request)
    snapshot_date = partition.get("dt") or planned_at[:10]
    subreddit = partition.get("subreddit") or "BangaloreRealEstates"
    watermark = {"source": "reddit_skipped", "high_watermark": planned_at}
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
            "facts": [],
            "fact_annotations": [],
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
                "address": optional_string(input_data.get("address")),
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
    categories = ("school", "metro", "hospital", "fitness", "eatery", "tech_park")

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
        "eatery": "restaurant cafe",
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
    projects = []  # type: List[Dict[str, Any]]
    skipped = 0
    for entry in entries:
        project_name = optional_text(entry, "project_name")
        if not project_name:
            skipped += 1
            continue
        projects.append(
            {
                "ack_number": optional_text(entry, "ack_number"),
                "registration_number": optional_text(entry, "registration_number"),
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
            "triggered_by": "asset_dag",
        }
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
    from pipeline.skills.search_reddit import (
        fetch_reddit_threads_with_retry,
        threads_to_skill_result,
    )

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
    build_result = result_builder or threads_to_skill_result
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
        annotations.append(
            {
                "entity_id": entity_id,
                "fact_key": fact.key,
                "display_template": fact.display_template,
                "answers_preferences_json": json.dumps(
                    fact.answers_preferences or [], separators=(",", ":")
                ),
                "scoring_direction": scoring.get("direction"),
                "scoring_weight": scoring.get("weight"),
                "scoring_thresholds_json": json.dumps(
                    scoring.get("thresholds") or [], separators=(",", ":")
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
