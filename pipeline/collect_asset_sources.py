"""Emit typed source inputs for the Rust asset DAG.

The Rust runner sends ``SourceInputRequest`` JSON on stdin. This module writes
only ``AssetSourceInputs`` JSON to stdout; progress and diagnostics go to
stderr. Durable Parquet writes, lineage, and promotion remain Rust-owned.
"""

import hashlib
import json
import logging
import sys
from datetime import datetime, timezone
from typing import Any, Callable, Dict, List, Tuple

from pipeline.skills.fetch_rera import LISTING_CACHE_PATH, LISTING_URL, scrape_rera_listing


logger = logging.getLogger(__name__)

RERA_REGISTRY_MONTHLY = "rera_registry_monthly"
REDDIT_THREADS_DAILY = "reddit_threads_daily"
REDDIT_RESIDENT_FACTS = "reddit_resident_facts"
GOOGLE_REVIEW_FACTS = "google_review_facts"
SUPPORTED_ASSETS = frozenset(
    (
        RERA_REGISTRY_MONTHLY,
        REDDIT_THREADS_DAILY,
        REDDIT_RESIDENT_FACTS,
        GOOGLE_REVIEW_FACTS,
    )
)


def collect_asset_sources(
    request: Dict[str, Any],
    rera_fetch: Callable[[], Any] = None,
    reddit_collect: Callable[[Dict[str, Any]], Tuple[Dict[str, Any], Dict[str, Any]]] = None,
    skill_collect: Callable[[Dict[str, Any], str, str], Dict[str, Any]] = None,
) -> Dict[str, Any]:
    requested = [asset_id for asset_id in request.get("requested_assets", []) if asset_id]
    unsupported = sorted(set(requested) - SUPPORTED_ASSETS)
    if unsupported:
        raise ValueError("unsupported source assets: {}".format(", ".join(unsupported)))

    output = {}  # type: Dict[str, Any]
    if RERA_REGISTRY_MONTHLY in requested:
        output[RERA_REGISTRY_MONTHLY] = collect_rera_registry(request, rera_fetch)
    if REDDIT_THREADS_DAILY in requested or REDDIT_RESIDENT_FACTS in requested:
        collect_reddit = reddit_collect or collect_reddit_assets
        reddit_threads, reddit_facts = collect_reddit(request)
        if REDDIT_THREADS_DAILY in requested:
            output[REDDIT_THREADS_DAILY] = reddit_threads
        if REDDIT_RESIDENT_FACTS in requested:
            output[REDDIT_RESIDENT_FACTS] = reddit_facts
    collect_skills = skill_collect or collect_skill_facts
    if GOOGLE_REVIEW_FACTS in requested:
        output[GOOGLE_REVIEW_FACTS] = collect_skills(
            request, "fetch_google_review_links", "google"
        )
    return output


def collect_rera_registry(
    request: Dict[str, Any], rera_fetch: Callable[[], Any] = None
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
    return {
        "snapshot_date": observed_at[:7],
        "projects": projects,
        "source_watermarks": [
            {"source": "karnataka_rera_listing", "high_watermark": observed_at}
        ],
    }


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
    from pipeline.skills.batch_runner import load_society_inputs
    from pipeline.skills.search_reddit import fetch_reddit_threads, threads_to_skill_result

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
        society_inputs = load_society_inputs("search_reddit")
    fetch_threads = thread_fetch or fetch_reddit_threads
    build_result = result_builder or threads_to_skill_result
    for slug, input_data in sorted(society_inputs.items()):
        query = input_data.get("query") or ""
        query_subreddit = input_data.get("subreddit") or subreddit
        threads = fetch_threads(query, query_subreddit)
        result = build_result(input_data, threads)
        result_facts, result_annotations = skill_result_rows(
            "society:{}".format(slug),
            "search_reddit",
            snapshot_date,
            input_data,
            result,
        )
        facts.extend(result_facts)
        annotations.extend(result_annotations)

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


def collect_skill_facts(
    request: Dict[str, Any], skill_id: str, source: str
) -> Dict[str, Any]:
    from pipeline.skills.batch_runner import get_skill_instance, load_society_inputs

    planned_at = normalized_planned_at(request)
    snapshot_date = partition_values(request).get("dt") or planned_at[:10]
    inputs = load_society_inputs(skill_id)
    skill = get_skill_instance(skill_id)
    facts = []  # type: List[Dict[str, Any]]
    annotations = []  # type: List[Dict[str, Any]]
    total_cost = 0.0

    for slug, input_data in sorted(inputs.items()):
        result = skill.run(input_data)
        total_cost += result.cost.estimated_usd
        result_facts, result_annotations = skill_result_rows(
            "society:{}".format(slug), skill_id, snapshot_date, input_data, result
        )
        facts.extend(result_facts)
        annotations.extend(result_annotations)

    logger.info(
        "Collected %d %s facts for %d societies (estimated cost $%.4f)",
        len(facts),
        source,
        len(inputs),
        total_cost,
    )
    return {
        "source": source,
        "snapshot_date": snapshot_date,
        "facts": facts,
        "fact_annotations": annotations,
        "source_watermarks": [
            {"source": skill_id, "high_watermark": planned_at}
        ],
    }


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
                "triggered_by": source.triggered_by,
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
