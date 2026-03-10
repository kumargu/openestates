"""
rank_for_intent — given a user search query and a set of societies,
produce a ranked list with explanations.

This replaces hardcoded ranking logic. The LLM reads all available
facts for each candidate society and produces a ranked list that
explains WHY each society is ranked where it is, what tradeoffs exist,
and what the user should watch out for.

Input:  {"query": "quiet family apartment near metro Whitefield", "area": "Whitefield"}
Output: SkillResult with ranked societies + explanations

Usage:
  python3 -m pipeline.skills.rank_for_intent "quiet family apartment Whitefield"
  python3 -m pipeline.skills.rank_for_intent "best value 3bhk Sarjapur Road"
  python3 -m pipeline.skills.rank_for_intent --test
"""

import json
import logging
import os
from pathlib import Path
from typing import Optional

from pipeline.skills.base import BaseSkill, SkillResult, SkillCost, SourcedFact, FactSource

logger = logging.getLogger(__name__)

PROJECT_ROOT = Path(__file__).resolve().parent.parent.parent
KG_DIR = PROJECT_ROOT / "data" / "knowledge" / "nodes" / "society"
SEED_DIR = PROJECT_ROOT / "data" / "seed"

RANKING_PROMPT = """You are a real estate advisor ranking apartment societies for a homebuyer.

## User's search query: "{query}"

## Candidate societies (with all available data):

{candidates_block}

---

## Task

Rank these societies from best to worst match for the user's query.
For each society in the ranking, provide:
- **rank**: 1-based position
- **slug**: the society slug (as given)
- **score**: 0-100 relevance score for this specific query
- **why**: 1-2 sentences explaining why this society matches (or doesn't match) the query
- **tradeoff**: 1 sentence on the key tradeoff the buyer should know
- **match_tags**: 2-4 short tags showing which aspects of the query this society addresses

## Rules
- Rank based on the SPECIFIC QUERY, not general quality. A quiet society ranks higher for "quiet" even if another has better amenities.
- Use evidence from the data. Don't invent facts.
- Be honest about mismatches. If a society doesn't match the query well, say so.
- Score relative to the query: 80+ = strong match, 50-79 = partial match, <50 = weak match.
- Include ALL candidates in the ranking, even poor matches (they get low scores with honest explanations).
- Keep all text SHORT. No filler words.

Return ONLY valid JSON:
{{
  "query_understood": "1-sentence restatement of what the user wants",
  "ranking": [
    {{
      "rank": 1,
      "slug": "society-slug",
      "score": 85,
      "why": "Short reason this is the best match.",
      "tradeoff": "Key tradeoff to consider.",
      "match_tags": ["quiet", "family-friendly"]
    }}
  ]
}}"""


def _call_gemini(prompt: str) -> Optional[dict]:
    """Call Gemini Flash API."""
    import urllib.request

    api_key = os.environ.get("GOOGLE_AI_API_KEY")
    if not api_key:
        logger.error("GOOGLE_AI_API_KEY not set")
        return None

    url = f"https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={api_key}"
    payload = json.dumps({
        "contents": [{"parts": [{"text": prompt}]}],
        "generationConfig": {
            "responseMimeType": "application/json",
            "maxOutputTokens": 8192,
        },
    }).encode()

    req = urllib.request.Request(url, data=payload, headers={"Content-Type": "application/json"})

    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            data = json.loads(resp.read().decode())
    except Exception as e:
        logger.error("Gemini API call failed: %s", e)
        return None

    try:
        text = data["candidates"][0]["content"]["parts"][0]["text"].strip()
        usage = data.get("usageMetadata", {})
        tokens = usage.get("totalTokenCount", 0)

        if text.startswith("```"):
            lines = text.split("\n")
            text = "\n".join(lines[1:-1] if lines[-1].strip() == "```" else lines[1:])

        result = json.loads(text)
        result["_tokens"] = tokens
        return result
    except (KeyError, IndexError) as e:
        logger.error("Failed to extract Gemini response: %s", e)
        return None
    except json.JSONDecodeError as e:
        logger.warning("Gemini JSON parse error: %s", e)
        # Try repair
        import re
        text = re.sub(r'[\x00-\x1f\x7f]', ' ', text)
        # Close unclosed braces
        text = re.sub(r',\s*"[^"]*$', '', text)
        text = re.sub(r',\s*$', '', text)
        text = re.sub(r':\s*"[^"]*$', ': ""', text)
        stack = []
        for ch in text:
            if ch in '{[':
                stack.append('}' if ch == '{' else ']')
            elif ch in '}]' and stack:
                stack.pop()
        text += ''.join(reversed(stack))
        try:
            result = json.loads(text)
            result["_tokens"] = tokens
            return result
        except json.JSONDecodeError:
            logger.error("Could not repair JSON. First 500 chars: %s", text[:500])
            return None


def _load_society_data(slug: str) -> dict:
    """Load all available data for a society."""
    # Knowledge graph
    kg_path = KG_DIR / f"{slug}.json"
    facts = {}
    if kg_path.exists():
        data = json.loads(kg_path.read_text())
        for f in data.get("facts", []):
            key = f["key"]
            val = f["value"].get("data", "")
            if key in ("embedding_computed", "city"):
                continue
            if isinstance(val, str) and len(val) > 150:
                val = val[:150] + "..."
            if isinstance(val, list) and len(val) > 5:
                val = val[:5]
            facts[key] = val

    return facts


def _format_candidate(name: str, slug: str, facts: dict, seed: dict) -> str:
    """Format a candidate society for the prompt."""
    lines = [f"### {name} (slug: {slug})"]

    # Seed metadata
    for key in ["builder_name", "area", "year_built", "total_units"]:
        val = seed.get(key) or facts.get(key)
        if val and val != "Unknown — needs enrichment":
            lines.append(f"  {key}: {val}")

    # Key facts
    priority_keys = [
        "google_rating", "google_review_count", "google_sentiment",
        "google_top_positives", "google_top_negatives",
        "reddit_thread_count", "reddit_threads",
        "rera_verified", "rera_status",
        "maintenance_quality", "family_suitability", "noise_level",
        "security_quality", "resident_sentiment", "sentiment_summary",
        "score_maintenance_quality", "score_family_friendly",
        "score_builder_trust", "score_calm_environment",
        "score_community_vibe", "overall_score",
        "best_for_label", "one_line_verdict",
        "top_signals", "top_cautions",
    ]

    for key in priority_keys:
        val = facts.get(key)
        if val is not None:
            if isinstance(val, list):
                val = ", ".join(str(x) for x in val[:5])
            lines.append(f"  {key}: {val}")

    # Summary if available
    summary = facts.get("summary") or seed.get("summary", "")
    if summary:
        lines.append(f"  summary: {summary[:200]}")

    return "\n".join(lines)


class RankForIntentSkill(BaseSkill):
    skill_id = "rank_for_intent"
    description = "Rank societies for a specific user search query using all available knowledge"
    version = "1.0"

    def execute(self, input_data: dict) -> SkillResult:
        query = input_data.get("query", "")
        area = input_data.get("area", "")

        if not query:
            logger.error("rank_for_intent requires a query")
            return SkillResult(confidence=0.0)

        # Load seed societies, optionally filter by area
        seed_path = SEED_DIR / "societies.json"
        if not seed_path.exists():
            return SkillResult(confidence=0.0)

        societies = json.loads(seed_path.read_text())
        if area:
            societies = [s for s in societies if area.lower() in s.get("area", "").lower()]

        if not societies:
            logger.warning("No societies found for area '%s'", area)
            return SkillResult(confidence=0.0)

        # Limit to top 15 candidates (prompt size)
        societies = societies[:15]

        # Build candidate blocks
        candidates = []
        for soc in societies:
            slug = soc["id"].replace("soc-", "")
            facts = _load_society_data(slug)
            block = _format_candidate(soc["name"], slug, facts, soc)
            candidates.append(block)

        candidates_block = "\n\n".join(candidates)

        prompt = RANKING_PROMPT.format(
            query=query,
            candidates_block=candidates_block,
        )

        result = _call_gemini(prompt)
        if not result:
            return SkillResult(confidence=0.1, cost=SkillCost(api_calls=1))

        tokens_used = result.pop("_tokens", 0)

        source = FactSource(
            source_type="Llm",
            model="gemini-2.5-flash",
            skill_id=self.skill_id,
            triggered_by=input_data.get("triggered_by"),
        )

        facts = []

        # Store the ranking as a structured fact
        ranking = result.get("ranking", [])
        query_understood = result.get("query_understood", "")

        facts.append(SourcedFact(
            key="query_understood",
            value={"type": "Text", "data": query_understood},
            confidence=0.8,
            source=source,
        ))

        facts.append(SourcedFact(
            key="ranked_results",
            value={"type": "Text", "data": json.dumps(ranking)},
            confidence=0.7,
            source=source,
        ))

        return SkillResult(
            facts=facts,
            confidence=0.7,
            cost=SkillCost(
                llm_tokens=tokens_used,
                api_calls=1,
                estimated_usd=tokens_used * 0.000001,
            ),
        )

    def estimated_cost(self) -> SkillCost:
        return SkillCost(llm_tokens=5000, api_calls=1, estimated_usd=0.005)


# ──────────────────────────────────────────────────────────────
# Effectiveness test
# ──────────────────────────────────────────────────────────────

def run_effectiveness_test():
    """
    Test ranking quality with known queries where we have domain expectations.
    """
    import sys

    logging.basicConfig(level=logging.INFO, format="%(message)s")

    env_path = PROJECT_ROOT / ".env"
    if env_path.exists():
        for line in env_path.read_text().splitlines():
            if "=" in line and not line.startswith("#"):
                k, v = line.split("=", 1)
                os.environ.setdefault(k.strip(), v.strip())

    skill = RankForIntentSkill()

    test_cases = [
        {
            "query": "quiet family apartment Whitefield",
            "area": "Whitefield",
            "expect_top3_contains": ["prestige-lakeside-habitat", "prestige-raintree-park"],
            "expect_not_top3": [],
            "description": "Family + quiet should favor large townships with amenities",
        },
        {
            "query": "best value 3bhk Sarjapur Road",
            "area": "Sarjapur",
            "expect_top3_contains": [],  # less specific — just check it runs
            "expect_not_top3": [],
            "description": "Value-focused query for Sarjapur corridor",
        },
    ]

    total_checks = 0
    passed_checks = 0

    for tc in test_cases:
        query = tc["query"]
        print(f"\n{'='*60}")
        print(f"  Query: \"{query}\"")
        print(f"  {tc['description']}")
        print(f"{'='*60}")

        result = skill.run({
            "query": query,
            "area": tc["area"],
            "triggered_by": "effectiveness_test",
        }, force=True)

        if not result.facts:
            print(f"  FAIL: No results produced")
            continue

        # Extract ranking
        ranking_json = None
        query_understood = ""
        for fact in result.facts:
            if fact.key == "ranked_results":
                ranking_json = json.loads(fact.value["data"])
            elif fact.key == "query_understood":
                query_understood = fact.value["data"]

        if not ranking_json:
            print(f"  FAIL: No ranking in results")
            continue

        print(f"\n  Understood: {query_understood}")
        print(f"\n  Ranking:")
        top3_slugs = []
        for entry in ranking_json:
            rank = entry.get("rank", "?")
            slug = entry.get("slug", "?")
            score = entry.get("score", "?")
            why = entry.get("why", "")
            tradeoff = entry.get("tradeoff", "")
            tags = entry.get("match_tags", [])

            if isinstance(rank, int) and rank <= 3:
                top3_slugs.append(slug)

            marker = "  " if isinstance(rank, int) and rank <= 3 else "  "
            print(f"    #{rank} [{score}] {slug}")
            print(f"       Why: {why}")
            print(f"       Tradeoff: {tradeoff}")
            print(f"       Tags: {', '.join(tags)}")

        # Check expectations
        print(f"\n  Expectation checks:")
        for expected_slug in tc["expect_top3_contains"]:
            total_checks += 1
            if expected_slug in top3_slugs:
                passed_checks += 1
                print(f"    PASS: {expected_slug} is in top 3")
            else:
                print(f"    FAIL: {expected_slug} not in top 3 (got: {top3_slugs})")

        for unexpected_slug in tc["expect_not_top3"]:
            total_checks += 1
            if unexpected_slug not in top3_slugs:
                passed_checks += 1
                print(f"    PASS: {unexpected_slug} correctly not in top 3")
            else:
                print(f"    FAIL: {unexpected_slug} should not be in top 3")

        # Always count that ranking was produced
        total_checks += 1
        if len(ranking_json) >= 3:
            passed_checks += 1
            print(f"    PASS: Produced {len(ranking_json)} ranked results")
        else:
            print(f"    FAIL: Only {len(ranking_json)} results")

        print(f"\n  Cost: {result.cost.llm_tokens} tokens, ${result.cost.estimated_usd:.4f}")

    print(f"\n{'='*60}")
    print(f"  RANKING TEST RESULTS")
    print(f"{'='*60}")
    pct = (passed_checks / total_checks * 100) if total_checks > 0 else 0
    print(f"  Checks passed: {passed_checks}/{total_checks} ({pct:.0f}%)")


def main():
    import argparse
    import sys

    parser = argparse.ArgumentParser(description="Rank societies for a search query")
    parser.add_argument("query", nargs="?", help="Search query")
    parser.add_argument("--area", default="", help="Filter by area")
    parser.add_argument("--test", action="store_true", help="Run effectiveness test")
    args = parser.parse_args()

    logging.basicConfig(level=logging.INFO, format="%(message)s")

    env_path = PROJECT_ROOT / ".env"
    if env_path.exists():
        for line in env_path.read_text().splitlines():
            if "=" in line and not line.startswith("#"):
                k, v = line.split("=", 1)
                os.environ.setdefault(k.strip(), v.strip())

    if args.test:
        run_effectiveness_test()
        return

    if not args.query:
        print("Usage: python3 -m pipeline.skills.rank_for_intent \"your search query\"")
        print("       python3 -m pipeline.skills.rank_for_intent --test")
        sys.exit(1)

    skill = RankForIntentSkill()
    result = skill.run({
        "query": args.query,
        "area": args.area,
        "triggered_by": "manual",
    }, force=True)

    if not result.facts:
        print("  No results produced")
        return

    for fact in result.facts:
        if fact.key == "query_understood":
            print(f"\n  Understood: {fact.value['data']}")
        elif fact.key == "ranked_results":
            ranking = json.loads(fact.value["data"])
            print(f"\n  Ranking ({len(ranking)} societies):\n")
            for entry in ranking:
                print(f"  #{entry['rank']} [{entry['score']}] {entry['slug']}")
                print(f"     {entry['why']}")
                print(f"     Tradeoff: {entry['tradeoff']}")
                print(f"     Tags: {', '.join(entry.get('match_tags', []))}")
                print()


if __name__ == "__main__":
    main()
