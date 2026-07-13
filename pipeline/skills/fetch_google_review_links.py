"""
fetch_google_review_links - collect navigable Google Maps review links.

This skill is deliberately deterministic. It never calls an LLM and never
stores review prose as durable truth. When SERPAPI_API_KEY is present, it uses
Google Maps results from SerpAPI to capture a precise place link, place ID,
rating, and review count. Without an API key, it still writes a low-confidence
Google Maps search URL so the UI can offer one-click navigation.
"""

import json
import logging
import os
import urllib.parse
import urllib.request
from typing import Any, Dict, List, Optional

from pipeline.skills.base import BaseSkill, FactSource, SkillCost, SkillResult, SourcedFact

logger = logging.getLogger(__name__)

SERPAPI_SEARCH_URL = "https://serpapi.com/search.json"
GOOGLE_MAPS_SEARCH_URL = "https://www.google.com/maps/search/"


class FetchGoogleReviewLinksSkill(BaseSkill):
    """Collect Google review navigation facts for a society."""

    skill_id = "fetch_google_review_links"
    description = "Collect Google Maps review links and place metadata without LLMs."
    version = "1.0"
    output_keys = [
        "google_reviews_url",
        "google_place_id",
        "google_rating",
        "google_review_count",
    ]

    def _cache_key(self, input_data: dict) -> str:
        cache_input = dict(input_data)
        cache_input["_source_mode"] = "serpapi" if serpapi_api_key() else "maps_search"
        return super()._cache_key(cache_input)

    def execute(self, input_data: dict) -> SkillResult:
        query = build_place_query(input_data)
        if not query:
            logger.warning("fetch_google_review_links requires name/society_name/query")
            return SkillResult(confidence=0.0)

        place_id = clean_text(input_data.get("google_place_id"))
        if place_id:
            return place_to_skill_result(query, {"place_id": place_id}, api_calls=0)

        api_key = serpapi_api_key()
        if api_key:
            payload = fetch_serpapi_maps(query, api_key)
            place = best_place_result(payload)
            if place:
                return place_to_skill_result(query, place, api_calls=1)
            logger.info("SerpAPI returned no Google Maps place for query: %s", query)

        return fallback_search_result(query)

    def estimated_cost(self) -> SkillCost:
        return SkillCost(api_calls=1, estimated_usd=0.002)


def build_place_query(input_data: dict) -> str:
    """Build a stable Maps query from the entity input."""
    name = clean_text(
        input_data.get("society_name")
        or input_data.get("name")
        or input_data.get("project_name")
        or input_data.get("query")
    )
    if not name:
        return ""

    parts = [name]
    for key in ("area", "city"):
        value = clean_text(input_data.get(key))
        if value and value.lower() not in name.lower():
            parts.append(value)
    return " ".join(parts)


def serpapi_api_key() -> str:
    return os.environ.get("SERPAPI_API_KEY") or os.environ.get("SERPAPI_KEY") or ""


def fetch_serpapi_maps(query: str, api_key: str) -> dict:
    """Fetch Google Maps search results via SerpAPI."""
    params = urllib.parse.urlencode(
        {
            "engine": "google_maps",
            "type": "search",
            "q": query,
            "api_key": api_key,
        }
    )
    url = f"{SERPAPI_SEARCH_URL}?{params}"
    req = urllib.request.Request(url, headers={"Accept": "application/json"})
    with urllib.request.urlopen(req, timeout=20) as resp:
        return json.loads(resp.read().decode("utf-8"))


def best_place_result(payload: dict) -> Optional[dict]:
    """Pick the highest-confidence place result from a SerpAPI payload."""
    place = payload.get("place_results")
    if isinstance(place, dict) and has_place_signal(place):
        return place

    local_results = payload.get("local_results")
    if isinstance(local_results, list):
        for result in local_results:
            if isinstance(result, dict) and has_place_signal(result):
                return result
    return None


def has_place_signal(result: dict) -> bool:
    return bool(
        clean_text(result.get("place_id"))
        or clean_text(result.get("link"))
        or clean_text(result.get("reviews_link"))
        or clean_text(result.get("title"))
    )


def place_to_skill_result(query: str, place: dict, api_calls: int) -> SkillResult:
    """Convert a Google Maps place payload to sourced KG facts."""
    place_id = clean_text(place.get("place_id"))
    url = (
        clean_text(place.get("reviews_link"))
        or clean_text(place.get("link"))
        or google_maps_search_url(query, place_id or None)
    )
    confidence = 0.85 if place_id or clean_text(place.get("link")) else 0.65
    source = FactSource(
        source_type="Google",
        url=url,
        skill_id=FetchGoogleReviewLinksSkill.skill_id,
    )

    facts: List[SourcedFact] = [
        SourcedFact(
            key="google_reviews_url",
            value={"type": "Text", "data": url},
            confidence=confidence,
            source=source,
            display_template="Google reviews: {value}",
            answers_preferences=["google reviews", "reviews", "resident feedback"],
        )
    ]

    if place_id:
        facts.append(
            SourcedFact(
                key="google_place_id",
                value={"type": "Text", "data": place_id},
                confidence=confidence,
                source=source,
                display_template="Google place id: {value}",
                answers_preferences=["google reviews", "maps"],
            )
        )

    rating = parse_float(place.get("rating"))
    if rating is not None:
        facts.append(
            SourcedFact(
                key="google_rating",
                value={"type": "Numeric", "data": rating},
                confidence=confidence,
                source=source,
                display_template="Google rating: {value}",
                answers_preferences=["high rating", "good reviews", "google rating"],
                scoring_hint={
                    "direction": "HigherIsBetter",
                    "weight": 1.0,
                    "thresholds": [4.4, 4.0],
                },
            )
        )

    review_count = parse_int(place.get("reviews") or place.get("reviews_original"))
    if review_count is not None:
        facts.append(
            SourcedFact(
                key="google_review_count",
                value={"type": "Numeric", "data": review_count},
                confidence=confidence,
                source=source,
                display_template="Google reviews: {value}",
                answers_preferences=["many reviews", "review count", "google reviews"],
                scoring_hint={
                    "direction": "HigherIsBetter",
                    "weight": 0.5,
                    "thresholds": [500.0, 100.0],
                },
            )
        )

    return SkillResult(
        facts=facts,
        confidence=confidence,
        cost=SkillCost(api_calls=api_calls, estimated_usd=0.002 if api_calls else 0.0),
    )


def fallback_search_result(query: str) -> SkillResult:
    url = google_maps_search_url(query)
    source = FactSource(
        source_type="Google",
        url=url,
        skill_id=FetchGoogleReviewLinksSkill.skill_id,
    )
    return SkillResult(
        facts=[
            SourcedFact(
                key="google_reviews_url",
                value={"type": "Text", "data": url},
                confidence=0.45,
                source=source,
                display_template="Google reviews: {value}",
                answers_preferences=["google reviews", "reviews", "resident feedback"],
            )
        ],
        confidence=0.45,
        cost=SkillCost(api_calls=0, estimated_usd=0.0),
    )


def google_maps_search_url(query: str, place_id: Optional[str] = None) -> str:
    params = {"api": "1", "query": query}
    if place_id:
        params["query_place_id"] = place_id
    return f"{GOOGLE_MAPS_SEARCH_URL}?{urllib.parse.urlencode(params)}"


def parse_float(value: Any) -> Optional[float]:
    if isinstance(value, (int, float)):
        return float(value)
    if isinstance(value, str):
        text = value.strip()
        if not text:
            return None
        try:
            return float(text)
        except ValueError:
            return None
    return None


def parse_int(value: Any) -> Optional[int]:
    if isinstance(value, int):
        return value
    if isinstance(value, float):
        return int(value)
    if isinstance(value, str):
        digits = "".join(ch for ch in value if ch.isdigit())
        if digits:
            return int(digits)
    return None


def clean_text(value: Any) -> str:
    return value.strip() if isinstance(value, str) else ""
