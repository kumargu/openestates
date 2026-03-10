"""
learn_area — enrich an area node using Gemini Flash with Google Search grounding.

Input: {"area_name": "Whitefield", "city": "Bengaluru"}
Output: SkillResult with facts about metro, traffic, schools, waterlogging, price trends.

Uses Gemini 2.0 Flash via REST with google_search grounding for real-time data.
"""

import json
import logging
import os
from typing import Optional

from pipeline.skills.base import BaseSkill, SkillResult, SkillCost, SourcedFact, FactSource

logger = logging.getLogger(__name__)

AREA_PROMPT = """Research {area_name}, {city} as a residential area. Return JSON:

{{
  "metro_status": "operational | under_construction | planned | none",
  "metro_details": "nearest station and distance",
  "traffic_reality": "1-2 sentences about daily commute",
  "waterlogging_risk": "low | moderate | high",
  "waterlogging_detail": "specific incidents if any",
  "school_quality": ["top 3 schools within 5km"],
  "upcoming_infra": "major infrastructure projects",
  "price_trend": "appreciating | stable | declining",
  "livability_summary": "2-3 sentence area summary",
  "vibe": "short description of area personality"
}}

IMPORTANT:
- Return ONLY the JSON object, no markdown or explanation
- Use null if information is unavailable
- Be specific and factual, grounded in real data"""


def call_gemini(prompt: str) -> Optional[dict]:
    """Call Gemini 2.0 Flash API with Google Search grounding and return parsed JSON."""
    api_key = os.environ.get("GOOGLE_AI_API_KEY")
    if not api_key:
        logger.error("GOOGLE_AI_API_KEY not set")
        return None

    import urllib.request

    payload = json.dumps({
        "contents": [{"parts": [{"text": prompt}]}],
        "tools": [{"google_search": {}}],
        "generationConfig": {
            "temperature": 0.2,
            "maxOutputTokens": 4096,
        },
    }).encode()

    url = f"https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={api_key}"

    req = urllib.request.Request(
        url,
        data=payload,
        headers={"Content-Type": "application/json"},
    )

    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            data = json.loads(resp.read().decode())
    except Exception as e:
        logger.error("Gemini API call failed: %s", e)
        return None

    # Extract text from response — concatenate all text parts
    # (Gemini 2.5 with grounding may return thinking + answer in separate parts)
    text_parts = []
    try:
        candidates = data.get("candidates", [])
        if candidates:
            parts = candidates[0].get("content", {}).get("parts", [])
            for part in parts:
                if "text" in part:
                    text_parts.append(part["text"])
    except (KeyError, IndexError) as e:
        logger.error("Failed to extract text from Gemini response: %s", e)
        return None

    if not text_parts:
        logger.error("Empty text in Gemini response")
        return None

    # Try each text part for valid JSON (the answer is usually the last part)
    usage = data.get("usageMetadata", {})
    tokens = usage.get("totalTokenCount", 0)

    for text in reversed(text_parts):
        text = text.strip()
        # Strip markdown code blocks
        if text.startswith("```"):
            lines = text.split("\n")
            text = "\n".join(lines[1:-1] if lines[-1].strip() == "```" else lines[1:])

        # Try to find JSON object in the text
        start = text.find("{")
        end = text.rfind("}")
        if start != -1 and end != -1 and end > start:
            try:
                result = json.loads(text[start:end + 1])
                result["_tokens"] = tokens
                return result
            except json.JSONDecodeError:
                continue

    logger.error("No valid JSON found in Gemini response parts (%d parts)", len(text_parts))
    return None


class LearnAreaSkill(BaseSkill):
    skill_id = "learn_area"
    description = "Enrich an area node using Gemini Flash with Google Search grounding"
    version = "1.0"

    def execute(self, input_data: dict) -> SkillResult:
        area_name = input_data.get("area_name", "")
        city = input_data.get("city", "Bengaluru")

        if not area_name:
            logger.error("learn_area requires area_name")
            return SkillResult(confidence=0.0)

        # Call Gemini with grounding
        prompt = AREA_PROMPT.format(area_name=area_name, city=city)
        synthesis = call_gemini(prompt)

        if not synthesis:
            return SkillResult(
                confidence=0.0,
                cost=SkillCost(api_calls=1),
            )

        tokens_used = synthesis.pop("_tokens", 0)

        # Convert synthesis into self-describing SourcedFacts
        source = FactSource(
            source_type="Google",
            model="gemini-2.5-flash",
            skill_id=self.skill_id,
            triggered_by=input_data.get("triggered_by"),
        )

        facts = []

        # Map synthesis fields to self-describing facts.
        # Each fact declares:
        #   - display_template: how to show it to users
        #   - answers_preferences: which user search preferences this fact satisfies
        #   - scoring_hint: how this fact should influence ranking
        #
        # This means new skills/facts require ZERO Rust code changes.
        fact_mappings = [
            {
                "key": "metro_status",
                "type": "Text",
                "template": "Metro: {value}",
                "preferences": ["metro access", "metro", "near metro"],
                "scoring": {"direction": "TextMatch", "weight": 2.0},
            },
            {
                "key": "metro_details",
                "type": "Text",
                "template": "{value}",
                "preferences": ["metro access"],
                "scoring": None,
            },
            {
                "key": "traffic_reality",
                "type": "Text",
                "template": "Traffic: {value}",
                "preferences": ["easy commute", "traffic"],
                "scoring": None,
            },
            {
                "key": "waterlogging_risk",
                "type": "Text",
                "template": "Waterlogging risk: {value}",
                "preferences": ["safe from flooding", "no waterlogging"],
                "scoring": {"direction": "TextMatch", "weight": 2.0},
            },
            {
                "key": "upcoming_infra",
                "type": "Text",
                "template": "Upcoming: {value}",
                "preferences": ["growing area", "appreciation"],
                "scoring": None,
            },
            {
                "key": "price_trend",
                "type": "Text",
                "template": "Price trend: {value}",
                "preferences": ["good investment", "appreciation"],
                "scoring": {"direction": "TextMatch", "weight": 1.5},
            },
            {
                "key": "livability_summary",
                "type": "Text",
                "template": "{value}",
                "preferences": [],
                "scoring": None,
            },
            {
                "key": "vibe",
                "fact_key": "area_vibe",
                "type": "Text",
                "template": "Vibe: {value}",
                "preferences": [],
                "scoring": None,
            },
        ]

        for mapping in fact_mappings:
            val = synthesis.get(mapping["key"])
            if val is not None:
                fact_key = mapping.get("fact_key", mapping["key"])
                facts.append(SourcedFact(
                    key=fact_key,
                    value={"type": mapping["type"], "data": val},
                    confidence=0.7,
                    source=source,
                    display_template=mapping["template"],
                    answers_preferences=mapping["preferences"] or None,
                    scoring_hint=mapping["scoring"],
                ))

        # Tag-type facts
        tag_mappings = [
            {
                "key": "school_quality",
                "type": "Tags",
                "template": "Top schools: {value}",
                "preferences": ["good schools", "schools nearby", "kids"],
                "scoring": None,
            },
        ]

        for mapping in tag_mappings:
            val = synthesis.get(mapping["key"])
            if val and isinstance(val, list):
                facts.append(SourcedFact(
                    key=mapping["key"],
                    value={"type": mapping["type"], "data": val},
                    confidence=0.7,
                    source=source,
                    display_template=mapping["template"],
                    answers_preferences=mapping["preferences"] or None,
                    scoring_hint=mapping["scoring"],
                ))

        # Also include waterlogging_detail as a supplementary text fact
        wl_detail = synthesis.get("waterlogging_detail")
        if wl_detail:
            facts.append(SourcedFact(
                key="waterlogging_detail",
                value={"type": "Text", "data": wl_detail},
                confidence=0.7,
                source=source,
                display_template="{value}",
            ))

        return SkillResult(
            facts=facts,
            confidence=0.7,
            cost=SkillCost(
                llm_tokens=tokens_used,
                api_calls=1,
                estimated_usd=tokens_used * 0.0000001,  # Gemini Flash pricing (~$0.10/1M tokens)
            ),
        )

    def estimated_cost(self) -> SkillCost:
        return SkillCost(llm_tokens=1500, api_calls=1, estimated_usd=0.001)


if __name__ == "__main__":
    """Quick test: python3 -m pipeline.skills.learn_area"""
    import sys
    logging.basicConfig(level=logging.INFO)

    skill = LearnAreaSkill()
    result = skill.run({
        "area_name": "Whitefield",
        "city": "Bengaluru",
        "triggered_by": "manual_test",
    })

    print(f"\n=== Results: {len(result.facts)} facts, confidence={result.confidence:.2f} ===")
    print(f"Cost: {result.cost.llm_tokens} tokens, ${result.cost.estimated_usd:.4f}")
    for fact in result.facts:
        print(f"  {fact.key}: {fact.value} (conf={fact.confidence}, src={fact.source.source_type})")
