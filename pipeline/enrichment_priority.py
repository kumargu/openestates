"""
Map search-recorded enrichment gaps to enrichment_targets.json plans (offline stub).

Usage:
    python3 -m pipeline.enrichment_priority [--gaps data/validation/enrichment_gaps.json]
"""

from __future__ import annotations

import argparse
import json
from collections import Counter
from pathlib import Path
from typing import Any, Dict, List

PROJECT_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_GAPS = PROJECT_ROOT / "data" / "validation" / "enrichment_gaps.json"
DEFAULT_TARGETS = PROJECT_ROOT / "app" / "config" / "dag" / "enrichment_targets.json"
DEFAULT_OUTPUT = PROJECT_ROOT / "data" / "validation" / "enrichment_priority_queue.json"


def load_json(path: Path) -> Any:
    if not path.exists():
        return []
    return json.loads(path.read_text(encoding="utf-8"))


def fact_key_to_target_ids(fact_key: str, targets: List[Dict[str, Any]]) -> List[str]:
    matched: List[str] = []
    for target in targets:
        leaf_keys = target.get("leaf_keys") or []
        if fact_key in leaf_keys:
            matched.append(str(target.get("target_id") or ""))
    return [target_id for target_id in matched if target_id]


def build_priority_queue(
    gaps: List[Dict[str, Any]],
    targets_doc: Dict[str, Any],
) -> List[Dict[str, Any]]:
    targets = targets_doc.get("targets") or []
    counter: Counter = Counter()

    for gap in gaps:
        entity_id = str(gap.get("entity_id") or "").strip()
        missing_fact = str(gap.get("missing_fact") or gap.get("fact_key") or "").strip()
        if not entity_id or not missing_fact:
            continue
        for target_id in fact_key_to_target_ids(missing_fact, targets):
            counter[(entity_id, target_id, missing_fact)] += 1

    queue: List[Dict[str, Any]] = []
    for (entity_id, target_id, missing_fact), count in counter.most_common():
        queue.append(
            {
                "entity_id": entity_id,
                "target_id": target_id,
                "missing_fact": missing_fact,
                "gap_count": count,
                "command_hint": "openestates-enrich --entity {} --surface {}".format(
                    entity_id, target_id
                ),
            }
        )
    return queue


def main() -> int:
    parser = argparse.ArgumentParser(description="Build enrichment priority queue from gaps")
    parser.add_argument("--gaps", default=str(DEFAULT_GAPS))
    parser.add_argument("--targets", default=str(DEFAULT_TARGETS))
    parser.add_argument("--output", default=str(DEFAULT_OUTPUT))
    args = parser.parse_args()

    gaps = load_json(Path(args.gaps))
    if not isinstance(gaps, list):
        gaps = []
    targets_doc = load_json(Path(args.targets))
    if not isinstance(targets_doc, dict):
        targets_doc = {"targets": []}

    queue = build_priority_queue(gaps, targets_doc)
    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(queue, indent=2), encoding="utf-8")
    print("Wrote {} priority entries to {}".format(len(queue), output_path))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
