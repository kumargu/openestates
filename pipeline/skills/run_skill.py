#!/usr/bin/env python3
"""
run_skill.py — CLI to run skills and push results to the knowledge graph.

Usage:
    python3 -m pipeline.skills.run_skill search_reddit \
        --query "Prestige Lakeside Habitat Whitefield"

    python3 -m pipeline.skills.run_skill fetch_rera \
        --project "Prestige Lakeside Habitat" \
        --node-id "society:prestige-lakeside-habitat"
"""

import argparse
import json
import logging
import sys

from pipeline.skills.graph_client import GraphClient
from pipeline.skills.fetch_rera import FetchReraSkill
from pipeline.skills.fetch_google_review_links import FetchGoogleReviewLinksSkill
from pipeline.skills.identify_gaps import IdentifyGapsSkill
from pipeline.skills.search_reddit import SearchRedditSkill

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(name)s] %(levelname)s: %(message)s",
)
logger = logging.getLogger(__name__)

SKILLS = {
    "search_reddit": SearchRedditSkill,
    "fetch_rera": FetchReraSkill,
    "fetch_google_review_links": FetchGoogleReviewLinksSkill,
    "identify_gaps": IdentifyGapsSkill,
}


def main():
    parser = argparse.ArgumentParser(description="Run a skill and push results to the graph")
    parser.add_argument("skill", choices=SKILLS.keys(), help="Skill to run")
    parser.add_argument("--project", help="Project name (for fetch_rera)")
    parser.add_argument("--query", help="Search query (for search_reddit)")
    parser.add_argument("--area", help="Area/locality for Google review link lookup")
    parser.add_argument("--city", default="Bengaluru", help="City for Google review link lookup")
    parser.add_argument("--society-id", help="Society ID (for identify_gaps)")
    parser.add_argument("--node-id", help="Graph node ID to push facts to")
    parser.add_argument("--api-base", default="http://localhost:4000", help="API base URL")
    parser.add_argument("--force", action="store_true", help="Skip cache")
    parser.add_argument("--retries", type=int, default=None, help="Override max retry count (default: 3)")
    parser.add_argument("--dry-run", action="store_true", help="Run skill but don't push to graph")

    args = parser.parse_args()

    # Build input based on skill
    if args.skill == "search_reddit":
        if not args.query:
            parser.error("search_reddit requires --query")
        input_data = {"query": args.query}
    elif args.skill == "fetch_rera":
        if not args.project:
            parser.error("fetch_rera requires --project")
        input_data = {"project_name": args.project, "entity_id": args.node_id}
    elif args.skill == "fetch_google_review_links":
        name = args.project or args.query
        if not name:
            parser.error("fetch_google_review_links requires --project or --query")
        input_data = {
            "society_name": name,
            "area": args.area or "",
            "city": args.city or "Bengaluru",
            "entity_id": args.node_id,
        }
    elif args.skill == "identify_gaps":
        society_id = args.society_id or args.node_id
        if not society_id:
            parser.error("identify_gaps requires --society-id or --node-id")
        if society_id.startswith("society:"):
            society_id = f"soc-{society_id.split(':', 1)[1]}"
        input_data = {"society_id": society_id}
    else:
        parser.error(f"Unknown skill: {args.skill}")
        return

    # Run the skill
    skill_class = SKILLS[args.skill]
    skill = skill_class()
    result = skill.run(input_data, force=args.force, max_retries=args.retries)

    # Print results
    print(f"\n{'='*60}")
    print(f"Skill: {args.skill}")
    print(f"Facts: {len(result.facts)}")
    print(f"Confidence: {result.confidence:.2f}")
    print(f"Cost: {result.cost.api_calls} API calls, ${result.cost.estimated_usd:.4f}")
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
