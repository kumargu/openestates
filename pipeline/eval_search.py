"""
Search evaluation script — Day 36 fuzzy testing checkpoint.

Runs 20 curated queries against the live backend, captures full responses,
runs automated checks, and outputs a structured evaluation report.

Usage:
    python3 -m pipeline.eval_search [--base-url http://localhost:4000] [--output docs/eval_search_v1.md]
"""

import json
import sys
import urllib.parse
import urllib.request
import urllib.error
import argparse
from datetime import datetime
from typing import Any

BASE_URL_DEFAULT = "http://localhost:4000"

# ---------------------------------------------------------------------------
# Test query corpus — 20 representative buyer queries
# ---------------------------------------------------------------------------

QUERIES = [
    # Intent clarity tests
    {"id": "Q01", "query": "3 BHK Whitefield around 2.5 cr family friendly",
     "category": "intent_clarity", "expected_archetype": "family",
     "expected_area": "Whitefield", "expected_bhk": 3},

    {"id": "Q02", "query": "quiet apartment near metro whitefield",
     "category": "intent_clarity", "expected_area": "Whitefield",
     "expected_prefs": ["quiet", "commute"]},

    {"id": "Q03", "query": "good society for family in east bangalore",
     "category": "intent_clarity", "expected_archetype": "family"},

    # Soft preference tests
    {"id": "Q04", "query": "something calmer for my parents, less chaos, more breathing room",
     "category": "soft_preference", "expected_prefs": ["quiet", "open_space"]},

    {"id": "Q05", "query": "good family project but not fake luxury and not too dense",
     "category": "soft_preference", "expected_archetype": "family",
     "expected_neg_prefs": ["premium", "open_space"]},

    {"id": "Q06", "query": "society that feels easier to live in, not just impressive on paper",
     "category": "soft_preference", "expected_prefs": ["livability"]},

    {"id": "Q07", "query": "peaceful less crowded with greenery, okay to be slightly far from city",
     "category": "soft_preference", "expected_prefs": ["quiet", "open_space", "greenery"]},

    # Negative preference tests
    {"id": "Q08", "query": "avoid water issues, no tanker dependency",
     "category": "negative_preference",
     "expected_neg_prefs": ["water_issues"], "expect_concerns": True},

    {"id": "Q09", "query": "don't want maintenance headaches or shady builder",
     "category": "negative_preference",
     "expected_neg_prefs": ["good_maintenance", "builder_trust"], "expect_concerns": True},

    {"id": "Q10", "query": "not too packed, avoid highway noise and construction dust",
     "category": "negative_preference",
     "expected_neg_prefs": ["open_space", "quiet"], "expect_concerns": True},

    # Buyer archetype tests
    {"id": "Q11", "query": "best investment opportunity in whitefield under 1.5 cr",
     "category": "archetype", "expected_archetype": "investor",
     "expected_area": "Whitefield"},

    {"id": "Q12", "query": "safe legal paperwork, reliable builder, no risk",
     "category": "archetype", "expected_archetype": "riskAverse",
     "expected_prefs": ["legal_safety", "builder_trust"]},

    {"id": "Q13", "query": "affordable 2BHK for young couple, good commute",
     "category": "archetype", "expected_archetype": "valueBuyer",
     "expected_bhk": 2, "expected_prefs": ["commute", "value_for_money"]},

    {"id": "Q14", "query": "premium 4BHK, builder reputation matters, willing to pay more",
     "category": "archetype", "expected_archetype": "luxuryBuyer",
     "expected_bhk": 4, "expected_prefs": ["premium"]},

    # Contradiction / ambiguity tests
    {"id": "Q15", "query": "cheap but also premium quality",
     "category": "contradiction"},

    {"id": "Q16", "query": "near whitefield but avoid traffic",
     "category": "contradiction", "expected_area": "Whitefield",
     "expected_neg_prefs": ["commute"]},

    {"id": "Q17", "query": "good for family AND good investment",
     "category": "dual_intent", "expected_archetype": "family",
     "expected_prefs": ["family_friendly", "investment"]},

    # Edge cases
    {"id": "Q18", "query": "apartments",
     "category": "edge_case"},

    {"id": "Q19", "query": "tell me what's actually good in bangalore",
     "category": "edge_case"},

    {"id": "Q20", "query": "something like prestige lakeside but cheaper",
     "category": "edge_case"},
]


# ---------------------------------------------------------------------------
# API caller
# ---------------------------------------------------------------------------

def call_search(base_url: str, query: str, timeout: int = 10) -> dict | None:
    url = f"{base_url}/api/search?q={urllib.parse.quote(query)}"
    try:
        with urllib.request.urlopen(url, timeout=timeout) as resp:
            return json.loads(resp.read())
    except urllib.error.URLError as e:
        print(f"  ERROR: {e}", file=sys.stderr)
        return None
    except json.JSONDecodeError as e:
        print(f"  ERROR: bad JSON: {e}", file=sys.stderr)
        return None


# ---------------------------------------------------------------------------
# Automated checks
# ---------------------------------------------------------------------------

def check_archetype(resp: dict, expected: str | None) -> tuple[bool, str]:
    if expected is None:
        return True, "no expectation"
    intent = resp.get("intent", {})
    got = intent.get("buyerArchetype") or intent.get("buyer_archetype")
    if normalize_identifier(got) == normalize_identifier(expected):
        return True, f"correctly detected {expected}"
    return False, f"expected {expected}, got {got}"


def preference_values(intent: dict, field: str) -> set[str]:
    prefs = intent.get(field) or intent.get(camelize(field)) or []
    values: set[str] = set()
    for pref in prefs:
        if not isinstance(pref, dict):
            continue
        for key in ("canonicalKey", "canonical_key", "rawText", "raw_text"):
            add_preference_value(values, pref.get(key))
        for key in ("expandedKeys", "expanded_keys"):
            for expanded in pref.get(key) or []:
                add_preference_value(values, expanded)
    return values


def add_preference_value(values: set[str], value: Any) -> None:
    if not isinstance(value, str) or not value.strip():
        return
    normalized = value.strip().lower().replace("-", "_").replace(" ", "_")
    values.add(normalized)
    values.update(PREFERENCE_ALIASES.get(normalized, set()))


def camelize(field: str) -> str:
    head, *tail = field.split("_")
    return head + "".join(part[:1].upper() + part[1:] for part in tail)


def normalize_identifier(value: Any) -> str | None:
    if not isinstance(value, str):
        return None
    normalized = ""
    for index, char in enumerate(value.strip()):
        if char in {"-", " "}:
            normalized += "_"
        elif char.isupper() and index > 0:
            normalized += "_" + char.lower()
        else:
            normalized += char.lower()
    return normalized


PREFERENCE_ALIASES: dict[str, set[str]] = {
    "quiet_neighborhood": {"quiet"},
    "noise_score": {"quiet"},
    "noise_level": {"quiet"},
    "metro_access": {"commute"},
    "metro_distance": {"commute"},
    "metro_distance_mins": {"commute"},
    "nearby_metro_stations": {"commute"},
    "traffic": {"commute"},
    "traffic_reality": {"commute"},
    "commute_reality": {"commute"},
    "greenery": {"greenery", "open_space"},
    "green_cover": {"greenery"},
    "open_space": {"open_space"},
    "open_space_score": {"open_space"},
    "density_risk": {"open_space"},
    "liveability": {"livability"},
    "livability": {"livability"},
    "livability_sentiment": {"livability"},
    "family_friendly": {"family_friendly"},
    "school_access": {"family_friendly"},
    "nearby_schools": {"family_friendly"},
    "resale_potential": {"investment"},
    "rental_and_resale_demand": {"investment"},
    "resale_strength_score": {"investment"},
    "pricing_insight": {"investment", "value_for_money"},
    "value_for_money": {"value_for_money"},
    "price_per_sqft": {"value_for_money"},
    "legal_safety": {"legal_safety"},
    "rera_number": {"rera_registration"},
    "rera_registered": {"rera_registration"},
    "rera_registration": {"rera_registration"},
    "rera_status": {"rera_registration"},
    "legal_risk": {"legal_safety"},
    "litigation_risk": {"legal_safety"},
    "builder_trust": {"builder_trust"},
    "reliable_builder": {"builder_trust"},
    "trusted_builder": {"builder_trust"},
    "builder_reputation": {"builder_trust"},
    "builder_quality_score": {"builder_trust"},
    "maintenance": {"good_maintenance"},
    "maintenance_quality": {"good_maintenance"},
    "good_society": {"good_maintenance"},
    "water_supply": {"water_issues"},
    "water_supply_risk": {"water_issues"},
    "operating.tanker_dependence": {"water_issues"},
    "tanker_dependence": {"water_issues"},
    "water_issues": {"water_issues"},
    "waterlogging_risk": {"water_issues"},
    "premium": {"premium"},
}


def check_preferences(resp: dict, expected_pos: list[str]) -> tuple[bool, str]:
    if not expected_pos:
        return True, "no expectation"
    pos_keys = preference_values(resp.get("intent", {}), "positive_preferences")
    missing = [k for k in expected_pos if k not in pos_keys]
    if not missing:
        return True, f"all expected preferences found: {expected_pos}"
    return False, f"missing canonical keys: {missing} (got: {sorted(pos_keys)})"


def check_negative_prefs(resp: dict, expected_neg: list[str]) -> tuple[bool, str]:
    if not expected_neg:
        return True, "no expectation"
    neg_keys = preference_values(resp.get("intent", {}), "negative_preferences")
    missing = [k for k in expected_neg if k not in neg_keys]
    if not missing:
        return True, f"all expected negative preferences found: {expected_neg}"
    return False, f"missing negative keys: {missing} (got: {sorted(neg_keys)})"


def check_concerns(resp: dict, expect_concerns: bool) -> tuple[bool, str]:
    if not expect_concerns:
        return True, "no expectation"
    results = resp.get("results", [])
    any_concerns = any(result_has_concern_signal(r) for r in results[:5])
    if any_concerns:
        return True, "concerns or negative evidence surfaced in top results"
    return False, "no concerns/negative evidence found in top 5 results (expected some)"


def result_has_concern_signal(result: dict) -> bool:
    if len(result.get("concerns", [])) > 0:
        return True

    explanation = result.get("match_explanation") or result.get("matchExplanation") or {}
    coverages = explanation.get("preference_coverage") or explanation.get("preferenceCoverage") or []
    for coverage in coverages:
        if not isinstance(coverage, dict):
            continue
        preference = coverage.get("preference", "")
        status = coverage.get("status")
        if isinstance(preference, str) and preference.startswith("avoid ") and status in {
            "risk",
            "matched",
            "partial",
        }:
            return True
    return False


def check_area(resp: dict, expected_area: str | None) -> tuple[bool, str]:
    if expected_area is None:
        return True, "no expectation"
    got = resp.get("intent", {}).get("area")
    if got and got.lower() == expected_area.lower():
        return True, f"area correctly detected: {got}"
    return False, f"expected area '{expected_area}', got '{got}'"


def check_bhk(resp: dict, expected_bhk: int | None) -> tuple[bool, str]:
    if expected_bhk is None:
        return True, "no expectation"
    got = resp.get("intent", {}).get("bhk")
    if got == expected_bhk:
        return True, f"BHK correctly detected: {got}"
    return False, f"expected BHK={expected_bhk}, got {got}"


def avg_society_score(results: list[dict]) -> float:
    scores = [r.get("societyScore") for r in results if r.get("societyScore") is not None]
    return sum(scores) / len(scores) if scores else 0.0


def top3_summary(results: list[dict]) -> list[dict]:
    out = []
    for r in results[:3]:
        out.append({
            "title": r.get("title", "?"),
            "area": r.get("area", "?"),
            "match_score": round(r.get("match_score", 0), 3),
            "match_label": r.get("match_label", "?"),
            "society_score": r.get("societyScore"),
            "society_confidence": r.get("societyConfidence"),
            "concerns_count": len(r.get("concerns", [])),
            "has_explanation": (
                r.get("explanationCard") is not None
                or r.get("match_explanation") is not None
                or r.get("matchExplanation") is not None
            ),
        })
    return out


def run_automated_checks(query_def: dict, resp: dict) -> list[dict]:
    checks = []

    ok, msg = check_archetype(resp, query_def.get("expected_archetype"))
    checks.append({"check": "archetype_detection", "passed": ok, "detail": msg})

    ok, msg = check_preferences(resp, query_def.get("expected_prefs", []))
    checks.append({"check": "positive_preferences", "passed": ok, "detail": msg})

    ok, msg = check_negative_prefs(resp, query_def.get("expected_neg_prefs", []))
    checks.append({"check": "negative_preferences", "passed": ok, "detail": msg})

    ok, msg = check_concerns(resp, query_def.get("expect_concerns", False))
    checks.append({"check": "concern_surfacing", "passed": ok, "detail": msg})

    ok, msg = check_area(resp, query_def.get("expected_area"))
    checks.append({"check": "area_detection", "passed": ok, "detail": msg})

    ok, msg = check_bhk(resp, query_def.get("expected_bhk"))
    checks.append({"check": "bhk_detection", "passed": ok, "detail": msg})

    return checks


# ---------------------------------------------------------------------------
# Report generation
# ---------------------------------------------------------------------------

def build_markdown_report(eval_results: list[dict], run_at: str) -> str:
    lines = []
    lines.append("# Search Evaluation Report — v1")
    lines.append(f"\nGenerated: {run_at}\n")

    # Summary stats
    total = len(eval_results)
    errors = sum(1 for r in eval_results if r.get("error"))
    all_checks = [c for r in eval_results for c in r.get("checks", [])]
    passed = sum(1 for c in all_checks if c["passed"])
    check_pct = round(100 * passed / len(all_checks)) if all_checks else 0
    avg_results = sum(r["num_results"] for r in eval_results if not r.get("error")) / max(total - errors, 1)
    also_consider_count = sum(r["also_consider_count"] for r in eval_results if not r.get("error"))

    lines.append("## 1. Summary\n")
    lines.append(f"- **Queries run:** {total} (errors: {errors})")
    lines.append(f"- **Automated checks:** {passed}/{len(all_checks)} passed ({check_pct}%)")
    lines.append(f"- **Avg results per query:** {avg_results:.1f}")
    lines.append(f"- **Also-consider triggered:** {also_consider_count} total results across all queries\n")

    # Category breakdown
    categories: dict[str, list] = {}
    for r in eval_results:
        cat = r["category"]
        categories.setdefault(cat, []).append(r)

    lines.append("### Category pass rates\n")
    lines.append("| Category | Queries | Checks passed |")
    lines.append("|----------|---------|--------------|")
    for cat, items in categories.items():
        cat_checks = [c for r in items for c in r.get("checks", [])]
        cat_pass = sum(1 for c in cat_checks if c["passed"])
        cat_pct = round(100 * cat_pass / len(cat_checks)) if cat_checks else 0
        lines.append(f"| {cat} | {len(items)} | {cat_pass}/{len(cat_checks)} ({cat_pct}%) |")

    lines.append("\n## 2. Per-Query Results\n")

    for r in eval_results:
        lines.append(f"### {r['id']}: {r['query']}\n")
        if r.get("error"):
            lines.append(f"**ERROR:** {r['error']}\n")
            continue

        intent = r.get("intent", {})
        lines.append(f"- **Intent parsed:** area={intent.get('area')}, bhk={intent.get('bhk')}, "
                     f"budget={intent.get('budget_max')}, "
                     f"archetype={intent.get('buyerArchetype') or intent.get('buyer_archetype')}")
        pos = sorted(preference_values(intent, "positive_preferences"))
        neg = sorted(preference_values(intent, "negative_preferences"))
        if pos:
            lines.append(f"- **Positive prefs:** {', '.join(pos)}")
        if neg:
            lines.append(f"- **Negative prefs:** {', '.join(neg)}")
        lines.append(f"- **Results:** {r['num_results']} primary, {r['also_consider_count']} also-consider")

        if r.get("top3"):
            lines.append("\n**Top 3:**\n")
            lines.append("| # | Title | Area | Score | Label | Concerns | Has explanation |")
            lines.append("|---|-------|------|-------|-------|----------|----------------|")
            for i, t in enumerate(r["top3"]):
                lines.append(
                    f"| {i+1} | {t['title']} | {t['area']} | {t['match_score']} | "
                    f"{t['match_label']} | {t['concerns_count']} | {t['has_explanation']} |"
                )

        checks = r.get("checks", [])
        failing = [c for c in checks if not c["passed"] and "no expectation" not in c["detail"]]
        if failing:
            lines.append("\n**Failed checks:**")
            for c in failing:
                lines.append(f"- `{c['check']}`: {c['detail']}")

        lines.append("")

    # Patterns analysis
    lines.append("## 3. Pattern Analysis\n")
    lines.append("_Fill in manually after reviewing query-by-query results above._\n")
    lines.append("### What works well\n- TBD\n")
    lines.append("### What fails\n- TBD\n")
    lines.append("### Structural issues\n- TBD\n")

    # Priority fixes
    lines.append("## 4. Priority Fixes for Days 37-38\n")
    lines.append("_Ranked list of issues to address, derived from failing checks and manual evaluation._\n")
    lines.append("1. TBD\n2. TBD\n3. TBD\n")

    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Main runner
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description="Search evaluation script")
    parser.add_argument("--base-url", default=BASE_URL_DEFAULT)
    parser.add_argument("--output", default="docs/eval_search_v1.md")
    parser.add_argument("--json-output", help="Also write raw JSON to this path")
    args = parser.parse_args()

    print(f"Running search evaluation against {args.base_url}")
    print(f"Queries: {len(QUERIES)}\n")

    eval_results = []
    run_at = datetime.utcnow().strftime("%Y-%m-%d %H:%M UTC")

    for q_def in QUERIES:
        qid = q_def["id"]
        query = q_def["query"]
        cat = q_def.get("category", "?")
        print(f"  [{qid}] {query[:60]}")

        resp = call_search(args.base_url, query)
        if resp is None:
            eval_results.append({
                "id": qid, "query": query, "category": cat,
                "error": "Request failed or backend unreachable",
                "num_results": 0, "also_consider_count": 0, "checks": [],
            })
            continue

        results = resp.get("results", [])
        also = resp.get("also_consider", [])
        top3 = top3_summary(results)
        checks = run_automated_checks(q_def, resp)
        pass_count = sum(1 for c in checks if c["passed"])

        print(f"         results={len(results)}, also={len(also)}, "
              f"checks={pass_count}/{len(checks)}")

        eval_results.append({
            "id": qid,
            "query": query,
            "category": cat,
            "num_results": len(results),
            "also_consider_count": len(also),
            "intent": resp.get("intent", {}),
            "top3": top3,
            "avg_society_score": avg_society_score(results),
            "checks": checks,
        })

    # Write raw JSON
    if args.json_output:
        with open(args.json_output, "w") as f:
            json.dump(eval_results, f, indent=2)
        print(f"\nRaw JSON written to {args.json_output}")

    # Write markdown report
    report = build_markdown_report(eval_results, run_at)
    with open(args.output, "w") as f:
        f.write(report)
    print(f"Report written to {args.output}")

    # Print pass summary
    all_checks = [c for r in eval_results for c in r.get("checks", [])]
    passed = sum(1 for c in all_checks if c["passed"] and "no expectation" not in c["detail"])
    with_expectation = sum(1 for c in all_checks if "no expectation" not in c["detail"])
    print(f"\nAutomated checks: {passed}/{with_expectation} with expectations passed")


if __name__ == "__main__":
    main()
