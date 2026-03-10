#!/usr/bin/env python3
"""
run_skill.py — CLI to run skills and push results to the knowledge graph.

Usage:
    python3 -m pipeline.skills.run_skill learn_society \
        --society "Prestige Lakeside Habitat" \
        --area Whitefield \
        --node-id "society:soc-prestige-lakeside"

    python3 -m pipeline.skills.run_skill search_reddit \
        --query "Prestige Lakeside Habitat Whitefield"
"""

import argparse
import json
import logging
import sys

from pipeline.skills.graph_client import GraphClient
from pipeline.skills.learn_society import LearnSocietySkill
from pipeline.skills.search_reddit import SearchRedditSkill

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(name)s] %(levelname)s: %(message)s",
)
logger = logging.getLogger(__name__)

SKILLS = {
    "learn_society": LearnSocietySkill,
    "search_reddit": SearchRedditSkill,
}


def main():
    parser = argparse.ArgumentParser(description="Run a skill and push results to the graph")
    parser.add_argument("skill", choices=SKILLS.keys(), help="Skill to run")
    parser.add_argument("--society", help="Society name (for learn_society)")
    parser.add_argument("--area", help="Area name")
    parser.add_argument("--city", default="Bengaluru", help="City name")
    parser.add_argument("--query", help="Search query (for search_reddit)")
    parser.add_argument("--node-id", help="Graph node ID to push facts to")
    parser.add_argument("--api-base", default="http://localhost:4000", help="API base URL")
    parser.add_argument("--force", action="store_true", help="Skip cache")
    parser.add_argument("--dry-run", action="store_true", help="Run skill but don't push to graph")

    args = parser.parse_args()

    # Build input based on skill
    if args.skill == "learn_society":
        if not args.society or not args.area:
            parser.error("learn_society requires --society and --area")
        input_data = {
            "society_name": args.society,
            "area": args.area,
            "city": args.city,
        }
    elif args.skill == "search_reddit":
        if not args.query:
            parser.error("search_reddit requires --query")
        input_data = {"query": args.query}
    else:
        parser.error(f"Unknown skill: {args.skill}")
        return

    # Run the skill
    skill_class = SKILLS[args.skill]
    skill = skill_class()
    result = skill.run(input_data, force=args.force)

    # Print results
    print(f"\n{'='*60}")
    print(f"Skill: {args.skill}")
    print(f"Facts: {len(result.facts)}")
    print(f"Confidence: {result.confidence:.2f}")
    print(f"Cost: {result.cost.llm_tokens} tokens, ${result.cost.estimated_usd:.4f}")
    print(f"Cached: {result.cached}")
    print(f"{'='*60}")

    for fact in result.facts:
        val_preview = str(fact.value.get("data", ""))[:80]
        print(f"  [{fact.source.source_type}] {fact.key} = {val_preview} (conf={fact.confidence:.1f})")

    # Push to graph if requested
    if args.node_id and not args.dry_run:
        print(f"\nPushing {len(result.facts)} facts to {args.node_id}...")
        client = GraphClient(api_base=args.api_base)
        response = client.push_skill_result(args.node_id, result)
        if response:
            print(f"Success: {response}")
        else:
            print("Failed to push facts to graph")
            sys.exit(1)
    elif args.dry_run:
        print("\n[DRY RUN] Would push to graph — skipping")
    elif not args.node_id:
        print("\n[INFO] No --node-id specified — facts not pushed to graph")
        print("  Use --node-id to push, or --dry-run to test")

    # Output JSON for piping
    print(f"\n--- JSON output ---")
    print(json.dumps(result.to_dict(), indent=2, default=str))


if __name__ == "__main__":
    main()
