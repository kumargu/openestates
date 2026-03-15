"""
learn_society — enrich a society node by synthesizing Reddit threads via LLM.

Input: {"society_name": "Prestige Lakeside Habitat", "area": "Whitefield", "city": "Bengaluru"}
Output: SkillResult with facts about maintenance, family-friendliness, signals, cautions.

Primary: search_reddit → Gemini 2.5 Flash synthesis → structured facts.
Fallback (no Reddit): Gemini with Google Search grounding.
Last resort: Claude Sonnet.
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


def _call_llm(prompt: str) -> Optional[dict]:
    """Call LLM API and return parsed JSON. Tries Gemini first, falls back to Claude."""
    import urllib.request

    # Try Gemini Flash first (cheaper, more reliable)
    gemini_key = os.environ.get("GOOGLE_AI_API_KEY")
    if gemini_key:
        result = _call_gemini_synthesis(prompt, gemini_key)
        if result is not None:
            return result
        logger.warning("Gemini synthesis failed, trying Claude fallback")

    # Fall back to Claude
    claude_key = os.environ.get("ANTHROPIC_API_KEY")
    if claude_key:
        return _call_claude_api(prompt, claude_key)

    logger.error("No LLM API key set (GOOGLE_AI_API_KEY or ANTHROPIC_API_KEY)")
    return None


def _call_gemini_synthesis(prompt: str, api_key: str) -> Optional[dict]:
    """Call Gemini 2.5 Flash for Reddit synthesis (no grounding needed)."""
    import urllib.request

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
        with urllib.request.urlopen(req, timeout=45) as resp:
            data = json.loads(resp.read().decode())
    except Exception as e:
        logger.error("Gemini synthesis API call failed: %s", e)
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
        logger.warning("Gemini JSON parse error: %s — attempting repair", e)
        repaired = _repair_json(text)
        if repaired:
            repaired["_tokens"] = tokens
            repaired["_model"] = "gemini-2.5-flash"
            return repaired
        logger.error("Could not repair JSON. First 300 chars: %s", text[:300])
        return None


def _repair_json(text: str) -> Optional[dict]:
    """Attempt to repair truncated or malformed JSON from LLM output."""
    import re
    text = re.sub(r'[\x00-\x1f\x7f]', ' ', text)
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        pass

    # Close any unclosed braces/brackets
    text = re.sub(r',\s*"[^"]*$', '', text)
    text = re.sub(r',\s*$', '', text)
    text = re.sub(r':\s*"[^"]*$', ': ""', text)

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
        return None


def _call_claude_api(prompt: str, api_key: str) -> Optional[dict]:
    """Call Claude API as fallback."""
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


GEMINI_SOCIETY_PROMPT = """Research the residential apartment society "{society_name}" in {area}, {city}, India.

Find real resident reviews, Google reviews, forum discussions, and any news about this society.

Return ONLY a JSON object:
{{
  "maintenance_quality": "good" | "average" | "poor" | null,
  "family_suitability": "high" | "moderate" | "low" | null,
  "noise_level": "quiet" | "moderate" | "noisy" | null,
  "security_quality": "good" | "average" | "poor" | null,
  "common_positives": ["list of 3-5 positive aspects"],
  "common_complaints": ["list of 3-5 complaints"],
  "signals": ["short signal tags like 'good-maintenance', 'family-friendly'"],
  "cautions": ["short caution tags like 'traffic-congestion', 'water-issues'"],
  "resident_sentiment": "positive" | "mixed" | "negative",
  "sentiment_summary": "1-2 sentence synthesis of overall resident feeling",
  "best_quote": "most insightful quote or paraphrase from reviews"
}}

Use null for fields where no reliable information is found. Be honest — do not fabricate."""


def _call_gemini_grounded(prompt: str) -> Optional[dict]:
    """Call Gemini 2.5 Flash with Google Search grounding."""
    api_key = os.environ.get("GOOGLE_AI_API_KEY")
    if not api_key:
        logger.error("GOOGLE_AI_API_KEY not set")
        return None

    import urllib.request

    payload = json.dumps({
        "contents": [{"parts": [{"text": prompt}]}],
        "tools": [{"google_search": {}}],
        "generationConfig": {"temperature": 0.2, "maxOutputTokens": 4000},
    }).encode()

    url = f"https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={api_key}"
    req = urllib.request.Request(url, data=payload, headers={"Content-Type": "application/json"})

    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            data = json.loads(resp.read().decode())
    except Exception as e:
        logger.error("Gemini API call failed: %s", e)
        return None

    text = ""
    for candidate in data.get("candidates", []):
        for part in candidate.get("content", {}).get("parts", []):
            if "text" in part:
                text = part["text"]
                break

    text = text.strip()
    if text.startswith("```"):
        lines = text.split("\n")
        text = "\n".join(lines[1:-1] if lines[-1].strip() == "```" else lines[1:])

    try:
        result = json.loads(text)
        tokens = data.get("usageMetadata", {}).get("totalTokenCount", 0)
        result["_tokens"] = tokens
        return result
    except json.JSONDecodeError:
        # Try to repair truncated JSON by closing open brackets/braces
        repaired = text.rstrip()
        # Close any open strings
        if repaired.count('"') % 2 == 1:
            repaired += '"'
        # Close arrays and objects
        open_brackets = repaired.count('[') - repaired.count(']')
        open_braces = repaired.count('{') - repaired.count('}')
        repaired += ']' * max(0, open_brackets)
        repaired += '}' * max(0, open_braces)
        try:
            result = json.loads(repaired)
            tokens = data.get("usageMetadata", {}).get("totalTokenCount", 0)
            result["_tokens"] = tokens
            logger.info("Repaired truncated Gemini JSON response")
            return result
        except json.JSONDecodeError:
            logger.error("Failed to parse Gemini response as JSON: %s", text[:200])
            return None


class LearnSocietySkill(BaseSkill):
    skill_id = "learn_society"
    description = "Enrich a society node by synthesizing Reddit discussions via Claude, with Gemini fallback"
    version = "2.0"
    output_keys = [
        "maintenance_quality", "family_suitability", "noise_level", "security_quality",
        "resident_sentiment", "sentiment_summary", "best_quote",
        "common_positives", "common_complaints", "signals", "cautions",
    ]

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
            logger.info("No Reddit threads for %s, falling back to Gemini grounded search", society_name)
            return self._gemini_fallback(society_name, area, city, input_data)

        # Step 2: Synthesize via LLM (Gemini primary, Claude fallback)
        threads_text = "\n".join(f"- {t}" for t in thread_titles)
        prompt = SYNTHESIS_PROMPT.format(
            society_name=society_name,
            area=area,
            city=city,
            thread_count=thread_count,
            threads_text=threads_text,
        )

        synthesis = _call_llm(prompt)
        if not synthesis:
            logger.warning("LLM synthesis failed for %s — returning Reddit facts only", society_name)
            return SkillResult(
                facts=reddit_result.facts,
                confidence=0.3,
                cost=SkillCost(api_calls=2),
            )

        tokens_used = synthesis.pop("_tokens", 0)
        model_used = synthesis.pop("_model", "unknown")

        # Step 3: Convert synthesis into SourcedFacts
        source = FactSource(
            source_type="Llm",
            model=model_used,
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

    def _gemini_fallback(self, society_name: str, area: str, city: str, input_data: dict) -> SkillResult:
        """Fallback: use Gemini with Google Search grounding when Reddit is unavailable."""
        prompt = GEMINI_SOCIETY_PROMPT.format(society_name=society_name, area=area, city=city)
        synthesis = _call_gemini_grounded(prompt)

        model_used = "gemini-2.5-flash"
        source_type = "Google"

        if not synthesis:
            # Try Claude as last resort
            claude_key = os.environ.get("ANTHROPIC_API_KEY")
            if claude_key:
                logger.info("Gemini grounded search failed for %s, trying Claude", society_name)
                synthesis = _call_claude_api(prompt, claude_key)
                model_used = "claude-sonnet-4-20250514"
                source_type = "Llm"
            if not synthesis:
                logger.warning("All LLM providers failed for %s", society_name)
                return SkillResult(confidence=0.0, cost=SkillCost(api_calls=1))

        tokens_used = synthesis.pop("_tokens", 0)
        # Use _model from response if present (overrides default)
        model_used = synthesis.pop("_model", model_used)

        source = FactSource(
            source_type=source_type,
            model=model_used,
            skill_id=self.skill_id,
            triggered_by=input_data.get("triggered_by"),
        )

        facts = []
        # Same fact mappings as the Reddit path, but lower confidence (0.5 vs 0.6)
        fact_mappings = [
            ("maintenance_quality", "Text", "Maintenance is {value}", ["good society", "well maintained", "maintenance"], {"direction": "TextMatch", "weight": 2.0}),
            ("family_suitability", "Text", "Family suitability: {value}", ["family friendly", "families", "kids"], {"direction": "TextMatch", "weight": 2.0}),
            ("noise_level", "Text", "Noise level: {value}", ["quiet neighborhood", "quiet", "peaceful", "calm"], {"direction": "TextMatch", "weight": 2.0}),
            ("security_quality", "Text", "Security: {value}", ["safe", "safety", "secure", "gated community"], {"direction": "TextMatch", "weight": 2.0}),
            ("resident_sentiment", "Text", "Resident sentiment: {value}", ["good reviews", "resident feedback"], {"direction": "TextMatch", "weight": 1.5}),
            ("sentiment_summary", "Text", "{value}", [], None),
            ("best_quote", "Text", 'Resident says: "{value}"', [], None),
        ]

        for key, vtype, template, prefs, scoring in fact_mappings:
            val = synthesis.get(key)
            if val is not None:
                facts.append(SourcedFact(
                    key=key,
                    value={"type": vtype, "data": val},
                    confidence=0.5,
                    source=source,
                    display_template=template,
                    answers_preferences=prefs or None,
                    scoring_hint=scoring,
                ))

        for key, vtype, template in [
            ("common_positives", "Tags", "Positives: {value}"),
            ("common_complaints", "Tags", "Complaints: {value}"),
            ("signals", "Tags", "Signals: {value}"),
            ("cautions", "Tags", "Cautions: {value}"),
        ]:
            val = synthesis.get(key)
            if val and isinstance(val, list):
                facts.append(SourcedFact(
                    key=key,
                    value={"type": vtype, "data": val},
                    confidence=0.5,
                    source=source,
                    display_template=template,
                ))

        return SkillResult(
            facts=facts,
            confidence=0.5,
            cost=SkillCost(llm_tokens=tokens_used, api_calls=1, estimated_usd=tokens_used * 0.0000001),
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
