"""
fetch_google_review_links - collect navigable Google Maps review links.

This skill is deliberately deterministic. It never calls an LLM and never
stores review prose as durable truth. When GOOGLE_PLACES_API_KEY is present,
it uses the official Google Places API to capture a precise place link, place
ID, rating, and review count. SERPAPI_API_KEY remains a fallback. Without an
API key, it still writes a low-confidence Google Maps search URL so the UI can
offer one-click navigation.
"""

import json
import logging
import os
import re
import time
import urllib.parse
import urllib.error
import urllib.request
import math
from pathlib import Path
from typing import Any, Dict, List, Optional

from pipeline.skills.base import BaseSkill, FactSource, SkillCost, SkillResult, SourcedFact

logger = logging.getLogger(__name__)

SERPAPI_SEARCH_URL = "https://serpapi.com/search.json"
GOOGLE_PLACES_TEXT_SEARCH_URL = "https://places.googleapis.com/v1/places:searchText"
GOOGLE_PLACES_DETAILS_URL = "https://places.googleapis.com/v1/{}"
GOOGLE_MAPS_SEARCH_URL = "https://www.google.com/maps/search/"
EARTH_RADIUS_KM = 6371.0088
_ORIGIN_LOCATION_CACHE = {}
_NEARBY_CATEGORY_CONFIG_CACHE = None
_PLACE_RESOLUTION_CONFIG_CACHE = None
GOOGLE_PLACES_FIELD_MASK = ",".join(
    [
        "places.id",
        "places.displayName",
        "places.formattedAddress",
        "places.googleMapsUri",
        "places.location",
        "places.primaryType",
        "places.rating",
        "places.types",
        "places.userRatingCount",
    ]
)
GOOGLE_PLACE_DETAILS_FIELD_MASK = ",".join(
    [
        "id",
        "displayName",
        "formattedAddress",
        "googleMapsUri",
        "location",
        "primaryType",
        "rating",
        "reviews",
        "types",
        "userRatingCount",
    ]
)


class FetchGoogleReviewLinksSkill(BaseSkill):
    """Collect Google review navigation facts for a society."""

    skill_id = "fetch_google_review_links"
    description = "Collect Google Maps review links and place metadata without LLMs."
    version = "1.6"
    output_keys = [
        "google_reviews_url",
        "google_place_id",
        "google_place_address",
        "google_rating",
        "google_review_count",
        "google_review_snippets",
    ]

    def _cache_key(self, input_data: dict) -> str:
        cache_input = dict(input_data)
        if google_places_api_key():
            cache_input["_source_mode"] = "google_places"
        elif serpapi_api_key():
            cache_input["_source_mode"] = "serpapi"
        else:
            cache_input["_source_mode"] = "maps_search"
        return super()._cache_key(cache_input)

    def execute(self, input_data: dict) -> SkillResult:
        queries = place_query_variants(input_data)
        if not queries:
            logger.warning("fetch_google_review_links requires name/society_name/query")
            return SkillResult(confidence=0.0)
        query = queries[0]["query"]

        place_id = clean_text(input_data.get("google_place_id"))
        if place_id:
            api_calls = 0
            place_payload = {"place_id": place_id}
            places_key = google_places_api_key()
            if places_key:
                details = fetch_google_place_details_if_available(place_id, places_key)
                api_calls = 1 if details else 0
                place_payload.update(google_places_to_place_payload(query, details or {}))
                place_payload["place_id"] = place_id
            return place_to_skill_result(
                query,
                place_payload,
                api_calls=api_calls,
                fetch_source="seeded_google_place_id",
            )

        places_key = google_places_api_key()
        if places_key:
            api_calls = 0
            resolution_log = []
            for query_plan in queries:
                candidate_query = query_plan["query"]
                payload = fetch_google_places_text_search(
                    candidate_query,
                    places_key,
                    max_result_count=5,
                )
                api_calls += 1
                resolution = resolve_google_project_place(
                    payload,
                    query_plan.get("resolution_input") or input_data,
                )
                place = (
                    resolution.get("place")
                    if resolution["status"] == "accepted"
                    else None
                )
                if place:
                    place_payload = google_places_to_place_payload(candidate_query, place)
                    place_id = clean_text(place_payload.get("place_id"))
                    if place_id:
                        details = fetch_google_place_details_if_available(
                            place_id, places_key
                        )
                        if details:
                            api_calls += 1
                            place_payload.update(
                                google_places_to_place_payload(candidate_query, details)
                            )
                    return place_to_skill_result(
                        candidate_query,
                        place_payload,
                        api_calls=api_calls,
                        fetch_source="google_places_text_search",
                    )
                resolution_log.append(
                    "{} -> {} ({})".format(
                        candidate_query,
                        resolution["status"],
                        "; ".join(resolution["reasons"]),
                    )
                )
            logger.info(
                "Google Places did not resolve an accepted place for %s: %s",
                query,
                " | ".join(resolution_log),
            )

        serpapi_key = serpapi_api_key()
        if serpapi_key:
            payload = fetch_serpapi_maps(query, serpapi_key)
            place = best_place_result(payload)
            if place:
                return place_to_skill_result(
                    query,
                    place,
                    api_calls=1,
                    fetch_source="serpapi_google_maps",
                )
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


def strip_project_phase_suffix(value: str) -> str:
    """Return the buyer-facing project name without trailing RERA phase suffixes."""
    name = clean_text(value)
    if not name:
        return ""
    stripped = re.sub(
        r"\s+(?:phase|ph)\s*[-:]?\s*(?:[ivxlcdm]+|\d+[a-z]?)"
        r"(?:\s*(?:,|/|&|and)\s*(?:[ivxlcdm]+|\d+[a-z]?))*\b.*$",
        "",
        name,
        flags=re.IGNORECASE,
    ).strip(" -:,.")
    return stripped or name


def place_query_variants(input_data: dict) -> List[Dict[str, Any]]:
    """Ordered Google project resolution attempts.

    Exact RERA/project name is always first. Broader phase/address evidence is
    only tried after that pass fails.
    """
    primary = build_place_query(input_data)
    if not primary:
        return []

    variants = []
    seen = set()

    def add_variant(query: str, resolution_input: dict, strategy: str) -> None:
        normalized = clean_text(query)
        if not normalized or normalized.lower() in seen:
            return
        variants.append(
            {
                "query": normalized,
                "resolution_input": resolution_input,
                "strategy": strategy,
            }
        )
        seen.add(normalized.lower())

    add_variant(primary, input_data, "exact_name")
    raw_name = clean_text(
        input_data.get("society_name")
        or input_data.get("name")
        or input_data.get("project_name")
    )
    base_name = strip_project_phase_suffix(raw_name)
    has_phase_suffix = bool(
        base_name and raw_name and base_name.lower() != raw_name.lower()
    )
    base_input = None
    if has_phase_suffix:
        base_input = dict(input_data)
        for key in ("society_name", "name", "project_name"):
            if clean_text(base_input.get(key)):
                base_input[key] = base_name
        base_query = build_place_query(base_input)
        add_variant(base_query, base_input, "phase_stripped_name")

    city = clean_text(input_data.get("city")) or "Bengaluru"
    if raw_name and city:
        add_variant(
            "{} {}".format(raw_name, city),
            input_data,
            "city_only_name",
        )
    if has_phase_suffix and base_input and city:
        add_variant(
            "{} {}".format(base_name, city),
            base_input,
            "phase_stripped_city_only_name",
        )

    address = clean_text(
        input_data.get("address") or input_data.get("rera_project_address")
    )
    if address:
        pre_address_variants = list(variants)
        if raw_name:
            add_variant(
                "{} {}".format(raw_name, address),
                input_data,
                "name_with_address",
            )
        if has_phase_suffix and base_input:
            add_variant(
                "{} {}".format(base_name, address),
                base_input,
                "phase_stripped_name_with_address",
            )
        for parent in pre_address_variants:
            add_variant(
                "{} {}".format(parent["query"], address),
                parent["resolution_input"],
                "{}_with_address".format(parent["strategy"]),
            )
    return variants


def serpapi_api_key() -> str:
    return os.environ.get("SERPAPI_API_KEY") or os.environ.get("SERPAPI_KEY") or ""


def google_places_api_key() -> str:
    return (
        os.environ.get("GOOGLE_PLACES_API_KEY")
        or os.environ.get("GOOGLE_MAPS_API_KEY")
        or os.environ.get("GOOGLE_API_KEY")
        or ""
    )


def fetch_google_places_text_search(
    query: str,
    api_key: str,
    max_result_count: int = 3,
    location_bias: Optional[Dict[str, float]] = None,
    radius_meters: Optional[int] = None,
) -> dict:
    """Fetch place candidates from the official Google Places Text Search API."""
    request_body = {
        "textQuery": query,
        "maxResultCount": max(1, min(int(max_result_count), 5)),
        "languageCode": "en",
        "regionCode": "IN",
    }
    if location_bias:
        radius = int(radius_meters or 8_000)
        request_body["locationBias"] = {
            "circle": {
                "center": {
                    "latitude": location_bias["latitude"],
                    "longitude": location_bias["longitude"],
                },
                "radius": max(500, min(radius, 50_000)),
            }
        }
    body = json.dumps(
        request_body
    ).encode("utf-8")
    headers = {
        "Accept": "application/json",
        "Content-Type": "application/json",
        "X-Goog-Api-Key": api_key,
        "X-Goog-FieldMask": GOOGLE_PLACES_FIELD_MASK,
    }
    last_error = ""
    for attempt in range(1, 4):
        req = urllib.request.Request(
            GOOGLE_PLACES_TEXT_SEARCH_URL,
            data=body,
            headers=headers,
        )
        try:
            with urllib.request.urlopen(req, timeout=20) as resp:
                return json.loads(resp.read().decode("utf-8"))
        except urllib.error.HTTPError as error:
            response_body = error.read().decode("utf-8", errors="replace")
            last_error = "HTTP {}: {}".format(error.code, response_body[:500])
            if attempt < 3 and retryable_google_places_error(error.code, response_body):
                time.sleep(float(attempt))
                continue
            raise RuntimeError("Google Places request failed: {}".format(last_error)) from error
    raise RuntimeError("Google Places request failed: {}".format(last_error))


def fetch_google_place_details_if_available(place_id: str, api_key: str) -> Optional[dict]:
    """Fetch Place Details for review excerpts, without failing the base place lookup."""
    try:
        return fetch_google_place_details(place_id, api_key)
    except Exception as error:
        logger.warning("Google Place Details failed for %s: %s", place_id, error)
        return None


def fetch_google_place_details(place_id: str, api_key: str) -> dict:
    """Fetch details for a known Google place ID using the official Places API."""
    resource_name = google_place_resource_name(place_id)
    url = GOOGLE_PLACES_DETAILS_URL.format(urllib.parse.quote(resource_name, safe="/"))
    headers = {
        "Accept": "application/json",
        "X-Goog-Api-Key": api_key,
        "X-Goog-FieldMask": GOOGLE_PLACE_DETAILS_FIELD_MASK,
    }
    last_error = ""
    for attempt in range(1, 4):
        req = urllib.request.Request(url, headers=headers)
        try:
            with urllib.request.urlopen(req, timeout=20) as resp:
                return json.loads(resp.read().decode("utf-8"))
        except urllib.error.HTTPError as error:
            response_body = error.read().decode("utf-8", errors="replace")
            last_error = "HTTP {}: {}".format(error.code, response_body[:500])
            if attempt < 3 and retryable_google_places_error(error.code, response_body):
                time.sleep(float(attempt))
                continue
            raise RuntimeError("Google Place Details request failed: {}".format(last_error)) from error
    raise RuntimeError("Google Place Details request failed: {}".format(last_error))


def google_place_resource_name(place_id: str) -> str:
    cleaned = clean_text(place_id)
    if cleaned.startswith("places/"):
        return cleaned
    return "places/{}".format(cleaned)


def retryable_google_places_error(status_code: int, response_body: str) -> bool:
    if status_code in (429, 500, 502, 503, 504):
        return True
    if status_code == 403:
        return any(
            marker in response_body
            for marker in ("SERVICE_DISABLED", "RESOURCE_EXHAUSTED", "RATE_LIMIT")
        )
    return False


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


def best_google_places_result(payload: dict) -> Optional[dict]:
    places = payload.get("places")
    if not isinstance(places, list):
        return None
    for place in places:
        if isinstance(place, dict) and has_google_places_signal(place):
            return place
    return None


def resolve_google_project_place(payload: dict, input_data: dict) -> Dict[str, Any]:
    places = payload.get("places")
    if not isinstance(places, list):
        return {"status": "rejected", "place": None, "reasons": ["no_candidates"]}
    evaluated = [
        evaluate_google_project_place(place, input_data)
        for place in places
        if isinstance(place, dict)
    ]
    accepted = [candidate for candidate in evaluated if candidate["eligible"]]
    accepted.sort(
        key=lambda candidate: (
            -candidate["score"],
            clean_text(candidate["place"].get("id")),
        )
    )
    if not accepted:
        reasons = sorted(
            {
                reason
                for candidate in evaluated
                for reason in candidate["reasons"]
            }
        ) or ["no_eligible_candidate"]
        return {"status": "rejected", "place": None, "reasons": reasons}
    policy = google_place_resolution_policy()
    if len(accepted) > 1 and (
        accepted[0]["score"] - accepted[1]["score"]
        < float(policy.get("ambiguity_margin") or 0.0)
    ):
        return {
            "status": "ambiguous",
            "place": None,
            "reasons": [
                "top_candidates_within_margin",
                "{}:{:.3f}".format(
                    google_place_display_name(accepted[0]["place"]), accepted[0]["score"]
                ),
                "{}:{:.3f}".format(
                    google_place_display_name(accepted[1]["place"]), accepted[1]["score"]
                ),
            ],
        }
    winner = accepted[0]
    return {
        "status": "accepted",
        "place": winner["place"],
        "score": winner["score"],
        "reasons": winner["reasons"],
    }


def evaluate_google_project_place(place: dict, input_data: dict) -> Dict[str, Any]:
    policy = google_place_resolution_policy()
    reasons = []
    place_id = clean_text(place.get("id"))
    location = google_place_location(place)
    if not place_id:
        reasons.append("missing_place_id")
    if not location:
        reasons.append("missing_location")

    expected_name = clean_text(
        input_data.get("society_name")
        or input_data.get("name")
        or input_data.get("project_name")
    )
    name_recall = token_recall(
        expected_name,
        google_place_display_name(place),
        policy.get("ignored_name_tokens") or [],
    )
    minimum_name_recall = float(policy.get("minimum_name_recall") or 0.0)
    if name_recall < minimum_name_recall:
        reasons.append("name_recall_below_threshold")

    place_types = google_place_types(place)
    rejected_types = normalized_config_values(policy.get("rejected_place_types"))
    accepted_types = normalized_config_values(policy.get("accepted_place_types"))
    rejected_type_match = bool(place_types & rejected_types)
    broad_place_types = {"establishment", "point_of_interest", "premise"}
    specific_accepted_types = accepted_types - broad_place_types
    if place_types & specific_accepted_types:
        type_match = 1.0
    elif place_types & accepted_types:
        type_match = float(policy.get("broad_place_type_score") or 1.0)
    else:
        type_match = 0.0
    if rejected_type_match and not (place_types & specific_accepted_types):
        reasons.append("rejected_place_type")
    if not type_match:
        reasons.append("place_type_not_accepted")

    address = clean_text(place.get("formattedAddress"))
    locality_values = [
        clean_text(input_data.get("area")),
        clean_text(input_data.get("city")),
    ]
    locality_values = [value for value in locality_values if value]
    locality_match = (
        sum(1.0 for value in locality_values if value.lower() in address.lower())
        / len(locality_values)
        if locality_values
        else 0.0
    )
    weights = policy.get("weights") or {}
    score = (
        name_recall * float(weights.get("name") or 0.0)
        + locality_match * float(weights.get("locality") or 0.0)
        + type_match * float(weights.get("place_type") or 0.0)
    )
    subplace_tokens = normalized_config_values(policy.get("demoted_name_tokens"))
    if subplace_tokens:
        expected_tokens = normalized_name_tokens(
            expected_name,
            policy.get("ignored_name_tokens") or [],
        )
        actual_tokens = normalized_name_tokens(
            google_place_display_name(place),
            policy.get("ignored_name_tokens") or [],
        )
        matched_subplace_tokens = sorted(
            (actual_tokens - expected_tokens) & subplace_tokens
        )
        if matched_subplace_tokens:
            penalty = float(policy.get("demoted_name_token_penalty") or 0.0)
            score = max(0.0, score - penalty)
            reasons.append(
                "demoted_name_tokens:{}".format(",".join(matched_subplace_tokens))
            )
    if score < float(policy.get("minimum_score") or 0.0):
        reasons.append("score_below_threshold")
    eligible = not any(
        reason
        in {
            "missing_place_id",
            "missing_location",
            "name_recall_below_threshold",
            "rejected_place_type",
            "place_type_not_accepted",
            "score_below_threshold",
        }
        for reason in reasons
    )
    if eligible:
        reasons.extend(
            [
                "name_recall:{:.3f}".format(name_recall),
                "locality_match:{:.3f}".format(locality_match),
                "place_type_match:{:.3f}".format(type_match),
            ]
        )
    return {"place": place, "eligible": eligible, "score": score, "reasons": reasons}


def google_place_resolution_policy() -> Dict[str, Any]:
    global _PLACE_RESOLUTION_CONFIG_CACHE
    if _PLACE_RESOLUTION_CONFIG_CACHE is None:
        path = (
            Path(__file__).resolve().parents[2]
            / "app"
            / "config"
            / "dag"
            / "google_place_resolution.json"
        )
        payload = json.loads(path.read_text(encoding="utf-8"))
        _PLACE_RESOLUTION_CONFIG_CACHE = payload.get("project_place") or {}
    return _PLACE_RESOLUTION_CONFIG_CACHE


def token_recall(expected: str, actual: str, ignored_tokens: List[str]) -> float:
    expected_tokens = normalized_name_tokens(expected, ignored_tokens)
    actual_tokens = normalized_name_tokens(actual, ignored_tokens)
    if not expected_tokens:
        return 0.0
    return len(expected_tokens & actual_tokens) / float(len(expected_tokens))


def normalized_name_tokens(value: str, ignored_tokens: List[str]) -> set:
    ignored = {normalize_match_token(token) for token in ignored_tokens}
    tokens = set()
    for token in value.split():
        normalized = normalize_match_token(token)
        if (
            normalized
            and normalized not in ignored
            and any(character.isalpha() for character in normalized)
        ):
            tokens.add(normalized)
    return tokens


def normalize_match_token(value: str) -> str:
    normalized = "".join(character.lower() for character in value if character.isalnum())
    if any(character.isalpha() for character in normalized):
        normalized = normalized.rstrip("0123456789")
    return normalized


def normalized_config_values(values: Any) -> set:
    return {
        clean_text(value).replace("-", "_").lower()
        for value in values or []
        if clean_text(value)
    }


def has_place_signal(result: dict) -> bool:
    return bool(
        clean_text(result.get("place_id"))
        or clean_text(result.get("link"))
        or clean_text(result.get("reviews_link"))
        or clean_text(result.get("title"))
    )


def has_google_places_signal(place: dict) -> bool:
    return bool(
        clean_text(place.get("id"))
        or clean_text(place.get("googleMapsUri"))
        or google_place_display_name(place)
    )


def google_places_to_place_payload(query: str, place: dict) -> dict:
    place_id = clean_text(place.get("id"))
    return {
        "place_id": place_id,
        "link": clean_text(place.get("googleMapsUri"))
        or google_maps_search_url(query, place_id or None),
        "title": google_place_display_name(place),
        "address": clean_text(place.get("formattedAddress")),
        "rating": place.get("rating"),
        "reviews": place.get("userRatingCount"),
        "location": place.get("location") or {},
        "primary_type": clean_text(place.get("primaryType")) or None,
        "place_types": sorted(google_place_types(place)),
        "review_snippets": google_place_review_snippets(place),
    }


def place_to_skill_result(
    query: str,
    place: dict,
    api_calls: int,
    fetch_source: str,
) -> SkillResult:
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
        triggered_by=f"{fetch_source}:{query}",
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

    address = clean_text(place.get("address"))
    if address:
        facts.append(
            SourcedFact(
                key="google_place_address",
                value={"type": "Text", "data": address},
                confidence=confidence,
                source=source,
                display_template="Google address: {value}",
                answers_preferences=["address", "location", "approach road", "access road"],
                scoring_hint={
                    "direction": "TextMatch",
                    "weight": 0.4,
                    "thresholds": [],
                },
            )
        )

    location = google_place_location(place)
    if location:
        facts.extend(
            [
                SourcedFact(
                    key="geo.latitude",
                    value={"type": "Numeric", "data": location["latitude"]},
                    confidence=min(confidence, 0.85),
                    source=source,
                    display_template="Latitude: {value}",
                    answers_preferences=["coordinates", "location", "latitude"],
                ),
                SourcedFact(
                    key="geo.longitude",
                    value={"type": "Numeric", "data": location["longitude"]},
                    confidence=min(confidence, 0.85),
                    source=source,
                    display_template="Longitude: {value}",
                    answers_preferences=["coordinates", "location", "longitude"],
                ),
            ]
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

    review_snippets = clean_string_list(place.get("review_snippets"))
    if review_snippets:
        facts.append(
            SourcedFact(
                key="google_review_snippets",
                value={"type": "Tags", "data": review_snippets},
                confidence=min(confidence, 0.8),
                source=source,
                display_template="Google review highlights: {value}",
                answers_preferences=[
                    "review highlights",
                    "resident feedback",
                    "google reviews",
                    "community signal",
                ],
                scoring_hint={
                    "direction": "TextMatch",
                    "weight": 1.2,
                    "thresholds": [],
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
        triggered_by=f"google_maps_search_fallback:{query}",
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


def fetch_google_places_nearby_text(
    input_data: dict,
    category: str,
    max_results: int = 3,
) -> List[Dict[str, Any]]:
    """Collect nearby category candidates using Google Places Text Search."""
    api_key = google_places_api_key()
    if not api_key:
        raise ValueError("GOOGLE_PLACES_API_KEY is required for Google nearby collection")
    label = nearby_category_label(category)
    base = build_place_query(input_data)
    if not base:
        return []
    origin = google_places_origin_location(input_data, api_key)
    if not origin:
        raise ValueError("Google nearby collection requires an accepted origin coordinate pair")
    query = f"{label} near {base}"
    payload = fetch_google_places_text_search(
        query,
        api_key,
        max_result_count=max_results,
        location_bias=origin,
        radius_meters=nearby_search_radius_meters(category),
    )
    places = payload.get("places") or []
    records = []
    for place in places:
        if not isinstance(place, dict) or not has_google_places_signal(place):
            continue
        if not google_place_matches_category(place, category):
            logger.info(
                "Skipping Google nearby %s result outside category: %s",
                category,
                google_place_display_name(place),
            )
            continue
        place_id = clean_text(place.get("id"))
        url = clean_text(place.get("googleMapsUri")) or google_maps_search_url(
            query, place_id or None
        )
        destination = google_place_location(place)
        records.append(
            {
                "query": query,
                "place_name": google_place_display_name(place),
                "place_id": place_id or None,
                "place_url": url,
                "distance_km": rounded_distance_km(origin, destination),
                "latitude": destination["latitude"] if destination else None,
                "longitude": destination["longitude"] if destination else None,
                "rating": parse_float(place.get("rating")),
                "review_count": parse_int(place.get("userRatingCount")),
                "primary_type": clean_text(place.get("primaryType")) or None,
                "place_types": sorted(google_place_types(place)),
                "confidence": 0.82 if place_id else 0.7,
                "fetch_source": "google_places_text_search_nearby",
            }
        )
    return records


def google_place_review_snippets(place: dict, limit: int = 5) -> List[str]:
    snippets = []
    reviews = place.get("reviews")
    if not isinstance(reviews, list):
        return snippets
    for review in reviews:
        if not isinstance(review, dict):
            continue
        text = google_review_text(review)
        if text and text not in snippets:
            snippets.append(text)
        if len(snippets) >= limit:
            break
    return snippets


def google_review_text(review: dict) -> str:
    for field in ("text", "originalText"):
        value = review.get(field)
        if isinstance(value, dict):
            text = clean_text(value.get("text"))
            if text:
                return compact_text(text)
        text = clean_text(value)
        if text:
            return compact_text(text)
    return ""


def compact_text(value: str, max_chars: int = 420) -> str:
    text = " ".join(value.split())
    if len(text) <= max_chars:
        return text
    return text[: max_chars - 3].rstrip() + "..."


def clean_string_list(value: Any) -> List[str]:
    if not isinstance(value, list):
        return []
    values = []
    for item in value:
        text = clean_text(item)
        if text and text not in values:
            values.append(text)
    return values


def nearby_search_radius_meters(category: str) -> int:
    config = nearby_category_config(category)
    if config:
        max_distance_km = parse_float(config.get("max_distance_km"))
        if max_distance_km and max_distance_km > 0:
            return int(max_distance_km * 1000)
    normalized = category.replace("-", "_").strip().lower()
    if normalized == "school":
        return 5_000
    if normalized == "metro":
        return 6_000
    if normalized == "hospital":
        return 8_000
    if normalized == "fitness":
        return 3_500
    if normalized == "eatery":
        return 3_000
    if normalized == "tech_park":
        return 15_000
    if normalized == "mall":
        return 10_000
    if normalized == "park":
        return 5_000
    return 8_000


def google_place_matches_category(place: dict, category: str) -> bool:
    config = nearby_category_config(category)
    if config:
        return google_place_matches_config(place, config)

    normalized = category.replace("-", "_").strip().lower()
    place_types = google_place_types(place)
    name = google_place_display_name(place).lower()

    if normalized == "school":
        return bool(
            place_types
            & {"school", "primary_school", "secondary_school", "preschool", "university"}
        )
    if normalized == "metro":
        return bool(
            place_types & {"subway_station", "metro_station", "light_rail_station"}
        ) or any(
            marker in name for marker in ("metro station", "namma metro", "subway station")
        )
    if normalized == "hospital":
        return bool(place_types & {"hospital", "doctor", "medical_lab", "health"})
    if normalized == "fitness":
        return bool(place_types & {"gym", "fitness_center", "sports_complex"}) or any(
            marker in name
            for marker in (
                "cult",
                "cult.fit",
                "gym",
                "fitness",
                "crossfit",
                "yoga",
                "sports club",
            )
        )
    if normalized == "eatery":
        return bool(
            place_types
            & {
                "restaurant",
                "cafe",
                "coffee_shop",
                "bakery",
                "meal_takeaway",
                "food",
            }
        )
    if normalized == "mall":
        return "shopping_mall" in place_types
    if normalized == "park":
        return bool(place_types & {"park", "garden", "national_park"})
    if normalized == "tech_park":
        if any(blocked in name for blocked in (" road", " bus stop", " metro station")):
            return False
        if place_types & {"business_center", "corporate_office"}:
            return True
        return any(
            marker in name
            for marker in (
                "tech park",
                "technology park",
                "it park",
                "itpb",
                "itpl",
                "business park",
                "tech forest",
                "office park",
            )
        )
    return True


def nearby_category_label(category: str) -> str:
    config = nearby_category_config(category)
    if config:
        return str(config.get("display_label") or category).replace("Nearby ", "").lower()
    normalized = category.replace("-", "_").strip().lower()
    labels = {
        "school": "school",
        "metro": "metro station",
        "hospital": "hospital",
        "fitness": "gym fitness",
        "eatery": "restaurant cafe",
        "tech_park": "tech park office",
    }
    return labels.get(normalized, normalized.replace("_", " "))


def google_place_matches_config(place: dict, config: Dict[str, Any]) -> bool:
    place_types = google_place_types(place)
    name = google_place_display_name(place).lower()
    for marker in config.get("name_block_markers") or []:
        if str(marker).lower() in name:
            return False
    name_marker_match = any(
        str(marker).lower() in name for marker in config.get("name_markers") or []
    )
    if config.get("require_name_marker"):
        return name_marker_match
    accepted_types = {
        str(value).replace("-", "_").strip().lower()
        for value in config.get("accepted_place_types") or []
    }
    if accepted_types and place_types & accepted_types:
        return True
    if name_marker_match:
        return True
    return bool(config.get("allow_missing_place_types")) and not place_types


def nearby_category_config(category: str) -> Optional[Dict[str, Any]]:
    normalized = category.replace("-", "_").strip().lower()
    for config in nearby_category_configs():
        aliases = {
            str(alias).replace("-", "_").strip().lower()
            for alias in config.get("category_aliases") or []
        }
        if normalized in aliases:
            return config
    return None


def nearby_category_configs() -> List[Dict[str, Any]]:
    global _NEARBY_CATEGORY_CONFIG_CACHE
    if _NEARBY_CATEGORY_CONFIG_CACHE is not None:
        return _NEARBY_CATEGORY_CONFIG_CACHE
    path = (
        Path(__file__).resolve().parents[2]
        / "app"
        / "config"
        / "dag"
        / "nearby_place_categories.json"
    )
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        _NEARBY_CATEGORY_CONFIG_CACHE = []
    else:
        _NEARBY_CATEGORY_CONFIG_CACHE = [
            config
            for config in payload.get("categories", [])
            if isinstance(config, dict) and config.get("category_aliases")
        ]
    return _NEARBY_CATEGORY_CONFIG_CACHE


def google_place_types(place: dict) -> set:
    values = set()
    primary_type = clean_text(place.get("primaryType"))
    if primary_type:
        values.add(primary_type.replace("-", "_").lower())
    types = place.get("types")
    if isinstance(types, list):
        for value in types:
            text = clean_text(value)
            if text:
                values.add(text.replace("-", "_").lower())
    return values


def google_places_origin_location(input_data: dict, api_key: str) -> Optional[Dict[str, float]]:
    seeded = parse_location_pair(input_data.get("latitude"), input_data.get("longitude"))
    if seeded:
        return seeded
    return None


def google_place_location(place: dict) -> Optional[Dict[str, float]]:
    if not isinstance(place, dict):
        return None
    location = place.get("location")
    if not isinstance(location, dict):
        return None
    return parse_location_pair(location.get("latitude"), location.get("longitude"))


def parse_location_pair(latitude: Any, longitude: Any) -> Optional[Dict[str, float]]:
    lat = parse_float(latitude)
    lon = parse_float(longitude)
    if lat is None or lon is None:
        return None
    if not (-90.0 <= lat <= 90.0 and -180.0 <= lon <= 180.0):
        return None
    return {"latitude": lat, "longitude": lon}


def rounded_distance_km(
    origin: Optional[Dict[str, float]], destination: Optional[Dict[str, float]]
) -> Optional[float]:
    if not origin or not destination:
        return None
    return round(
        haversine_km(
            origin["latitude"],
            origin["longitude"],
            destination["latitude"],
            destination["longitude"],
        ),
        2,
    )


def haversine_km(lat1: float, lon1: float, lat2: float, lon2: float) -> float:
    lat1_rad = math.radians(lat1)
    lat2_rad = math.radians(lat2)
    delta_lat = lat2_rad - lat1_rad
    delta_lon = math.radians(lon2 - lon1)
    a = (
        math.sin(delta_lat / 2.0) ** 2
        + math.cos(lat1_rad) * math.cos(lat2_rad) * math.sin(delta_lon / 2.0) ** 2
    )
    return EARTH_RADIUS_KM * 2.0 * math.asin(math.sqrt(a))


def google_place_display_name(place: dict) -> str:
    display_name = place.get("displayName")
    if isinstance(display_name, dict):
        text = clean_text(display_name.get("text"))
        if text:
            return text
    return clean_text(place.get("title") or place.get("name"))


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
