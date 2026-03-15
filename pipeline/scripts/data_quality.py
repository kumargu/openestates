"""
data_quality — evaluate data quality for all society nodes in the knowledge graph.

Measures 5 dimensions:
  1. Identity completeness (name, area, city, builder_name)
  2. RERA completeness (16 key RERA facts)
  3. Enrichment depth (Reddit, Google, pricing, embeddings)
  4. Fact freshness (age vs thresholds)
  5. Consistency (area/builder node existence, edges)

Outputs:
  - Human-readable table to stdout
  - Machine-readable JSON report to data/validation/data_quality_report.json

No LLM calls. Pure local evaluation.

Usage:
  python3 -m pipeline.scripts.data_quality
  python3 -m pipeline.scripts.data_quality --verbose
"""

import argparse
import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional

KNOWLEDGE_DIR = Path("data/knowledge/nodes")
SOCIETY_DIR = KNOWLEDGE_DIR / "society"
BUILDER_DIR = KNOWLEDGE_DIR / "builder"
AREA_DIR = KNOWLEDGE_DIR / "area"
EDGES_FILE = Path("data/knowledge/edges.json")
REPORT_PATH = Path("data/validation/data_quality_report.json")

# --- Dimension definitions ---

IDENTITY_KEYS = ["name", "area", "city", "builder_name"]

RERA_KEYS = [
    "rera_registered", "rera_number", "rera_ack_number", "rera_status",
    "rera_approved_on", "rera_completion_date", "rera_original_completion_date",
    "rera_project_type", "rera_promoter_name", "rera_land_litigation",
    "rera_complaints_count", "rera_complaints_resolved_pct",
    "rera_builder_revocations", "rera_builder_states",
    "rera_portal_url", "rera_start_date",
]

ENRICHMENT_CATEGORIES = {
    "reddit": ["reddit_thread_count", "reddit_threads", "reddit_total_comments", "reddit_total_score"],
    "google": ["google_sentiment", "google_top_positives", "google_top_negatives", "google_common_themes"],
    "pricing": ["price_per_sqft", "pricing_2bhk", "pricing_3bhk"],
    "embeddings": ["embedding_computed"],
    "summary": ["summary", "sentiment_summary"],
}

# Freshness thresholds in days
FRESHNESS_FRESH = 30
FRESHNESS_ACCEPTABLE = 90
FRESHNESS_STALE = 180


def get_fact_keys(node: dict) -> set[str]:
    """Return set of all fact keys present in a node."""
    return {fact["key"] for fact in node.get("facts", [])}


def get_fact_value(node: dict, key: str) -> Optional[Any]:
    """Get the latest fact value for a given key."""
    best = None
    best_version = -1
    for fact in node.get("facts", []):
        if fact["key"] == key:
            v = fact.get("version", 1)
            if v > best_version:
                best_version = v
                best = fact.get("value", {}).get("data")
    return best


def get_fact_timestamps(node: dict) -> list[Optional[str]]:
    """Return list of timestamps from all facts.

    Priority: learned_at (primary field) > created_at > source.collected_at.
    Falls back to node-level updated_at if no fact-level timestamps found.
    """
    timestamps = []
    for fact in node.get("facts", []):
        ts = fact.get("learned_at")
        if not ts:
            ts = fact.get("created_at")
        if not ts:
            source = fact.get("source", {})
            ts = source.get("collected_at")
        timestamps.append(ts)

    # If no fact-level timestamps found, use node-level updated_at as fallback
    if not any(timestamps):
        node_updated = node.get("updated_at")
        if node_updated:
            timestamps = [node_updated]

    return timestamps


# --- Scoring functions ---

def score_identity(node: dict) -> float:
    """Score identity completeness (0-1)."""
    fact_keys = get_fact_keys(node)
    present = 0
    for key in IDENTITY_KEYS:
        if key in fact_keys:
            val = get_fact_value(node, key)
            if val and str(val).lower() not in ("unknown", "none", ""):
                present += 1
    return present / len(IDENTITY_KEYS)


def score_rera(node: dict) -> float:
    """Score RERA completeness (0-1). Fraction of 16 key RERA facts present."""
    fact_keys = get_fact_keys(node)
    present = sum(1 for key in RERA_KEYS if key in fact_keys)
    return present / len(RERA_KEYS)


def score_enrichment(node: dict) -> float:
    """Score enrichment depth (0-1). Each enrichment category contributes equally."""
    fact_keys = get_fact_keys(node)
    categories_present = 0
    for category, keys in ENRICHMENT_CATEGORIES.items():
        if any(k in fact_keys for k in keys):
            categories_present += 1
    return categories_present / len(ENRICHMENT_CATEGORIES)


def score_freshness(node: dict) -> float:
    """Score fact freshness (0-1). Based on average age of facts."""
    now = datetime.now(timezone.utc)
    timestamps = get_fact_timestamps(node)
    ages_days: list[float] = []

    for ts in timestamps:
        if ts:
            try:
                dt = datetime.fromisoformat(ts.replace("Z", "+00:00"))
                age = (now - dt).days
                ages_days.append(age)
            except (ValueError, TypeError):
                pass

    if not ages_days:
        return 0.3  # No timestamps = moderate penalty, not zero

    avg_age = sum(ages_days) / len(ages_days)

    if avg_age <= FRESHNESS_FRESH:
        return 1.0
    elif avg_age <= FRESHNESS_ACCEPTABLE:
        return 0.8 - 0.3 * (avg_age - FRESHNESS_FRESH) / (FRESHNESS_ACCEPTABLE - FRESHNESS_FRESH)
    elif avg_age <= FRESHNESS_STALE:
        return 0.5 - 0.3 * (avg_age - FRESHNESS_ACCEPTABLE) / (FRESHNESS_STALE - FRESHNESS_ACCEPTABLE)
    else:
        return 0.2


def score_consistency(
    node: dict,
    area_slugs: set[str],
    builder_slugs: set[str],
    edges: list[dict],
) -> float:
    """Score consistency (0-1). Checks cross-references and edges."""
    node_id = node.get("id", "")
    checks_passed = 0
    total_checks = 4

    # 1. Area node exists for this society's area
    area_val = get_fact_value(node, "area")
    if area_val and str(area_val).lower() not in ("unknown", "none", ""):
        area_slug = _slugify(str(area_val))
        if area_slug in area_slugs:
            checks_passed += 1
    # If area is unknown, count as failed

    # 2. Builder node exists for this society's builder
    builder_val = get_fact_value(node, "builder_name")
    if builder_val:
        builder_slug = _slugify(str(builder_val))
        if builder_slug in builder_slugs:
            checks_passed += 1

    # 3. SocietyInArea edge exists
    has_area_edge = any(
        e["from"] == node_id and e["relation"] == "SocietyInArea"
        for e in edges
    )
    if has_area_edge:
        checks_passed += 1

    # 4. BuiltBy edge exists
    has_builder_edge = any(
        e["from"] == node_id and e["relation"] == "BuiltBy"
        for e in edges
    )
    if has_builder_edge:
        checks_passed += 1

    return checks_passed / total_checks


def _slugify(name: str) -> str:
    """Convert a name to a URL-friendly slug."""
    import re
    slug = re.sub(r'[^a-z0-9]+', '-', name.lower()).strip('-')
    slug = re.sub(r'-+', '-', slug)
    return slug


def classify_tier(score: float) -> str:
    """Classify a score into a quality tier."""
    if score >= 0.8:
        return "A"
    elif score >= 0.6:
        return "B"
    elif score >= 0.4:
        return "C"
    else:
        return "D"


# --- Main evaluation ---

def evaluate_all(verbose: bool = False) -> dict:
    """Evaluate all society nodes and return a structured report."""
    # Load reference data
    area_slugs = {f.stem for f in AREA_DIR.glob("*.json")} if AREA_DIR.exists() else set()
    builder_slugs = {f.stem for f in BUILDER_DIR.glob("*.json")} if BUILDER_DIR.exists() else set()
    edges = json.loads(EDGES_FILE.read_text()) if EDGES_FILE.exists() else []

    results: list[dict] = []
    tier_counts = {"A": 0, "B": 0, "C": 0, "D": 0}
    dimension_totals = {
        "identity": 0.0,
        "rera": 0.0,
        "enrichment": 0.0,
        "freshness": 0.0,
        "consistency": 0.0,
    }

    if not SOCIETY_DIR.exists():
        print("ERROR: Society directory not found.", file=sys.stderr)
        return {"nodes": [], "summary": {}}

    society_files = sorted(SOCIETY_DIR.glob("*.json"))

    for node_file in society_files:
        try:
            data = json.loads(node_file.read_text())
        except (json.JSONDecodeError, OSError) as e:
            print(f"  WARN: Skipping {node_file.name}: {e}", file=sys.stderr)
            continue

        identity = score_identity(data)
        rera = score_rera(data)
        enrichment = score_enrichment(data)
        freshness = score_freshness(data)
        consistency = score_consistency(data, area_slugs, builder_slugs, edges)

        # Weighted overall: identity (20%), RERA (25%), enrichment (25%), freshness (15%), consistency (15%)
        overall = (
            identity * 0.20
            + rera * 0.25
            + enrichment * 0.25
            + freshness * 0.15
            + consistency * 0.15
        )
        tier = classify_tier(overall)
        tier_counts[tier] += 1

        dimension_totals["identity"] += identity
        dimension_totals["rera"] += rera
        dimension_totals["enrichment"] += enrichment
        dimension_totals["freshness"] += freshness
        dimension_totals["consistency"] += consistency

        result = {
            "slug": node_file.stem,
            "name": data.get("name", node_file.stem),
            "root_source": data.get("root_source", "unknown"),
            "num_facts": len(data.get("facts", [])),
            "scores": {
                "identity": round(identity, 3),
                "rera": round(rera, 3),
                "enrichment": round(enrichment, 3),
                "freshness": round(freshness, 3),
                "consistency": round(consistency, 3),
                "overall": round(overall, 3),
            },
            "tier": tier,
        }
        results.append(result)

    n = len(results)
    averages = {k: round(v / n, 3) if n > 0 else 0.0 for k, v in dimension_totals.items()}
    avg_overall = round(sum(r["scores"]["overall"] for r in results) / n, 3) if n > 0 else 0.0

    summary = {
        "total_societies": n,
        "tier_distribution": tier_counts,
        "dimension_averages": averages,
        "overall_average": avg_overall,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "area_nodes": len(area_slugs),
        "builder_nodes": len(builder_slugs),
        "total_edges": len(edges),
    }

    return {"nodes": results, "summary": summary}


def print_report(report: dict, verbose: bool = False):
    """Print human-readable quality report to stdout."""
    summary = report["summary"]
    nodes = report["nodes"]

    print("=" * 90)
    print("  DATA QUALITY REPORT")
    print("=" * 90)
    print()
    print(f"  Societies evaluated: {summary['total_societies']}")
    print(f"  Area nodes: {summary['area_nodes']}, Builder nodes: {summary['builder_nodes']}, Edges: {summary['total_edges']}")
    print()

    # Tier distribution
    print("  TIER DISTRIBUTION")
    print("  -----------------")
    tiers = summary["tier_distribution"]
    total = summary["total_societies"]
    for tier_name in ["A", "B", "C", "D"]:
        count = tiers[tier_name]
        pct = (count / total * 100) if total > 0 else 0
        bar = "#" * int(pct / 2)
        print(f"    {tier_name}: {count:>3} ({pct:5.1f}%)  {bar}")
    print()

    # Dimension averages
    print("  DIMENSION AVERAGES")
    print("  ------------------")
    avgs = summary["dimension_averages"]
    for dim, val in avgs.items():
        bar = "#" * int(val * 20)
        print(f"    {dim:<14} {val:.3f}  {bar}")
    print(f"    {'overall':<14} {summary['overall_average']:.3f}")
    print()

    # Per-node table
    print(f"  {'Society':<45} {'Ident':>5} {'RERA':>5} {'Enrch':>5} {'Fresh':>5} {'Const':>5} {'Score':>6} {'Tier'}")
    print("  " + "-" * 88)

    sorted_nodes = sorted(nodes, key=lambda x: x["scores"]["overall"], reverse=True)
    for r in sorted_nodes:
        s = r["scores"]
        name = r["name"][:43]
        print(
            f"  {name:<45} {s['identity']:>5.2f} {s['rera']:>5.2f} "
            f"{s['enrichment']:>5.2f} {s['freshness']:>5.2f} {s['consistency']:>5.2f} "
            f"{s['overall']:>6.3f} {r['tier']}"
        )

    print()
    print(f"  Report saved to {REPORT_PATH}")


def save_report(report: dict):
    """Save JSON report to data/validation/."""
    REPORT_PATH.parent.mkdir(parents=True, exist_ok=True)
    tmp = REPORT_PATH.with_suffix(".json.tmp")
    tmp.write_text(json.dumps(report, indent=2, ensure_ascii=False))
    os.rename(tmp, REPORT_PATH)


def main():
    parser = argparse.ArgumentParser(description="Data quality scoring for society nodes")
    parser.add_argument("--verbose", action="store_true", help="Show detailed per-fact breakdown")
    args = parser.parse_args()

    report = evaluate_all(verbose=args.verbose)
    print_report(report, verbose=args.verbose)
    save_report(report)

    # Exit with 0 if report is valid (has at least 1 node)
    if report["summary"]["total_societies"] == 0:
        sys.exit(1)


if __name__ == "__main__":
    main()
