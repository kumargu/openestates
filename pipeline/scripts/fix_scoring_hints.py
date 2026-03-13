"""
fix_scoring_hints.py — One-time migration to add scoring_hints and answers_preferences
to facts that are missing them.

Only adds — never overwrites existing values.

Usage:
    python3 -m pipeline.scripts.fix_scoring_hints [--dry-run]
"""

import json
import sys
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(PROJECT_ROOT))

KG_NODES_DIR = PROJECT_ROOT / "data" / "knowledge" / "nodes"

# scoring_hint defaults — only applied if fact.scoring_hint is missing
HINT_DEFAULTS: dict[str, dict] = {
    "maintenance_quality": {"direction": "HigherIsBetter", "weight": 2.0, "thresholds": [0.7, 0.4]},
    "maintenance_sentiment": {"direction": "TextMatch", "weight": 2.0},
    "family_friendly": {"direction": "HigherIsBetter", "weight": 2.0, "thresholds": [0.7, 0.4]},
    "family_suitability": {"direction": "TextMatch", "weight": 2.0},
    "water_supply": {"direction": "HigherIsBetter", "weight": 2.0, "thresholds": [0.6, 0.3]},
    "waterlogging_risk": {"direction": "LowerIsBetter", "weight": 2.0, "thresholds": [0.2, 0.5]},
    "tanker_dependency": {"direction": "LowerIsBetter", "weight": 1.5, "thresholds": [0.2, 0.5]},
    "noise_score": {"direction": "LowerIsBetter", "weight": 1.5, "thresholds": [0.3, 0.6]},
    "traffic_score": {"direction": "LowerIsBetter", "weight": 1.5, "thresholds": [0.3, 0.6]},
    "builder_reputation": {"direction": "HigherIsBetter", "weight": 1.5, "thresholds": [0.7, 0.4]},
    "builder_quality_score": {"direction": "HigherIsBetter", "weight": 1.5, "thresholds": [0.7, 0.4]},
    "open_space_score": {"direction": "HigherIsBetter", "weight": 1.5, "thresholds": [0.7, 0.4]},
    "greenery_score": {"direction": "HigherIsBetter", "weight": 1.5, "thresholds": [0.6, 0.3]},
    "community_vibe": {"direction": "TextMatch", "weight": 1.5},
    "livability_score": {"direction": "HigherIsBetter", "weight": 2.0, "thresholds": [0.7, 0.4]},
    "livability_sentiment": {"direction": "TextMatch", "weight": 2.0},
    "resale_strength": {"direction": "HigherIsBetter", "weight": 2.0, "thresholds": [0.7, 0.4]},
    "resale_strength_score": {"direction": "HigherIsBetter", "weight": 2.0, "thresholds": [0.7, 0.4]},
    "metro_distance": {"direction": "LowerIsBetter", "weight": 1.5, "thresholds": [10.0, 20.0]},
    "rera_status": {"direction": "TextMatch", "weight": 1.5},
    "litigation_risk": {"direction": "LowerIsBetter", "weight": 2.0, "thresholds": [0.1, 0.4]},
    "document_completeness": {"direction": "HigherIsBetter", "weight": 1.5, "thresholds": [0.8, 0.5]},
    "delivery_track_record": {"direction": "HigherIsBetter", "weight": 1.5, "thresholds": [0.7, 0.4]},
    "safety_rating": {"direction": "HigherIsBetter", "weight": 1.5, "thresholds": [0.7, 0.4]},
    "market_activity": {"direction": "TextMatch", "weight": 1.5},
    "resident_sentiment": {"direction": "TextMatch", "weight": 1.5},
    "school_nearby": {"direction": "HigherIsBetter", "weight": 1.0, "thresholds": [0.7, 0.4]},
    "amenity_quality": {"direction": "HigherIsBetter", "weight": 1.0, "thresholds": [0.7, 0.4]},
    "reddit_thread_count": {"direction": "HigherIsBetter", "weight": 0.5, "thresholds": [5.0, 2.0]},
}

# answers_preferences defaults — only applied if fact.answers_preferences is empty or missing
PREF_DEFAULTS: dict[str, list[str]] = {
    "maintenance_quality": ["good maintenance", "well maintained", "maintenance", "good society"],
    "maintenance_sentiment": ["good maintenance", "well maintained", "maintenance"],
    "family_friendly": ["family friendly", "family", "kids", "children", "good for family"],
    "family_suitability": ["family friendly", "family", "kids", "children"],
    "water_supply": ["water", "water issues", "water supply", "tanker"],
    "waterlogging_risk": ["flooding", "waterlogging", "water issues"],
    "tanker_dependency": ["tanker", "water issues", "water supply"],
    "noise_score": ["quiet", "peaceful", "calm", "noise", "noise level"],
    "traffic_score": ["commute", "traffic", "connectivity", "avoid traffic"],
    "builder_reputation": ["builder trust", "trusted builder", "reliable builder", "good builder", "shady builder"],
    "builder_quality_score": ["builder trust", "trusted builder", "reliable builder"],
    "open_space_score": ["open space", "breathing room", "less crowded", "spacious", "not crowded"],
    "greenery_score": ["green", "greenery", "park", "garden", "nature", "trees"],
    "community_vibe": ["good society", "livability", "community", "family friendly"],
    "livability_score": ["livability", "livable", "easy to live", "daily life", "daily convenience"],
    "livability_sentiment": ["livability", "livable", "easy to live"],
    "resale_strength": ["investment", "resale", "appreciation", "returns", "roi"],
    "resale_strength_score": ["investment", "resale", "appreciation"],
    "metro_distance": ["metro access", "commute", "near metro", "connectivity", "metro"],
    "rera_status": ["legal", "rera", "safe documents", "legal safety"],
    "litigation_risk": ["legal", "safe documents", "no risk", "risk free"],
    "document_completeness": ["legal", "safe documents", "documentation", "paperwork"],
    "delivery_track_record": ["builder trust", "reliable builder", "builder reputation"],
    "safety_rating": ["safety", "safe", "secure", "security"],
    "school_nearby": ["family friendly", "kids", "school"],
    "amenity_quality": ["premium", "luxury", "good society"],
    "resident_sentiment": ["good society", "livability", "maintenance"],
}


def fix_node_file(path: Path, dry_run: bool) -> int:
    """Fix a single node file. Returns number of facts modified."""
    with path.open() as f:
        node = json.load(f)

    modified = 0
    for fact in node.get("facts", []):
        key = fact.get("key", "")
        changed = False

        # Add scoring_hint if missing
        if key in HINT_DEFAULTS and not fact.get("scoring_hint"):
            hint = HINT_DEFAULTS[key].copy()
            if "thresholds" in hint:
                fact["scoring_hint"] = {
                    "direction": hint["direction"],
                    "weight": hint["weight"],
                    "thresholds": hint["thresholds"],
                }
            else:
                fact["scoring_hint"] = {
                    "direction": hint["direction"],
                    "weight": hint["weight"],
                }
            changed = True

        # Add answers_preferences if missing
        if key in PREF_DEFAULTS:
            existing = fact.get("answers_preferences", [])
            if not existing:
                fact["answers_preferences"] = PREF_DEFAULTS[key]
                changed = True
            # Don't overwrite existing — only add if completely empty

        if changed:
            modified += 1

    if modified > 0 and not dry_run:
        tmp = path.with_suffix(".tmp")
        with tmp.open("w") as f:
            json.dump(node, f, indent=2, default=str)
        tmp.rename(path)

    return modified


def main():
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--dry-run", action="store_true", help="Preview changes without writing")
    args = parser.parse_args()

    total_files = 0
    total_modified = 0

    for node_type in ("society", "area"):
        nodes_dir = KG_NODES_DIR / node_type
        if not nodes_dir.exists():
            continue
        for path in sorted(nodes_dir.glob("*.json")):
            total_files += 1
            n = fix_node_file(path, args.dry_run)
            if n > 0:
                total_modified += 1
                action = "Would update" if args.dry_run else "Updated"
                print(f"  {action}: {path.stem} ({n} facts modified)")

    print(f"\n{'[DRY RUN] ' if args.dry_run else ''}Total: {total_modified}/{total_files} files modified")


if __name__ == "__main__":
    main()
