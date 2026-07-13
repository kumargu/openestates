"""
One-shot backfill: set root_source on all existing knowledge graph nodes.

Classification rules:
- If node has any fact with source.source_type == "Rera" or skill_id containing "rera" -> "Rera"
- If node has any legacy discovery fact -> "Discovered"
- Else -> "Legacy"

Atomic writes: write to .tmp, rename.

Usage: python3 -m pipeline.skills.backfill_root_source
"""

import json
import os
import sys
from pathlib import Path


KNOWLEDGE_DIR = Path("data/knowledge/nodes")
LEGACY_DISCOVERY_SKILL_ID = "discover_properties"


def classify_root_source(node: dict) -> str:
    """Determine root_source from a node's facts."""
    for fact in node.get("facts", []):
        source = fact.get("source", {})
        source_type = source.get("source_type", "")
        skill_id = source.get("skill_id", "") or ""

        if source_type == "Rera" or "rera" in skill_id.lower():
            return "Rera"

    for fact in node.get("facts", []):
        source = fact.get("source", {})
        skill_id = source.get("skill_id", "") or ""

        if skill_id == LEGACY_DISCOVERY_SKILL_ID:
            return "Discovered"

    return "Legacy"


def backfill():
    """Walk all node files and set root_source."""
    counts = {"Rera": 0, "Discovered": 0, "Legacy": 0, "Seller": 0}
    total = 0
    already_set = 0

    for node_type_dir in sorted(KNOWLEDGE_DIR.iterdir()):
        if not node_type_dir.is_dir():
            continue

        for node_file in sorted(node_type_dir.glob("*.json")):
            total += 1
            try:
                data = json.loads(node_file.read_text())
            except (json.JSONDecodeError, OSError) as e:
                print(f"  SKIP {node_file}: {e}", file=sys.stderr)
                continue

            root_source = classify_root_source(data)
            old_source = data.get("root_source")

            if old_source == root_source:
                already_set += 1
                counts[root_source] += 1
                continue

            data["root_source"] = root_source
            counts[root_source] += 1

            # Atomic write: .tmp then rename
            tmp_path = node_file.with_suffix(".json.tmp")
            tmp_path.write_text(json.dumps(data, indent=2, ensure_ascii=False))
            os.rename(tmp_path, node_file)

    print(f"\nBackfill complete: {total} nodes processed")
    print(f"  Already set: {already_set}")
    print(f"  Classification breakdown:")
    for source, count in sorted(counts.items()):
        print(f"    {source}: {count}")


if __name__ == "__main__":
    backfill()
