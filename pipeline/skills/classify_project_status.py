"""
classify_project_status — pure computation skill (no LLM, zero cost).

Reads RERA date facts from knowledge graph nodes and produces a project_status
SourcedFact for each society that has sufficient date information.

Classification logic:
- completion_date <= today                              -> "ready_to_move"
- start_date and (today - start_date) <= 365 days       -> "new_launch"
- completion_date > today and > original_completion_date -> "delayed"
- completion_date > today                               -> "under_construction"
- start_date > today                                    -> "upcoming"
- else                                                  -> skip (insufficient data)

Date parsing: DD/MM/YYYY or DD-MM-YYYY (inconsistent from RERA portal).

Usage: python3 -m pipeline.skills.classify_project_status
"""

import json
import os
import sys
from datetime import date, datetime
from pathlib import Path
from typing import Optional

from pipeline.skills.base import FactSource, SourcedFact


KNOWLEDGE_DIR = Path("data/knowledge/nodes")
SOCIETY_DIR = KNOWLEDGE_DIR / "society"


def parse_rera_date(date_str: str) -> Optional[date]:
    """Parse DD/MM/YYYY or DD-MM-YYYY format dates from RERA portal."""
    if not date_str or not isinstance(date_str, str):
        return None

    for fmt in ("%d/%m/%Y", "%d-%m-%Y"):
        try:
            return datetime.strptime(date_str.strip(), fmt).date()
        except ValueError:
            continue
    return None


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


# Status metadata: display_template, answers_preferences per status
STATUS_META = {
    "ready_to_move": {
        "display_template": "Ready to Move",
        "answers_preferences": [
            "ready to move", "ready possession", "completed project",
            "move in now", "immediate possession", "ready flat",
            "delivered", "completed",
        ],
    },
    "new_launch": {
        "display_template": "New Launch",
        "answers_preferences": [
            "new launch", "new project", "newly launched",
            "fresh project", "just launched", "latest project",
        ],
    },
    "under_construction": {
        "display_template": "Under Construction",
        "answers_preferences": [
            "under construction", "ongoing project", "building",
            "in progress", "pre-launch",
        ],
    },
    "delayed": {
        "display_template": "Delayed",
        "answers_preferences": [
            "delayed project", "delayed possession", "delayed",
            "behind schedule",
        ],
    },
    "upcoming": {
        "display_template": "Upcoming",
        "answers_preferences": [
            "upcoming project", "future project", "upcoming launch",
            "not started",
        ],
    },
}


def classify_status(
    completion_date: Optional[date],
    original_completion_date: Optional[date],
    start_date: Optional[date],
    today: date,
) -> Optional[str]:
    """Apply the classification logic from the day plan."""
    if completion_date and completion_date <= today:
        return "ready_to_move"
    elif start_date and (today - start_date).days <= 365:
        return "new_launch"
    elif completion_date and completion_date > today:
        if original_completion_date and completion_date > original_completion_date:
            return "delayed"
        else:
            return "under_construction"
    elif start_date and start_date > today:
        return "upcoming"
    return None


def build_fact(status: str, completion_date_str: Optional[str]) -> dict:
    """Build a SourcedFact dict for the project_status."""
    meta = STATUS_META[status]

    display = meta["display_template"]
    if status == "ready_to_move" and completion_date_str:
        display = f"Ready to Move — delivered {completion_date_str}"

    fact = SourcedFact(
        key="project_status",
        value={"type": "Text", "data": status},
        confidence=1.0,
        source=FactSource(
            source_type="Computed",
            skill_id="classify_project_status",
        ),
        display_template=display,
        answers_preferences=meta["answers_preferences"],
        scoring_hint={"direction": "TextMatch", "weight": 3.0},
    )
    return fact.to_dict()


def run():
    """Process all society nodes and add project_status facts."""
    today = date.today()
    classified = 0
    skipped = 0
    already_has = 0
    status_counts: dict[str, int] = {}

    if not SOCIETY_DIR.exists():
        print(f"Society directory not found: {SOCIETY_DIR}", file=sys.stderr)
        return

    for node_file in sorted(SOCIETY_DIR.glob("*.json")):
        try:
            data = json.loads(node_file.read_text())
        except (json.JSONDecodeError, OSError) as e:
            print(f"  SKIP {node_file.name}: {e}", file=sys.stderr)
            continue

        # Check if already has project_status fact from this skill
        existing = [
            f for f in data.get("facts", [])
            if f["key"] == "project_status"
            and f.get("source", {}).get("skill_id") == "classify_project_status"
        ]
        if existing:
            already_has += 1
            continue

        # Extract date facts
        completion_str = get_fact_value(data, "rera_completion_date")
        original_completion_str = get_fact_value(data, "rera_original_completion_date")
        start_str = get_fact_value(data, "rera_start_date")

        completion_date = parse_rera_date(completion_str) if completion_str else None
        original_completion_date = parse_rera_date(original_completion_str) if original_completion_str else None
        start_date = parse_rera_date(start_str) if start_str else None

        if not completion_date and not start_date:
            skipped += 1
            continue

        status = classify_status(completion_date, original_completion_date, start_date, today)
        if not status:
            skipped += 1
            continue

        # Build and append the fact
        fact_dict = build_fact(status, completion_str)
        data.setdefault("facts", []).append(fact_dict)

        # Atomic write
        tmp_path = node_file.with_suffix(".json.tmp")
        tmp_path.write_text(json.dumps(data, indent=2, ensure_ascii=False))
        os.rename(tmp_path, node_file)

        classified += 1
        status_counts[status] = status_counts.get(status, 0) + 1
        print(f"  {data.get('name', node_file.stem)}: {status}")

    print(f"\nClassification complete:")
    print(f"  Classified: {classified}")
    print(f"  Skipped (no dates): {skipped}")
    print(f"  Already had status: {already_has}")
    print(f"  Status breakdown:")
    for status, count in sorted(status_counts.items()):
        print(f"    {status}: {count}")


if __name__ == "__main__":
    run()
