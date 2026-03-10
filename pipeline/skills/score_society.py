"""
score_society — Claude skill that reads all available facts for a society
and produces dimension scores with explanations.

This replaces traditional ML scoring. Claude reads raw data (Google reviews,
Reddit threads, RERA status, seed metadata) and produces structured scores
with reasoning that can be displayed to users.

Input:  {"society_id": "soc-prestige-lakeside-habitat"}
Output: SkillResult with scored dimension facts + overall assessment

Usage:
  python3 -m pipeline.skills.score_society                               # score all
  python3 -m pipeline.skills.score_society --id soc-prestige-lakeside-habitat
  python3 -m pipeline.skills.score_society --test                        # effectiveness test
  python3 -m pipeline.skills.score_society --force                       # re-score even if cached
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

# The 6 dimensions we score societies on.
# Each maps to specific user-facing transparency signals.
DIMENSIONS = [
    "maintenance_quality",
    "family_friendly",
    "builder_trust",
    "value_for_money",
    "calm_environment",
    "community_vibe",
]

SCORING_PROMPT = """You are a real estate analyst scoring an apartment society for homebuyers.

## Society: {name}
## Area: {area}, {city}

## All available data:

{facts_block}

## Seed metadata:
{seed_block}

---

## Task

Score this society on these 6 dimensions. Each score is 0-100.
For each dimension, provide:
- score (0-100 integer)
- explanation (1-2 sentences explaining the score, citing specific evidence)
- confidence ("high" | "medium" | "low") — based on how much data supports the score

### Dimensions:

1. **maintenance_quality** — How well is the society maintained? (common areas, responsiveness, cleanliness)
2. **family_friendly** — How suitable for families with children? (safety, amenities, schools nearby, community)
3. **builder_trust** — How trustworthy is the builder? (reputation, construction quality, RERA compliance, delivery track record)
4. **value_for_money** — Is the pricing fair for what you get? (price/sqft vs area median, amenities, location)
5. **calm_environment** — How quiet and peaceful is it? (noise, traffic, green spaces, density)
6. **community_vibe** — How good is the community? (resident satisfaction, social life, management responsiveness)

Also provide:
- **overall_score** (0-100): weighted average reflecting overall livability
- **best_for_label**: a short label like "Best for families" or "Premium investment pick"
- **one_line_verdict**: 1 sentence overall assessment
- **top_signals**: 3-5 short positive signal tags (lowercase, hyphenated)
- **top_cautions**: 2-4 short caution tags

## Rules
- Base scores on EVIDENCE. Missing data = score 40-60, confidence "low".
- Google 4.0+ rating with 100+ reviews = strong positive.
- Reddit complaints about safety/waterlogging = strong negative.
- RERA verified = builder_trust boost.
- Calibration: 80+ = excellent, 30- = problematic. Don't inflate.
- Keep explanations SHORT (max 20 words each). No filler.

Return ONLY valid JSON matching this exact structure:
{{
  "dimensions": {{
    "maintenance_quality": {{"score": 70, "explanation": "short reason", "confidence": "medium"}},
    "family_friendly": {{"score": 65, "explanation": "short reason", "confidence": "high"}},
    "builder_trust": {{"score": 80, "explanation": "short reason", "confidence": "high"}},
    "value_for_money": {{"score": 55, "explanation": "short reason", "confidence": "low"}},
    "calm_environment": {{"score": 60, "explanation": "short reason", "confidence": "medium"}},
    "community_vibe": {{"score": 72, "explanation": "short reason", "confidence": "medium"}}
  }},
  "overall_score": 67,
  "best_for_label": "Good all-rounder",
  "one_line_verdict": "Short verdict.",
  "top_signals": ["well-maintained", "trusted-builder"],
  "top_cautions": ["traffic-congestion"]
}}"""


def _call_llm(prompt: str) -> Optional[dict]:
    """Call LLM API and return parsed JSON. Tries Gemini first, falls back to Claude."""
    import urllib.request

    # Try Gemini Flash first (cheaper, available)
    gemini_key = os.environ.get("GOOGLE_AI_API_KEY")
    if gemini_key:
        return _call_gemini(prompt, gemini_key)

    # Fall back to Claude
    claude_key = os.environ.get("ANTHROPIC_API_KEY")
    if claude_key:
        return _call_claude_api(prompt, claude_key)

    logger.error("No LLM API key set (GOOGLE_AI_API_KEY or ANTHROPIC_API_KEY)")
    return None


def _call_gemini(prompt: str, api_key: str) -> Optional[dict]:
    """Call Gemini Flash API."""
    import urllib.request

    url = f"https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={api_key}"
    payload = json.dumps({
        "contents": [{"parts": [{"text": prompt}]}],
        "generationConfig": {
            "responseMimeType": "application/json",
            "maxOutputTokens": 4096,
        },
    }).encode()

    req = urllib.request.Request(url, data=payload, headers={"Content-Type": "application/json"})

    try:
        with urllib.request.urlopen(req, timeout=45) as resp:
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
        result["_model"] = "gemini-2.5-flash"
        return result
    except (KeyError, IndexError) as e:
        logger.error("Failed to extract Gemini response: %s", e)
        return None
    except json.JSONDecodeError as e:
        logger.warning("Gemini JSON parse error: %s", e)
        # Try to repair truncated/malformed JSON
        repaired = _repair_json(text)
        if repaired:
            repaired["_tokens"] = tokens
            repaired["_model"] = "gemini-2.5-flash"
            return repaired
        logger.error("Could not repair JSON. First 500 chars: %s", text[:500])
        return None


def _repair_json(text: str) -> Optional[dict]:
    """Attempt to repair truncated or malformed JSON from LLM output."""
    import re

    # Remove control characters
    text = re.sub(r'[\x00-\x1f\x7f]', ' ', text)

    # Try direct parse first
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        pass

    # Strategy: close any unclosed braces/brackets
    # Count opening vs closing
    opens = text.count('{') + text.count('[')
    closes = text.count('}') + text.count(']')

    if opens > closes:
        # Find the last complete key-value pair and truncate there
        # Then close all remaining braces
        # First, try to find the last comma or complete value
        # Remove any trailing partial string
        text = re.sub(r',\s*"[^"]*$', '', text)  # remove trailing partial key
        text = re.sub(r',\s*$', '', text)  # remove trailing comma
        text = re.sub(r':\s*"[^"]*$', ': ""', text)  # close truncated string value

        # Close remaining braces/brackets in reverse order
        stack = []
        for ch in text:
            if ch in '{[':
                stack.append('}' if ch == '{' else ']')
            elif ch in '}]':
                if stack:
                    stack.pop()

        text += ''.join(reversed(stack))

        try:
            return json.loads(text)
        except json.JSONDecodeError:
            pass

    return None


def _call_claude_api(prompt: str, api_key: str) -> Optional[dict]:
    """Call Claude API."""
    import urllib.request

    payload = json.dumps({
        "model": "claude-sonnet-4-20250514",
        "max_tokens": 1500,
        "messages": [{"role": "user", "content": prompt}],
    }).encode()

    req = urllib.request.Request(
        "https://api.anthropic.com/v1/messages",
        data=payload,
        headers={
            "Content-Type": "application/json",
            "x-api-key": api_key,
            "anthropic-version": "2023-06-01",
        },
    )

    try:
        with urllib.request.urlopen(req, timeout=45) as resp:
            data = json.loads(resp.read().decode())
    except Exception as e:
        logger.error("Claude API call failed: %s", e)
        return None

    text = ""
    for block in data.get("content", []):
        if block.get("type") == "text":
            text = block["text"]
            break

    text = text.strip()
    if text.startswith("```"):
        lines = text.split("\n")
        text = "\n".join(lines[1:-1] if lines[-1].strip() == "```" else lines[1:])

    try:
        result = json.loads(text)
        usage = data.get("usage", {})
        result["_tokens"] = usage.get("input_tokens", 0) + usage.get("output_tokens", 0)
        result["_model"] = "claude-sonnet-4-20250514"
        return result
    except json.JSONDecodeError:
        logger.error("Failed to parse Claude response as JSON: %s", text[:300])
        return None


def _load_kg_facts(society_slug: str) -> list[dict]:
    """Load facts from knowledge graph node."""
    path = KG_DIR / f"{society_slug}.json"
    if not path.exists():
        return []
    data = json.loads(path.read_text())
    return data.get("facts", [])


def _load_seed_entry(society_id: str) -> Optional[dict]:
    """Load society entry from seed data."""
    path = SEED_DIR / "societies.json"
    if not path.exists():
        return None
    societies = json.loads(path.read_text())
    for s in societies:
        if s["id"] == society_id:
            return s
    return None


def _format_facts_for_prompt(facts: list[dict]) -> str:
    """Format knowledge graph facts into a readable block for the prompt."""
    lines = []
    for f in facts:
        key = f["key"]
        val = f["value"]
        data = val.get("data", "")
        src = f.get("source", {}).get("source_type", "?")
        conf = f.get("confidence", 0)

        # Skip metadata-only facts
        if key in ("embedding_computed", "name", "city"):
            continue

        if isinstance(data, list):
            data_str = ", ".join(str(x) for x in data[:7])
            if len(data) > 7:
                data_str += f" (+{len(data)-7} more)"
        elif isinstance(data, str) and len(data) > 200:
            data_str = data[:200] + "..."
        else:
            data_str = str(data)

        lines.append(f"- **{key}** [{src}, conf={conf}]: {data_str}")

    return "\n".join(lines) if lines else "(no enrichment data available)"


def _format_seed_for_prompt(seed: Optional[dict]) -> str:
    """Format seed metadata into a readable block."""
    if not seed:
        return "(no seed data)"

    parts = []
    for key in ["builder_name", "year_built", "total_units", "area", "summary",
                 "maintenance_sentiment", "livability_sentiment",
                 "common_positives", "common_complaints"]:
        val = seed.get(key)
        if val and val != "Unknown — needs enrichment":
            parts.append(f"- {key}: {val}")

    return "\n".join(parts) if parts else "(minimal seed data)"


class ScoreSocietySkill(BaseSkill):
    skill_id = "score_society"
    description = "Score a society on 6 dimensions using all available knowledge"
    version = "1.0"

    def execute(self, input_data: dict) -> SkillResult:
        society_id = input_data.get("society_id", "")
        if not society_id:
            logger.error("score_society requires society_id")
            return SkillResult(confidence=0.0)

        # Derive slug from society_id
        slug = society_id.replace("soc-", "")

        # Load all available data
        kg_facts = _load_kg_facts(slug)
        seed = _load_seed_entry(society_id)

        name = seed["name"] if seed else slug.replace("-", " ").title()
        area = seed.get("area", "") if seed else ""
        city = seed.get("city", "Bengaluru") if seed else "Bengaluru"

        if not kg_facts and not seed:
            logger.warning("[%s] No data available — cannot score", society_id)
            return SkillResult(confidence=0.0)

        # Build prompt
        facts_block = _format_facts_for_prompt(kg_facts)
        seed_block = _format_seed_for_prompt(seed)

        prompt = SCORING_PROMPT.format(
            name=name,
            area=area,
            city=city,
            facts_block=facts_block,
            seed_block=seed_block,
        )

        # Call Claude
        result = _call_llm(prompt)
        if not result:
            return SkillResult(confidence=0.1, cost=SkillCost(api_calls=1))

        tokens_used = result.pop("_tokens", 0)
        model_used = result.pop("_model", "unknown")
        dimensions = result.get("dimensions", {})

        # Convert to SourcedFacts
        source = FactSource(
            source_type="Llm",
            model=model_used,
            skill_id=self.skill_id,
            triggered_by=input_data.get("triggered_by"),
        )

        facts = []

        # Dimension score facts
        dim_display = {
            "maintenance_quality": ("Maintenance", ["well maintained", "good society", "maintenance"]),
            "family_friendly": ("Family Friendly", ["family friendly", "families", "kids", "children"]),
            "builder_trust": ("Builder Trust", ["trusted builder", "reputed builder", "reliable"]),
            "value_for_money": ("Value", ["good value", "affordable", "worth it", "value for money"]),
            "calm_environment": ("Calm & Green", ["quiet", "peaceful", "calm", "green", "serene"]),
            "community_vibe": ("Community", ["good community", "friendly neighbors", "social"]),
        }

        for dim_key, dim_data in dimensions.items():
            if dim_key not in dim_display:
                continue

            label, prefs = dim_display[dim_key]
            score = dim_data.get("score", 50)
            explanation = dim_data.get("explanation", "")
            confidence_str = dim_data.get("confidence", "medium")
            confidence_val = {"high": 0.8, "medium": 0.6, "low": 0.4}.get(confidence_str, 0.5)

            facts.append(SourcedFact(
                key=f"score_{dim_key}",
                value={"type": "Numeric", "data": score},
                confidence=confidence_val,
                source=source,
                display_template=f"{label}: {{value}}/100",
                answers_preferences=prefs,
                scoring_hint={
                    "direction": "HigherIsBetter",
                    "weight": 2.0,
                    "thresholds": [70.0, 40.0],
                },
            ))

            # Also store the explanation as a separate fact
            if explanation:
                facts.append(SourcedFact(
                    key=f"score_{dim_key}_reason",
                    value={"type": "Text", "data": explanation},
                    confidence=confidence_val,
                    source=source,
                    display_template=f"{label}: {{value}}",
                ))

        # Overall score
        overall = result.get("overall_score", 50)
        facts.append(SourcedFact(
            key="overall_score",
            value={"type": "Numeric", "data": overall},
            confidence=0.7,
            source=source,
            display_template="Overall Score: {value}/100",
            scoring_hint={"direction": "HigherIsBetter", "weight": 3.0, "thresholds": [70.0, 40.0]},
        ))

        # Best-for label
        best_for = result.get("best_for_label", "")
        if best_for:
            facts.append(SourcedFact(
                key="best_for_label",
                value={"type": "Text", "data": best_for},
                confidence=0.7,
                source=source,
                display_template="{value}",
            ))

        # One-line verdict
        verdict = result.get("one_line_verdict", "")
        if verdict:
            facts.append(SourcedFact(
                key="one_line_verdict",
                value={"type": "Text", "data": verdict},
                confidence=0.7,
                source=source,
                display_template="{value}",
            ))

        # Signal/caution tags
        for tag_key in ("top_signals", "top_cautions"):
            tags = result.get(tag_key, [])
            if tags:
                facts.append(SourcedFact(
                    key=tag_key,
                    value={"type": "Tags", "data": tags},
                    confidence=0.7,
                    source=source,
                    display_template=f"{tag_key.replace('_', ' ').title()}: {{value}}",
                ))

        return SkillResult(
            facts=facts,
            confidence=0.7,
            cost=SkillCost(
                llm_tokens=tokens_used,
                api_calls=1,
                estimated_usd=tokens_used * 0.000003,
            ),
        )

    def estimated_cost(self) -> SkillCost:
        return SkillCost(llm_tokens=3000, api_calls=1, estimated_usd=0.01)


# ──────────────────────────────────────────────────────────────
# Effectiveness test
# ──────────────────────────────────────────────────────────────

def run_effectiveness_test():
    """
    Test score-society effectiveness by scoring 3 societies with known
    characteristics and validating the output makes sense.

    Ground truth expectations (based on domain knowledge):
    - Prestige Lakeside Habitat: large, well-known, safety incidents in news → mixed signals
    - Sobha Marvella: premium Sobha, small (86 units), high Google rating → strong scores
    - Abhee Celestial City: lesser-known builder, Sarjapur Road → moderate scores
    """
    import sys

    logging.basicConfig(level=logging.INFO, format="%(message)s")

    # Load .env
    env_path = PROJECT_ROOT / ".env"
    if env_path.exists():
        for line in env_path.read_text().splitlines():
            if "=" in line and not line.startswith("#"):
                k, v = line.split("=", 1)
                os.environ.setdefault(k.strip(), v.strip())

    skill = ScoreSocietySkill()

    # Ground truth: what we expect directionally
    test_cases = [
        {
            "society_id": "soc-sobha-marvella",
            "expect": {
                # Sobha is premium but RERA not found + mixed maintenance reviews
                "family_friendly": (">=", 55, "Good amenities, security praised"),
                "community_vibe": (">=", 50, "Generally positive resident sentiment"),
                "overall_score": (">=", 50, "Premium society, some concerns"),
            },
        },
        {
            "society_id": "soc-prestige-lakeside-habitat",
            "expect": {
                # Safety incidents (child death, electrocution) are in Reddit data
                "family_friendly": ("<=", 60, "Safety incidents on Reddit should drag this down"),
                "maintenance_quality": (">=", 55, "Maintenance praised in Google reviews"),
                "overall_score": ("range", 35, 75, "Mixed — good amenities but safety concerns"),
            },
        },
        {
            "society_id": "soc-abhee-celestial-city",
            "expect": {
                # Google reviews mention incomplete facilities, waterlogging
                "maintenance_quality": ("<=", 55, "Google reviews cite maintenance issues"),
                "overall_score": ("range", 25, 65, "Weaker society with real complaints"),
            },
        },
    ]

    total_checks = 0
    passed_checks = 0
    results_summary = []

    for tc in test_cases:
        sid = tc["society_id"]
        print(f"\n{'='*60}")
        print(f"  Scoring: {sid}")
        print(f"{'='*60}")

        result = skill.run({"society_id": sid, "triggered_by": "effectiveness_test"}, force=True)

        if not result.facts:
            print(f"  FAIL: No facts produced")
            results_summary.append({"id": sid, "status": "FAIL", "reason": "no facts"})
            continue

        # Extract scores from facts
        scores = {}
        reasons = {}
        for fact in result.facts:
            if fact.key.startswith("score_") and not fact.key.endswith("_reason"):
                dim = fact.key.replace("score_", "")
                scores[dim] = fact.value["data"]
            elif fact.key.startswith("score_") and fact.key.endswith("_reason"):
                dim = fact.key.replace("score_", "").replace("_reason", "")
                reasons[dim] = fact.value["data"]
            elif fact.key == "overall_score":
                scores["overall_score"] = fact.value["data"]
            elif fact.key == "best_for_label":
                print(f"  Label: {fact.value['data']}")
            elif fact.key == "one_line_verdict":
                print(f"  Verdict: {fact.value['data']}")

        print(f"\n  Scores:")
        for dim in DIMENSIONS + ["overall_score"]:
            score = scores.get(dim, "?")
            reason = reasons.get(dim, "")
            print(f"    {dim:25s}: {score:>3}")
            if reason:
                print(f"      → {reason}")

        # Run expectation checks
        print(f"\n  Expectation checks:")
        case_passed = 0
        case_total = 0

        for dim, expectation in tc["expect"].items():
            score = scores.get(dim)
            if score is None:
                print(f"    SKIP {dim}: no score produced")
                continue

            case_total += 1
            total_checks += 1

            if expectation[0] == ">=":
                threshold = expectation[1]
                ok = score >= threshold
                symbol = ">="
                compare_val = threshold
            elif expectation[0] == "<=":
                threshold = expectation[1]
                ok = score <= threshold
                symbol = "<="
                compare_val = threshold
            elif expectation[0] == "range":
                low, high = expectation[1], expectation[2]
                ok = low <= score <= high
                symbol = f"in [{low},{high}]"
                compare_val = f"{low}-{high}"
            else:
                ok = False
                symbol = "?"
                compare_val = "?"

            status = "PASS" if ok else "FAIL"
            reason = expectation[-1]
            if ok:
                passed_checks += 1
                case_passed += 1
            print(f"    {status} {dim}: {score} {symbol} {compare_val} — {reason}")

        results_summary.append({
            "id": sid,
            "scores": scores,
            "passed": case_passed,
            "total": case_total,
        })

        print(f"\n  Cost: {result.cost.llm_tokens} tokens, ${result.cost.estimated_usd:.4f}")

    # Final summary
    print(f"\n{'='*60}")
    print(f"  EFFECTIVENESS TEST RESULTS")
    print(f"{'='*60}")
    print(f"  Checks passed: {passed_checks}/{total_checks}")
    pct = (passed_checks / total_checks * 100) if total_checks > 0 else 0
    print(f"  Pass rate: {pct:.0f}%")

    for r in results_summary:
        status = f"{r.get('passed', 0)}/{r.get('total', 0)}" if 'passed' in r else r.get('status', '?')
        print(f"    {r['id']}: {status}")

    if pct >= 80:
        print(f"\n  VERDICT: Skill is effective — scores align with ground truth")
    elif pct >= 50:
        print(f"\n  VERDICT: Skill is partially effective — some calibration needed")
    else:
        print(f"\n  VERDICT: Skill needs work — scores don't match expectations")

    return passed_checks, total_checks


def main():
    import argparse

    parser = argparse.ArgumentParser(description="Score societies using Claude")
    parser.add_argument("--id", help="Score a single society by ID")
    parser.add_argument("--test", action="store_true", help="Run effectiveness test")
    parser.add_argument("--force", action="store_true", help="Re-score even if cached")
    parser.add_argument("--limit", type=int, default=0, help="Limit number of societies to score")
    args = parser.parse_args()

    logging.basicConfig(level=logging.INFO, format="%(message)s")

    # Load .env
    env_path = PROJECT_ROOT / ".env"
    if env_path.exists():
        for line in env_path.read_text().splitlines():
            if "=" in line and not line.startswith("#"):
                k, v = line.split("=", 1)
                os.environ.setdefault(k.strip(), v.strip())

    if args.test:
        run_effectiveness_test()
        return

    skill = ScoreSocietySkill()

    if args.id:
        society_ids = [args.id]
    else:
        # Score all societies from seed data
        seed_path = SEED_DIR / "societies.json"
        societies = json.loads(seed_path.read_text())
        society_ids = [s["id"] for s in societies]

    if args.limit:
        society_ids = society_ids[:args.limit]

    print(f"\n  Scoring {len(society_ids)} societies\n")

    for i, sid in enumerate(society_ids, 1):
        print(f"  [{i}/{len(society_ids)}] {sid}")
        result = skill.run({"society_id": sid, "triggered_by": "manual"}, force=args.force)

        if result.facts:
            scores = {f.key.replace("score_", ""): f.value["data"]
                      for f in result.facts
                      if f.key.startswith("score_") and not f.key.endswith("_reason")}
            overall = next((f.value["data"] for f in result.facts if f.key == "overall_score"), "?")
            print(f"    Overall: {overall} | {scores}")
        else:
            print(f"    No scores produced")

        if result.cached:
            print(f"    (cached)")

    print()


if __name__ == "__main__":
    main()
