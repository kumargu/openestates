"""
dedup_facts.py — Remove duplicate fact keys from knowledge graph nodes.

When a node has multiple facts with the same key, keeps the best one:
  1. Highest confidence
  2. Most recent learned_at timestamp
  3. Highest version number

Walks ALL node types (society, area, property, builder).
Atomic writes via .tmp + rename.

Usage:
    python3 -m pipeline.scripts.dedup_facts [--dry-run]
"""

import json
import os
import sys
from collections import Counter
from datetime import datetime
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(PROJECT_ROOT))

KG_NODES_DIR = PROJECT_ROOT / "data" / "knowledge" / "nodes"

NODE_TYPES = ("society", "area", "property", "builder")


def _parse_timestamp(ts: str) -> float:
    """Parse ISO 8601 timestamp to epoch float. Returns 0.0 on failure."""
    if not ts:
        return 0.0
    try:
        # Handle both naive and timezone-aware timestamps
        ts_clean = ts.replace("Z", "+00:00")
        dt = datetime.fromisoformat(ts_clean)
        return dt.timestamp()
    except (ValueError, TypeError):
        return 0.0


def _sort_key(fact: dict) -> tuple:
    """Sort key for facts: highest confidence, most recent, highest version."""
    return (
        -fact.get("confidence", 0.0),
        -_parse_timestamp(fact.get("learned_at", "")),
        -fact.get("version", 1),
    )


def dedup_node_facts(node: dict) -> tuple[list[dict], int]:
    """Deduplicate facts in a node. Returns (deduped_facts, removed_count)."""
    facts = node.get("facts", [])
    if not facts:
        return facts, 0

    key_counts = Counter(f["key"] for f in facts)
    duplicated_keys = {k for k, v in key_counts.items() if v > 1}

    if not duplicated_keys:
        return facts, 0

    # Group by key, keep best for duplicated keys, keep all for unique keys
    kept: list[dict] = []
    seen_keys: set[str] = set()

    # Sort all facts so best comes first per key
    sorted_facts = sorted(facts, key=_sort_key)

    for fact in sorted_facts:
        key = fact["key"]
        if key in duplicated_keys:
            if key not in seen_keys:
                kept.append(fact)
                seen_keys.add(key)
            # else: duplicate, skip
        else:
            kept.append(fact)

    removed = len(facts) - len(kept)
    return kept, removed


def dedup_node_file(path: Path, dry_run: bool) -> tuple[int, list[str]]:
    """Deduplicate facts in a single node file.

    Returns (removed_count, list of deduped keys).
    """
    with path.open() as f:
        node = json.load(f)

    facts = node.get("facts", [])
    key_counts = Counter(f["key"] for f in facts)
    duplicated_keys = sorted(k for k, v in key_counts.items() if v > 1)

    if not duplicated_keys:
        return 0, []

    deduped_facts, removed = dedup_node_facts(node)

    if removed > 0 and not dry_run:
        node["facts"] = deduped_facts
        tmp = path.with_suffix(".tmp")
        with tmp.open("w") as f:
            json.dump(node, f, indent=2, default=str)
        os.rename(tmp, path)

    return removed, duplicated_keys


def main():
    import argparse
    parser = argparse.ArgumentParser(description="Deduplicate fact keys in KG nodes")
    parser.add_argument("--dry-run", action="store_true", help="Preview changes without writing")
    args = parser.parse_args()

    total_files = 0
    total_modified = 0
    total_removed = 0

    for node_type in NODE_TYPES:
        nodes_dir = KG_NODES_DIR / node_type
        if not nodes_dir.exists():
            continue

        type_modified = 0
        for path in sorted(nodes_dir.glob("*.json")):
            total_files += 1
            removed, dup_keys = dedup_node_file(path, args.dry_run)
            if removed > 0:
                total_modified += 1
                type_modified += 1
                total_removed += removed
                action = "Would dedup" if args.dry_run else "Deduped"
                dup_desc = ", ".join(dup_keys)
                print(f"  {action}: {node_type}/{path.stem} — removed {removed} dupes [{dup_desc}]")

        if type_modified > 0:
            print(f"  [{node_type}] {type_modified} files modified\n")

    prefix = "[DRY RUN] " if args.dry_run else ""
    print(f"{prefix}Total: {total_modified}/{total_files} files modified, {total_removed} duplicate facts removed")


if __name__ == "__main__":
    main()
