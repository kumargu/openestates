"""
reembed_all.py — Re-embed all society and area nodes using KG fact intelligence.

Reads every society/area node from data/knowledge/nodes/, merges with seed data
for common_complaints/positives, builds enriched embedding text from KG facts,
calls the embedding API, and writes updated summary_embedding back to each node.

Usage:
    python3 -m pipeline.scripts.reembed_all
    python3 -m pipeline.scripts.reembed_all --dry-run     # preview text, no API calls
    python3 -m pipeline.scripts.reembed_all --type society  # only society nodes
    python3 -m pipeline.scripts.reembed_all --type area     # only area nodes
    python3 -m pipeline.scripts.reembed_all --test-queries  # run similarity test after embedding

Rate limits: 1 embedding call per second (Google rate limits).
"""

from __future__ import annotations

import argparse
import json
import logging
import os
import sys
import time
from pathlib import Path
from typing import Optional

# Add project root to path
PROJECT_ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(PROJECT_ROOT))

from pipeline.skills.embed_entity import build_summary_text, build_aspect_texts, call_embedding_api

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    datefmt="%H:%M:%S",
)
log = logging.getLogger("reembed_all")

KG_NODES_DIR = PROJECT_ROOT / "data" / "knowledge" / "nodes"
SEED_FILE = PROJECT_ROOT / "data" / "seed" / "societies.json"


def load_seed_map() -> dict[str, dict]:
    """Load seed societies indexed by name (lowercase) and by id."""
    if not SEED_FILE.exists():
        return {}
    with SEED_FILE.open() as f:
        societies = json.load(f)
    result = {}
    for s in societies:
        result[s.get("id", "").lower()] = s
        result[s.get("name", "").lower()] = s
    return result


def build_society_input(node: dict, seed_map: dict) -> dict:
    """Build input_data for EmbedEntitySkill from a KG society node + seed data."""
    name = node.get("name", "")
    input_data = {
        "entity_id": node["id"],
        "entity_type": "society",
        "name": name,
        "facts": node.get("facts", []),
    }

    # Merge in seed data if available
    seed = seed_map.get(name.lower()) or seed_map.get(node["id"].lower())
    if seed:
        input_data["summary"] = seed.get("summary", "")
        input_data["common_complaints"] = seed.get("common_complaints", [])
        input_data["common_positives"] = seed.get("common_positives", [])

    return input_data


def build_area_input(node: dict) -> dict:
    """Build input_data for EmbedEntitySkill from a KG area node."""
    return {
        "entity_id": node["id"],
        "entity_type": "area",
        "name": node.get("name", ""),
        "facts": node.get("facts", []),
    }


def write_node_embedding(
    node_path: Path,
    embedding: list[float],
    aspect_embeddings: dict | None = None,
) -> None:
    """Atomically write updated summary_embedding (and aspect_embeddings) to a node JSON file."""
    with node_path.open() as f:
        node = json.load(f)
    node["summary_embedding"] = embedding
    if aspect_embeddings:
        node["aspect_embeddings"] = aspect_embeddings
    tmp_path = node_path.with_suffix(".tmp")
    with tmp_path.open("w") as f:
        json.dump(node, f, indent=2, default=str)
    tmp_path.rename(node_path)


def cosine_similarity(a: list[float], b: list[float]) -> float:
    """Compute cosine similarity between two vectors."""
    dot = sum(x * y for x, y in zip(a, b))
    norm_a = sum(x * x for x in a) ** 0.5
    norm_b = sum(x * x for x in b) ** 0.5
    if norm_a == 0 or norm_b == 0:
        return 0.0
    return dot / (norm_a * norm_b)


def run_test_queries(node_type: str = "society") -> None:
    """Run 5 test queries to validate embedding quality."""
    log.info("Running embedding quality test queries...")

    test_queries = [
        "family friendly quiet society",
        "avoid water issues tanker dependency",
        "good maintenance reliable builder",
        "investment opportunity good resale",
        "peaceful less crowded greenery",
    ]

    nodes_dir = KG_NODES_DIR / node_type
    if not nodes_dir.exists():
        log.warning("No %s nodes directory found", node_type)
        return

    # Load all nodes with embeddings
    nodes_with_embeddings = []
    for node_path in sorted(nodes_dir.glob("*.json")):
        with node_path.open() as f:
            node = json.load(f)
        emb = node.get("summary_embedding")
        if emb:
            nodes_with_embeddings.append((node.get("name", node_path.stem), emb))

    if not nodes_with_embeddings:
        log.warning("No embedded nodes found for test queries")
        return

    log.info("Testing against %d embedded %s nodes", len(nodes_with_embeddings), node_type)

    for query in test_queries:
        query_emb = call_embedding_api(query)
        if not query_emb:
            log.warning("Could not embed query: %s", query)
            continue

        scores = [
            (name, cosine_similarity(query_emb, emb))
            for name, emb in nodes_with_embeddings
        ]
        scores.sort(key=lambda x: x[1], reverse=True)
        top3 = scores[:3]
        log.info(
            "Query: '%s'\n  Top 3: %s",
            query,
            " | ".join(f"{n} ({s:.3f})" for n, s in top3),
        )
        time.sleep(1)  # rate limit


def main() -> None:
    parser = argparse.ArgumentParser(description="Re-embed all KG nodes with enriched text")
    parser.add_argument("--dry-run", action="store_true",
                        help="Print embedding text without making API calls")
    parser.add_argument("--type", choices=["society", "area", "all"], default="all",
                        help="Which node type to embed (default: all)")
    parser.add_argument("--test-queries", action="store_true",
                        help="Run test queries after embedding to validate quality")
    args = parser.parse_args()

    seed_map = load_seed_map()
    log.info("Loaded %d seed entries", len(seed_map))

    node_types = []
    if args.type in ("society", "all"):
        node_types.append("society")
    if args.type in ("area", "all"):
        node_types.append("area")

    total_processed = 0
    total_embedded = 0
    total_skipped = 0
    total_errors = 0

    for node_type in node_types:
        nodes_dir = KG_NODES_DIR / node_type
        if not nodes_dir.exists():
            log.warning("Directory not found: %s", nodes_dir)
            continue

        node_files = sorted(nodes_dir.glob("*.json"))
        log.info("Processing %d %s nodes...", len(node_files), node_type)

        for node_path in node_files:
            with node_path.open() as f:
                node = json.load(f)

            total_processed += 1
            name = node.get("name", node_path.stem)

            if node_type == "society":
                input_data = build_society_input(node, seed_map)
            else:
                input_data = build_area_input(node)

            # Build embedding text
            text = build_summary_text(input_data)
            word_count = len(text.split())

            if args.dry_run:
                log.info("[DRY RUN] %s (%d words):\n  %s", name, word_count, text[:200])
                total_skipped += 1
                continue

            if word_count < 3:
                log.warning("Skipping %s — text too short (%d words)", name, word_count)
                total_skipped += 1
                continue

            log.info("Embedding %s (%d words)...", name, word_count)

            embedding = call_embedding_api(text)
            if not embedding:
                log.error("Failed to embed %s", name)
                total_errors += 1
                time.sleep(1)
                continue

            # Also embed per-aspect texts for society/area nodes
            aspect_embeddings = {}
            if node_type in ("society", "area"):
                aspect_texts = build_aspect_texts(input_data)
                for aspect, asp_text in aspect_texts.items():
                    time.sleep(1)  # rate limit between API calls
                    asp_emb = call_embedding_api(asp_text)
                    if asp_emb:
                        aspect_embeddings[aspect] = asp_emb
                        log.info("  -> aspect '%s' embedded (%d dims)", aspect, len(asp_emb))
                    else:
                        log.warning("  -> aspect '%s' failed", aspect)

            write_node_embedding(node_path, embedding, aspect_embeddings or None)
            log.info("  -> summary %d dims, %d aspects", len(embedding), len(aspect_embeddings))
            total_embedded += 1

            # Rate limit: 1 call per second (summary call already done above)
            time.sleep(1)

    log.info(
        "\nSummary: %d processed, %d embedded, %d skipped, %d errors",
        total_processed, total_embedded, total_skipped, total_errors,
    )

    if args.test_queries and not args.dry_run:
        run_test_queries("society")


if __name__ == "__main__":
    main()
