"""
Integration test: Scrape RERA for existing societies and write facts to KG.

Usage:
    # Test with 5 societies first
    python3 -m tests.integration_rera --limit 5

    # All 55 societies
    python3 -m tests.integration_rera --all

    # Specific society
    python3 -m tests.integration_rera --name "Sobha Insignia"

    # Just verify what's already scraped
    python3 -m tests.integration_rera --verify-only
"""

import argparse
import json
import logging
import os
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

# Add project root to path
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from pipeline.entity_resolver import EntityResolver
from pipeline.skills.fetch_rera import FetchReraSkill, scrape_rera_listing
from pipeline.skills.base import SourcedFact

logger = logging.getLogger(__name__)

KG_NODES_DIR = Path("data/knowledge/nodes/society")


def slug_to_name(slug: str) -> str:
    """Convert slug to title case name."""
    return slug.replace("-", " ").title()


def write_facts_to_kg(entity_id: str, facts: list):
    """Write RERA facts to the society's KG node file."""
    slug = entity_id.split(":", 1)[-1]
    node_path = KG_NODES_DIR / f"{slug}.json"

    if node_path.exists():
        with open(node_path) as f:
            node = json.load(f)
    else:
        node = {
            "id": entity_id,
            "name": slug_to_name(slug),
            "node_type": "society",
            "facts": [],
            "edges": [],
        }

    # Remove existing RERA facts
    existing_facts = [f for f in node.get("facts", []) if not f.get("key", "").startswith("rera_")]

    # Add new RERA facts
    now_iso = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%fZ")
    for fact in facts:
        fact_dict = {
            "key": fact.key,
            "value": fact.value,
            "confidence": fact.confidence,
            "source": {
                "source_type": fact.source.source_type,
                "url": fact.source.url,
                "skill_id": fact.source.skill_id,
            },
            "learned_at": now_iso,
            "version": 1,
        }
        if fact.display_template:
            fact_dict["display_template"] = fact.display_template
        if fact.answers_preferences:
            fact_dict["answers_preferences"] = fact.answers_preferences
        if fact.scoring_hint:
            fact_dict["scoring_hint"] = fact.scoring_hint
        existing_facts.append(fact_dict)

    node["facts"] = existing_facts

    # Atomic write
    tmp_path = node_path.with_suffix(".tmp")
    with open(tmp_path, "w") as f:
        json.dump(node, f, indent=2)
    tmp_path.rename(node_path)

    return len(facts)


def scrape_and_store(society_name: str, entity_id: str, skill: FetchReraSkill, force: bool = False):
    """Scrape RERA for one society and store facts in KG."""
    print(f"\n{'='*60}")
    print(f"  {society_name} ({entity_id})")
    print(f"{'='*60}")

    result = skill.run({"project_name": society_name, "entity_id": entity_id}, force=force)

    if result.facts:
        # Check if registered
        reg_fact = next((f for f in result.facts if f.key == "rera_registered"), None)
        is_registered = reg_fact and reg_fact.value.get("data") is True

        if is_registered:
            # Write to KG
            written = write_facts_to_kg(entity_id, result.facts)
            print(f"  ✓ RERA FOUND — {len(result.facts)} facts written to KG")

            # Print key fields
            for f in result.facts:
                if f.key in ("rera_number", "rera_status", "rera_total_project_cost_inr",
                             "rera_total_units", "rera_complaints_count", "rera_completion_date"):
                    val = f.value.get("data", "")
                    print(f"    {f.key}: {val}")
            return "found", len(result.facts)
        else:
            # Still write the "not found" fact
            write_facts_to_kg(entity_id, result.facts)
            print(f"  ✗ Not found on RERA portal")
            return "not_found", 1
    else:
        print(f"  ✗ Scrape failed (no facts)")
        return "failed", 0


def verify_existing():
    """Check which societies already have RERA facts in KG."""
    print("\n" + "="*60)
    print("  EXISTING RERA DATA IN KNOWLEDGE GRAPH")
    print("="*60)

    found = 0
    not_found = 0
    no_rera = 0

    for node_file in sorted(KG_NODES_DIR.glob("*.json")):
        with open(node_file) as f:
            node = json.load(f)

        rera_facts = [f for f in node.get("facts", []) if f.get("key", "").startswith("rera_")]
        if rera_facts:
            reg = next((f for f in rera_facts if f["key"] == "rera_registered"), None)
            if reg and reg.get("value", {}).get("data") is True:
                name = node.get("name", node_file.stem)
                reg_num = next((f for f in rera_facts if f["key"] == "rera_number"), None)
                reg_val = reg_num["value"]["data"] if reg_num else "?"
                print(f"  ✓ {name}: {len(rera_facts)} facts (reg: {reg_val})")
                found += 1
            else:
                print(f"  ✗ {node.get('name', node_file.stem)}: not found on RERA")
                not_found += 1
        else:
            no_rera += 1

    print(f"\nSummary: {found} found, {not_found} not on RERA, {no_rera} not yet scraped")
    return found, not_found, no_rera


def main():
    parser = argparse.ArgumentParser(description="RERA Integration Test")
    parser.add_argument("--limit", type=int, default=5, help="Number of societies to scrape")
    parser.add_argument("--all", action="store_true", help="Scrape all societies")
    parser.add_argument("--name", help="Scrape a specific society by name")
    parser.add_argument("--force", action="store_true", help="Force re-scrape (skip cache)")
    parser.add_argument("--verify-only", action="store_true", help="Just check existing data")
    args = parser.parse_args()

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )

    if args.verify_only:
        verify_existing()
        return

    resolver = EntityResolver()
    skill = FetchReraSkill()

    # Build list of societies to scrape
    if args.name:
        # Single society by name
        eid = resolver.resolve_society(args.name)
        societies = [(eid, args.name)]
    else:
        all_societies = sorted(resolver.get_all_entities("society"))
        if args.all:
            societies = [(eid, resolver.get_name(eid) or eid.split(":", 1)[-1]) for eid in all_societies]
        else:
            # Pick a representative sample: mix of well-known builders
            priority_names = [
                "Sobha Insignia", "Prestige Lakeside Habitat", "Brigade Woods",
                "Godrej Splendour", "Century Ethos", "Purva Meraki",
                "Adarsh Tropica", "Sobha Neopolis", "Prestige Park Grove",
                "Kolte Patil 24K Grazio",
            ]
            societies = []
            for name in priority_names[:args.limit]:
                eid = resolver.resolve_society(name)
                societies.append((eid, name))

    print(f"\nScraping RERA for {len(societies)} societies...")
    print(f"Rate limited: ~1 sec between requests, ~3-5 sec per society")

    stats = {"found": 0, "not_found": 0, "failed": 0, "total_facts": 0}
    start_time = time.time()

    for eid, name in societies:
        try:
            status, fact_count = scrape_and_store(name, eid, skill, force=args.force)
            stats[status] = stats.get(status, 0) + 1
            stats["total_facts"] += fact_count
        except Exception as e:
            print(f"  ✗ ERROR: {e}")
            stats["failed"] += 1

    elapsed = time.time() - start_time

    print(f"\n{'='*60}")
    print(f"  INTEGRATION TEST RESULTS")
    print(f"{'='*60}")
    print(f"  Societies scraped: {len(societies)}")
    print(f"  Found on RERA:     {stats['found']}")
    print(f"  Not on RERA:       {stats['not_found']}")
    print(f"  Failed:            {stats['failed']}")
    print(f"  Total facts:       {stats['total_facts']}")
    print(f"  Time:              {elapsed:.1f}s ({elapsed/max(len(societies),1):.1f}s/society)")

    # Verify what's in KG now
    print()
    verify_existing()


if __name__ == "__main__":
    main()
