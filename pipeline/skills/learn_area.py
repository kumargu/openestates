"""
learn_area — enrich an area node using Reddit resident intelligence + Gemini grounding.

Input: {"area_name": "Whitefield", "city": "Bengaluru"}
Output: SkillResult with facts about living experience sourced from r/bangalore discussions.

Primary source: Reddit threads from r/bangalore (resident opinions, real experiences).
Fallback: Gemini Flash with Google Search grounding (for areas with no Reddit coverage).

Flow:
1. Fetch Reddit threads via multiple area-focused queries (deduplicated)
2. Fetch top comments from highest-engagement threads
3. Synthesize via Gemini (primary) or Claude (fallback) into structured area intelligence
4. Convert synthesis into self-describing SourcedFacts
"""

import json
import logging
import os
import time
from typing import List, Optional
from urllib.request import Request, urlopen

from pipeline.skills.base import BaseSkill, SkillResult, SkillCost, SourcedFact, FactSource
from pipeline.skills.search_reddit import fetch_reddit_threads

logger = logging.getLogger(__name__)

# Maximum chars of thread text to send to LLM (avoid token waste)
MAX_THREAD_TEXT_CHARS = 8000


# ---------------------------------------------------------------------------
# Reddit fetching helpers
# ---------------------------------------------------------------------------

def fetch_area_reddit_threads(area_name: str, city: str = "Bangalore") -> List[dict]:
    """Fetch Reddit threads about an area from r/bangalore.

    Uses multiple search queries to get comprehensive coverage.
    Deduplicates by URL. Returns combined threads sorted by engagement.
    """
    all_threads: List[dict] = []
    seen_urls: set = set()

    queries = [
        f"{area_name} living",
        f"{area_name} review",
        f"{area_name} pros cons",
        f"{area_name} safety",
        f"{area_name} traffic",
        f"{area_name} water",
        f"{area_name} schools",
        f"{area_name} rent buy",
    ]

    for query in queries:
        threads = fetch_reddit_threads(query, subreddit="bangalore", limit=10)
        for t in threads:
            if t["url"] not in seen_urls:
                seen_urls.add(t["url"])
                all_threads.append(t)
        time.sleep(1)  # Rate limit Reddit API

    # Sort by engagement (score * comment count)
    all_threads.sort(
        key=lambda t: t.get("score", 0) * max(t.get("num_comments", 1), 1),
        reverse=True,
    )
    return all_threads


def fetch_thread_comments(thread_url: str, limit: int = 50) -> List[str]:
    """Fetch top comments from a Reddit thread.

    Uses Reddit's JSON API: append .json to the thread URL.
    Returns list of comment texts (top-level only, sorted by score).
    """
    # Convert permalink URL to JSON API URL
    json_url = thread_url.rstrip("/") + ".json"
    req = Request(json_url, headers={"User-Agent": "OpenEstates/1.0"})

    try:
        with urlopen(req, timeout=10) as resp:
            data = json.loads(resp.read().decode())

        comments = []
        if len(data) > 1:
            children = data[1].get("data", {}).get("children", [])
            for child in children[:limit]:
                body = child.get("data", {}).get("body", "")
                if body and len(body) > 20:  # Skip very short comments
                    comments.append(body[:500])  # Cap individual comment length
        return comments
    except Exception as e:
        logger.warning("Failed to fetch comments from %s: %s", thread_url, e)
        return []


def build_thread_text(threads: List[dict], comments_by_url: dict) -> str:
    """Build a combined text block from threads and their comments.

    Caps total output at MAX_THREAD_TEXT_CHARS to avoid token waste.
    """
    parts = []
    char_count = 0

    for t in threads:
        title = t.get("title", "")
        selftext = t.get("selftext", "")
        score = t.get("score", 0)
        num_comments = t.get("num_comments", 0)

        section = f"### Thread (score={score}, comments={num_comments}): {title}\n"
        if selftext:
            section += f"{selftext}\n"

        # Add comments if available
        url = t.get("url", "")
        thread_comments = comments_by_url.get(url, [])
        if thread_comments:
            section += "Top comments:\n"
            for c in thread_comments[:5]:
                section += f"  - {c}\n"

        section += "\n"

        if char_count + len(section) > MAX_THREAD_TEXT_CHARS:
            # Add as much as we can fit
            remaining = MAX_THREAD_TEXT_CHARS - char_count
            if remaining > 100:
                parts.append(section[:remaining] + "...\n")
            break

        parts.append(section)
        char_count += len(section)

    return "".join(parts)


# ---------------------------------------------------------------------------
# LLM synthesis
# ---------------------------------------------------------------------------

AREA_SYNTHESIS_PROMPT = """You are an area intelligence analyst for Bangalore real estate.
Analyze these Reddit discussions about {area_name} and extract structured intelligence.

Reddit threads and comments:
{thread_text}

Based on ONLY what residents actually discuss in these threads, produce a JSON object with these fields.
Use null for any field where you don't have enough evidence. Be specific — quote or paraphrase actual resident experiences.

{{
  "safety_perception": "one sentence summary of safety based on resident experiences",
  "commute_reality": "actual commute times and modes mentioned by residents",
  "water_supply": "water supply reliability based on resident reports",
  "power_supply": "power supply reliability",
  "noise_level": "noise situation based on resident reports",
  "green_cover": "parks, lakes, trees mentioned by residents",
  "walkability": "how walkable is the area according to residents",
  "typical_residents": "who typically lives here (families, young professionals, etc.)",
  "community_vibe": "the social/community feel of the area",
  "pet_friendliness": "how pet-friendly is the area",
  "grocery_shopping": "shopping options mentioned by residents",
  "healthcare_access": "hospitals/clinics mentioned",
  "school_quality": "specific schools mentioned and their reputation",
  "restaurant_scene": "food/dining options mentioned",
  "commute_to_cbd": "commute time to city center mentioned by residents",
  "recurring_complaints": ["top 5 complaints residents mention repeatedly"],
  "deal_breakers": ["things that make people want to leave"],
  "hidden_gems": ["things residents love that aren't obvious to outsiders"],
  "overall_sentiment": "positive or mixed or negative",
  "confidence_note": "how confident are you in this analysis given the data"
}}

IMPORTANT: Only include information actually supported by the Reddit discussions. Do not add general knowledge that isn't in the threads. If threads don't discuss a topic, use null.
Return ONLY the JSON object, no markdown or explanation."""

# Gemini-only fallback prompt (no Reddit data)
GEMINI_AREA_PROMPT = """Research {area_name}, {city} as a residential area. Return JSON:

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


def _call_gemini(prompt: str) -> Optional[dict]:
    """Call Gemini 2.5 Flash API with Google Search grounding and return parsed JSON."""
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

    usage = data.get("usageMetadata", {})
    tokens = usage.get("totalTokenCount", 0)

    # Try each text part for valid JSON (the answer is usually the last part)
    for text in reversed(text_parts):
        text = text.strip()
        if text.startswith("```"):
            lines = text.split("\n")
            text = "\n".join(lines[1:-1] if lines[-1].strip() == "```" else lines[1:])

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


def _call_claude(prompt: str, model: str = "claude-sonnet-4-20250514") -> Optional[dict]:
    """Call Claude API and return parsed JSON response. Fallback for Gemini."""
    api_key = os.environ.get("ANTHROPIC_API_KEY")
    if not api_key:
        logger.error("ANTHROPIC_API_KEY not set")
        return None

    import urllib.request

    payload = json.dumps({
        "model": model,
        "max_tokens": 2000,
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

    text = ""
    for block in data.get("content", []):
        if block.get("type") == "text":
            text = block["text"]
            break

    text = text.strip()
    if text.startswith("```"):
        lines = text.split("\n")
        text = "\n".join(lines[1:-1] if lines[-1].strip() == "```" else lines[1:])

    start = text.find("{")
    end = text.rfind("}")
    if start != -1 and end != -1 and end > start:
        try:
            result = json.loads(text[start:end + 1])
            usage = data.get("usage", {})
            result["_tokens"] = usage.get("input_tokens", 0) + usage.get("output_tokens", 0)
            return result
        except json.JSONDecodeError:
            pass

    logger.error("Failed to parse Claude response as JSON: %s", text[:200])
    return None


def call_llm_synthesis(prompt: str) -> tuple[Optional[dict], str]:
    """Call Gemini (primary) or Claude (fallback) for synthesis.

    Returns (result_dict, model_name).
    """
    # Try Gemini first (cheaper, faster)
    if os.environ.get("GOOGLE_AI_API_KEY"):
        result = _call_gemini(prompt)
        if result:
            return result, "gemini-2.5-flash"

    # Fallback to Claude
    if os.environ.get("ANTHROPIC_API_KEY"):
        result = _call_claude(prompt)
        if result:
            return result, "claude-sonnet-4-20250514"

    logger.error("No LLM API key available (need GOOGLE_AI_API_KEY or ANTHROPIC_API_KEY)")
    return None, ""


# ---------------------------------------------------------------------------
# Fact conversion
# ---------------------------------------------------------------------------

def reddit_synthesis_to_facts(
    area_name: str,
    synthesis: dict,
    thread_count: int,
    thread_urls: List[str],
    model_name: str,
) -> List[SourcedFact]:
    """Convert LLM area synthesis (from Reddit data) into self-describing SourcedFacts."""
    source = FactSource(
        source_type="Reddit",
        url=thread_urls[0] if thread_urls else None,
        model=model_name,
        skill_id="learn_area",
        triggered_by="area_enrichment",
    )

    facts: List[SourcedFact] = []

    # Map synthesis fields to self-describing facts.
    # Each fact declares how to display, which preferences it answers, and how it affects ranking.
    # Adding new dimensions requires ZERO Rust code changes.
    field_configs = {
        "safety_perception": {
            "display": "Safety: {value}",
            "prefs": ["safe", "safety", "crime", "secure"],
            "hint": {"direction": "TextMatch", "weight": 2.0},
        },
        "commute_reality": {
            "display": "Commute: {value}",
            "prefs": ["commute", "traffic", "office", "travel time"],
            "hint": {"direction": "TextMatch", "weight": 2.0},
        },
        "water_supply": {
            "display": "Water Supply: {value}",
            "prefs": ["water", "water supply", "cauvery", "borewell"],
            "hint": None,
        },
        "power_supply": {
            "display": "Power Supply: {value}",
            "prefs": ["power", "electricity", "power cuts"],
            "hint": None,
        },
        "noise_level": {
            "display": "Noise: {value}",
            "prefs": ["quiet", "peaceful", "noise", "calm"],
            "hint": {"direction": "TextMatch", "weight": 1.5},
        },
        "green_cover": {
            "display": "Green Cover: {value}",
            "prefs": ["green", "parks", "nature", "trees", "lake"],
            "hint": None,
        },
        "walkability": {
            "display": "Walkability: {value}",
            "prefs": ["walkable", "walking", "pedestrian"],
            "hint": None,
        },
        "typical_residents": {
            "display": "Typical Residents: {value}",
            "prefs": ["family friendly", "young professionals"],
            "hint": None,
        },
        "community_vibe": {
            "display": "Community: {value}",
            "prefs": ["community", "neighbors", "social", "family friendly"],
            "hint": None,
        },
        "pet_friendliness": {
            "display": "Pet-Friendly: {value}",
            "prefs": ["pets", "dogs", "pet friendly"],
            "hint": None,
        },
        "grocery_shopping": {
            "display": "Shopping: {value}",
            "prefs": ["shopping", "grocery", "supermarket"],
            "hint": None,
        },
        "healthcare_access": {
            "display": "Healthcare: {value}",
            "prefs": ["hospital", "healthcare", "medical", "clinic"],
            "hint": None,
        },
        "school_quality": {
            "display": "Schools: {value}",
            "prefs": ["schools", "education", "kids", "children"],
            "hint": {"direction": "TextMatch", "weight": 2.0},
        },
        "restaurant_scene": {
            "display": "Dining: {value}",
            "prefs": ["restaurants", "food", "dining", "cafes"],
            "hint": None,
        },
        "commute_to_cbd": {
            "display": "CBD Commute: {value}",
            "prefs": ["commute", "city center", "majestic", "mg road"],
            "hint": None,
        },
        "overall_sentiment": {
            "display": "Resident Sentiment: {value}",
            "prefs": ["good area", "livable"],
            "hint": {"direction": "TextMatch", "weight": 1.5},
        },
        "confidence_note": {
            "display": "Analysis Note: {value}",
            "prefs": None,
            "hint": None,
        },
    }

    for field_key, config in field_configs.items():
        value = synthesis.get(field_key)
        if value is not None and value != "null" and str(value).strip():
            facts.append(SourcedFact(
                key=field_key,
                value={"type": "Text", "data": str(value)},
                confidence=0.7,  # Reddit-sourced, LLM-synthesized
                source=source,
                display_template=config["display"],
                answers_preferences=config.get("prefs"),
                scoring_hint=config.get("hint"),
            ))

    # Tags-type facts
    for tag_field in ["recurring_complaints", "deal_breakers", "hidden_gems"]:
        tags = synthesis.get(tag_field, [])
        if tags and isinstance(tags, list):
            display_map = {
                "recurring_complaints": "Common complaints: {value}",
                "deal_breakers": "Deal breakers: {value}",
                "hidden_gems": "Hidden gems: {value}",
            }
            facts.append(SourcedFact(
                key=tag_field,
                value={"type": "Tags", "data": tags},
                confidence=0.7,
                source=source,
                display_template=display_map.get(tag_field),
                answers_preferences=None,
            ))

    # Thread count metadata
    facts.append(SourcedFact(
        key="reddit_area_thread_count",
        value={"type": "Numeric", "data": thread_count},
        confidence=1.0,
        source=FactSource(source_type="Reddit", skill_id="learn_area"),
        display_template="Based on {value} Reddit discussions",
    ))

    return facts


def gemini_fallback_to_facts(synthesis: dict, area_name: str, city: str) -> List[SourcedFact]:
    """Convert Gemini-only fallback synthesis into SourcedFacts.

    Used when Reddit has no threads for this area.
    """
    source = FactSource(
        source_type="Google",
        model="gemini-2.5-flash",
        skill_id="learn_area",
    )

    facts = []

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
    school_val = synthesis.get("school_quality")
    if school_val and isinstance(school_val, list):
        facts.append(SourcedFact(
            key="school_quality",
            value={"type": "Tags", "data": school_val},
            confidence=0.7,
            source=source,
            display_template="Top schools: {value}",
            answers_preferences=["good schools", "schools nearby", "kids"],
        ))

    # Waterlogging detail
    wl_detail = synthesis.get("waterlogging_detail")
    if wl_detail:
        facts.append(SourcedFact(
            key="waterlogging_detail",
            value={"type": "Text", "data": wl_detail},
            confidence=0.7,
            source=source,
            display_template="{value}",
        ))

    return facts


# ---------------------------------------------------------------------------
# Skill class
# ---------------------------------------------------------------------------

class LearnAreaSkill(BaseSkill):
    skill_id = "learn_area"
    description = "Enrich an area node using Reddit resident intelligence + Gemini grounding"
    version = "2.0"  # Bumped: Reddit-first approach
    output_keys = [
        "metro_status", "metro_details", "traffic_reality", "waterlogging_risk",
        "upcoming_infra", "price_trend", "livability_summary", "area_vibe",
        "school_quality", "waterlogging_detail",
        "reddit_area_thread_count", "reddit_area_threads", "reddit_area_total_score",
    ]

    def execute(self, input_data: dict) -> SkillResult:
        area_name = input_data.get("area_name", "")
        city = input_data.get("city", "Bengaluru")

        if not area_name:
            logger.error("learn_area requires area_name")
            return SkillResult(confidence=0.0)

        # Step 1: Fetch Reddit threads about this area
        logger.info("Fetching Reddit threads for area: %s", area_name)
        threads = fetch_area_reddit_threads(area_name, city)
        logger.info("Found %d unique Reddit threads for %s", len(threads), area_name)

        # If no Reddit threads, fall back to Gemini-only approach
        if not threads:
            logger.info("No Reddit threads found for %s, falling back to Gemini-only", area_name)
            return self._gemini_fallback(area_name, city, input_data)

        # Step 2: Fetch comments from top threads (up to 5)
        comments_by_url: dict = {}
        top_threads = threads[:5]
        total_selftext_len = sum(len(t.get("selftext", "")) for t in threads)

        # Only fetch comments if we don't have enough selftext content
        if total_selftext_len < 2000:
            logger.info("Fetching comments from top %d threads", len(top_threads))
            for t in top_threads:
                url = t.get("url", "")
                if url:
                    comments = fetch_thread_comments(url, limit=20)
                    if comments:
                        comments_by_url[url] = comments
                    time.sleep(1)  # Rate limit Reddit API
            logger.info(
                "Fetched comments from %d threads (%d total comments)",
                len(comments_by_url),
                sum(len(c) for c in comments_by_url.values()),
            )

        # Step 3: Build combined text block for LLM
        thread_text = build_thread_text(threads, comments_by_url)

        if not thread_text.strip():
            logger.warning("No usable thread text for %s, falling back to Gemini-only", area_name)
            return self._gemini_fallback(area_name, city, input_data)

        # Step 4: Synthesize via LLM
        prompt = AREA_SYNTHESIS_PROMPT.format(
            area_name=area_name,
            thread_text=thread_text,
        )

        synthesis, model_name = call_llm_synthesis(prompt)

        thread_urls = [t["url"] for t in threads if t.get("url")]
        api_calls = 1 + len(comments_by_url)  # Reddit searches + comment fetches

        if not synthesis:
            # LLM failed — return raw Reddit thread facts (still valuable)
            logger.warning("LLM synthesis failed for %s, returning raw Reddit facts", area_name)
            raw_facts = self._raw_reddit_facts(threads, thread_urls)
            return SkillResult(
                facts=raw_facts,
                confidence=0.4,
                cost=SkillCost(api_calls=api_calls),
            )

        tokens_used = synthesis.pop("_tokens", 0)
        api_calls += 1  # LLM call

        # Step 5: Convert synthesis to SourcedFacts
        facts = reddit_synthesis_to_facts(
            area_name=area_name,
            synthesis=synthesis,
            thread_count=len(threads),
            thread_urls=thread_urls,
            model_name=model_name,
        )

        return SkillResult(
            facts=facts,
            confidence=0.7,
            cost=SkillCost(
                llm_tokens=tokens_used,
                api_calls=api_calls,
                estimated_usd=tokens_used * 0.0000001,  # Gemini Flash pricing
            ),
        )

    def _gemini_fallback(self, area_name: str, city: str, input_data: dict) -> SkillResult:
        """Fallback: use Gemini with Google Search grounding (no Reddit data)."""
        prompt = GEMINI_AREA_PROMPT.format(area_name=area_name, city=city)
        synthesis = _call_gemini(prompt)

        if not synthesis:
            # Try Claude as last resort
            synthesis = _call_claude(prompt)
            if not synthesis:
                return SkillResult(
                    confidence=0.0,
                    cost=SkillCost(api_calls=1),
                )

        tokens_used = synthesis.pop("_tokens", 0)
        facts = gemini_fallback_to_facts(synthesis, area_name, city)

        return SkillResult(
            facts=facts,
            confidence=0.7,
            cost=SkillCost(
                llm_tokens=tokens_used,
                api_calls=1,
                estimated_usd=tokens_used * 0.0000001,
            ),
        )

    def _raw_reddit_facts(self, threads: List[dict], thread_urls: List[str]) -> List[SourcedFact]:
        """Return basic Reddit facts when LLM synthesis is unavailable."""
        source = FactSource(
            source_type="Reddit",
            url=thread_urls[0] if thread_urls else None,
            skill_id="learn_area",
        )

        facts = [
            SourcedFact(
                key="reddit_area_thread_count",
                value={"type": "Numeric", "data": len(threads)},
                confidence=1.0,
                source=source,
                display_template="Based on {value} Reddit discussions",
            ),
            SourcedFact(
                key="reddit_area_threads",
                value={
                    "type": "Tags",
                    "data": [t["title"] for t in threads[:10]],
                },
                confidence=0.7,
                source=source,
                display_template="Discussed on Reddit: {value}",
                answers_preferences=["resident feedback", "reddit", "reviews"],
            ),
        ]

        total_score = sum(t.get("score", 0) for t in threads)
        if total_score > 0:
            facts.append(SourcedFact(
                key="reddit_area_total_score",
                value={"type": "Numeric", "data": total_score},
                confidence=0.7,
                source=source,
                display_template="Reddit community score: {value}",
            ))

        return facts

    def estimated_cost(self) -> SkillCost:
        return SkillCost(llm_tokens=2000, api_calls=10, estimated_usd=0.002)


if __name__ == "__main__":
    """Quick test: python3 -m pipeline.skills.learn_area"""
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(name)s: %(message)s")

    area = "Whitefield"
    city = "Bengaluru"

    print(f"\n=== Learn Area: {area}, {city} ===\n")

    # Step 1: Show Reddit thread fetching
    print("--- Fetching Reddit threads ---")
    threads = fetch_area_reddit_threads(area, city)
    print(f"Found {len(threads)} unique threads\n")

    if threads:
        print("Top 5 threads by engagement:")
        for i, t in enumerate(threads[:5], 1):
            print(f"  {i}. [{t['score']} pts, {t['num_comments']} comments] {t['title']}")
        print()

    # Step 2: Run full skill
    has_api_key = bool(os.environ.get("GOOGLE_AI_API_KEY") or os.environ.get("ANTHROPIC_API_KEY"))

    if has_api_key:
        print("--- Running full skill with LLM synthesis ---")
        skill = LearnAreaSkill()
        result = skill.run(
            {
                "area_name": area,
                "city": city,
                "triggered_by": "manual_test",
            },
            force=True,  # Skip cache for testing
        )

        print(f"\n=== Results: {len(result.facts)} facts, confidence={result.confidence:.2f} ===")
        print(f"Cost: {result.cost.llm_tokens} tokens, {result.cost.api_calls} API calls, ${result.cost.estimated_usd:.4f}")
        print()
        for fact in result.facts:
            template = fact.display_template or "{value}"
            display = template.replace("{value}", str(fact.value.get("data", "")))
            print(f"  [{fact.key}] {display}")
            if fact.answers_preferences:
                print(f"    answers: {fact.answers_preferences}")
            if fact.scoring_hint:
                print(f"    scoring: {fact.scoring_hint}")
    else:
        print("--- No API key available (GOOGLE_AI_API_KEY or ANTHROPIC_API_KEY) ---")
        print("Showing raw threads only. Set an API key to run full synthesis.")
        if threads:
            print(f"\nSample selftext from top thread:")
            print(f"  {threads[0].get('selftext', '(none)')[:200]}")
