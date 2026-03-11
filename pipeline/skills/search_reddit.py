"""
search_reddit — find Reddit threads mentioning a society or area.

Input: {"query": "Prestige Lakeside Habitat Whitefield", "subreddit": "bangalore"}
Output: SkillResult with facts about thread count, sentiment indicators, and raw threads.

Primary: Gemini with Google Search grounding (searches Reddit via Google).
Fallback: Reddit JSON API (currently blocked without OAuth2).
"""

import json
import logging
import os
import time
from typing import List, Optional
from urllib.request import Request, urlopen
from urllib.parse import quote_plus

from pipeline.skills.base import BaseSkill, SkillResult, SkillCost, SourcedFact, FactSource

logger = logging.getLogger(__name__)

REDDIT_SEARCH_URL = "https://www.reddit.com/r/{subreddit}/search.json?q={query}&restrict_sr=1&sort=relevance&limit=15"

GEMINI_REDDIT_PROMPT = """What Reddit discussions exist about "{query}" on the r/{subreddit} subreddit?

List each thread with its exact title and a brief summary of what was discussed.
If no Reddit threads exist about this topic, say "No threads found"."""

GEMINI_PARSE_PROMPT = """Extract thread information from this text into JSON.

Text:
{text}

Return JSON:
{{"threads": [{{"title": "exact thread title", "summary": "1 sentence summary"}}]}}

If the text says no threads were found, return {{"threads": []}}."""


def _search_via_gemini(query: str, subreddit: str = "bangalore") -> List[dict]:
    """Search Reddit via Gemini: grounded search to find threads, then parse."""
    api_key = os.environ.get("GOOGLE_AI_API_KEY")
    if not api_key:
        logger.warning("GOOGLE_AI_API_KEY not set, cannot search Reddit via Gemini")
        return []

    # Step 1: Grounded search (natural language — JSON format breaks grounding)
    prompt = GEMINI_REDDIT_PROMPT.format(query=query, subreddit=subreddit)
    payload = json.dumps({
        "contents": [{"parts": [{"text": prompt}]}],
        "tools": [{"google_search": {}}],
        "generationConfig": {"temperature": 0.1, "maxOutputTokens": 4000},
    }).encode()

    url = f"https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={api_key}"
    req = Request(url, data=payload, headers={"Content-Type": "application/json"})

    try:
        with urlopen(req, timeout=120) as resp:
            data = json.loads(resp.read().decode())
    except Exception as e:
        logger.warning("Gemini Reddit search failed: %s", e)
        return []

    search_text = ""
    for candidate in data.get("candidates", []):
        for part in candidate.get("content", {}).get("parts", []):
            if "text" in part:
                search_text += part["text"] + "\n"

    if not search_text.strip() or "no threads found" in search_text.lower():
        return []

    # Step 2: Parse natural language into structured JSON (no grounding needed)
    parse_prompt = GEMINI_PARSE_PROMPT.format(text=search_text[:3000])
    parse_payload = json.dumps({
        "contents": [{"parts": [{"text": parse_prompt}]}],
        "generationConfig": {"temperature": 0.0, "maxOutputTokens": 2000},
    }).encode()

    parse_req = Request(url, data=parse_payload, headers={"Content-Type": "application/json"})

    try:
        with urlopen(parse_req, timeout=30) as resp:
            parse_data = json.loads(resp.read().decode())
    except Exception as e:
        logger.warning("Gemini parse step failed: %s", e)
        return []

    parse_text = ""
    for candidate in parse_data.get("candidates", []):
        for part in candidate.get("content", {}).get("parts", []):
            if "text" in part:
                parse_text = part["text"]
                break

    parse_text = parse_text.strip()
    if parse_text.startswith("```"):
        lines = parse_text.split("\n")
        parse_text = "\n".join(lines[1:-1] if lines[-1].strip() == "```" else lines[1:])

    try:
        result = json.loads(parse_text)
    except json.JSONDecodeError:
        logger.warning("Failed to parse Gemini Reddit search response")
        return []

    threads = result.get("threads", [])
    normalized = []
    for t in threads:
        if not t.get("title"):
            continue
        normalized.append({
            "title": t.get("title", "").replace(" : r/bangalore", "").replace(" : r/" + subreddit, ""),
            "url": "",
            "subreddit": subreddit,
            "score": int(t.get("score", 0)),
            "num_comments": int(t.get("num_comments", 0)),
            "selftext": t.get("summary", "")[:500],
        })

    return normalized


def _search_via_reddit_api(query: str, subreddit: str = "bangalore", limit: int = 15) -> List[dict]:
    """Fetch Reddit search results via direct JSON API (may be blocked)."""
    url = REDDIT_SEARCH_URL.format(
        subreddit=subreddit,
        query=quote_plus(query),
    )
    req = Request(url, headers={
        "User-Agent": "python:openestates:v1.0 (by /u/openestates_bot)",
    })

    try:
        with urlopen(req, timeout=10) as resp:
            data = json.loads(resp.read().decode())
    except Exception as e:
        logger.debug("Reddit direct API failed for '%s': %s", query, e)
        return []

    threads = []
    for child in data.get("data", {}).get("children", [])[:limit]:
        post = child.get("data", {})
        threads.append({
            "title": post.get("title", ""),
            "url": f"https://reddit.com{post.get('permalink', '')}",
            "subreddit": post.get("subreddit", ""),
            "score": post.get("score", 0),
            "num_comments": post.get("num_comments", 0),
            "created_utc": post.get("created_utc", 0),
            "selftext": (post.get("selftext", "") or "")[:500],
        })

    return threads


def fetch_reddit_threads(query: str, subreddit: str = "bangalore", limit: int = 15) -> List[dict]:
    """Fetch Reddit threads — tries Gemini grounded search first, then direct API."""
    # Try direct Reddit API first (fast, free, but may be blocked)
    threads = _search_via_reddit_api(query, subreddit, limit)
    if threads:
        logger.info("Found %d threads via Reddit API for '%s'", len(threads), query)
        return threads

    # Fall back to Gemini with Google Search grounding
    threads = _search_via_gemini(query, subreddit)
    if threads:
        logger.info("Found %d threads via Gemini grounded search for '%s'", len(threads), query)
    return threads


class SearchRedditSkill(BaseSkill):
    skill_id = "search_reddit"
    description = "Search Reddit for threads mentioning a society, area, or topic"
    version = "2.0"  # v2: Gemini grounded search fallback
    output_keys = ["reddit_thread_count", "reddit_total_score", "reddit_total_comments", "reddit_threads"]

    def execute(self, input_data: dict) -> SkillResult:
        query = input_data.get("query", "")
        subreddit = input_data.get("subreddit", "bangalore")

        if not query:
            return SkillResult(confidence=0.0)

        threads = fetch_reddit_threads(query, subreddit)

        if not threads:
            return SkillResult(
                facts=[
                    SourcedFact(
                        key="reddit_thread_count",
                        value={"type": "Numeric", "data": 0},
                        confidence=0.5,
                        source=FactSource(
                            source_type="Reddit",
                            skill_id=self.skill_id,
                            triggered_by=input_data.get("triggered_by"),
                        ),
                    )
                ],
                confidence=0.3,
                cost=SkillCost(api_calls=1),
            )

        total_score = sum(t["score"] for t in threads)
        total_comments = sum(t["num_comments"] for t in threads)

        facts = [
            SourcedFact(
                key="reddit_thread_count",
                value={"type": "Numeric", "data": len(threads)},
                confidence=0.7,
                source=FactSource(
                    source_type="Reddit",
                    url=threads[0]["url"] if threads else None,
                    skill_id=self.skill_id,
                    triggered_by=input_data.get("triggered_by"),
                ),
                display_template="{value} Reddit discussions found",
                answers_preferences=["good reviews", "resident feedback", "reddit"],
                scoring_hint={"direction": "HigherIsBetter", "weight": 1.0, "thresholds": [5.0, 2.0]},
            ),
            SourcedFact(
                key="reddit_total_score",
                value={"type": "Numeric", "data": total_score},
                confidence=0.7,
                source=FactSource(
                    source_type="Reddit",
                    skill_id=self.skill_id,
                ),
                display_template="Reddit community score: {value}",
            ),
            SourcedFact(
                key="reddit_total_comments",
                value={"type": "Numeric", "data": total_comments},
                confidence=0.7,
                source=FactSource(
                    source_type="Reddit",
                    skill_id=self.skill_id,
                ),
                display_template="{value} comments across Reddit threads",
            ),
            SourcedFact(
                key="reddit_threads",
                value={
                    "type": "Tags",
                    "data": [t["title"] for t in threads[:5]],
                },
                confidence=0.7,
                source=FactSource(
                    source_type="Reddit",
                    skill_id=self.skill_id,
                ),
                display_template="Discussed on Reddit: {value}",
            ),
        ]

        return SkillResult(
            facts=facts,
            confidence=min(0.7, len(threads) / 10),
            cost=SkillCost(api_calls=1, estimated_usd=0.0003),
        )

    def estimated_cost(self) -> SkillCost:
        return SkillCost(api_calls=1, estimated_usd=0.0003)
