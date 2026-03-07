"""
Reddit Client — Minimal Safe Sampler with Slow Crawl Support

Fetches posts from subreddits using Reddit's public JSON feed.
No authentication required.

Safety rules:
  - Always cache responses locally
  - Never fetch the same content twice within cache_max_age
  - Keep volume low (default 25 posts per request)
  - Store only derived summaries, not full raw content

Slow crawl: fetch across multiple subreddits and sorts over time.
Each run is additive — the taxonomy accumulates.
"""

import hashlib
import json
import os
import time
import urllib.request
import urllib.error
from typing import Dict, List, Optional

PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CACHE_DIR = os.path.join(PROJECT_ROOT, "data", "reddit_cache")

USER_AGENT = "OpenEstates/0.1 (research prototype; local dev only; contact: local)"
REQUEST_TIMEOUT = 15
MIN_DELAY_BETWEEN_REQUESTS = 2.0  # seconds — be polite

_last_request_time: float = 0.0


# Default subreddits to crawl for Bangalore real estate research
DEFAULT_SUBREDDITS = [
    "BangaloreRealEstates",
    "bangalore",
    "india",  # filter to real estate posts via search
]


def _rate_limit():
    """Ensure minimum delay between requests to be polite."""
    global _last_request_time
    elapsed = time.time() - _last_request_time
    if elapsed < MIN_DELAY_BETWEEN_REQUESTS:
        time.sleep(MIN_DELAY_BETWEEN_REQUESTS - elapsed)
    _last_request_time = time.time()


def _cache_path(subreddit: str, sort: str, limit: int) -> str:
    key = f"{subreddit}_{sort}_{limit}"
    hashed = hashlib.md5(key.encode()).hexdigest()[:12]
    return os.path.join(CACHE_DIR, f"{subreddit}_{sort}_{hashed}.json")


def _is_cache_fresh(path: str, max_age_seconds: int) -> bool:
    if not os.path.exists(path):
        return False
    return (time.time() - os.path.getmtime(path)) < max_age_seconds


def _fetch_json(url: str) -> dict:
    """Fetch a URL with rate limiting and return parsed JSON."""
    _rate_limit()
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(req, timeout=REQUEST_TIMEOUT) as resp:
        return json.loads(resp.read().decode("utf-8"))


def _parse_posts(data: dict) -> List[Dict]:
    """Parse Reddit API response into clean post dicts."""
    posts = []
    for child in data.get("data", {}).get("children", []):
        p = child.get("data", {})
        posts.append({
            "id": p.get("id", ""),
            "title": p.get("title", ""),
            # Truncate body — store summaries not dumps
            "selftext": (p.get("selftext", "") or "")[:600],
            "score": p.get("score", 0),
            "num_comments": p.get("num_comments", 0),
            "created_utc": p.get("created_utc", 0),
            "permalink": p.get("permalink", ""),
            "author": p.get("author", ""),
            "subreddit": p.get("subreddit", ""),
        })
    return posts


def fetch_posts(
    subreddit: str,
    limit: int = 25,
    sort: str = "new",
    use_cache: bool = True,
    cache_max_age: int = 3600,
) -> List[Dict]:
    """
    Fetch recent posts from a subreddit.

    Args:
        subreddit:     Without r/ prefix (e.g. "BangaloreRealEstates")
        limit:         Max posts (keep small, default 25)
        sort:          "new", "hot", or "top"
        use_cache:     Use local cache
        cache_max_age: Cache freshness in seconds (default 1 hour)

    Returns:
        List of post dicts with: id, title, selftext, score, num_comments,
        created_utc, permalink, author, subreddit
    """
    cache_file = _cache_path(subreddit, sort, limit)

    if use_cache and _is_cache_fresh(cache_file, cache_max_age):
        with open(cache_file) as f:
            return json.load(f)

    url = f"https://www.reddit.com/r/{subreddit}/{sort}.json?limit={limit}"
    try:
        data = _fetch_json(url)
    except (urllib.error.URLError, urllib.error.HTTPError, OSError) as e:
        raise ConnectionError(f"Failed to fetch r/{subreddit}: {e}") from e

    posts = _parse_posts(data)

    if use_cache:
        os.makedirs(CACHE_DIR, exist_ok=True)
        with open(cache_file, "w") as f:
            json.dump(posts, f, indent=2)

    return posts


def slow_crawl(
    subreddits: Optional[List[str]] = None,
    limit_per_subreddit: int = 25,
    sorts: Optional[List[str]] = None,
    cache_max_age: int = 7200,
) -> List[Dict]:
    """
    Fetch posts from multiple subreddits across multiple sort orders.
    Deduplicates by post ID. Caches aggressively to avoid repeat hits.

    Args:
        subreddits:          List of subreddit names (no r/ prefix)
        limit_per_subreddit: Posts per subreddit per sort (default 25)
        sorts:               Sort orders to try (default: ["new", "hot"])
        cache_max_age:       Seconds before re-fetching (default 2 hours)

    Returns:
        Deduplicated list of posts from all sources.
    """
    if subreddits is None:
        subreddits = ["BangaloreRealEstates"]
    if sorts is None:
        sorts = ["new", "hot"]

    seen_ids: set = set()
    all_posts: List[Dict] = []

    for subreddit in subreddits:
        for sort in sorts:
            try:
                posts = fetch_posts(
                    subreddit,
                    limit=limit_per_subreddit,
                    sort=sort,
                    use_cache=True,
                    cache_max_age=cache_max_age,
                )
                for p in posts:
                    if p["id"] not in seen_ids:
                        seen_ids.add(p["id"])
                        all_posts.append(p)
            except ConnectionError as e:
                # Log but continue — partial results are still useful
                print(f"  [warn] r/{subreddit}/{sort}: {e}")

    return all_posts
