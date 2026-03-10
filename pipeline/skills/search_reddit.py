"""
search_reddit — fetch Reddit threads mentioning a society or area.

Input: {"query": "Prestige Lakeside Habitat Whitefield", "subreddit": "bangalore"}
Output: SkillResult with facts about thread count, sentiment indicators, and raw threads.

No LLM required — uses Reddit's public JSON API.
"""

import json
import logging
import time
from typing import List
from urllib.request import Request, urlopen
from urllib.parse import quote_plus

from pipeline.skills.base import BaseSkill, SkillResult, SkillCost, SourcedFact, FactSource

logger = logging.getLogger(__name__)

REDDIT_SEARCH_URL = "https://www.reddit.com/r/{subreddit}/search.json?q={query}&restrict_sr=1&sort=relevance&limit=15"
USER_AGENT = "OpenEstates/1.0 (knowledge-graph-builder)"


def fetch_reddit_threads(query: str, subreddit: str = "bangalore", limit: int = 15) -> List[dict]:
    """Fetch Reddit search results as JSON."""
    url = REDDIT_SEARCH_URL.format(
        subreddit=subreddit,
        query=quote_plus(query),
    )
    req = Request(url, headers={"User-Agent": USER_AGENT})

    try:
        with urlopen(req, timeout=10) as resp:
            data = json.loads(resp.read().decode())
    except Exception as e:
        logger.warning("Reddit search failed for '%s': %s", query, e)
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


class SearchRedditSkill(BaseSkill):
    skill_id = "search_reddit"
    description = "Search Reddit for threads mentioning a society, area, or topic"
    version = "1.0"
    output_keys = ["reddit_thread_count", "reddit_total_score", "reddit_total_comments", "reddit_threads"]

    def execute(self, input_data: dict) -> SkillResult:
        query = input_data.get("query", "")
        subreddit = input_data.get("subreddit", "bangalore")

        if not query:
            return SkillResult(confidence=0.0)

        threads = fetch_reddit_threads(query, subreddit)

        # Rate limit: be polite to Reddit
        time.sleep(1.0)

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
            cost=SkillCost(api_calls=1),
        )

    def estimated_cost(self) -> SkillCost:
        return SkillCost(api_calls=1, estimated_usd=0.0)
