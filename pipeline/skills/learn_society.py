"""
learn_society — enrich a society node by synthesizing Reddit threads via Claude.

Input: {"society_name": "Prestige Lakeside Habitat", "area": "Whitefield", "city": "Bengaluru"}
Output: SkillResult with facts about maintenance, family-friendliness, signals, cautions.

Composes: search_reddit → Claude synthesis → structured facts.
"""

import json
import logging
import os
from typing import Optional

from pipeline.skills.base import BaseSkill, SkillResult, SkillCost, SourcedFact, FactSource
from pipeline.skills.search_reddit import SearchRedditSkill

logger = logging.getLogger(__name__)

SYNTHESIS_PROMPT = """You are a real estate researcher analyzing Reddit discussions about an apartment society.

**Society:** {society_name}
**Area:** {area}, {city}

**Reddit threads found ({thread_count} threads):**
{threads_text}

Based on these Reddit discussions, extract structured intelligence about this society.
Return a JSON object with these fields (use null if information is not available):

{{
  "maintenance_quality": "good" | "average" | "poor" | null,
  "family_suitability": "high" | "moderate" | "low" | null,
  "noise_level": "quiet" | "moderate" | "noisy" | null,
  "security_quality": "good" | "average" | "poor" | null,
  "common_positives": ["list of 3-5 positive aspects mentioned"],
  "common_complaints": ["list of 3-5 complaints mentioned"],
  "signals": ["short signal tags like 'good-maintenance', 'family-friendly', 'metro-adjacent'"],
  "cautions": ["short caution tags like 'traffic-congestion', 'water-issues', 'noisy-construction'"],
  "resident_sentiment": "positive" | "mixed" | "negative",
  "sentiment_summary": "1-2 sentence synthesis of overall resident feeling",
  "best_quote": "most insightful verbatim or close-paraphrase quote from discussions"
}}

IMPORTANT:
- Only include information actually supported by the Reddit threads
- If threads don't mention a topic, use null
- Keep signal/caution tags short and reusable (lowercase, hyphenated)
- Be honest about sentiment — don't sugarcoat if residents complain"""


def call_claude(prompt: str, model: str = "claude-sonnet-4-20250514") -> Optional[dict]:
    """Call Claude API and return parsed JSON response."""
    api_key = os.environ.get("ANTHROPIC_API_KEY")
    if not api_key:
        logger.error("ANTHROPIC_API_KEY not set")
        return None

    import urllib.request

    payload = json.dumps({
        "model": model,
        "max_tokens": 1024,
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
        with urllib.request.urlopen(req, timeout=30) as resp:
            data = json.loads(resp.read().decode())
    except Exception as e:
        logger.error("Claude API call failed: %s", e)
        return None

    # Extract text content
    text = ""
    for block in data.get("content", []):
        if block.get("type") == "text":
            text = block["text"]
            break

    # Parse JSON from response (handle markdown code blocks)
    text = text.strip()
    if text.startswith("```"):
        lines = text.split("\n")
        text = "\n".join(lines[1:-1] if lines[-1].strip() == "```" else lines[1:])

    try:
        result = json.loads(text)
        # Return result + token usage
        usage = data.get("usage", {})
        result["_tokens"] = usage.get("input_tokens", 0) + usage.get("output_tokens", 0)
        return result
    except json.JSONDecodeError:
        logger.error("Failed to parse Claude response as JSON: %s", text[:200])
        return None


class LearnSocietySkill(BaseSkill):
    skill_id = "learn_society"
    description = "Enrich a society node by synthesizing Reddit discussions via Claude"
    version = "1.0"

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.reddit_skill = SearchRedditSkill(**kwargs)

    def execute(self, input_data: dict) -> SkillResult:
        society_name = input_data.get("society_name", "")
        area = input_data.get("area", "")
        city = input_data.get("city", "Bengaluru")

        if not society_name or not area:
            logger.error("learn_society requires society_name and area")
            return SkillResult(confidence=0.0)

        # Step 1: Search Reddit
        query = f"{society_name} {area}"
        reddit_result = self.reddit_skill.run({
            "query": query,
            "subreddit": "bangalore",
            "triggered_by": input_data.get("triggered_by"),
        })

        thread_titles = []
        for fact in reddit_result.facts:
            if fact.key == "reddit_threads":
                thread_titles = fact.value.get("data", [])

        thread_count = 0
        for fact in reddit_result.facts:
            if fact.key == "reddit_thread_count":
                thread_count = int(fact.value.get("data", 0))

        if thread_count == 0:
            logger.info("No Reddit threads found for %s", society_name)
            return SkillResult(
                facts=reddit_result.facts,
                confidence=0.2,
                cost=reddit_result.cost,
            )

        # Step 2: Synthesize via Claude
        threads_text = "\n".join(f"- {t}" for t in thread_titles)
        prompt = SYNTHESIS_PROMPT.format(
            society_name=society_name,
            area=area,
            city=city,
            thread_count=thread_count,
            threads_text=threads_text,
        )

        synthesis = call_claude(prompt)
        if not synthesis:
            return SkillResult(
                facts=reddit_result.facts,
                confidence=0.3,
                cost=SkillCost(api_calls=2),
            )

        tokens_used = synthesis.pop("_tokens", 0)

        # Step 3: Convert synthesis into SourcedFacts
        source = FactSource(
            source_type="Llm",
            model="claude-sonnet-4-20250514",
            skill_id=self.skill_id,
            triggered_by=input_data.get("triggered_by"),
        )

        facts = list(reddit_result.facts)  # Include raw Reddit facts

        # Map synthesis fields to self-describing facts.
        # Each fact declares:
        #   - display_template: how to show it to users
        #   - answers_preferences: which user search preferences this fact satisfies
        #   - scoring_hint: how this fact should influence ranking
        #
        # This means new skills/facts require ZERO Rust code changes.
        fact_mappings = [
            {
                "key": "maintenance_quality",
                "type": "Text",
                "template": "Maintenance is {value}",
                "preferences": ["good society", "well maintained", "maintenance"],
                "scoring": {"direction": "TextMatch", "weight": 2.0},
            },
            {
                "key": "family_suitability",
                "type": "Text",
                "template": "Family suitability: {value}",
                "preferences": ["family friendly", "families", "kids"],
                "scoring": {"direction": "TextMatch", "weight": 2.0},
            },
            {
                "key": "noise_level",
                "type": "Text",
                "template": "Noise level: {value}",
                "preferences": ["quiet neighborhood", "quiet", "peaceful", "calm"],
                "scoring": {"direction": "TextMatch", "weight": 2.0},
            },
            {
                "key": "security_quality",
                "type": "Text",
                "template": "Security: {value}",
                "preferences": ["safe", "safety", "secure", "gated community"],
                "scoring": {"direction": "TextMatch", "weight": 2.0},
            },
            {
                "key": "resident_sentiment",
                "type": "Text",
                "template": "Resident sentiment: {value}",
                "preferences": ["good reviews", "resident feedback"],
                "scoring": {"direction": "TextMatch", "weight": 1.5},
            },
            {
                "key": "sentiment_summary",
                "type": "Text",
                "template": "{value}",
                "preferences": [],
                "scoring": None,
            },
            {
                "key": "best_quote",
                "type": "Text",
                "template": 'Resident says: "{value}"',
                "preferences": [],
                "scoring": None,
            },
        ]

        for mapping in fact_mappings:
            val = synthesis.get(mapping["key"])
            if val is not None:
                facts.append(SourcedFact(
                    key=mapping["key"],
                    value={"type": mapping["type"], "data": val},
                    confidence=0.6,
                    source=source,
                    display_template=mapping["template"],
                    answers_preferences=mapping["preferences"] or None,
                    scoring_hint=mapping["scoring"],
                ))

        # Tag-type facts
        tag_mappings = [
            ("common_positives", "Tags", "Positives: {value}"),
            ("common_complaints", "Tags", "Complaints: {value}"),
            ("signals", "Tags", "Signals: {value}"),
            ("cautions", "Tags", "Cautions: {value}"),
        ]

        for key, value_type, template in tag_mappings:
            val = synthesis.get(key)
            if val and isinstance(val, list):
                facts.append(SourcedFact(
                    key=key,
                    value={"type": value_type, "data": val},
                    confidence=0.6,
                    source=source,
                    display_template=template,
                ))

        return SkillResult(
            facts=facts,
            confidence=min(0.7, thread_count / 10),
            cost=SkillCost(
                llm_tokens=tokens_used,
                api_calls=2,  # 1 Reddit + 1 Claude
                estimated_usd=tokens_used * 0.000003,  # rough Sonnet pricing
            ),
        )

    def estimated_cost(self) -> SkillCost:
        return SkillCost(llm_tokens=2000, api_calls=2, estimated_usd=0.02)


if __name__ == "__main__":
    """Quick test: python3 -m pipeline.skills.learn_society"""
    import sys
    logging.basicConfig(level=logging.INFO)

    skill = LearnSocietySkill()
    result = skill.run({
        "society_name": "Prestige Lakeside Habitat",
        "area": "Whitefield",
        "city": "Bengaluru",
        "triggered_by": "manual_test",
    })

    print(f"\n=== Results: {len(result.facts)} facts, confidence={result.confidence:.2f} ===")
    print(f"Cost: {result.cost.llm_tokens} tokens, ${result.cost.estimated_usd:.4f}")
    for fact in result.facts:
        print(f"  {fact.key}: {fact.value} (conf={fact.confidence}, src={fact.source.source_type})")
