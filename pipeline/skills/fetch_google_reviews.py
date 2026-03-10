"""
fetch_google_reviews — fetch Google review intelligence for a society using Gemini with grounded search.

Input: {"society_name": "Prestige Lakeside Habitat", "area": "Whitefield", "city": "Bengaluru"}
Output: SkillResult with facts about Google rating, review excerpts, sentiment, common themes.

Uses Gemini Flash with Google Search grounding — no Google Places API needed.
"""

import json
import logging
import os
import urllib.request
from typing import Optional

from pipeline.skills.base import BaseSkill, SkillResult, SkillCost, SourcedFact, FactSource

logger = logging.getLogger(__name__)

REVIEW_PROMPT = """Research "{society_name}" apartment complex in {area}, {city} on Google Maps.

Find the overall Google rating and approximate number of reviews. Summarize what residents commonly praise and commonly complain about. Do NOT quote reviews verbatim — paraphrase and synthesize themes.

Also find a representative exterior/aerial photo URL of this apartment complex from the web (Google Images, housing sites, builder website). The URL should be a direct image link (ending in .jpg, .png, .webp, or from a CDN).

Output ONLY a JSON object (no explanation, no markdown):
{{
  "google_rating": 4.2,
  "review_count": 150,
  "positive_themes": ["well maintained common areas", "good security", "nice amenities"],
  "negative_themes": ["traffic congestion nearby", "parking issues"],
  "common_topics": ["maintenance", "security", "parking", "location"],
  "sentiment_summary": "A 1-2 sentence synthesis of overall resident feeling.",
  "photo_url": "https://example.com/photo.jpg",
  "found": true
}}

Use null for fields you cannot determine. If you cannot find any review data, return: {{"found": false}}"""


def call_gemini(prompt: str, api_key: str) -> Optional[dict]:
    """Call Gemini Flash API with Google Search grounding and return parsed JSON response."""
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

    # Extract text from Gemini response — concatenate all text parts
    # (Gemini 2.5 with grounding may return thinking + answer in separate parts)
    text_parts = []
    try:
        candidates = data.get("candidates", [])
        if not candidates:
            logger.error("Gemini returned no candidates")
            return None
        parts = candidates[0].get("content", {}).get("parts", [])
        for part in parts:
            if "text" in part:
                text_parts.append(part["text"])
    except (KeyError, IndexError) as e:
        logger.error("Failed to extract text from Gemini response: %s", e)
        return None

    if not text_parts:
        # Debug: log the part types we got
        try:
            parts = data.get("candidates", [{}])[0].get("content", {}).get("parts", [])
            part_types = [list(p.keys()) for p in parts]
            logger.error("No text in Gemini response. Part types: %s", part_types)
        except Exception:
            logger.error("No text in Gemini response (could not inspect parts)")
        return None

    # Extract token usage
    usage = data.get("usageMetadata", {})
    token_count = usage.get("totalTokenCount", 0)

    # Try each text part for valid JSON (answer is usually the last part)
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
                result["_tokens"] = token_count
                return result
            except json.JSONDecodeError:
                continue

    logger.error("No valid JSON found in Gemini response parts (%d parts)", len(text_parts))
    return None


def verify_image_url(url: str, timeout: int = 5) -> bool:
    """HEAD-check that a URL returns 200 with an image content type."""
    if not url or not url.startswith("http"):
        return False
    try:
        req = urllib.request.Request(url, method="HEAD")
        req.add_header("User-Agent", "Mozilla/5.0")
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            ct = resp.headers.get("Content-Type", "")
            return resp.status == 200 and ("image/" in ct or ct == "application/octet-stream")
    except Exception:
        return False


class FetchGoogleReviewsSkill(BaseSkill):
    skill_id = "fetch_google_reviews"
    description = "Fetch Google review intelligence for a society using Gemini with grounded search"
    version = "1.2"  # v1.2: verify photo URLs before storing

    def execute(self, input_data: dict) -> SkillResult:
        society_name = input_data.get("society_name", "")
        area = input_data.get("area", "")
        city = input_data.get("city", "Bengaluru")

        if not society_name or not area:
            logger.error("fetch_google_reviews requires society_name and area")
            return SkillResult(confidence=0.0)

        api_key = os.environ.get("GOOGLE_AI_API_KEY")
        if not api_key:
            logger.error("GOOGLE_AI_API_KEY not set")
            return SkillResult(confidence=0.0)

        # Call Gemini with grounded search
        prompt = REVIEW_PROMPT.format(
            society_name=society_name,
            area=area,
            city=city,
        )

        response = call_gemini(prompt, api_key)
        if not response:
            return SkillResult(confidence=0.0, cost=SkillCost(api_calls=1))

        tokens_used = response.pop("_tokens", 0)

        # If Gemini says not found
        if response.get("found") is False:
            logger.info("No Google reviews found for %s", society_name)
            return SkillResult(
                confidence=0.1,
                cost=SkillCost(
                    llm_tokens=tokens_used,
                    api_calls=1,
                    estimated_usd=tokens_used * 0.0000001,  # Gemini Flash pricing
                ),
            )

        # Build source
        maps_url = response.get("maps_url")
        source = FactSource(
            source_type="Google",
            url=maps_url,
            model="gemini-2.5-flash",
            skill_id=self.skill_id,
            triggered_by=input_data.get("triggered_by"),
        )

        facts = []

        # --- Numeric facts ---

        google_rating = response.get("google_rating")
        if google_rating is not None:
            facts.append(SourcedFact(
                key="google_rating",
                value={"type": "Numeric", "data": google_rating},
                confidence=0.8,
                source=source,
                display_template="Google Rating: {value}/5",
                answers_preferences=["good reviews", "well rated", "popular", "resident feedback"],
                scoring_hint={"direction": "HigherIsBetter", "weight": 2.0, "thresholds": [4.0, 3.5]},
            ))

        review_count = response.get("review_count")
        if review_count is not None:
            facts.append(SourcedFact(
                key="google_review_count",
                value={"type": "Numeric", "data": review_count},
                confidence=0.8,
                source=source,
                display_template="{value} Google reviews",
                answers_preferences=["popular", "well known"],
                scoring_hint={"direction": "HigherIsBetter", "weight": 1.0, "thresholds": [50.0, 20.0]},
            ))

        # --- Text facts ---

        sentiment_summary = response.get("sentiment_summary")
        if sentiment_summary:
            facts.append(SourcedFact(
                key="google_sentiment",
                value={"type": "Text", "data": sentiment_summary},
                confidence=0.8,
                source=source,
                display_template="Google Reviews: {value}",
                answers_preferences=["good reviews", "resident feedback"],
                scoring_hint={"direction": "TextMatch", "weight": 1.5},
            ))

        # --- Tag facts ---

        top_positives = response.get("positive_themes") or response.get("top_positive_reviews")
        if top_positives and isinstance(top_positives, list):
            facts.append(SourcedFact(
                key="google_top_positives",
                value={"type": "Tags", "data": top_positives},
                confidence=0.8,
                source=source,
                display_template="Praised for: {value}",
            ))

        top_negatives = response.get("negative_themes") or response.get("top_negative_reviews")
        if top_negatives and isinstance(top_negatives, list):
            facts.append(SourcedFact(
                key="google_top_negatives",
                value={"type": "Tags", "data": top_negatives},
                confidence=0.8,
                source=source,
                display_template="Criticized for: {value}",
            ))

        photo_url = response.get("photo_url")
        if photo_url and isinstance(photo_url, str) and photo_url.startswith("http") and verify_image_url(photo_url):
            facts.append(SourcedFact(
                key="photo_url",
                value={"type": "Text", "data": photo_url},
                confidence=0.7,
                source=source,
                display_template="Photo available",
            ))

        common_themes = response.get("common_topics") or response.get("common_themes")
        if common_themes and isinstance(common_themes, list):
            facts.append(SourcedFact(
                key="google_common_themes",
                value={"type": "Tags", "data": common_themes},
                confidence=0.8,
                source=source,
                display_template="Review themes: {value}",
            ))

        # Compute confidence based on review count
        confidence = 0.8
        if review_count and review_count > 100:
            confidence = 0.9
        elif review_count and review_count < 10:
            confidence = 0.5

        return SkillResult(
            facts=facts,
            confidence=confidence,
            cost=SkillCost(
                llm_tokens=tokens_used,
                api_calls=1,
                estimated_usd=tokens_used * 0.0000001,  # Gemini Flash pricing (~$0.10/1M tokens)
            ),
        )

    def estimated_cost(self) -> SkillCost:
        return SkillCost(llm_tokens=1500, api_calls=1, estimated_usd=0.001)


if __name__ == "__main__":
    """Quick test: python3 -m pipeline.skills.fetch_google_reviews"""
    import sys
    logging.basicConfig(level=logging.INFO)

    skill = FetchGoogleReviewsSkill()
    result = skill.run({
        "society_name": "Prestige Lakeside Habitat",
        "area": "Whitefield",
        "city": "Bengaluru",
        "triggered_by": "manual_test",
    })

    print(f"\n=== Results: {len(result.facts)} facts, confidence={result.confidence:.2f} ===")
    print(f"Cost: {result.cost.llm_tokens} tokens, ${result.cost.estimated_usd:.6f}")
    print(f"Cached: {result.cached}")
    for fact in result.facts:
        template = fact.display_template or "{value}"
        display_val = fact.value.get("data", "")
        if isinstance(display_val, list):
            display_val = ", ".join(str(v) for v in display_val[:3])
        rendered = template.replace("{value}", str(display_val))
        print(f"  [{fact.key}] {rendered} (conf={fact.confidence}, src={fact.source.source_type})")
