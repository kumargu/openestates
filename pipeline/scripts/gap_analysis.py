"""
gap_analysis.py — Audit KG nodes for missing critical facts, scoring_hints, and answers_preferences.

Scans all society and area nodes and generates a prioritized gap report.

Usage:
    python3 -m pipeline.scripts.gap_analysis
    python3 -m pipeline.scripts.gap_analysis --output docs/enrichment_gaps.md
"""

import json
import sys
from pathlib import Path
from datetime import datetime

PROJECT_ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(PROJECT_ROOT))

KG_NODES_DIR = PROJECT_ROOT / "data" / "knowledge" / "nodes"

# Critical facts that every society should have
SOCIETY_CRITICAL_FACTS = [
    "maintenance_quality",
    "family_friendly",
    "builder_reputation",
    "water_supply",
    "waterlogging_risk",
    "noise_score",
    "community_vibe",
    "livability_score",
    "open_space_score",
    "greenery_score",
]

# Critical facts that every area should have
AREA_CRITICAL_FACTS = [
    "waterlogging_risk",
    "traffic_score",
    "metro_distance",
    "livability_score",
    "greenery_score",
    "noise_score",
]

# Fact keys that should have scoring_hints
EXPECTED_SCORING_HINTS = {
    "maintenance_quality": {"direction": "HigherIsBetter", "weight": 2.0},
    "family_friendly": {"direction": "HigherIsBetter", "weight": 2.0},
    "water_supply": {"direction": "HigherIsBetter", "weight": 2.0},
    "waterlogging_risk": {"direction": "LowerIsBetter", "weight": 2.0},
    "noise_score": {"direction": "LowerIsBetter", "weight": 1.5},
    "traffic_score": {"direction": "LowerIsBetter", "weight": 1.5},
    "builder_reputation": {"direction": "HigherIsBetter", "weight": 1.5},
    "builder_quality_score": {"direction": "HigherIsBetter", "weight": 1.5},
    "open_space_score": {"direction": "HigherIsBetter", "weight": 1.5},
    "greenery_score": {"direction": "HigherIsBetter", "weight": 1.5},
    "community_vibe": {"direction": "HigherIsBetter", "weight": 1.5},
    "livability_score": {"direction": "HigherIsBetter", "weight": 2.0},
    "resale_strength": {"direction": "HigherIsBetter", "weight": 2.0},
    "metro_distance": {"direction": "LowerIsBetter", "weight": 1.5},
    "rera_status": {"direction": "HigherIsBetter", "weight": 1.5},
    "litigation_risk": {"direction": "LowerIsBetter", "weight": 2.0},
}

# Fact keys that should have answers_preferences
EXPECTED_ANSWERS_PREFERENCES = {
    "maintenance_quality": ["good maintenance", "well maintained", "maintenance", "good society"],
    "family_friendly": ["family friendly", "family", "kids", "children"],
    "water_supply": ["water", "water issues", "tanker"],
    "waterlogging_risk": ["flooding", "waterlogging", "water issues"],
    "noise_score": ["quiet", "peaceful", "calm", "noise"],
    "traffic_score": ["commute", "traffic", "connectivity"],
    "builder_reputation": ["builder trust", "trusted builder", "reliable builder", "good builder"],
    "open_space_score": ["open space", "breathing room", "less crowded", "spacious"],
    "greenery_score": ["green", "greenery", "park", "garden"],
    "community_vibe": ["good society", "livability", "community"],
    "livability_score": ["livability", "livable", "easy to live", "daily life"],
    "resale_strength": ["investment", "resale", "appreciation"],
    "metro_distance": ["metro access", "commute", "near metro"],
    "litigation_risk": ["legal", "rera", "safe documents", "legal safety"],
}


def load_nodes(node_type: str) -> list[dict]:
    """Load all nodes of a given type from the KG."""
    nodes_dir = KG_NODES_DIR / node_type
    if not nodes_dir.exists():
        return []
    nodes = []
    for path in sorted(nodes_dir.glob("*.json")):
        with path.open() as f:
            try:
                node = json.load(f)
                node["_path"] = str(path)
                nodes.append(node)
            except json.JSONDecodeError:
                print(f"  WARN: Failed to parse {path}", file=sys.stderr)
    return nodes


def audit_node(node: dict, critical_facts: list[str]) -> dict:
    """Audit a single node for missing facts, scoring_hints, and answers_preferences."""
    fact_map = {}
    for fact in node.get("facts", []):
        key = fact.get("key", "")
        if key not in fact_map or fact.get("version", 0) > fact_map[key].get("version", 0):
            fact_map[key] = fact

    missing_facts = [k for k in critical_facts if k not in fact_map]

    missing_scoring_hints = []
    for key, fact in fact_map.items():
        if key in EXPECTED_SCORING_HINTS and not fact.get("scoring_hint"):
            missing_scoring_hints.append(key)

    missing_answers_prefs = []
    for key, fact in fact_map.items():
        if key in EXPECTED_ANSWERS_PREFERENCES:
            existing = set(fact.get("answers_preferences", []))
            expected = set(EXPECTED_ANSWERS_PREFERENCES[key])
            if not existing or not expected.intersection(existing):
                missing_answers_prefs.append(key)

    has_embedding = bool(node.get("summary_embedding"))
    has_aspect_embeddings = bool(node.get("aspect_embeddings"))

    return {
        "id": node.get("id", "?"),
        "name": node.get("name", "?"),
        "fact_count": len(fact_map),
        "missing_facts": missing_facts,
        "missing_scoring_hints": missing_scoring_hints,
        "missing_answers_prefs": missing_answers_prefs,
        "has_embedding": has_embedding,
        "has_aspect_embeddings": has_aspect_embeddings,
        "gap_score": len(missing_facts) * 3 + len(missing_scoring_hints) + len(missing_answers_prefs) * 2,
    }


def generate_report(society_audits: list[dict], area_audits: list[dict]) -> str:
    lines = []
    lines.append("# Enrichment Gap Analysis")
    lines.append(f"\nGenerated: {datetime.utcnow().strftime('%Y-%m-%d %H:%M UTC')}\n")

    # Overall stats
    total_soc = len(society_audits)
    total_area = len(area_audits)
    soc_with_gaps = sum(1 for a in society_audits if a["gap_score"] > 0)
    area_with_gaps = sum(1 for a in area_audits if a["gap_score"] > 0)
    no_embedding = sum(1 for a in society_audits if not a["has_embedding"])
    no_aspects = sum(1 for a in society_audits if not a["has_aspect_embeddings"])

    lines.append("## Summary\n")
    lines.append(f"| Metric | Societies | Areas |")
    lines.append(f"|--------|-----------|-------|")
    lines.append(f"| Total nodes | {total_soc} | {total_area} |")
    lines.append(f"| With any gap | {soc_with_gaps} | {area_with_gaps} |")
    lines.append(f"| Missing summary embedding | {no_embedding} | - |")
    lines.append(f"| Missing aspect embeddings | {no_aspects} | - |")

    # Society gap detail
    lines.append("\n## Society Gaps (sorted by gap score)\n")
    lines.append("| Society | Facts | Missing facts | Missing scoring_hints | Missing answers_prefs | Gap score |")
    lines.append("|---------|-------|---------------|-----------------------|-----------------------|-----------|")
    for a in sorted(society_audits, key=lambda x: -x["gap_score"])[:30]:
        mf = ", ".join(a["missing_facts"][:3]) + ("..." if len(a["missing_facts"]) > 3 else "")
        mh = ", ".join(a["missing_scoring_hints"][:3]) + ("..." if len(a["missing_scoring_hints"]) > 3 else "")
        mp = ", ".join(a["missing_answers_prefs"][:3]) + ("..." if len(a["missing_answers_prefs"]) > 3 else "")
        lines.append(f"| {a['name']} | {a['fact_count']} | {mf or '-'} | {mh or '-'} | {mp or '-'} | {a['gap_score']} |")

    # Area gap detail
    lines.append("\n## Area Gaps\n")
    lines.append("| Area | Facts | Missing facts | Gap score |")
    lines.append("|------|-------|---------------|-----------|")
    for a in sorted(area_audits, key=lambda x: -x["gap_score"]):
        mf = ", ".join(a["missing_facts"][:3]) + ("..." if len(a["missing_facts"]) > 3 else "")
        lines.append(f"| {a['name']} | {a['fact_count']} | {mf or '-'} | {a['gap_score']} |")

    # Priority fixes
    lines.append("\n## Priority Fixes\n")
    top_gap_societies = sorted(society_audits, key=lambda x: -x["gap_score"])[:10]
    lines.append("### Top 10 societies needing enrichment\n")
    for i, a in enumerate(top_gap_societies, 1):
        lines.append(f"{i}. **{a['name']}** (gap={a['gap_score']})")
        if a["missing_facts"]:
            lines.append(f"   - Missing facts: {', '.join(a['missing_facts'])}")
        if a["missing_scoring_hints"]:
            lines.append(f"   - Missing scoring_hints: {', '.join(a['missing_scoring_hints'])}")
        if not a["has_embedding"]:
            lines.append(f"   - No embedding — run embed_entity")
        if not a["has_aspect_embeddings"]:
            lines.append(f"   - No aspect embeddings — run reembed_all")

    # Fix commands
    lines.append("\n## Fix Commands\n")
    lines.append("```bash")
    lines.append("# Fix scoring_hints and answers_preferences on existing facts")
    lines.append("python3 -m pipeline.scripts.fix_scoring_hints")
    lines.append("")
    lines.append("# Re-embed all society nodes with updated facts + aspect embeddings")
    lines.append("python3 -m pipeline.scripts.reembed_all --type all")
    lines.append("")
    lines.append("# Re-run evaluation after fixes")
    lines.append("python3 -m pipeline.eval_search --output docs/eval_search_v2.md")
    lines.append("```")

    return "\n".join(lines)


def main():
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", default="docs/enrichment_gaps.md")
    args = parser.parse_args()

    print("Loading KG nodes...")
    society_nodes = load_nodes("society")
    area_nodes = load_nodes("area")
    print(f"  {len(society_nodes)} society nodes, {len(area_nodes)} area nodes")

    print("Auditing society nodes...")
    society_audits = [audit_node(n, SOCIETY_CRITICAL_FACTS) for n in society_nodes]

    print("Auditing area nodes...")
    area_audits = [audit_node(n, AREA_CRITICAL_FACTS) for n in area_nodes]

    # Summary to console
    total_gaps = sum(a["gap_score"] for a in society_audits + area_audits)
    print(f"\nTotal gap score: {total_gaps}")
    print(f"Societies with gaps: {sum(1 for a in society_audits if a['gap_score'] > 0)}/{len(society_audits)}")
    print(f"Areas with gaps: {sum(1 for a in area_audits if a['gap_score'] > 0)}/{len(area_audits)}")

    report = generate_report(society_audits, area_audits)
    output_path = PROJECT_ROOT / args.output
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(report)
    print(f"\nReport written to {output_path}")


if __name__ == "__main__":
    main()
