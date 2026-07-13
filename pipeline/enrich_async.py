"""
Progressive Async Enrichment Daemon — search-triggered background learning.

Watches `data/knowledge/enrichment_pending.jsonl` for requests written by the
Rust backend after each search. Processes them in progressive waves:

  Wave 1: Exact matches, free skills (Reddit, RERA) — 5s delays
  Wave 2: Exact matches, non-free deterministic skills — 10s delays
  Wave 3: Area neighbors, free skills — 15s delays
  Wave 4: Area neighbors, non-free deterministic skills — 20s delays

Usage:
    python3 -m pipeline.enrich_async                  # daemon mode (loop)
    python3 -m pipeline.enrich_async --once            # process pending and exit
    python3 -m pipeline.enrich_async --dry-run         # show what would be processed
    python3 -m pipeline.enrich_async --delay-multiplier 0.1  # fast for testing
"""

import argparse
import asyncio
import json
import logging
import os
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, List, Optional

from pipeline.entity_resolver import EntityResolver, FreshnessTracker
from pipeline.enrich import (
    FRESHNESS_SOURCE_MAP,
    SKILL_REGISTRY,
    _build_input,
    _make_skill,
    _push_facts,
    is_backed_off,
    load_failures,
    record_failure,
    record_success,
    save_failures,
)
from pipeline.utils import reload_backend

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Wave configuration
# ---------------------------------------------------------------------------

WAVE_CONFIG = {
    1: {"delay_between_calls": 5, "max_entities": None, "cost_tiers": ["free"]},
    2: {"delay_between_calls": 10, "max_entities": None, "cost_tiers": ["free", "cheap", "moderate"]},
    3: {"delay_between_calls": 15, "max_entities": 10, "cost_tiers": ["free"]},
    4: {"delay_between_calls": 20, "max_entities": 5, "cost_tiers": ["free", "cheap", "moderate"]},
}

PENDING_PATH = Path("data/knowledge/enrichment_pending.jsonl")
POLL_INTERVAL = 10  # seconds


# ---------------------------------------------------------------------------
# Request reading & deduplication
# ---------------------------------------------------------------------------

def read_and_truncate(path: Path) -> List[dict]:
    """Read all pending requests and truncate the file."""
    if not path.exists():
        return []

    try:
        content = path.read_text()
    except OSError:
        return []

    if not content.strip():
        return []

    requests = []
    for line in content.strip().split("\n"):
        line = line.strip()
        if not line:
            continue
        try:
            requests.append(json.loads(line))
        except json.JSONDecodeError:
            logger.warning("Skipping malformed JSONL line: %s", line[:100])

    # Truncate after reading
    try:
        path.write_text("")
    except OSError as e:
        logger.warning("Failed to truncate pending file: %s", e)

    return requests


def deduplicate_requests(requests: List[dict]) -> Dict[str, dict]:
    """Deduplicate by area, keeping the most recent request per area."""
    by_area: Dict[str, dict] = {}
    for req in requests:
        area = req.get("area", "").lower()
        if not area:
            continue

        # Merge entities and preferences across requests for the same area
        if area in by_area:
            existing = by_area[area]
            # Merge entity lists (deduplicated)
            existing_entities = set(existing.get("entities", []))
            existing_entities.update(req.get("entities", []))
            existing["entities"] = list(existing_entities)
            # Merge preferences (deduplicated)
            existing_prefs = set(existing.get("preferences", []))
            existing_prefs.update(req.get("preferences", []))
            existing["preferences"] = list(existing_prefs)
            # Keep most recent query
            existing["query"] = req.get("query", existing.get("query", ""))
        else:
            by_area[area] = {
                "area": req.get("area", ""),
                "entities": list(set(req.get("entities", []))),
                "preferences": list(set(req.get("preferences", []))),
                "query": req.get("query", ""),
            }

    return by_area


# ---------------------------------------------------------------------------
# Area neighbor discovery
# ---------------------------------------------------------------------------

def find_area_neighbors(area: str, exclude: List[str]) -> List[str]:
    """Find all society entity IDs in the same area, excluding already-targeted ones."""
    exclude_set = set(exclude)
    neighbors = []

    kg_dir = Path("data/knowledge/nodes/society")
    if kg_dir.exists():
        for node_path in sorted(kg_dir.glob("*.json")):
            try:
                node = json.loads(node_path.read_text())
            except (json.JSONDecodeError, OSError):
                continue

            node_area = node.get("area") or _fact_text(node, "area")
            if not node_area or node_area.lower() != area.lower():
                continue

            node_id = node.get("id") or f"society:{node_path.stem}"
            if node_id not in exclude_set:
                neighbors.append(node_id)

    if neighbors:
        return neighbors

    # Legacy fallback for older checkouts.
    societies_path = Path("data/seed/societies.json")
    if not societies_path.exists():
        return []

    try:
        societies = json.loads(societies_path.read_text())
    except (json.JSONDecodeError, OSError):
        return []

    for soc in societies:
        soc_area = soc.get("area", "")
        soc_id = soc.get("id", "")
        if not soc_area or not soc_id:
            continue

        # Match area (case-insensitive)
        if soc_area.lower() != area.lower():
            continue

        # Convert society ID to KG node ID format
        node_id = f"society:{soc_id}"
        if node_id.startswith("society:soc-"):
            node_id = "society:" + soc_id[4:]  # Remove "soc-" prefix

        if node_id not in exclude_set:
            neighbors.append(node_id)

    return neighbors


def _fact_text(node: dict, key: str) -> str:
    for fact in node.get("facts", []):
        if fact.get("key") != key:
            continue
        value = fact.get("value", {})
        data = value.get("data") if isinstance(value, dict) else value
        return str(data or "")
    return ""


# ---------------------------------------------------------------------------
# Freshness check
# ---------------------------------------------------------------------------

def needs_enrichment(
    entity_id: str,
    skill_id: str,
    tracker: FreshnessTracker,
) -> bool:
    """Check if an entity needs a specific skill run."""
    config = SKILL_REGISTRY.get(skill_id)
    if not config:
        return False

    # Check entity type compatibility
    entity_type = entity_id.split(":")[0]
    if entity_type not in config["node_types"]:
        return False

    freshness_source = FRESHNESS_SOURCE_MAP.get(skill_id, skill_id)
    return tracker.is_stale(entity_id, freshness_source, ttl_days=config["max_age_days"])


# ---------------------------------------------------------------------------
# Wave execution
# ---------------------------------------------------------------------------

async def run_wave(
    wave_num: int,
    entities: List[str],
    preferences: List[str],
    resolver: EntityResolver,
    tracker: FreshnessTracker,
    failures: dict,
    delay_multiplier: float = 1.0,
    dry_run: bool = False,
) -> dict:
    """Run a single wave of enrichment.

    Returns stats dict: {executed, succeeded, failed, skipped}.
    """
    config = WAVE_CONFIG[wave_num]
    delay = config["delay_between_calls"] * delay_multiplier
    max_entities = config["max_entities"]
    allowed_tiers = set(config["cost_tiers"])

    target_entities = entities[:max_entities] if max_entities else entities

    stats = {"executed": 0, "succeeded": 0, "failed": 0, "skipped": 0, "facts": 0}

    for entity_id in target_entities:
        for skill_id, skill_config in SKILL_REGISTRY.items():
            # Skip if wrong cost tier for this wave
            if skill_config["cost_tier"] not in allowed_tiers:
                continue

            # Skip if entity type doesn't match skill
            entity_type = entity_id.split(":")[0]
            if entity_type not in skill_config["node_types"]:
                continue

            # Skip if already fresh
            if not needs_enrichment(entity_id, skill_id, tracker):
                stats["skipped"] += 1
                continue

            # Check pool backoff
            pool = skill_config["pool"]
            if is_backed_off(failures, pool):
                logger.info("  Wave %d: [skip-backoff] %s %s (pool: %s)", wave_num, entity_id, skill_id, pool)
                stats["skipped"] += 1
                continue

            if dry_run:
                logger.info("  Wave %d: [dry-run] %s %s (%s)", wave_num, entity_id, skill_id, skill_config["cost_tier"])
                stats["executed"] += 1
                continue

            # Run skill
            logger.info("  Wave %d: [run] %s %s", wave_num, entity_id, skill_id)
            try:
                skill = _make_skill(skill_id)
                input_data = _build_input(skill_id, entity_id, resolver)
                result = skill.run(input_data)

                stats["executed"] += 1
                stats["succeeded"] += 1
                stats["facts"] += len(result.facts)

                if result.facts:
                    _push_facts(entity_id, result)

                freshness_source = FRESHNESS_SOURCE_MAP.get(skill_id, skill_id)
                tracker.mark_fresh(entity_id, freshness_source, {"fact_count": len(result.facts)})
                record_success(failures, pool)

            except Exception as e:
                stats["executed"] += 1
                stats["failed"] += 1
                logger.warning("  Wave %d: [fail] %s %s: %s", wave_num, entity_id, skill_id, e)
                record_failure(failures, pool)

            # Throttle between calls
            if delay > 0 and not dry_run:
                await asyncio.sleep(delay)

    return stats


# ---------------------------------------------------------------------------
# Main processing loop
# ---------------------------------------------------------------------------

async def process_requests(
    requests: List[dict],
    delay_multiplier: float = 1.0,
    dry_run: bool = False,
) -> dict:
    """Process a batch of enrichment requests through all 4 waves."""
    by_area = deduplicate_requests(requests)

    if not by_area:
        return {"areas": 0, "total_executed": 0, "total_facts": 0}

    resolver = EntityResolver()
    tracker = FreshnessTracker()
    failures = load_failures()

    total_stats = {"areas": 0, "total_executed": 0, "total_succeeded": 0, "total_failed": 0, "total_facts": 0}

    for area, request in by_area.items():
        entities = request["entities"]
        preferences = request["preferences"]
        logger.info("Processing enrichment for %s: %d entities, preferences=%s",
                     request["area"], len(entities), preferences)

        total_stats["areas"] += 1

        # Wave 1: exact matches, free skills
        w1 = await run_wave(1, entities, preferences, resolver, tracker, failures, delay_multiplier, dry_run)
        logger.info("  Wave 1 complete: %d executed, %d succeeded, %d facts", w1["executed"], w1["succeeded"], w1["facts"])

        # Wave 2: exact matches, non-free deterministic skills
        w2 = await run_wave(2, entities, preferences, resolver, tracker, failures, delay_multiplier, dry_run)
        logger.info("  Wave 2 complete: %d executed, %d succeeded, %d facts", w2["executed"], w2["succeeded"], w2["facts"])

        # Wave 3: area neighbors, free skills
        neighbors = find_area_neighbors(request["area"], entities)
        if neighbors:
            logger.info("  Found %d area neighbors for %s", len(neighbors), request["area"])
            w3 = await run_wave(3, neighbors, preferences, resolver, tracker, failures, delay_multiplier, dry_run)
            logger.info("  Wave 3 complete: %d executed, %d succeeded, %d facts", w3["executed"], w3["succeeded"], w3["facts"])
        else:
            w3 = {"executed": 0, "succeeded": 0, "failed": 0, "facts": 0}

        # Wave 4: area neighbors, non-free deterministic skills
        if neighbors:
            w4 = await run_wave(4, neighbors[:5], preferences, resolver, tracker, failures, delay_multiplier, dry_run)
            logger.info("  Wave 4 complete: %d executed, %d succeeded, %d facts", w4["executed"], w4["succeeded"], w4["facts"])
        else:
            w4 = {"executed": 0, "succeeded": 0, "failed": 0, "facts": 0}

        for w in [w1, w2, w3, w4]:
            total_stats["total_executed"] += w["executed"]
            total_stats["total_succeeded"] += w["succeeded"]
            total_stats["total_failed"] += w["failed"]
            total_stats["total_facts"] += w["facts"]

        # Reload backend after each area's waves complete
        if not dry_run and total_stats["total_facts"] > 0:
            reload_backend()

    # Save state
    tracker.save()
    save_failures(failures)

    return total_stats


async def run_daemon(delay_multiplier: float = 1.0):
    """Watch for enrichment requests, process in waves."""
    logger.info("Enrichment daemon started (poll every %ds, delay multiplier: %.1f)",
                POLL_INTERVAL, delay_multiplier)

    while True:
        requests = read_and_truncate(PENDING_PATH)

        if not requests:
            await asyncio.sleep(POLL_INTERVAL)
            continue

        logger.info("Found %d pending enrichment request(s)", len(requests))
        stats = await process_requests(requests, delay_multiplier=delay_multiplier)
        logger.info(
            "Batch complete: %d areas, %d skills executed, %d facts written",
            stats["areas"], stats["total_executed"], stats["total_facts"],
        )


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(
        description="Progressive Enrichment Daemon — search-triggered background learning",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--once", action="store_true",
                        help="Process current pending requests and exit (no loop)")
    parser.add_argument("--dry-run", action="store_true",
                        help="Show what would be processed without executing skills")
    parser.add_argument("--delay-multiplier", type=float, default=1.0,
                        help="Multiply all delays by this factor (0.1 for testing)")

    args = parser.parse_args()

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(message)s",
    )

    if args.once:
        requests = read_and_truncate(PENDING_PATH)
        if not requests:
            print("No pending enrichment requests.")
            return
        print(f"Processing {len(requests)} pending request(s)...")
        stats = asyncio.run(process_requests(
            requests,
            delay_multiplier=args.delay_multiplier,
            dry_run=args.dry_run,
        ))
        print(f"\nDone: {stats['areas']} areas, {stats['total_executed']} skills, {stats['total_facts']} facts")
    else:
        asyncio.run(run_daemon(delay_multiplier=args.delay_multiplier))


if __name__ == "__main__":
    main()
