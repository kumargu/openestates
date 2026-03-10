"""
discover_properties — find real properties near a location using Gemini with Google Search.

Input: {"location": "Kadugodi metro station", "city": "Bengaluru", "radius_context": "within 3km"}
Output: SkillResult with property facts + new_nodes for discovered properties.

Uses Gemini 2.5 Flash with google_search grounding to discover real listings.
"""
from __future__ import annotations

import json
import logging
import os
import re
from typing import Optional

from pipeline.skills.base import BaseSkill, SkillResult, SkillCost, SourcedFact, FactSource

logger = logging.getLogger(__name__)

DISCOVER_PROMPT = """Find real residential properties (apartments/flats) for sale near {location}, {city}.
Search for actual projects currently available or recently launched.

Return a JSON array of properties. For each property, include as much real data as you can find:

{{
  "properties": [
    {{
      "project_name": "actual project name",
      "builder_name": "actual builder/developer name",
      "society_name": "society/complex name if different from project",
      "area": "micro-market / locality name",
      "sub_area": "specific sub-locality if known",
      "configurations": ["2 BHK", "3 BHK", "4 BHK"],
      "price_range_min_lakhs": 60,
      "price_range_max_lakhs": 150,
      "price_per_sqft_approx": 8500,
      "carpet_area_range": "1100-1800 sqft",
      "total_floors": 15,
      "total_units_approx": 500,
      "possession_status": "ready_to_move | under_construction | new_launch",
      "year_built_or_expected": 2024,
      "distance_from_location_km": 1.5,
      "rera_number": "RERA number if known, null otherwise",
      "key_highlights": ["2 sentences about what makes this project notable"],
      "source_url": "URL where you found this info"
    }}
  ],
  "area_identified": "canonical area name (e.g., Whitefield)",
  "nearby_landmarks": ["metro station", "IT park", etc]
}}

IMPORTANT:
- Return ONLY real, verifiable projects — no made-up data
- Include 5-8 properties (no more than 8)
- Price in lakhs (1 lakh = 100,000 INR)
- Return ONLY the JSON object, no markdown or explanation
- Use null for fields you can't verify
- Focus on projects within ~3-5 km of {location}"""


def call_gemini_discover(prompt: str) -> Optional[dict]:
    """Call Gemini with Google Search grounding for property discovery."""
    api_key = os.environ.get("GOOGLE_AI_API_KEY")
    if not api_key:
        logger.error("GOOGLE_AI_API_KEY not set")
        return None

    import urllib.request

    payload = json.dumps({
        "contents": [{"parts": [{"text": prompt}]}],
        "tools": [{"google_search": {}}],
        "generationConfig": {
            "temperature": 0.1,  # Low temp for factual accuracy
            "maxOutputTokens": 16384,  # Need enough tokens for property lists
        },
    }).encode()

    url = f"https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={api_key}"

    req = urllib.request.Request(
        url,
        data=payload,
        headers={"Content-Type": "application/json"},
    )

    try:
        with urllib.request.urlopen(req, timeout=90) as resp:
            data = json.loads(resp.read().decode())
    except Exception as e:
        logger.error("Gemini API call failed: %s", e)
        return None

    # Extract text from multipart response (thinking + answer)
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

    # Check if response was truncated
    finish_reason = ""
    try:
        finish_reason = data["candidates"][0].get("finishReason", "")
    except (KeyError, IndexError):
        pass
    if finish_reason == "MAX_TOKENS":
        logger.warning("Gemini response truncated (MAX_TOKENS) — will try to salvage partial JSON")

    # Strip markdown from each part individually first
    cleaned_parts = []
    for raw in text_parts:
        t = raw.strip()
        if t.startswith("```"):
            lines = t.split("\n")
            if lines[-1].strip() == "```":
                t = "\n".join(lines[1:-1])
            else:
                t = "\n".join(lines[1:])
        cleaned_parts.append(t.strip())

    # Try each cleaned part in reverse order (answer is usually last)
    for text in reversed(cleaned_parts):

        # Try parsing the full text as JSON first
        text = text.strip()
        try:
            result = json.loads(text)
            if isinstance(result, dict):
                result["_tokens"] = tokens
                return result
        except json.JSONDecodeError:
            pass

        # Find the outermost JSON object using brace counting
        start = text.find("{")
        if start != -1:
            depth = 0
            in_string = False
            escape = False
            for i in range(start, len(text)):
                c = text[i]
                if escape:
                    escape = False
                    continue
                if c == '\\' and in_string:
                    escape = True
                    continue
                if c == '"' and not escape:
                    in_string = not in_string
                    continue
                if in_string:
                    continue
                if c == '{':
                    depth += 1
                elif c == '}':
                    depth -= 1
                    if depth == 0:
                        try:
                            result = json.loads(text[start:i + 1])
                            if isinstance(result, dict):
                                result["_tokens"] = tokens
                                return result
                        except json.JSONDecodeError:
                            break

    # Last resort: try to salvage truncated JSON by finding complete property objects
    if finish_reason == "MAX_TOKENS":
        for text in cleaned_parts:
            # Find the properties array and extract complete objects
            idx = text.find('"properties"')
            if idx == -1:
                continue
            arr_start = text.find("[", idx)
            if arr_start == -1:
                continue
            # Find each complete {...} object in the array
            salvaged = []
            depth = 0
            obj_start = None
            in_str = False
            esc = False
            for i in range(arr_start + 1, len(text)):
                c = text[i]
                if esc:
                    esc = False
                    continue
                if c == '\\' and in_str:
                    esc = True
                    continue
                if c == '"' and not esc:
                    in_str = not in_str
                    continue
                if in_str:
                    continue
                if c == '{':
                    if depth == 0:
                        obj_start = i
                    depth += 1
                elif c == '}':
                    depth -= 1
                    if depth == 0 and obj_start is not None:
                        try:
                            obj = json.loads(text[obj_start:i + 1])
                            salvaged.append(obj)
                        except json.JSONDecodeError:
                            pass
                        obj_start = None
            if salvaged:
                logger.info("Salvaged %d complete property objects from truncated response", len(salvaged))
                return {"properties": salvaged, "area_identified": "", "nearby_landmarks": [], "_tokens": tokens}

    logger.error("No valid JSON found in Gemini response (%d parts)", len(text_parts))
    for i, t in enumerate(text_parts):
        logger.warning("Part %d (first 800 chars): %s", i, t[:800])
    return None


def slugify(text: str | None) -> str:
    """Convert text to a URL-safe slug."""
    if not text:
        return "unknown"
    text = text.lower().strip()
    text = re.sub(r'[^a-z0-9\s-]', '', text)
    text = re.sub(r'[\s-]+', '-', text)
    return text.strip('-')


class DiscoverPropertiesSkill(BaseSkill):
    skill_id = "discover_properties"
    description = "Discover real properties near a location using Gemini with Google Search"
    version = "1.0"

    def execute(self, input_data: dict) -> SkillResult:
        location = input_data.get("location", "")
        city = input_data.get("city", "Bengaluru")

        if not location:
            logger.error("discover_properties requires location")
            return SkillResult(confidence=0.0)

        prompt = DISCOVER_PROMPT.format(location=location, city=city)
        synthesis = call_gemini_discover(prompt)

        if not synthesis:
            return SkillResult(confidence=0.0, cost=SkillCost(api_calls=1))

        tokens_used = synthesis.pop("_tokens", 0)
        properties = synthesis.get("properties", [])
        area_identified = synthesis.get("area_identified", "")
        nearby_landmarks = synthesis.get("nearby_landmarks", [])

        if not properties:
            logger.warning("Gemini returned no properties for '%s'", location)
            return SkillResult(
                confidence=0.1,
                cost=SkillCost(llm_tokens=tokens_used, api_calls=1, estimated_usd=tokens_used * 0.0000001),
            )

        source = FactSource(
            source_type="Google",
            model="gemini-2.5-flash",
            skill_id=self.skill_id,
            triggered_by=input_data.get("triggered_by"),
        )

        all_facts = []
        new_nodes = []
        edges = []

        for i, prop in enumerate(properties):
            project_name = prop.get("project_name") or f"Unknown Project {i}"
            builder = prop.get("builder_name") or "Unknown Builder"
            area = prop.get("area") or area_identified or "Unknown"
            slug = slugify(project_name)
            prop_base_id = f"discovered-{slug}"

            configs = prop.get("configurations") or []
            price_min = prop.get("price_range_min_lakhs")
            price_max = prop.get("price_range_max_lakhs")
            price_psf = prop.get("price_per_sqft_approx")
            carpet_range = prop.get("carpet_area_range") or ""
            total_floors = prop.get("total_floors")
            total_units = prop.get("total_units_approx")
            possession = prop.get("possession_status") or "unknown"
            year = prop.get("year_built_or_expected")
            distance = prop.get("distance_from_location_km")
            rera = prop.get("rera_number")
            highlights = prop.get("key_highlights") or []
            if isinstance(highlights, str):
                highlights = [highlights]
            source_url = prop.get("source_url")

            # Create a new_node entry for orchestration
            node_data = {
                "project_name": project_name,
                "builder_name": builder,
                "society_name": prop.get("society_name", project_name),
                "area": area,
                "sub_area": prop.get("sub_area"),
                "city": city,
                "configurations": configs,
                "price_range_min_lakhs": price_min,
                "price_range_max_lakhs": price_max,
                "price_per_sqft_approx": price_psf,
                "carpet_area_range": carpet_range,
                "total_floors": total_floors,
                "total_units_approx": total_units,
                "possession_status": possession,
                "year_built_or_expected": year,
                "distance_from_location_km": distance,
                "rera_number": rera,
                "key_highlights": highlights,
                "source_url": source_url,
                "slug": slug,
                "base_id": prop_base_id,
            }
            new_nodes.append(node_data)

            # Create facts for each discovered property
            prop_source = FactSource(
                source_type="Google",
                model="gemini-2.5-flash",
                skill_id=self.skill_id,
                url=source_url,
                triggered_by=input_data.get("triggered_by"),
            )

            prop_facts = []
            prop_facts.append(SourcedFact(
                key="project_name", value={"type": "Text", "data": project_name},
                confidence=0.8, source=prop_source,
            ))
            prop_facts.append(SourcedFact(
                key="builder_name", value={"type": "Text", "data": builder},
                confidence=0.8, source=prop_source,
            ))
            prop_facts.append(SourcedFact(
                key="area", value={"type": "Text", "data": area},
                confidence=0.8, source=prop_source,
            ))
            prop_facts.append(SourcedFact(
                key="city", value={"type": "Text", "data": city},
                confidence=0.8, source=prop_source,
            ))
            if configs:
                prop_facts.append(SourcedFact(
                    key="configurations", value={"type": "Tags", "data": configs},
                    confidence=0.7, source=prop_source,
                    display_template="Available: {value}",
                ))
            if price_min and price_max:
                prop_facts.append(SourcedFact(
                    key="price_range_label",
                    value={"type": "Text", "data": f"₹{price_min}L - ₹{price_max}L"},
                    confidence=0.6, source=prop_source,
                    display_template="Price range: {value}",
                ))
            if price_min:
                prop_facts.append(SourcedFact(
                    key="price_min_lakhs", value={"type": "Numeric", "data": float(price_min)},
                    confidence=0.6, source=prop_source,
                ))
            if price_max:
                prop_facts.append(SourcedFact(
                    key="price_max_lakhs", value={"type": "Numeric", "data": float(price_max)},
                    confidence=0.6, source=prop_source,
                ))
            if price_psf:
                prop_facts.append(SourcedFact(
                    key="price_per_sqft", value={"type": "Numeric", "data": float(price_psf)},
                    confidence=0.6, source=prop_source,
                    display_template="₹{value}/sqft",
                ))
            if possession:
                prop_facts.append(SourcedFact(
                    key="possession_status", value={"type": "Text", "data": possession},
                    confidence=0.7, source=prop_source,
                    display_template="Possession: {value}",
                ))
            if total_floors:
                prop_facts.append(SourcedFact(
                    key="total_floors", value={"type": "Numeric", "data": float(total_floors)},
                    confidence=0.6, source=prop_source,
                ))
            if distance is not None:
                prop_facts.append(SourcedFact(
                    key="distance_from_query_km",
                    value={"type": "Numeric", "data": float(distance)},
                    confidence=0.5, source=prop_source,
                    display_template="{value} km from " + location,
                ))
            if highlights:
                prop_facts.append(SourcedFact(
                    key="highlights", value={"type": "Tags", "data": highlights},
                    confidence=0.6, source=prop_source,
                    display_template="{value}",
                ))
            if rera:
                prop_facts.append(SourcedFact(
                    key="rera_number", value={"type": "Text", "data": rera},
                    confidence=0.5, source=prop_source,
                    display_template="RERA: {value}",
                ))

            all_facts.extend(prop_facts)

            # Edge: property → area
            area_slug = slugify(area)
            edges.append({
                "from": f"property:{prop_base_id}",
                "to": f"area:{area_slug}",
                "edge_type": "LocatedIn",
            })
            # Edge: property → builder
            builder_slug = slugify(builder)
            edges.append({
                "from": f"property:{prop_base_id}",
                "to": f"builder:{builder_slug}",
                "edge_type": "BuiltBy",
            })

        # Store area context as metadata
        if area_identified:
            all_facts.append(SourcedFact(
                key="_discovery_area", value={"type": "Text", "data": area_identified},
                confidence=0.8, source=source,
            ))
        if nearby_landmarks:
            all_facts.append(SourcedFact(
                key="_discovery_landmarks", value={"type": "Tags", "data": nearby_landmarks},
                confidence=0.7, source=source,
            ))

        return SkillResult(
            facts=all_facts,
            edges=edges,
            new_nodes=new_nodes,
            confidence=0.7,
            cost=SkillCost(
                llm_tokens=tokens_used,
                api_calls=1,
                estimated_usd=tokens_used * 0.0000001,
            ),
        )

    def estimated_cost(self) -> SkillCost:
        return SkillCost(llm_tokens=3000, api_calls=1, estimated_usd=0.003)


if __name__ == "__main__":
    """Quick test: python3 -m pipeline.skills.discover_properties"""
    import sys
    logging.basicConfig(level=logging.INFO)

    location = sys.argv[1] if len(sys.argv) > 1 else "Kadugodi metro station"
    skill = DiscoverPropertiesSkill()
    result = skill.run({
        "location": location,
        "city": "Bengaluru",
        "triggered_by": "manual_test",
    })

    print(f"\n=== Discovered {len(result.new_nodes)} properties ===")
    print(f"Cost: {result.cost.llm_tokens} tokens, ${result.cost.estimated_usd:.4f}")
    for node in result.new_nodes:
        price = ""
        if node.get("price_range_min_lakhs") and node.get("price_range_max_lakhs"):
            price = f" ₹{node['price_range_min_lakhs']}L-{node['price_range_max_lakhs']}L"
        configs = ", ".join(node.get("configurations", []))
        print(f"  {node['project_name']} by {node['builder_name']} ({configs}){price}")
        if node.get("distance_from_location_km"):
            print(f"    {node['distance_from_location_km']}km away | {node.get('possession_status', '?')}")
