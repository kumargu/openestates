"""
backfill_built_by_edges — pure computation skill (no LLM, zero cost).

For each society that has a builder_name fact but no BuiltBy edge in edges.json,
fuzzy-match the builder name to existing builder nodes and create the missing edge.

Usage: python3 -m pipeline.skills.backfill_built_by_edges
"""

import json
import os
import re
import sys
from pathlib import Path
from typing import Dict, List, Optional, Tuple

KNOWLEDGE_DIR = Path("data/knowledge/nodes")
SOCIETY_DIR = KNOWLEDGE_DIR / "society"
BUILDER_DIR = KNOWLEDGE_DIR / "builder"
EDGES_FILE = Path("data/knowledge/edges.json")

# Words to strip from names before tokenizing for fuzzy match
# (copied from seed_from_rera.py to keep the skill self-contained)
NOISE_WORDS = frozenset({
    "phase", "tower", "towers", "pvt", "ltd", "private", "limited",
    "group", "properties", "developers", "constructions", "realty",
    "real", "estate", "holdings", "infracon", "infra", "infrastructures",
    "corporation", "corp", "builder", "builders",
    "residential", "apartments", "apartment", "flats", "flat",
    "block", "wing", "plot", "plots", "layout",
    "the", "of", "at", "in", "by", "and", "&", "-", "–",
    "i", "ii", "iii", "iv", "v", "vi", "vii", "viii", "ix", "x",
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "10",
    "a", "b", "c", "d", "e",
})


def tokenize(name: str) -> set:
    """Tokenize a name: lowercase, split on non-alpha, remove noise."""
    tokens = set(re.findall(r'[a-z0-9]+', name.lower()))
    return tokens - NOISE_WORDS


def jaccard_similarity(a: set, b: set) -> float:
    """Jaccard token similarity: |intersection| / |union|."""
    if not a or not b:
        return 0.0
    return len(a & b) / len(a | b)


def get_fact_value(node: dict, key: str) -> Optional[str]:
    """Get the latest fact value for a given key (highest version)."""
    best = None
    best_version = -1
    for fact in node.get("facts", []):
        if fact["key"] == key:
            v = fact.get("version", 1)
            if v > best_version:
                best_version = v
                best = fact.get("value", {}).get("data")
    return best


def load_edges() -> list:
    """Load existing edges."""
    if EDGES_FILE.exists():
        return json.loads(EDGES_FILE.read_text())
    return []


def save_edges(edges: list):
    """Save edges atomically."""
    tmp = EDGES_FILE.with_suffix(".json.tmp")
    tmp.write_text(json.dumps(edges, indent=2, ensure_ascii=False))
    os.rename(tmp, EDGES_FILE)


def edge_exists(edges: list, from_id: str, to_id: str, relation: str) -> bool:
    """Check if an edge already exists."""
    for e in edges:
        if e["from"] == from_id and e["to"] == to_id and e["relation"] == relation:
            return True
    return False


def build_builder_index() -> Dict[str, Tuple[str, str]]:
    """Build a mapping from builder slug to (node_id, name) for all builder nodes."""
    index: Dict[str, Tuple[str, str]] = {}
    if not BUILDER_DIR.exists():
        return index
    for f in BUILDER_DIR.glob("*.json"):
        try:
            data = json.loads(f.read_text())
            node_id = data.get("id", f"builder:{f.stem}")
            name = data.get("name", f.stem)
            index[f.stem] = (node_id, name)
        except (json.JSONDecodeError, OSError):
            pass
    return index


def fuzzy_match_builder(
    builder_name: str,
    builder_index: Dict[str, Tuple[str, str]],
    threshold: float = 0.4,
) -> Optional[Tuple[str, str, float]]:
    """Find the best matching builder node for a given builder name.

    Returns (builder_node_id, builder_name, score) or None.
    """
    query_tokens = tokenize(builder_name)
    if not query_tokens:
        return None

    best_match = None
    best_score = 0.0

    for slug, (node_id, name) in builder_index.items():
        candidate_tokens = tokenize(name)
        score = jaccard_similarity(query_tokens, candidate_tokens)
        if score > best_score:
            best_score = score
            best_match = (node_id, name, score)

    if best_match and best_match[2] >= threshold:
        return best_match
    return None


def run():
    """Backfill missing BuiltBy edges by fuzzy-matching society builder_name facts to builder nodes."""
    print("=== Backfill BuiltBy Edges ===\n")

    # Load builder index
    builder_index = build_builder_index()
    print(f"  {len(builder_index)} builder nodes loaded\n")

    if not builder_index:
        print("No builder nodes found. Nothing to do.")
        return

    # Load edges and find societies that already have BuiltBy edges
    edges = load_edges()
    existing_built_by: set = set()
    for e in edges:
        if e.get("relation") == "BuiltBy":
            existing_built_by.add(e["from"])

    print(f"  {len(existing_built_by)} societies already have BuiltBy edges\n")

    # Process each society
    new_edges = 0
    unmatched = 0
    already_has = 0

    print(f"{'Society':<45} {'Builder Fact':<40} {'Matched To':<35} {'Score':>6} {'Action'}")
    print("-" * 140)

    for node_file in sorted(SOCIETY_DIR.glob("*.json")):
        try:
            data = json.loads(node_file.read_text())
        except (json.JSONDecodeError, OSError) as e:
            print(f"  SKIP {node_file.name}: {e}", file=sys.stderr)
            continue

        society_id = data.get("id", f"society:{node_file.stem}")
        society_name = data.get("name", node_file.stem)

        # Skip if already has BuiltBy edge
        if society_id in existing_built_by:
            already_has += 1
            continue

        # Get builder_name fact
        builder_name = get_fact_value(data, "builder_name")
        if not builder_name:
            # Also check node-level fields
            builder_name = data.get("builder_name")
        if not builder_name:
            continue

        # Fuzzy match to builder nodes
        match = fuzzy_match_builder(builder_name, builder_index)

        if not match:
            print(f"  {society_name:<45} {builder_name:<40} {'NO MATCH':<35} {'':>6} SKIP")
            unmatched += 1
            continue

        builder_node_id, matched_name, score = match

        # Add edge
        if not edge_exists(edges, society_id, builder_node_id, "BuiltBy"):
            edges.append({
                "from": society_id,
                "to": builder_node_id,
                "relation": "BuiltBy",
                "weight": 1.0,
                "source": {
                    "source_type": "Computed",
                    "skill_id": "backfill_built_by_edges",
                },
            })
            new_edges += 1
            print(f"  {society_name:<45} {builder_name:<40} {matched_name:<35} {score:>6.2f} ADDED")
        else:
            already_has += 1

    # Save edges
    if new_edges > 0:
        save_edges(edges)
        print(f"\n  Edges saved ({len(edges)} total)")

    print(f"\nBackfill complete:")
    print(f"  New BuiltBy edges created: {new_edges}")
    print(f"  Already had edges:         {already_has}")
    print(f"  Unmatched societies:       {unmatched}")


if __name__ == "__main__":
    run()
