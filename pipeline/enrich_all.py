"""
Batch enrichment — run ALL skills across ALL entities.

Usage:
    export $(grep -E "^[A-Z]" .env | xargs)
    python3 -m pipeline.enrich_all [--skip-claude] [--clear-cache]

Runs ~130+ API calls:
  - learn_area × 5 areas (Gemini grounded search)
  - fetch_google_reviews × 12 societies (Gemini grounded search)
  - learn_society × 12 societies (Reddit + Claude) [skipped with --skip-claude]
  - verify_rera × 12 societies (RERA portal + Claude fallback) [skipped with --skip-claude]
  - embed_entity × 43 entities (Google embedding API)

After enrichment, run validation:
    python3 -m pipeline.test_enrichment
"""

from __future__ import annotations

import json
import logging
import os
import sys
import time
import traceback
from pathlib import Path
from typing import Dict, List, Optional

PROJECT_ROOT = Path(__file__).parent.parent
sys.path.insert(0, str(PROJECT_ROOT))

from pipeline.skills.learn_area import LearnAreaSkill
from pipeline.skills.fetch_google_reviews import FetchGoogleReviewsSkill
from pipeline.skills.embed_entity import EmbedEntitySkill

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(levelname)s %(message)s",
    datefmt="%H:%M:%S",
)
logger = logging.getLogger(__name__)

GRAPH_DIR = PROJECT_ROOT / "data" / "knowledge" / "nodes"
EDGES_PATH = PROJECT_ROOT / "data" / "knowledge" / "edges.json"

# ---------------------------------------------------------------------------
# Per-entity file helpers (read/write individual node JSON files)
# ---------------------------------------------------------------------------

def load_node(node_id: str) -> dict | None:
    """Load a node from per-entity file. Returns None if not found."""
    parts = node_id.split(":", 1)
    if len(parts) != 2:
        return None
    node_type, slug = parts
    path = GRAPH_DIR / node_type / f"{slug}.json"
    if not path.exists():
        return None
    with open(path) as f:
        return json.load(f)


def save_node(node_id: str, node: dict):
    """Save a node to its per-entity file."""
    parts = node_id.split(":", 1)
    if len(parts) != 2:
        return
    node_type, slug = parts
    path = GRAPH_DIR / node_type / f"{slug}.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".tmp")
    with open(tmp, "w") as f:
        json.dump(node, f, indent=2, default=str)
    tmp.rename(path)


def list_nodes(node_type: str) -> list[dict]:
    """List all nodes of a given type."""
    type_dir = GRAPH_DIR / node_type
    if not type_dir.exists():
        return []
    nodes = []
    for path in sorted(type_dir.glob("*.json")):
        with open(path) as f:
            nodes.append(json.load(f))
    return nodes


def all_node_ids() -> list[str]:
    """Get all node IDs across all types."""
    ids = []
    if not GRAPH_DIR.exists():
        return ids
    for type_dir in sorted(GRAPH_DIR.iterdir()):
        if not type_dir.is_dir():
            continue
        for path in sorted(type_dir.glob("*.json")):
            with open(path) as f:
                node = json.load(f)
                ids.append(node.get("id", ""))
    return [i for i in ids if i]


# ---------------------------------------------------------------------------
# Fact helpers
# ---------------------------------------------------------------------------

def add_facts_to_node(node: dict, facts: list) -> int:
    """Add SourcedFact objects to a node dict. Returns count added."""
    if "facts" not in node:
        node["facts"] = []

    added = 0
    for fact in facts:
        fact_dict = fact.to_dict()
        # Replace existing fact with same key, or append
        replaced = False
        for i, existing in enumerate(node["facts"]):
            if existing["key"] == fact_dict["key"]:
                node["facts"][i] = fact_dict
                replaced = True
                break
        if not replaced:
            node["facts"].append(fact_dict)
        added += 1
    return added


def get_fact_value(node: dict, key: str, default=None):
    """Get a fact value from a node."""
    for f in node.get("facts", []):
        if f.get("key") == key:
            val = f.get("value", {})
            return val.get("data", default) if isinstance(val, dict) else val
    return default


def get_society_area(node: dict) -> str:
    """Get the area name for a society node."""
    area = get_fact_value(node, "area", "")
    if not area:
        # Fallback: load edges and find SocietyInArea
        if EDGES_PATH.exists():
            edges = json.loads(EDGES_PATH.read_text())
            for edge in edges:
                if edge.get("from") == node.get("id") and edge.get("relation") == "SocietyInArea":
                    area_node = load_node(edge.get("to", ""))
                    if area_node:
                        area = area_node.get("name", "")
                    break
    return area


# ---------------------------------------------------------------------------
# Phase runners
# ---------------------------------------------------------------------------

class EnrichmentStats:
    def __init__(self):
        self.calls = 0
        self.facts_added = 0
        self.errors = 0
        self.cached = 0
        self.cost_usd = 0.0

    def record(self, result, facts_added: int):
        self.calls += 1
        self.facts_added += facts_added
        if result.cached:
            self.cached += 1
        self.cost_usd += result.cost.estimated_usd

    def record_error(self):
        self.calls += 1
        self.errors += 1

    def summary(self) -> str:
        return (f"{self.calls} calls, {self.facts_added} facts, "
                f"{self.cached} cached, {self.errors} errors, ${self.cost_usd:.4f}")


def run_learn_area(stats: EnrichmentStats):
    """Phase 1: Gemini grounded search for area intelligence."""
    skill = LearnAreaSkill()
    areas = list_nodes("area")
    logger.info("=== Phase 1: learn_area (%d areas) ===", len(areas))

    for node in areas:
        node_id = node["id"]
        name = node.get("name", "")
        logger.info("  [learn_area] %s", name)

        try:
            result = skill.run({
                "area_name": name,
                "city": get_fact_value(node, "city", "Bengaluru"),
                "triggered_by": "batch_enrich",
            })
            if result.facts:
                added = add_facts_to_node(node, result.facts)
                save_node(node_id, node)
                stats.record(result, added)
                logger.info("    +%d facts (cached=%s)", added, result.cached)
            else:
                stats.record(result, 0)
                logger.warning("    no facts returned")

            if not result.cached:
                time.sleep(2)
        except Exception as e:
            stats.record_error()
            logger.error("    ERROR: %s", e)


def run_fetch_google_reviews(stats: EnrichmentStats):
    """Phase 2: Gemini grounded search for Google review intelligence."""
    skill = FetchGoogleReviewsSkill()
    societies = list_nodes("society")
    logger.info("=== Phase 2: fetch_google_reviews (%d societies) ===", len(societies))

    for node in societies:
        node_id = node["id"]
        name = node.get("name", "")
        area = get_society_area(node)
        logger.info("  [google_reviews] %s (%s)", name, area)

        try:
            result = skill.run({
                "society_name": name,
                "area": area,
                "city": get_fact_value(node, "city", "Bengaluru"),
                "triggered_by": "batch_enrich",
            })
            if result.facts:
                added = add_facts_to_node(node, result.facts)
                save_node(node_id, node)
                stats.record(result, added)
                logger.info("    +%d facts (cached=%s)", added, result.cached)
            else:
                stats.record(result, 0)
                logger.warning("    no facts (society not found on Google)")

            if not result.cached:
                time.sleep(3)
        except Exception as e:
            stats.record_error()
            logger.error("    ERROR: %s", e)


def run_learn_society(stats: EnrichmentStats):
    """Phase 3: Claude + Reddit enrichment for societies."""
    try:
        from pipeline.skills.learn_society import LearnSocietySkill
    except ImportError:
        logger.warning("learn_society skill not available, skipping")
        return

    skill = LearnSocietySkill()
    societies = list_nodes("society")
    logger.info("=== Phase 3: learn_society (%d societies, Claude+Reddit) ===", len(societies))

    for node in societies:
        node_id = node["id"]
        name = node.get("name", "")
        area = get_society_area(node)
        logger.info("  [learn_society] %s (%s)", name, area)

        try:
            result = skill.run({
                "society_name": name,
                "area": area,
                "city": get_fact_value(node, "city", "Bengaluru"),
                "triggered_by": "batch_enrich",
            })
            if result.facts:
                added = add_facts_to_node(node, result.facts)
                save_node(node_id, node)
                stats.record(result, added)
                logger.info("    +%d facts (cached=%s)", added, result.cached)
            else:
                stats.record(result, 0)
                logger.warning("    no facts returned")

            if not result.cached:
                time.sleep(2)
        except Exception as e:
            stats.record_error()
            logger.error("    ERROR: %s", e)


def run_verify_rera(stats: EnrichmentStats):
    """Phase 4: RERA verification for societies."""
    try:
        from pipeline.skills.verify_rera import VerifyReraSkill
    except ImportError:
        logger.warning("verify_rera skill not available, skipping")
        return

    skill = VerifyReraSkill()
    societies = list_nodes("society")
    logger.info("=== Phase 4: verify_rera (%d societies) ===", len(societies))

    for node in societies:
        node_id = node["id"]
        name = node.get("name", "")
        builder = get_fact_value(node, "builder_name", "")
        logger.info("  [verify_rera] %s (builder: %s)", name, builder)

        try:
            result = skill.run({
                "project_name": name,
                "builder_name": builder,
                "area": get_society_area(node),
                "city": get_fact_value(node, "city", "Bengaluru"),
                "triggered_by": "batch_enrich",
            })
            if result.facts:
                added = add_facts_to_node(node, result.facts)
                save_node(node_id, node)
                stats.record(result, added)
                logger.info("    +%d facts (cached=%s)", added, result.cached)
            else:
                stats.record(result, 0)
                logger.warning("    no facts returned")

            if not result.cached:
                time.sleep(2)
        except Exception as e:
            stats.record_error()
            logger.error("    ERROR: %s", e)


def run_embeddings(stats: EnrichmentStats):
    """Phase 5: Embed all entities (societies, areas, properties, builders)."""
    skill = EmbedEntitySkill()

    # Build targets from all entity types
    targets = []

    for node in list_nodes("society"):
        info = {"entity_id": node["id"], "entity_type": "society", "name": node.get("name", "")}
        info["summary"] = get_fact_value(node, "summary", "")
        info["best_for"] = get_fact_value(node, "best_for", "")
        # Enrich summary with Google/Reddit data if available
        sentiment = get_fact_value(node, "google_sentiment", "")
        if sentiment:
            info["summary"] = f"{info['summary']}. {sentiment}"
        positives = get_fact_value(node, "google_top_positives", [])
        if isinstance(positives, list):
            info["signals"] = positives[:5]
        else:
            info["signals"] = []
        info["triggered_by"] = "batch_enrich"
        targets.append(info)

    for node in list_nodes("area"):
        info = {"entity_id": node["id"], "entity_type": "area", "name": node.get("name", "")}
        info["livability_summary"] = get_fact_value(node, "livability_summary", "")
        info["metro_access"] = get_fact_value(node, "metro_details", "") or get_fact_value(node, "metro_access", "")
        info["vibe"] = get_fact_value(node, "area_vibe", "") or get_fact_value(node, "vibe", "")
        info["triggered_by"] = "batch_enrich"
        targets.append(info)

    for node in list_nodes("property"):
        info = {"entity_id": node["id"], "entity_type": "property", "name": node.get("name", "")}
        info["title"] = node.get("name", "")
        info["description_summary"] = get_fact_value(node, "description_summary", "")
        info["area"] = get_fact_value(node, "area", "")
        tags = get_fact_value(node, "transparency_tags", [])
        info["tags"] = tags if isinstance(tags, list) else []
        info["triggered_by"] = "batch_enrich"
        targets.append(info)

    for node in list_nodes("builder"):
        info = {"entity_id": node["id"], "entity_type": "builder", "name": node.get("name", "")}
        info["summary"] = get_fact_value(node, "summary", "")
        info["triggered_by"] = "batch_enrich"
        targets.append(info)

    logger.info("=== Phase 5: embed_entity (%d entities) ===", len(targets))

    for target in targets:
        logger.info("  [embed] %s (%s)", target["name"], target["entity_id"])

        try:
            result = skill.run(target)

            if result.facts:
                node = load_node(target["entity_id"])
                if node:
                    add_facts_to_node(node, result.facts)

                    # Store embedding vector
                    if result.new_nodes:
                        for nn in result.new_nodes:
                            emb = nn.get("summary_embedding")
                            if emb:
                                node["summary_embedding"] = emb
                                logger.info("    embedded (%d dims, cached=%s)", len(emb), result.cached)

                    save_node(target["entity_id"], node)
                    stats.record(result, len(result.facts))
                else:
                    stats.record(result, 0)
                    logger.warning("    node not found on disk")
            else:
                stats.record(result, 0)
                logger.warning("    no embedding returned")

            if not result.cached:
                time.sleep(0.5)
        except Exception as e:
            stats.record_error()
            logger.error("    ERROR: %s", e)


def print_report(phase_stats: dict[str, EnrichmentStats]):
    """Print final enrichment report."""
    print("\n" + "=" * 70)
    print("ENRICHMENT REPORT")
    print("=" * 70)

    total_calls = 0
    total_facts = 0
    total_errors = 0
    total_cost = 0.0

    for phase, stats in phase_stats.items():
        print(f"  {phase}: {stats.summary()}")
        total_calls += stats.calls
        total_facts += stats.facts_added
        total_errors += stats.errors
        total_cost += stats.cost_usd

    print(f"\n  TOTAL: {total_calls} API calls, {total_facts} facts added, "
          f"{total_errors} errors, ${total_cost:.4f}")

    # Node-level summary
    print("\n  Per-type summary:")
    for ntype in ["area", "society", "property", "builder"]:
        nodes = list_nodes(ntype)
        fact_counts = [len(n.get("facts", [])) for n in nodes]
        emb_count = sum(1 for n in nodes if n.get("summary_embedding"))
        avg_facts = sum(fact_counts) / len(fact_counts) if fact_counts else 0
        print(f"    {ntype}: {len(nodes)} nodes, {sum(fact_counts)} facts "
              f"(avg {avg_facts:.0f}/node), {emb_count} with embeddings")

    print("=" * 70)


def main():
    skip_claude = "--skip-claude" in sys.argv
    clear_cache = "--clear-cache" in sys.argv

    if not os.environ.get("GOOGLE_AI_API_KEY"):
        print("ERROR: GOOGLE_AI_API_KEY not set. Run:")
        print("  export $(grep -E '^[A-Z]' .env | xargs)")
        sys.exit(1)

    if clear_cache:
        cache_dir = PROJECT_ROOT / "data" / "cache" / "skills"
        if cache_dir.exists():
            count = len(list(cache_dir.glob("*.json")))
            for f in cache_dir.glob("*.json"):
                f.unlink()
            logger.info("Cleared %d cached skill results", count)

    if not GRAPH_DIR.exists():
        print("ERROR: Per-entity files not found at", GRAPH_DIR)
        print("Start the backend once to trigger migration from graph.json")
        sys.exit(1)

    node_ids = all_node_ids()
    logger.info("Found %d entities in per-entity storage", len(node_ids))

    phase_stats = {}

    # Phase 1: Area intelligence (Gemini)
    s = EnrichmentStats()
    run_learn_area(s)
    phase_stats["learn_area"] = s

    # Phase 2: Google reviews (Gemini)
    s = EnrichmentStats()
    run_fetch_google_reviews(s)
    phase_stats["fetch_google_reviews"] = s

    # Phase 3: Society enrichment (Claude + Reddit)
    if not skip_claude:
        if os.environ.get("ANTHROPIC_API_KEY"):
            s = EnrichmentStats()
            run_learn_society(s)
            phase_stats["learn_society"] = s
        else:
            logger.warning("ANTHROPIC_API_KEY not set, skipping learn_society")

    # Phase 4: RERA verification (RERA portal + Claude)
    if not skip_claude:
        s = EnrichmentStats()
        run_verify_rera(s)
        phase_stats["verify_rera"] = s

    # Phase 5: Embeddings for ALL entities (Google)
    s = EnrichmentStats()
    run_embeddings(s)
    phase_stats["embed_entity"] = s

    print_report(phase_stats)
    logger.info("Run validation: python3 -m pipeline.test_enrichment")


if __name__ == "__main__":
    main()
