"""
search_reddit — find Reddit threads mentioning a society or area.

Input: {"query": "Prestige Lakeside Habitat Whitefield", "subreddit": "bangalore"}
Output: SkillResult with facts about thread count, sentiment indicators, and raw threads.

Uses Reddit's public JSON search endpoint only. If the endpoint is blocked or
returns no data, the skill records zero observed threads instead of calling an
LLM-backed search fallback.
"""

import json
import logging
import socket
import time
from typing import List, Optional
from urllib.error import HTTPError, URLError
from urllib.parse import quote_plus
from urllib.request import Request, urlopen

from pipeline.skills.base import BaseSkill, SkillResult, SkillCost, SourcedFact, FactSource

logger = logging.getLogger(__name__)

REDDIT_SEARCH_URL = "https://www.reddit.com/r/{subreddit}/search.json?q={query}&restrict_sr=1&sort=relevance&limit=15"


class RedditSourceError(RuntimeError):
    """A Reddit source failure that must not be interpreted as an empty result."""

    status = "error"


class RedditSourceBlocked(RedditSourceError):
    status = "blocked"


class RedditSourceInvalidResponse(RedditSourceError):
    status = "invalid_response"


class RedditSourceUnavailable(OSError):
    """A transient Reddit source failure eligible for BaseSkill retries."""

    status = "unavailable"


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
    except HTTPError as error:
        if error.code in (401, 403):
            raise RedditSourceBlocked(
                "Reddit search blocked with HTTP {} for '{}'".format(error.code, query)
            ) from error
        if error.code == 429 or error.code >= 500:
            raise RedditSourceUnavailable(
                "Reddit search unavailable with HTTP {} for '{}'".format(
                    error.code, query
                )
            ) from error
        raise RedditSourceError(
            "Reddit search failed with HTTP {} for '{}'".format(error.code, query)
        ) from error
    except (TimeoutError, socket.timeout) as error:
        raise RedditSourceUnavailable(
            "Reddit search timed out for '{}'".format(query)
        ) from error
    except URLError as error:
        raise RedditSourceUnavailable(
            "Reddit search transport failed for '{}': {}".format(query, error.reason)
        ) from error
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RedditSourceInvalidResponse(
            "Reddit search returned an invalid response for '{}'".format(query)
        ) from error

    listing = data.get("data") if isinstance(data, dict) else None
    children = listing.get("children") if isinstance(listing, dict) else None
    if not isinstance(children, list):
        raise RedditSourceInvalidResponse(
            "Reddit search returned an unexpected response shape for '{}'".format(query)
        )

    threads = []
    for child in children[:limit]:
        if not isinstance(child, dict):
            continue
        post = child.get("data", {})
        if not isinstance(post, dict):
            continue
        threads.append({
            "id": post.get("id") or post.get("name") or "",
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
    """Fetch Reddit threads from the public Reddit JSON API."""
    threads = _search_via_reddit_api(query, subreddit, limit)
    if threads:
        logger.info("Found %d threads via Reddit API for '%s'", len(threads), query)
    return threads


def fetch_reddit_threads_with_retry(
    query: str,
    subreddit: str = "bangalore",
    limit: int = 15,
    max_attempts: int = 3,
    sleep=time.sleep,
) -> List[dict]:
    """Retry only transient Reddit failures; blocking and invalid data fail fast."""
    attempts = max(1, max_attempts)
    for attempt in range(1, attempts + 1):
        try:
            return fetch_reddit_threads(query, subreddit, limit)
        except RedditSourceUnavailable:
            if attempt == attempts:
                raise
            sleep(min(2 ** (attempt - 1), 4))
    return []


def threads_to_skill_result(input_data: dict, threads: List[dict]) -> SkillResult:
    """Convert one exact Reddit thread set into deterministic sourced facts."""
    if not threads:
        return SkillResult(
            facts=[
                SourcedFact(
                    key="reddit_thread_count",
                    value={"type": "Numeric", "data": 0},
                    confidence=0.5,
                    source=FactSource(
                        source_type="Reddit",
                        skill_id="search_reddit",
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
                skill_id="search_reddit",
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
            source=FactSource(source_type="Reddit", skill_id="search_reddit"),
            display_template="Reddit community score: {value}",
        ),
        SourcedFact(
            key="reddit_total_comments",
            value={"type": "Numeric", "data": total_comments},
            confidence=0.7,
            source=FactSource(source_type="Reddit", skill_id="search_reddit"),
            display_template="{value} comments across Reddit threads",
        ),
        SourcedFact(
            key="reddit_threads",
            value={"type": "Tags", "data": [t["title"] for t in threads[:5]]},
            confidence=0.7,
            source=FactSource(source_type="Reddit", skill_id="search_reddit"),
            display_template="Discussed on Reddit: {value}",
        ),
    ]
    return SkillResult(
        facts=facts,
        confidence=min(0.7, len(threads) / 10),
        cost=SkillCost(api_calls=1, estimated_usd=0.0003),
    )


class SearchRedditSkill(BaseSkill):
    skill_id = "search_reddit"
    description = "Search Reddit for threads mentioning a society, area, or topic"
    version = "3.0"  # v3: direct Reddit only, no LLM fallback
    output_keys = ["reddit_thread_count", "reddit_total_score", "reddit_total_comments", "reddit_threads"]

    def execute(self, input_data: dict) -> SkillResult:
        query = input_data.get("query", "")
        subreddit = input_data.get("subreddit", "bangalore")

        if not query:
            return SkillResult(confidence=0.0)

        threads = fetch_reddit_threads(query, subreddit)
        return threads_to_skill_result(input_data, threads)

    def estimated_cost(self) -> SkillCost:
        return SkillCost(api_calls=1, estimated_usd=0.0003)
