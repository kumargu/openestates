"""
identify_gaps — analyze a society's knowledge graph node and determine
what data is missing, what should be enriched next, and why.

No LLM call needed. This is pure logic — compare what we have vs what
we want, prioritize by impact on scoring.

Input:  {"society_id": "soc-prestige-lakeside-habitat"} or {} for all
Output: SkillResult with gap analysis facts, or printed report

Usage:
  python3 -m pipeline.skills.identify_gaps                          # all societies
  python3 -m pipeline.skills.identify_gaps --id soc-sobha-marvella  # single
  python3 -m pipeline.skills.identify_gaps --summary                # coverage matrix
"""

import json
import logging
from dataclasses import dataclass
from pathlib import Path

from pipeline.skills.base import BaseSkill, SkillResult, SkillCost, SourcedFact, FactSource

logger = logging.getLogger(__name__)

PROJECT_ROOT = Path(__file__).resolve().parent.parent.parent
KG_DIR = PROJECT_ROOT / "data" / "knowledge" / "nodes" / "society"
SEED_DIR = PROJECT_ROOT / "data" / "seed"

# What a "fully enriched" society should have, ordered by priority.
# Each entry: (fact_key, priority 1-5, description, enrichment_action)
DESIRED_FACTS = [
    # Priority 1: Core identity (should always exist)
    ("name", 1, "Society name", "seed_data"),
    ("area", 1, "Area/locality", "seed_data"),
    ("builder_name", 1, "Builder/developer name", "seed_data"),

    # Priority 2: Trust signals (high impact on scoring)
    ("google_rating", 2, "Google Maps rating", "fetch_google_reviews"),
    ("google_review_count", 2, "Number of Google reviews", "fetch_google_reviews"),
    ("google_sentiment", 2, "Google review sentiment summary", "fetch_google_reviews"),
    ("google_top_positives", 2, "Top positive themes from reviews", "fetch_google_reviews"),
    ("google_top_negatives", 2, "Top negative themes from reviews", "fetch_google_reviews"),
    ("rera_verified", 2, "RERA compliance status", "verify_rera"),

    # Priority 3: Community intelligence (moderate impact)
    ("reddit_threads", 3, "Reddit discussion threads", "search_reddit"),
    ("reddit_thread_count", 3, "Number of Reddit threads", "search_reddit"),

    # Priority 4: Structured enrichment (from LLM synthesis)
    ("maintenance_quality", 4, "Maintenance quality assessment", "learn_society"),
    ("family_suitability", 4, "Family suitability assessment", "learn_society"),
    ("noise_level", 4, "Noise level assessment", "learn_society"),
    ("security_quality", 4, "Security quality assessment", "learn_society"),
    ("resident_sentiment", 4, "Overall resident sentiment", "learn_society"),

    # Priority 5: Scoring (from score_society skill)
    ("score_maintenance_quality", 5, "Maintenance dimension score", "score_society"),
    ("score_family_friendly", 5, "Family friendly dimension score", "score_society"),
    ("score_builder_trust", 5, "Builder trust dimension score", "score_society"),
    ("overall_score", 5, "Overall livability score", "score_society"),

    # Priority 3: Metadata (nice to have)
    ("year_built", 3, "Year of construction/completion", "seed_data"),
    ("total_units", 3, "Total number of units", "seed_data"),
    ("embedding_computed", 4, "Vector embedding for search", "embed_entity"),
]


@dataclass
class GapInfo:
    fact_key: str
    priority: int
    description: str
    enrichment_action: str


def analyze_gaps(society_id: str) -> dict:
    """
    Analyze what data is missing for a society.

    Returns dict with:
    - gaps: list of missing facts with priority and recommended action
    - coverage: fraction of desired facts present
    - next_action: the single most impactful thing to do next
    - readiness: "ready" | "partial" | "minimal" | "empty"
    """
    slug = society_id.replace("soc-", "")
    kg_path = KG_DIR / f"{slug}.json"

    existing_keys = set()
    if kg_path.exists():
        data = json.loads(kg_path.read_text())
        existing_keys = {f["key"] for f in data.get("facts", [])}

    gaps = []
    for key, priority, desc, action in DESIRED_FACTS:
        if key not in existing_keys:
            gaps.append(GapInfo(key, priority, desc, action))

    # Sort by priority (lower = more important)
    gaps.sort(key=lambda g: g.priority)

    total_desired = len(DESIRED_FACTS)
    present = total_desired - len(gaps)
    coverage = present / total_desired if total_desired > 0 else 0

    # Determine readiness level
    has_google = "google_rating" in existing_keys
    has_reddit = "reddit_threads" in existing_keys
    has_scores = "overall_score" in existing_keys
    has_identity = "name" in existing_keys

    if has_scores and has_google and has_reddit:
        readiness = "ready"
    elif has_google or has_reddit:
        readiness = "partial"
    elif has_identity:
        readiness = "minimal"
    else:
        readiness = "empty"

    # Determine the single most impactful next action
    # Group gaps by enrichment_action, pick the action that fills the most high-priority gaps
    action_impact = {}
    for g in gaps:
        if g.enrichment_action not in action_impact:
            action_impact[g.enrichment_action] = {"count": 0, "max_priority": 99, "keys": []}
        action_impact[g.enrichment_action]["count"] += 1
        action_impact[g.enrichment_action]["max_priority"] = min(
            action_impact[g.enrichment_action]["max_priority"], g.priority
        )
        action_impact[g.enrichment_action]["keys"].append(g.fact_key)

    # Sort by priority (lower first), then by count (more gaps filled first)
    next_action = None
    if action_impact:
        sorted_actions = sorted(
            action_impact.items(),
            key=lambda x: (x[1]["max_priority"], -x[1]["count"])
        )
        best = sorted_actions[0]
        next_action = {
            "skill": best[0],
            "fills": best[1]["count"],
            "keys": best[1]["keys"],
        }

    return {
        "society_id": society_id,
        "slug": slug,
        "coverage": round(coverage, 2),
        "present": present,
        "total": total_desired,
        "readiness": readiness,
        "gaps": [
            {
                "key": g.fact_key,
                "priority": g.priority,
                "description": g.description,
                "action": g.enrichment_action,
            }
            for g in gaps
        ],
        "next_action": next_action,
    }


class IdentifyGapsSkill(BaseSkill):
    skill_id = "identify_gaps"
    description = "Analyze knowledge gaps for a society and recommend enrichment priorities"
    version = "1.0"

    def execute(self, input_data: dict) -> SkillResult:
        society_id = input_data.get("society_id", "")
        if not society_id:
            return SkillResult(confidence=0.0)

        analysis = analyze_gaps(society_id)

        source = FactSource(
            source_type="Computed",
            skill_id=self.skill_id,
            triggered_by=input_data.get("triggered_by"),
        )

        facts = [
            SourcedFact(
                key="knowledge_coverage",
                value={"type": "Numeric", "data": analysis["coverage"]},
                confidence=1.0,
                source=source,
                display_template="Knowledge coverage: {value}%",
            ),
            SourcedFact(
                key="knowledge_readiness",
                value={"type": "Text", "data": analysis["readiness"]},
                confidence=1.0,
                source=source,
                display_template="Data readiness: {value}",
            ),
            SourcedFact(
                key="enrichment_gaps",
                value={"type": "Tags", "data": [g["key"] for g in analysis["gaps"][:5]]},
                confidence=1.0,
                source=source,
                display_template="Missing: {value}",
            ),
        ]

        if analysis["next_action"]:
            facts.append(SourcedFact(
                key="next_enrichment_action",
                value={"type": "Text", "data": analysis["next_action"]["skill"]},
                confidence=1.0,
                source=source,
                display_template="Next step: run {value}",
            ))

        return SkillResult(facts=facts, confidence=1.0)

    def estimated_cost(self) -> SkillCost:
        return SkillCost()  # Free — no API calls


def print_coverage_matrix():
    """Print a coverage matrix across all societies."""
    seed_path = SEED_DIR / "societies.json"
    if not seed_path.exists():
        print("  No societies.json found")
        return

    societies = json.loads(seed_path.read_text())
    analyses = []

    for soc in societies:
        analysis = analyze_gaps(soc["id"])
        analysis["name"] = soc["name"]
        analyses.append(analysis)

    # Sort by coverage (lowest first — most needy)
    analyses.sort(key=lambda a: a["coverage"])

    # Summary by readiness
    readiness_counts = {}
    for a in analyses:
        r = a["readiness"]
        readiness_counts[r] = readiness_counts.get(r, 0) + 1

    print(f"\n  Knowledge Gap Analysis — {len(analyses)} societies")
    print(f"  {'='*60}")
    print(f"\n  Readiness: {readiness_counts}")

    # Action queue: aggregate what enrichment actions are needed most
    action_queue = {}
    for a in analyses:
        if a["next_action"]:
            skill = a["next_action"]["skill"]
            if skill not in action_queue:
                action_queue[skill] = []
            action_queue[skill].append(a["name"])

    print(f"\n  Enrichment queue:")
    for skill, names in sorted(action_queue.items(), key=lambda x: -len(x[1])):
        print(f"    {skill}: {len(names)} societies")
        for name in names[:3]:
            print(f"      - {name}")
        if len(names) > 3:
            print(f"      ... and {len(names)-3} more")

    # Per-society detail
    print(f"\n  Per-society coverage (sorted worst → best):")
    print(f"  {'Name':40s} {'Coverage':>8s} {'Readiness':>10s} {'Next Action':>20s}")
    print(f"  {'-'*80}")
    for a in analyses:
        next_act = a["next_action"]["skill"] if a["next_action"] else "—"
        name = a["name"][:38]
        print(f"  {name:40s} {a['coverage']:>7.0%} {a['readiness']:>10s} {next_act:>20s}")


def main():
    import argparse

    parser = argparse.ArgumentParser(description="Identify knowledge gaps")
    parser.add_argument("--id", help="Analyze a single society")
    parser.add_argument("--summary", action="store_true", help="Show coverage matrix")
    args = parser.parse_args()

    logging.basicConfig(level=logging.INFO, format="%(message)s")

    if args.summary:
        print_coverage_matrix()
        return

    if args.id:
        analysis = analyze_gaps(args.id)
        print(f"\n  Gap analysis for {args.id}")
        print(f"  Coverage: {analysis['coverage']:.0%} ({analysis['present']}/{analysis['total']})")
        print(f"  Readiness: {analysis['readiness']}")
        if analysis["next_action"]:
            na = analysis["next_action"]
            print(f"  Next action: run `{na['skill']}` (fills {na['fills']} gaps: {', '.join(na['keys'][:5])})")
        print(f"\n  Gaps (by priority):")
        for g in analysis["gaps"]:
            print(f"    P{g['priority']} {g['key']:30s} → {g['action']}")
    else:
        print_coverage_matrix()


if __name__ == "__main__":
    main()
