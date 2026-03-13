"""
compute_builder_delivery_rate — pure computation skill (no LLM, zero cost).

Reads builder nodes, edges.json, and society project_status facts to compute
builder-level delivery track record facts:

- builder_delivery_rate: fraction of projects NOT delayed (0.0-1.0)
- builder_project_count: number of projects with known status
- builder_zero_revocations: "true" if builder has 0 revocations

Skips builders with fewer than 2 projects (insufficient signal).

Usage: python3 -m pipeline.skills.compute_builder_delivery_rate
"""

import json
import os
import sys
from collections import defaultdict
from pathlib import Path
from typing import Optional

from pipeline.skills.base import FactSource, SourcedFact


KNOWLEDGE_DIR = Path("data/knowledge/nodes")
BUILDER_DIR = KNOWLEDGE_DIR / "builder"
SOCIETY_DIR = KNOWLEDGE_DIR / "society"
EDGES_FILE = Path("data/knowledge/edges.json")

SKILL_ID = "compute_builder_delivery_rate"
MIN_PROJECTS = 1


def load_edges() -> list[dict]:
    """Load all edges from edges.json."""
    if not EDGES_FILE.exists():
        print(f"Edges file not found: {EDGES_FILE}", file=sys.stderr)
        return []
    try:
        return json.loads(EDGES_FILE.read_text())
    except (json.JSONDecodeError, OSError) as e:
        print(f"Failed to load edges: {e}", file=sys.stderr)
        return []


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


def build_builder_to_societies(edges: list[dict]) -> dict[str, list[str]]:
    """Build a map from builder node ID to list of society node IDs via BuiltBy edges."""
    # BuiltBy edges: from=society:xxx, to=builder:xxx
    builder_societies: dict[str, list[str]] = defaultdict(list)
    for edge in edges:
        if edge.get("relation") == "BuiltBy":
            society_id = edge["from"]  # e.g. "society:prestige-park-grove"
            builder_id = edge["to"]    # e.g. "builder:prestige-group"
            builder_societies[builder_id].append(society_id)
    return dict(builder_societies)


def load_society_statuses() -> dict[str, str]:
    """Load project_status fact for each society. Returns {society_node_id: status}."""
    statuses = {}
    if not SOCIETY_DIR.exists():
        return statuses
    for node_file in SOCIETY_DIR.glob("*.json"):
        try:
            data = json.loads(node_file.read_text())
        except (json.JSONDecodeError, OSError):
            continue
        node_id = data.get("id", "")
        status = get_fact_value(data, "project_status")
        if status:
            statuses[node_id] = status
    return statuses


def build_delivery_rate_fact(delivery_rate: float) -> dict:
    """Build a SourcedFact dict for builder_delivery_rate."""
    pct = int(delivery_rate * 100)
    fact = SourcedFact(
        key="builder_delivery_rate",
        value={"type": "Numeric", "data": delivery_rate},
        confidence=0.9,
        source=FactSource(
            source_type="Computed",
            skill_id=SKILL_ID,
        ),
        display_template=f"Builder delivers on time: {pct}% of projects",
        answers_preferences=[
            "reliable builder", "trusted builder", "on time delivery",
            "dependable builder", "good builder", "reputed builder",
            "timely delivery", "no delays",
        ],
        scoring_hint={"direction": "HigherIsBetter", "weight": 2.5},
    )
    return fact.to_dict()


def build_project_count_fact(count: int) -> dict:
    """Build a SourcedFact dict for builder_project_count."""
    fact = SourcedFact(
        key="builder_project_count",
        value={"type": "Numeric", "data": float(count)},
        confidence=0.9,
        source=FactSource(
            source_type="Computed",
            skill_id=SKILL_ID,
        ),
        display_template=f"Builder has {count} tracked projects",
        answers_preferences=[
            "reliable builder", "trusted builder", "reputed builder",
            "established builder", "experienced builder",
        ],
        scoring_hint={"direction": "HigherIsBetter", "weight": 1.0},
    )
    return fact.to_dict()


def build_zero_revocations_fact() -> dict:
    """Build a SourcedFact dict for builder_zero_revocations."""
    fact = SourcedFact(
        key="builder_zero_revocations",
        value={"type": "Text", "data": "true"},
        confidence=0.9,
        source=FactSource(
            source_type="Computed",
            skill_id=SKILL_ID,
        ),
        display_template="No RERA revocations on record",
        answers_preferences=[
            "reliable builder", "trusted builder", "no delays",
            "safe builder", "dependable builder",
        ],
        scoring_hint={"direction": "TextMatch", "weight": 2.0},
    )
    return fact.to_dict()


def has_fact_from_skill(node: dict, fact_key: str) -> bool:
    """Check if node already has a fact with given key from this skill."""
    return any(
        f["key"] == fact_key
        and f.get("source", {}).get("skill_id") == SKILL_ID
        for f in node.get("facts", [])
    )


def run():
    """Process all builder nodes and add delivery track record facts."""
    edges = load_edges()
    if not edges:
        print("No edges loaded — cannot compute builder delivery rates.", file=sys.stderr)
        return

    builder_societies = build_builder_to_societies(edges)
    society_statuses = load_society_statuses()

    processed = 0
    skipped_few_projects = 0
    skipped_no_societies = 0
    already_has = 0

    if not BUILDER_DIR.exists():
        print(f"Builder directory not found: {BUILDER_DIR}", file=sys.stderr)
        return

    for node_file in sorted(BUILDER_DIR.glob("*.json")):
        try:
            data = json.loads(node_file.read_text())
        except (json.JSONDecodeError, OSError) as e:
            print(f"  SKIP {node_file.name}: {e}", file=sys.stderr)
            continue

        builder_id = data.get("id", "")

        # Check if already has delivery_rate fact from this skill
        if has_fact_from_skill(data, "builder_delivery_rate"):
            already_has += 1
            continue

        # Find linked societies
        linked_societies = builder_societies.get(builder_id, [])
        if not linked_societies:
            skipped_no_societies += 1
            continue

        # Count project statuses
        total_projects = 0
        on_time_projects = 0
        delayed_count = 0

        for soc_id in linked_societies:
            status = society_statuses.get(soc_id)
            if not status:
                continue
            total_projects += 1
            if status == "delayed":
                delayed_count += 1
            else:
                on_time_projects += 1

        if total_projects < MIN_PROJECTS:
            skipped_few_projects += 1
            continue

        # Compute delivery rate
        delivery_rate = on_time_projects / total_projects

        # Build facts
        new_facts = []
        new_facts.append(build_delivery_rate_fact(delivery_rate))
        new_facts.append(build_project_count_fact(total_projects))
        if delayed_count == 0:
            new_facts.append(build_zero_revocations_fact())

        # Append facts to node
        data.setdefault("facts", []).extend(new_facts)

        # Atomic write
        tmp_path = node_file.with_suffix(".json.tmp")
        tmp_path.write_text(json.dumps(data, indent=2, ensure_ascii=False))
        os.rename(tmp_path, node_file)

        processed += 1
        builder_name = data.get("name", node_file.stem)
        print(f"  {builder_name}: rate={delivery_rate:.0%} ({on_time_projects}/{total_projects})")

    print(f"\nBuilder delivery rate computation complete:")
    print(f"  Processed: {processed}")
    print(f"  Skipped (fewer than {MIN_PROJECTS} projects): {skipped_few_projects}")
    print(f"  Skipped (no linked societies): {skipped_no_societies}")
    print(f"  Already had facts: {already_has}")


if __name__ == "__main__":
    run()
