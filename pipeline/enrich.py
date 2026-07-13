"""
Unified Enrichment Engine — one command to enrich everything.

Replaces orchestrate.py with a registry-driven engine that:
- Scans all KG nodes for missing/stale facts
- Plans work in dependency + priority order
- Executes with budget caps and failure backoff
- Prints structured output

Usage:
    python3 -m pipeline.enrich                              # fill all gaps
    python3 -m pipeline.enrich --plan                       # show what would run
    python3 -m pipeline.enrich --node society:sobha-insignia # one entity
    python3 -m pipeline.enrich --cost-tier free              # only free skills
    python3 -m pipeline.enrich --budget 1.00                 # cost cap
    python3 -m pipeline.enrich --type society                # one entity type
    python3 -m pipeline.enrich --force                       # ignore freshness
    python3 -m pipeline.enrich --reload                      # notify backend after
"""

import argparse
import json
import logging
import os
import time
import urllib.request
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, List, Optional

from pipeline.entity_resolver import EntityResolver, FreshnessTracker

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Skill Registry
# ---------------------------------------------------------------------------

SKILL_REGISTRY: Dict[str, dict] = {
    "search_reddit": {
        "node_types": ["society"],
        "pool": "reddit",
        "max_age_days": 14,
        "cost_tier": "free",
        "priority": 1,
        "depends_on": [],
    },
    "fetch_rera": {
        "node_types": ["society"],
        "pool": "rera",
        "max_age_days": 30,
        "cost_tier": "free",
        "priority": 1,
        "depends_on": [],
    },
    "fetch_images": {
        "node_types": ["society"],
        "pool": "serpapi",
        "max_age_days": 30,
        "cost_tier": "cheap",
        "priority": 3,
        "depends_on": [],
    },
    "fetch_google_review_links": {
        "node_types": ["society"],
        "pool": "serpapi",
        "max_age_days": 7,
        "cost_tier": "cheap",
        "priority": 3,
        "depends_on": [],
    },
    "identify_gaps": {
        "node_types": ["society"],
        "pool": "local",
        "max_age_days": 7,
        "cost_tier": "free",
        "priority": 4,
        "depends_on": ["search_reddit", "fetch_rera"],
    },
}

# Map skill_id -> freshness tracker source name
# (orchestrate.py used different names for some sources)
FRESHNESS_SOURCE_MAP = {
    "search_reddit": "reddit",
    "fetch_rera": "rera",
    "fetch_images": "fetch_images",
    "fetch_google_review_links": "google_review_links",
    "identify_gaps": "identify_gaps",
}

COST_TIER_ORDER = {"free": 0, "cheap": 1, "moderate": 2, "expensive": 3}

FAILURES_PATH = Path("data/cache/enrich_failures.json")


# ---------------------------------------------------------------------------
# Data structures
# ---------------------------------------------------------------------------

@dataclass
class WorkItem:
    node_id: str
    skill_id: str
    reason: str  # "missing" or "stale (N days)"
    priority: int = 0
    cost_tier: str = "free"


@dataclass
class ExecutionStats:
    executed: int = 0
    succeeded: int = 0
    cached: int = 0
    failed: int = 0
    skipped_backoff: int = 0
    skipped_budget: int = 0
    facts_written: int = 0
    total_cost: float = 0.0
    start_time: float = 0.0
    errors: List[str] = field(default_factory=list)


# ---------------------------------------------------------------------------
# Failure tracking
# ---------------------------------------------------------------------------

def load_failures() -> dict:
    if FAILURES_PATH.exists():
        try:
            return json.loads(FAILURES_PATH.read_text())
        except (json.JSONDecodeError, OSError):
            pass
    return {}


def save_failures(failures: dict):
    FAILURES_PATH.parent.mkdir(parents=True, exist_ok=True)
    tmp = FAILURES_PATH.with_suffix(".tmp")
    tmp.write_text(json.dumps(failures, indent=2, default=str))
    tmp.rename(FAILURES_PATH)


def is_backed_off(failures: dict, pool: str) -> bool:
    entry = failures.get(pool, {})
    if entry.get("consecutive_failures", 0) < 3:
        return False
    backoff_until = entry.get("backoff_until")
    if not backoff_until:
        return False
    try:
        return datetime.now(timezone.utc) < datetime.fromisoformat(backoff_until)
    except (ValueError, TypeError):
        return False


def record_failure(failures: dict, pool: str):
    entry = failures.setdefault(pool, {"consecutive_failures": 0})
    entry["consecutive_failures"] = entry.get("consecutive_failures", 0) + 1
    entry["last_failure"] = datetime.now(timezone.utc).isoformat()
    # Exponential backoff: 1h, 4h, 16h, capped at 24h
    hours = min(24, 4 ** (entry["consecutive_failures"] - 1))
    from datetime import timedelta
    entry["backoff_until"] = (datetime.now(timezone.utc) + timedelta(hours=hours)).isoformat()


def record_success(failures: dict, pool: str):
    if pool in failures:
        failures[pool] = {"consecutive_failures": 0}


# ---------------------------------------------------------------------------
# Gap detection
# ---------------------------------------------------------------------------

def detect_gaps(
    resolver: EntityResolver,
    tracker: FreshnessTracker,
    node_filter: Optional[str] = None,
    type_filter: Optional[str] = None,
    cost_tier_max: Optional[str] = None,
    force: bool = False,
) -> List[WorkItem]:
    """Scan all nodes for missing/stale facts. Zero LLM calls."""

    work: List[WorkItem] = []
    max_tier = COST_TIER_ORDER.get(cost_tier_max, 99) if cost_tier_max else 99

    # Collect all entities
    entities_by_type: Dict[str, List[str]] = {}
    for etype in ["society", "area", "property"]:
        if type_filter and etype != type_filter:
            continue
        entities = resolver.get_all_entities(etype)
        if entities:
            entities_by_type[etype] = entities

    for skill_id, config in SKILL_REGISTRY.items():
        if COST_TIER_ORDER.get(config["cost_tier"], 99) > max_tier:
            continue

        freshness_source = FRESHNESS_SOURCE_MAP.get(skill_id, skill_id)

        for node_type in config["node_types"]:
            entities = entities_by_type.get(node_type, [])
            for entity_id in entities:
                if node_filter and entity_id != node_filter:
                    continue

                if force:
                    reason = "forced"
                elif tracker.is_stale(entity_id, freshness_source, ttl_days=config["max_age_days"]):
                    # Distinguish missing vs stale
                    entry = tracker.data.get(entity_id, {}).get(freshness_source)
                    if not entry or not entry.get("last_fetched"):
                        reason = "missing"
                    else:
                        try:
                            fetched = datetime.fromisoformat(entry["last_fetched"])
                            age = (datetime.now(timezone.utc) - fetched).days
                            reason = f"stale ({age} days)"
                        except (ValueError, TypeError):
                            reason = "missing"
                else:
                    continue

                work.append(WorkItem(
                    node_id=entity_id,
                    skill_id=skill_id,
                    reason=reason,
                    priority=config["priority"],
                    cost_tier=config["cost_tier"],
                ))

    # Sort by priority, then by node_id for deterministic order
    work.sort(key=lambda w: (w.priority, COST_TIER_ORDER.get(w.cost_tier, 99), w.node_id))
    return work


# ---------------------------------------------------------------------------
# Skill instantiation & execution
# ---------------------------------------------------------------------------

def _make_skill(skill_id: str):
    """Lazy-import and instantiate a skill by ID."""
    if skill_id == "search_reddit":
        from pipeline.skills.search_reddit import SearchRedditSkill
        return SearchRedditSkill()
    elif skill_id == "fetch_rera":
        from pipeline.skills.fetch_rera import FetchReraSkill
        return FetchReraSkill()
    elif skill_id == "fetch_images":
        from pipeline.skills.fetch_images import FetchImagesSkill
        return FetchImagesSkill()
    elif skill_id == "fetch_google_review_links":
        from pipeline.skills.fetch_google_review_links import FetchGoogleReviewLinksSkill
        return FetchGoogleReviewLinksSkill()
    elif skill_id == "identify_gaps":
        from pipeline.skills.identify_gaps import IdentifyGapsSkill
        return IdentifyGapsSkill()
    else:
        raise ValueError(f"Unknown skill: {skill_id}")


def _get_node_fact(entity_id: str, fact_key: str) -> str:
    """Read a single fact value from a KG node file."""
    etype, slug = entity_id.split(":", 1)
    node_path = Path("data/knowledge/nodes") / etype / f"{slug}.json"
    if not node_path.exists():
        return ""
    try:
        with open(node_path) as f:
            node = json.load(f)
        for fact in node.get("facts", []):
            if fact.get("key") == fact_key:
                val = fact.get("value", {})
                if isinstance(val, dict):
                    return str(val.get("data", ""))
                return str(val)
    except Exception:
        pass
    return ""


def _resolve_area(entity_id: str) -> str:
    """Try multiple strategies to find area for a society entity."""
    # 1. Direct fact on node
    area = _get_node_fact(entity_id, "area")
    if area:
        return area

    # 2. Check edges (LocatedIn edge → area node)
    edges_path = Path("data/knowledge/edges.json")
    if edges_path.exists():
        try:
            with open(edges_path) as f:
                edges = json.load(f)
            for edge in edges:
                if edge.get("from") == entity_id and edge.get("relation") == "LocatedIn":
                    target = edge.get("to", "")
                    if target.startswith("area:"):
                        return target.split(":", 1)[-1].replace("-", " ").title()
        except Exception:
            pass

    # 3. Check seed data for matching society
    slug = entity_id.split(":", 1)[-1]
    seed_path = Path("data/seed/properties.json")
    if seed_path.exists():
        try:
            with open(seed_path) as f:
                properties = json.load(f)
            for p in properties:
                sid = p.get("society_id", "")
                # Match soc-slug or society-slug patterns
                if sid.replace("soc-", "") == slug or sid.replace("society-", "") == slug:
                    return p.get("area", "")
        except Exception:
            pass

    return ""


def _build_input(skill_id: str, entity_id: str, resolver: EntityResolver) -> dict:
    """Build the input dict a skill expects from an entity ID."""
    name = resolver.get_name(entity_id) or entity_id.split(":", 1)[-1]
    area = _resolve_area(entity_id) if entity_id.startswith("society:") else ""
    city = _get_node_fact(entity_id, "city") or "Bengaluru"

    if skill_id == "search_reddit":
        return {"query": name, "subreddit": "bangalore"}
    elif skill_id == "fetch_rera":
        return {"project_name": name, "entity_id": entity_id}
    elif skill_id == "fetch_images":
        area = _resolve_area(entity_id) if entity_id.startswith("society:") else ""
        return {
            "entity_id": entity_id,
            "entity_type": entity_id.split(":")[0],
            "name": name,
            "area": area,
        }
    elif skill_id == "fetch_google_review_links":
        area = _resolve_area(entity_id) if entity_id.startswith("society:") else ""
        return {
            "entity_id": entity_id,
            "entity_type": entity_id.split(":")[0],
            "society_name": name,
            "area": area,
            "city": city,
            "google_place_id": _get_node_fact(entity_id, "google_place_id"),
        }
    elif skill_id == "identify_gaps":
        slug = entity_id.split(":", 1)[-1]
        return {"society_id": f"soc-{slug}"}
    else:
        return {"entity_id": entity_id}


def _persist_facts_locally(entity_id: str, result) -> int:
    """Write skill result facts directly to the node JSON file on disk.

    This is the fallback when the Rust backend is not running.
    Merges new facts with existing ones (newer version wins for same key).
    Returns count of facts written.
    """
    if not result.facts:
        return 0

    parts = entity_id.split(":", 1)
    if len(parts) != 2:
        return 0
    etype, slug = parts
    node_path = Path("data/knowledge/nodes") / etype / f"{slug}.json"
    if not node_path.exists():
        logger.debug("Node file not found for %s, skipping local persist", entity_id)
        return 0

    try:
        node = json.loads(node_path.read_text())
    except (json.JSONDecodeError, OSError) as e:
        logger.warning("Failed to read node file %s: %s", node_path, e)
        return 0

    existing_facts = node.get("facts", [])
    # Build index of existing facts by key for dedup
    existing_by_key: Dict[str, dict] = {}
    for f in existing_facts:
        key = f.get("key", "")
        ver = f.get("version", 1)
        if key not in existing_by_key or ver > existing_by_key[key].get("version", 1):
            existing_by_key[key] = f

    new_count = 0
    for fact in result.facts:
        fd = fact.to_dict()
        key = fd["key"]
        new_ver = fd.get("version", 1)
        if key in existing_by_key:
            old_ver = existing_by_key[key].get("version", 1)
            old_confidence = float(existing_by_key[key].get("confidence", 0.0) or 0.0)
            new_confidence = float(fd.get("confidence", 0.0) or 0.0)
            if new_ver < old_ver or (new_ver == old_ver and new_confidence <= old_confidence):
                continue  # Existing fact is same or newer version
        existing_by_key[key] = fd
        new_count += 1

    if new_count == 0:
        return 0

    # Rebuild facts list from the deduplicated map
    node["facts"] = list(existing_by_key.values())
    node["updated_at"] = datetime.now(timezone.utc).isoformat()

    # Atomic write via tmp+rename
    tmp = node_path.with_suffix(".json.tmp")
    tmp.write_text(json.dumps(node, indent=2, ensure_ascii=False))
    os.rename(tmp, node_path)

    logger.debug("Persisted %d facts locally for %s", new_count, entity_id)
    return new_count


def _push_facts(entity_id: str, result):
    """Push skill result facts to the Rust backend, with local file fallback."""
    try:
        from pipeline.skills.graph_client import GraphClient
        client = GraphClient()
        if client.push_skill_result(entity_id, result):
            return
        logger.debug("Backend push returned no response, persisting locally")
    except Exception as e:
        logger.debug("Backend push skipped (%s), persisting locally", e)
    _persist_facts_locally(entity_id, result)


def _should_mark_fresh(skill_id: str, result) -> bool:
    """Decide whether a skill result is good enough to suppress near-term reruns."""
    if not result.facts:
        return False
    if skill_id != "fetch_google_review_links":
        return True

    precise_keys = {"google_place_id", "google_rating", "google_review_count"}
    for fact in result.facts:
        if fact.key in precise_keys:
            return True
        if fact.key == "google_reviews_url" and fact.confidence >= 0.6:
            return True
    return False


# ---------------------------------------------------------------------------
# Execution engine
# ---------------------------------------------------------------------------

def execute(
    work: List[WorkItem],
    resolver: EntityResolver,
    tracker: FreshnessTracker,
    budget: Optional[float] = None,
    force: bool = False,
) -> ExecutionStats:
    """Run skills in dependency + priority order."""

    stats = ExecutionStats(start_time=time.time())
    failures = load_failures()
    completed: set = set()  # (node_id, skill_id) pairs that succeeded
    attempted: set = set()  # (node_id, skill_id) pairs that were attempted (success or fail)
    work_set: set = {(w.node_id, w.skill_id) for w in work}

    for item in work:
        # Check pool backoff
        pool = SKILL_REGISTRY[item.skill_id]["pool"]
        if is_backed_off(failures, pool):
            logger.info("  [skip] %s %s — pool '%s' backed off", item.node_id, item.skill_id, pool)
            stats.skipped_backoff += 1
            continue

        # Check dependencies: if a dependency is in the work queue but hasn't completed, skip
        deps = SKILL_REGISTRY[item.skill_id]["depends_on"]
        deps_satisfied = True
        for dep in deps:
            if (item.node_id, dep) in work_set and (item.node_id, dep) not in completed:
                logger.info("  [defer] %s %s — waiting for %s", item.node_id, item.skill_id, dep)
                deps_satisfied = False
                break
        if not deps_satisfied:
            continue

        # Check budget
        if budget is not None and stats.total_cost >= budget:
            logger.info("  Budget cap reached ($%.2f). Stopping.", budget)
            stats.skipped_budget += len(work) - stats.executed - stats.skipped_backoff
            break

        # Run the skill
        logger.info("  [run] %s %s (%s)", item.node_id, item.skill_id, item.reason)
        try:
            skill = _make_skill(item.skill_id)
            input_data = _build_input(item.skill_id, item.node_id, resolver)
            result = skill.run(input_data, force=force)

            stats.executed += 1

            if result.cached:
                stats.cached += 1
            else:
                stats.succeeded += 1

            stats.facts_written += len(result.facts)
            stats.total_cost += result.cost.estimated_usd

            if result.facts:
                _push_facts(item.node_id, result)

            if _should_mark_fresh(item.skill_id, result):
                freshness_source = FRESHNESS_SOURCE_MAP.get(item.skill_id, item.skill_id)
                tracker.mark_fresh(item.node_id, freshness_source, {"fact_count": len(result.facts)})

            completed.add((item.node_id, item.skill_id))
            attempted.add((item.node_id, item.skill_id))
            record_success(failures, pool)

        except Exception as e:
            stats.executed += 1
            stats.failed += 1
            stats.errors.append(f"{item.node_id}/{item.skill_id}: {e}")
            logger.warning("  [fail] %s %s: %s", item.node_id, item.skill_id, e)
            attempted.add((item.node_id, item.skill_id))
            record_failure(failures, pool)

    # Second pass: handle deferred items whose dependencies are now met
    deferred = [
        item for item in work
        if (item.node_id, item.skill_id) not in attempted
        and not is_backed_off(failures, SKILL_REGISTRY[item.skill_id]["pool"])
        and all(
            (item.node_id, dep) in completed or (item.node_id, dep) not in work_set
            for dep in SKILL_REGISTRY[item.skill_id]["depends_on"]
        )
    ]

    for item in deferred:
        if budget is not None and stats.total_cost >= budget:
            stats.skipped_budget += 1
            continue

        pool = SKILL_REGISTRY[item.skill_id]["pool"]
        if is_backed_off(failures, pool):
            stats.skipped_backoff += 1
            continue

        logger.info("  [run-deferred] %s %s (%s)", item.node_id, item.skill_id, item.reason)
        try:
            skill = _make_skill(item.skill_id)
            input_data = _build_input(item.skill_id, item.node_id, resolver)
            result = skill.run(input_data, force=force)

            stats.executed += 1
            if result.cached:
                stats.cached += 1
            else:
                stats.succeeded += 1

            stats.facts_written += len(result.facts)
            stats.total_cost += result.cost.estimated_usd

            if result.facts:
                _push_facts(item.node_id, result)
            if _should_mark_fresh(item.skill_id, result):
                freshness_source = FRESHNESS_SOURCE_MAP.get(item.skill_id, item.skill_id)
                tracker.mark_fresh(item.node_id, freshness_source, {"fact_count": len(result.facts)})

            completed.add((item.node_id, item.skill_id))
            record_success(failures, pool)

        except Exception as e:
            stats.executed += 1
            stats.failed += 1
            stats.errors.append(f"{item.node_id}/{item.skill_id}: {e}")
            logger.warning("  [fail] %s %s: %s", item.node_id, item.skill_id, e)
            record_failure(failures, pool)

    tracker.save()
    save_failures(failures)

    return stats


# ---------------------------------------------------------------------------
# Output formatting
# ---------------------------------------------------------------------------

def print_plan(work: List[WorkItem]):
    """Print structured plan output."""
    if not work:
        print("\nENRICHMENT PLAN")
        print("===============")
        print("Nothing to do — all entities are fresh.")
        return

    # Group by priority + cost tier
    by_priority: Dict[int, List[WorkItem]] = {}
    for item in work:
        by_priority.setdefault(item.priority, []).append(item)

    # Count node types
    node_types = {}
    for item in work:
        ntype = item.node_id.split(":")[0]
        node_types[ntype] = node_types.get(ntype, 0) + 1

    print("\nENRICHMENT PLAN")
    print("===============")

    type_parts = ", ".join(f"{c} {t}" for t, c in sorted(node_types.items()))
    unique_nodes = len(set(w.node_id for w in work))
    print(f"Nodes affected: {unique_nodes} ({type_parts})")
    print(f"Work items: {len(work)}")

    for priority in sorted(by_priority):
        items = by_priority[priority]
        tier = items[0].cost_tier if items else "?"
        print(f"\n  Priority {priority} ({tier}):")
        for item in items[:15]:
            dep_info = ""
            deps = SKILL_REGISTRY[item.skill_id]["depends_on"]
            if deps:
                dep_info = f"  (depends: {', '.join(deps)})"
            print(f"    {item.node_id:<40s} {item.skill_id:<25s} {item.reason}{dep_info}")
        if len(items) > 15:
            print(f"    ... ({len(items) - 15} more)")

    # Rough cost estimate based on tier
    tier_costs = {"free": 0.0, "cheap": 0.002, "moderate": 0.01, "expensive": 0.05}
    est_cost = sum(tier_costs.get(w.cost_tier, 0.01) for w in work)
    print(f"\nEstimated cost: ${est_cost:.2f}")


def print_summary(stats: ExecutionStats):
    """Print structured execution summary."""
    elapsed = time.time() - stats.start_time

    print("\nENRICHMENT COMPLETE")
    print("===================")
    print(f"  Executed: {stats.executed} skills")
    print(f"  Succeeded: {stats.succeeded}")
    print(f"  Cached (skipped): {stats.cached}")
    print(f"  Failed: {stats.failed}")
    if stats.skipped_backoff:
        print(f"  Skipped (backoff): {stats.skipped_backoff}")
    if stats.skipped_budget:
        print(f"  Skipped (budget): {stats.skipped_budget}")
    print(f"  Facts written: {stats.facts_written}")
    print(f"  Cost: ${stats.total_cost:.2f}")

    if elapsed >= 60:
        print(f"  Time: {int(elapsed // 60)}m {int(elapsed % 60)}s")
    else:
        print(f"  Time: {elapsed:.1f}s")

    if stats.errors:
        print(f"\n  Errors ({len(stats.errors)}):")
        for err in stats.errors[:10]:
            print(f"    - {err}")


# ---------------------------------------------------------------------------
# Backend notification
# ---------------------------------------------------------------------------

def notify_backend():
    """Tell Rust backend to reload knowledge graph."""
    from pipeline.utils import reload_backend as _reload
    if _reload():
        # Print for CLI output (reload_backend logs internally)
        pass
    else:
        pass  # Already logged by reload_backend


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(
        description="OpenEstates Enrichment Engine — one command to enrich everything",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""Examples:
  python3 -m pipeline.enrich                              # fill all gaps
  python3 -m pipeline.enrich --plan                       # show what would run
  python3 -m pipeline.enrich --node society:sobha-insignia # one entity
  python3 -m pipeline.enrich --cost-tier free              # only free skills
  python3 -m pipeline.enrich --budget 1.00                 # cost cap
  python3 -m pipeline.enrich --type society                # one entity type
  python3 -m pipeline.enrich --force                       # ignore freshness
  python3 -m pipeline.enrich --reload                      # notify backend after
""",
    )
    parser.add_argument("--plan", action="store_true", help="Show what would run, no execution")
    parser.add_argument("--node", help="Enrich a specific node (e.g., society:sobha-insignia)")
    parser.add_argument("--type", dest="entity_type", choices=["society", "area", "property"],
                        help="Only enrich nodes of this type")
    parser.add_argument("--cost-tier", choices=["free", "cheap", "moderate"],
                        help="Only run skills at or below this cost tier")
    parser.add_argument("--budget", type=float, help="Stop after spending this much ($)")
    parser.add_argument("--force", action="store_true", help="Ignore freshness, re-run everything")
    parser.add_argument("--reload", action="store_true", help="Notify backend to reload after enrichment")

    args = parser.parse_args()

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(message)s",
    )

    resolver = EntityResolver()
    tracker = FreshnessTracker()

    # Detect gaps
    work = detect_gaps(
        resolver=resolver,
        tracker=tracker,
        node_filter=args.node,
        type_filter=args.entity_type,
        cost_tier_max=args.cost_tier,
        force=args.force,
    )

    if args.plan:
        print_plan(work)
        return

    if not work:
        print("\nNothing to do — all entities are fresh.")
        if args.reload:
            notify_backend()
        return

    print_plan(work)
    print("\nExecuting...\n")

    stats = execute(
        work=work,
        resolver=resolver,
        tracker=tracker,
        budget=args.budget,
        force=args.force,
    )

    print_summary(stats)

    if args.reload and (stats.succeeded > 0 or stats.facts_written > 0):
        notify_backend()


if __name__ == "__main__":
    main()
