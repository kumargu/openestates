"""
normalize_rera.py — Fix RERA badge eligibility and normalize uppercase society names.

Two normalizations:
  1. RERA badge: ensure rera_registered=True for societies with RERA evidence
  2. Name case: convert ALL-UPPERCASE society names to Title Case

Usage:
    python3 -m pipeline.scripts.normalize_rera [--dry-run]
    python3 -m pipeline.scripts.normalize_rera --names-only [--dry-run]
"""

import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(PROJECT_ROOT))

KG_SOCIETY_DIR = PROJECT_ROOT / "data" / "knowledge" / "nodes" / "society"


def _is_rera_registered(value: dict) -> bool:
    """Check if a rera_registered fact value represents 'true'."""
    data = value.get("data")
    return data in (True, "true", "True", "yes")


def normalize_node(path: Path, dry_run: bool) -> bool:
    """Normalize RERA registration for a single node.

    Returns True if the node was modified (or would be in dry-run).
    """
    with path.open() as f:
        node = json.load(f)

    root_source = node.get("root_source", "")
    if root_source not in ("Rera", "rera"):
        return False

    facts = node.get("facts", [])
    fact_keys = {f["key"] for f in facts}

    # Safety check: must have rera_number or rera_ack_number
    has_rera_evidence = "rera_number" in fact_keys or "rera_ack_number" in fact_keys
    if not has_rera_evidence:
        return False

    # Find rera_registered fact
    rera_idx = None
    for i, fact in enumerate(facts):
        if fact["key"] == "rera_registered":
            rera_idx = i
            break

    if rera_idx is not None:
        existing_value = facts[rera_idx].get("value", {})
        if _is_rera_registered(existing_value):
            return False  # Already correct

        # Update existing fact
        facts[rera_idx]["value"] = {"type": "Bool", "data": True}
        facts[rera_idx]["confidence"] = 1.0
        facts[rera_idx]["display_template"] = "RERA Registered: Yes"
        facts[rera_idx]["scoring_hint"] = {"direction": "TextMatch", "weight": 3.0}
        facts[rera_idx]["answers_preferences"] = [
            "rera verified", "legally safe", "rera registered", "safe investment",
        ]
    else:
        # Add missing rera_registered fact
        new_fact = {
            "key": "rera_registered",
            "value": {"type": "Bool", "data": True},
            "confidence": 1.0,
            "source": {
                "source_type": "Rera",
                "skill_id": "normalize_rera",
            },
            "learned_at": datetime.now(timezone.utc).isoformat(),
            "version": 1,
            "display_template": "RERA Registered: Yes",
            "answers_preferences": [
                "rera verified", "legally safe", "rera registered", "safe investment",
            ],
            "scoring_hint": {"direction": "TextMatch", "weight": 3.0},
        }
        facts.append(new_fact)

    if not dry_run:
        node["facts"] = facts
        tmp = path.with_suffix(".tmp")
        with tmp.open("w") as f:
            json.dump(node, f, indent=2, default=str)
        os.rename(tmp, path)

    return True


def _is_all_uppercase(name: str) -> bool:
    """Check if a name is ALL-UPPERCASE (ignoring numbers and punctuation)."""
    alpha_chars = [c for c in name if c.isalpha()]
    if not alpha_chars:
        return False
    return all(c.isupper() for c in alpha_chars)


def normalize_names(dry_run: bool = False) -> int:
    """Convert ALL-UPPERCASE society names to Title Case.

    Updates both the top-level 'name' field and any 'name' fact in the facts list.
    Returns count of nodes modified.
    """
    print("\n=== Name Normalization: ALL-UPPERCASE to Title Case ===\n")

    if not KG_SOCIETY_DIR.exists():
        print("No society nodes directory found.")
        return 0

    modified_count = 0

    for path in sorted(KG_SOCIETY_DIR.glob("*.json")):
        try:
            with path.open() as f:
                node = json.load(f)
        except (json.JSONDecodeError, OSError) as e:
            print(f"  SKIP {path.name}: {e}")
            continue

        name = node.get("name", "")
        if not _is_all_uppercase(name):
            continue

        new_name = name.title()
        action = "Would rename" if dry_run else "Renamed"
        print(f"  {action}: {name!r} -> {new_name!r}")

        if not dry_run:
            node["name"] = new_name

            # Also update the 'name' fact if present
            for fact in node.get("facts", []):
                if fact.get("key") == "name":
                    val = fact.get("value", {})
                    if isinstance(val, dict) and _is_all_uppercase(str(val.get("data", ""))):
                        val["data"] = new_name

            tmp = path.with_suffix(".tmp")
            with tmp.open("w") as f:
                json.dump(node, f, indent=2, ensure_ascii=False, default=str)
            os.rename(tmp, path)

        modified_count += 1

    prefix = "[DRY RUN] " if dry_run else ""
    print(f"\n{prefix}Name normalization: {modified_count} societies updated")
    return modified_count


def main():
    import argparse
    parser = argparse.ArgumentParser(description="Normalize RERA badge eligibility and society names")
    parser.add_argument("--dry-run", action="store_true", help="Preview changes without writing")
    parser.add_argument("--names-only", action="store_true", help="Only run name normalization")
    args = parser.parse_args()

    if not KG_SOCIETY_DIR.exists():
        print("No society nodes directory found.")
        return

    if not args.names_only:
        # RERA badge normalization
        print("=== RERA Badge Normalization ===\n")
        total_files = 0
        total_modified = 0

        for path in sorted(KG_SOCIETY_DIR.glob("*.json")):
            total_files += 1
            modified = normalize_node(path, args.dry_run)
            if modified:
                total_modified += 1
                action = "Would normalize" if args.dry_run else "Normalized"
                print(f"  {action}: {path.stem}")

        prefix = "[DRY RUN] " if args.dry_run else ""
        print(f"\n{prefix}Total: {total_modified}/{total_files} society nodes normalized")

    # Name normalization (always run)
    normalize_names(dry_run=args.dry_run)


if __name__ == "__main__":
    main()
