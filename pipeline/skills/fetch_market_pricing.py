"""
fetch_market_pricing — enrich societies with per-BHK market pricing via Gemini + Google Search.

RERA gives us: total project cost, total units, carpet area (aggregate).
This skill adds: per-BHK configurations, sqft ranges, current market prices,
price appreciation, and comparable projects — all via Gemini grounded search.

Usage:
  python3 -m pipeline.skills.fetch_market_pricing --society prestige-lakeside-habitat
  python3 -m pipeline.skills.fetch_market_pricing --all --limit 10
  python3 -m pipeline.skills.fetch_market_pricing --all --limit 10 --dry-run
"""

import argparse
import json
import logging
import os
import re
import time
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, List, Optional, Tuple

from pipeline.skills.base import BaseSkill, FactSource, SkillCost, SkillResult, SourcedFact

logger = logging.getLogger(__name__)

KNOWLEDGE_DIR = Path("data/knowledge/nodes")
SOCIETY_DIR = KNOWLEDGE_DIR / "society"
CACHE_DIR = Path("data/cache/skills/market_pricing")
CACHE_TTL_DAYS = 14  # market prices change, refresh every 2 weeks
CACHE_SCHEMA_VERSION = 2
PRICE_MATH_TOLERANCE = 0.35

PRICING_PROMPT = """You are a real estate market analyst. Research current selling prices for this apartment project.

**Project:** {society_name}
**Location:** {area}, Bengaluru, India
**Builder:** {builder_name}
**RERA Status:** {project_status}
**Total Units:** {total_units}
**Completion Date:** {completion_date}
**Project Type:** {project_type}

Find the current market selling prices for each available BHK configuration (1BHK, 2BHK, 3BHK, 4BHK, 5BHK — only include configs that actually exist in this project).

Pricing source policy:
- Start with MagicBricks project/listing pages. Use MagicBricks as the primary price source when it has this project.
- If MagicBricks is unavailable or incomplete, use another marketplace such as Housing, 99acres, SquareYards, or NoBroker.
- Do not use RERA total project cost or cost per unit as the buyer-facing sale price.
- Do not use developer "Price on request" as a numeric price.
- Include the source URL for every price range. If no price source URL is available, return found=false.
- Treat prices as asking-price market signals, not legal/project truth.

For each configuration include:
- Super built-up area range in sq ft
- Current market price range in INR
- Approximate price per sq ft
- The marketplace/source URL used for this configuration

Also find:
- Overall price appreciation since launch (if known)
- 3-5 comparable/competing projects in the same area with similar price brackets
- Whether this is a new launch, under construction, ready to move, or resale market

Output ONLY a JSON object (no explanation, no markdown):
{{
  "found": true,
  "primary_source_name": "MagicBricks",
  "primary_source_url": "https://www.magicbricks.com/...",
  "source_urls": ["https://www.magicbricks.com/..."],
  "configurations": [
    {{
      "bhk": "2BHK",
      "sqft_range": "1100-1250",
      "price_range_lakh": "85-110",
      "price_per_sqft": "7700-8800",
      "source_name": "MagicBricks",
      "source_url": "https://www.magicbricks.com/..."
    }},
    {{
      "bhk": "3BHK",
      "sqft_range": "1500-1800",
      "price_range_lakh": "120-165",
      "price_per_sqft": "8000-9200",
      "source_name": "MagicBricks",
      "source_url": "https://www.magicbricks.com/..."
    }}
  ],
  "market_status": "ready_to_move",
  "price_appreciation_pct": 45,
  "appreciation_period": "since 2020 launch",
  "avg_price_per_sqft": 8500,
  "comparable_projects": [
    "Project Name 1",
    "Project Name 2",
    "Project Name 3"
  ],
  "pricing_notes": "Brief 1-2 line market insight about this project's pricing position"
}}

If you cannot find reliable pricing data with a source URL, return: {{"found": false, "reason": "..."}}
Prices should be in lakhs (1 lakh = 100,000 INR). Use current 2026 market prices, not launch prices."""


def call_gemini_pricing(prompt: str) -> Optional[dict]:
    """Call Gemini Flash with Google Search grounding."""
    api_key = os.environ.get("GOOGLE_AI_API_KEY")
    if not api_key:
        logger.error("GOOGLE_AI_API_KEY not set")
        return None

    payload = json.dumps({
        "contents": [{"parts": [{"text": prompt}]}],
        "tools": [{"google_search": {}}],
        "generationConfig": {
            "temperature": 0.2,
            "maxOutputTokens": 16384,  # Gemini 2.5 uses thinking tokens; need headroom
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

    # Extract text parts (Gemini 2.5 with grounding returns thinking + answer)
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
        logger.error("No text in Gemini response")
        return None

    # Try each text part for valid JSON (answer is usually the last part)
    for text in reversed(text_parts):
        # Strip markdown code fences
        text = re.sub(r'^```(?:json)?\s*\n?', '', text.strip())
        text = re.sub(r'\n?```\s*$', '', text.strip())
        try:
            return json.loads(text)
        except json.JSONDecodeError:
            continue

    logger.error("Could not parse JSON from Gemini response")
    return None


# ---------------------------------------------------------------------------
# Caching
# ---------------------------------------------------------------------------

def _cache_path(society_slug: str) -> Path:
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    return CACHE_DIR / f"{society_slug}.json"


def _get_cached(society_slug: str) -> Optional[dict]:
    p = _cache_path(society_slug)
    if not p.exists():
        return None
    try:
        data = json.loads(p.read_text())
        if data.get("schema_version") != CACHE_SCHEMA_VERSION:
            return None
        cached_at = datetime.fromisoformat(data.get("cached_at", "2000-01-01"))
        age_days = (datetime.now(timezone.utc) - cached_at).days
        if age_days > CACHE_TTL_DAYS:
            return None
        return data.get("result")
    except (json.JSONDecodeError, ValueError):
        return None


def _save_cache(society_slug: str, result: dict):
    p = _cache_path(society_slug)
    p.write_text(json.dumps({
        "schema_version": CACHE_SCHEMA_VERSION,
        "cached_at": datetime.now(timezone.utc).isoformat(),
        "result": result,
    }, indent=2, ensure_ascii=False))


# ---------------------------------------------------------------------------
# Node helpers
# ---------------------------------------------------------------------------

def get_fact_value(node: dict, key: str):
    """Get the latest fact value for a given key."""
    best = None
    best_version = -1
    for fact in node.get("facts", []):
        if fact["key"] == key:
            v = fact.get("version", 1)
            if v > best_version:
                best_version = v
                best = fact.get("value", {}).get("data")
    return best


def atomic_write_json(path: Path, data: dict):
    """Write JSON atomically via .tmp + rename."""
    tmp_path = path.with_suffix(".json.tmp")
    tmp_path.write_text(json.dumps(data, indent=2, ensure_ascii=False))
    os.rename(tmp_path, path)


# ---------------------------------------------------------------------------
# Pricing → SourcedFacts
# ---------------------------------------------------------------------------

def pricing_to_facts(pricing: dict) -> List[SourcedFact]:
    """Convert Gemini pricing response to self-describing SourcedFacts."""
    primary_source_name = _clean_text(pricing.get("primary_source_name")) or "Marketplace"
    primary_source_url = _primary_source_url(pricing)
    source = FactSource(
        source_type="Google",
        url=primary_source_url,
        model="gemini-2.5-flash",
        skill_id="fetch_market_pricing",
    )
    facts = []

    if primary_source_url:
        facts.append(SourcedFact(
            key="pricing_source",
            value={"type": "Object", "data": {
                "source_name": primary_source_name,
                "source_url": primary_source_url,
                "basis": "marketplace_asking_price",
            }},
            confidence=0.8 if _is_magicbricks(primary_source_name, primary_source_url) else 0.65,
            source=source,
            display_template=f"Pricing source: {primary_source_name}",
            answers_preferences=["price source", "market price source", "asking price"],
        ))

    accepted_config_list = []
    configs = pricing.get("configurations", [])
    for cfg in configs:
        bhk = cfg.get("bhk", "").lower().replace(" ", "")  # "3bhk"
        sqft = cfg.get("sqft_range", "")
        price_range = cfg.get("price_range_lakh", "")
        price_psf = cfg.get("price_per_sqft", "")
        source_name = _clean_text(cfg.get("source_name")) or primary_source_name
        source_url = _clean_text(cfg.get("source_url")) or primary_source_url

        if not bhk or not price_range or not sqft or not source_url:
            continue
        if not _price_math_is_plausible(price_range, sqft, price_psf):
            logger.warning(
                "Skipping implausible pricing config: bhk=%s price=%s sqft=%s psf=%s source=%s",
                bhk,
                price_range,
                sqft,
                price_psf,
                source_url,
            )
            continue

        config_source = FactSource(
            source_type="Google",
            url=source_url,
            model="gemini-2.5-flash",
            skill_id="fetch_market_pricing",
        )
        if not source.url:
            source.url = source_url
        config_confidence = 0.75 if _is_magicbricks(source_name, source_url) else 0.65
        accepted_config_list.append(cfg.get("bhk", bhk))

        # Per-config fact
        facts.append(SourcedFact(
            key=f"pricing_{bhk}",
            value={"type": "Object", "data": {
                "bhk": cfg.get("bhk", bhk),
                "sqft_range": sqft,
                "price_range_lakh": price_range,
                "price_per_sqft": price_psf,
                "source_name": source_name,
                "source_url": source_url,
                "basis": "marketplace_asking_price",
            }},
            confidence=config_confidence,
            source=config_source,
            display_template=f"{cfg.get('bhk', bhk)}: {sqft} sq ft — ₹{price_range} Lakh",
            answers_preferences=[
                bhk, f"{bhk} price", f"{bhk} flat",
                f"under {_upper_bound_crore(price_range)} crore",
            ],
            scoring_hint={"direction": "TextMatch", "weight": 2.0},
        ))

    # Avg price per sqft
    avg_psf = pricing.get("avg_price_per_sqft")
    if avg_psf and source.url and accepted_config_list:
        try:
            avg_psf_num = float(avg_psf) if isinstance(avg_psf, str) else avg_psf
            display_psf = f"₹{avg_psf_num:,.0f}/sq ft (market rate)"
        except (ValueError, TypeError):
            avg_psf_num = avg_psf
            display_psf = f"₹{avg_psf}/sq ft (market rate)"
        facts.append(SourcedFact(
            key="price_per_sqft",
            value={"type": "Numeric", "data": avg_psf_num},
            confidence=0.7,
            source=source,
            display_template=display_psf,
            answers_preferences=["price per sqft", "rate", "affordable", "expensive", "budget"],
            scoring_hint={"direction": "LowerIsBetter", "weight": 1.5},
        ))

    # Configurations available
    config_list = [cfg for cfg in accepted_config_list if cfg]
    if config_list:
        facts.append(SourcedFact(
            key="configurations",
            value={"type": "List", "data": config_list},
            confidence=0.8,
            source=source,
            display_template=f"Available: {', '.join(config_list)}",
            answers_preferences=[c.lower().replace(" ", "") for c in config_list],
        ))

    # Market status
    market_status = pricing.get("market_status")
    if market_status and source.url and accepted_config_list:
        facts.append(SourcedFact(
            key="market_status",
            value={"type": "Text", "data": market_status},
            confidence=0.7,
            source=source,
            display_template=f"Market: {market_status.replace('_', ' ').title()}",
            answers_preferences=_market_status_prefs(market_status),
        ))

    # Appreciation
    appreciation = pricing.get("price_appreciation_pct")
    if appreciation is not None and source.url and accepted_config_list:
        period = pricing.get("appreciation_period", "")
        facts.append(SourcedFact(
            key="price_appreciation",
            value={"type": "Object", "data": {
                "pct": appreciation,
                "period": period,
            }},
            confidence=0.6,
            source=source,
            display_template=f"{appreciation}% appreciation {period}",
            answers_preferences=["good investment", "appreciation", "value growth"],
        ))

    # Comparable projects
    comparables = pricing.get("comparable_projects", [])
    if comparables and source.url and accepted_config_list:
        facts.append(SourcedFact(
            key="comparable_projects",
            value={"type": "List", "data": comparables},
            confidence=0.6,
            source=source,
            display_template=f"Similar: {', '.join(comparables[:3])}",
        ))

    # Pricing notes
    notes = pricing.get("pricing_notes")
    if notes and source.url and accepted_config_list:
        facts.append(SourcedFact(
            key="pricing_insight",
            value={"type": "Text", "data": notes},
            confidence=0.6,
            source=source,
            display_template=notes,
        ))

    return facts


def _upper_bound_crore(price_range_lakh: str) -> str:
    """Extract upper bound and convert to crore for search matching."""
    try:
        parts = str(price_range_lakh).replace(",", "").split("-")
        upper = float(parts[-1].strip())
        if upper >= 100:
            return f"{upper / 100:.1f}"
        return f"{upper / 100:.2f}"
    except (ValueError, IndexError):
        return "?"


def _clean_text(value: object) -> str:
    """Return stripped text for simple JSON string fields."""
    return value.strip() if isinstance(value, str) else ""


def _primary_source_url(pricing: dict) -> Optional[str]:
    """Pick the best source URL from the pricing response."""
    primary = _clean_text(pricing.get("primary_source_url"))
    if primary:
        return primary
    source_urls = pricing.get("source_urls")
    if isinstance(source_urls, list):
        for value in source_urls:
            url = _clean_text(value)
            if url:
                return url
    return None


def _is_magicbricks(source_name: str, source_url: Optional[str]) -> bool:
    marker = f"{source_name} {source_url or ''}".lower()
    return "magicbricks" in marker or "magic bricks" in marker


def _parse_numeric_range(raw: object, *, crore_to_lakh: bool = False) -> Optional[Tuple[float, float]]:
    """Parse a numeric range like '259-353', '2.59-3.53 Cr', or '2,004-2,482'."""
    text = _clean_text(raw).lower().replace(",", "")
    if not text:
        return None
    numbers = [float(part) for part in re.findall(r"\d+(?:\.\d+)?", text)]
    if not numbers:
        return None
    multiplier = 100.0 if crore_to_lakh and ("cr" in text or "crore" in text) else 1.0
    if len(numbers) == 1:
        value = numbers[0] * multiplier
        return value, value
    return numbers[0] * multiplier, numbers[-1] * multiplier


def _ranges_overlap(left: Tuple[float, float], right: Tuple[float, float]) -> bool:
    return max(left[0], right[0]) <= min(left[1], right[1])


def _price_math_is_plausible(price_range_lakh: object, sqft_range: object, price_per_sqft: object) -> bool:
    """Validate that price, area, and psf ranges can describe the same units."""
    price = _parse_numeric_range(price_range_lakh, crore_to_lakh=True)
    sqft = _parse_numeric_range(sqft_range)
    if not price or not sqft or sqft[0] <= 0 or sqft[1] <= 0:
        return False

    expected_low = (price[0] * 100_000.0) / sqft[1]
    expected_high = (price[1] * 100_000.0) / sqft[0]
    expected = (
        expected_low * (1.0 - PRICE_MATH_TOLERANCE),
        expected_high * (1.0 + PRICE_MATH_TOLERANCE),
    )

    psf = _parse_numeric_range(price_per_sqft)
    if not psf:
        return True
    return _ranges_overlap(expected, psf)


def _market_status_prefs(status: str) -> List[str]:
    """Map market status to search preferences."""
    mapping = {
        "ready_to_move": ["ready to move", "ready possession", "immediate"],
        "under_construction": ["under construction", "new project"],
        "new_launch": ["new launch", "upcoming", "pre-launch"],
        "resale": ["resale", "second hand", "pre-owned"],
    }
    return mapping.get(status, [status.replace("_", " ")])


# ---------------------------------------------------------------------------
# Main enrichment logic
# ---------------------------------------------------------------------------

def enrich_society(society_slug: str, dry_run: bool = False) -> Optional[int]:
    """Enrich a single society with market pricing. Returns number of facts added."""
    node_path = SOCIETY_DIR / f"{society_slug}.json"
    if not node_path.exists():
        logger.error("Society not found: %s", society_slug)
        return None

    node = json.loads(node_path.read_text())
    name = node.get("name", society_slug)

    # Check cache
    cached = _get_cached(society_slug)
    if cached:
        pricing = cached
    else:
        # Build context from existing facts
        area = get_fact_value(node, "area") or "Bengaluru"
        builder = get_fact_value(node, "builder_name") or get_fact_value(node, "rera_promoter_name") or "Unknown"
        total_units = get_fact_value(node, "rera_total_units") or get_fact_value(node, "total_units") or "Unknown"
        completion = get_fact_value(node, "rera_completion_date") or "Unknown"
        project_type = get_fact_value(node, "rera_project_sub_type") or get_fact_value(node, "rera_project_type") or "Apartment"
        status = get_fact_value(node, "project_status") or "unknown"

        prompt = PRICING_PROMPT.format(
            society_name=name,
            area=area,
            builder_name=builder,
            total_units=total_units,
            completion_date=completion,
            project_type=project_type,
            project_status=status,
        )

        pricing = call_gemini_pricing(prompt)
        if not pricing:
            return None

        # Cache result
        _save_cache(society_slug, pricing)

    if not pricing.get("found", False):
        reason = pricing.get("reason", "no data")
        print(f"  {name:<50} NOT-FOUND: {reason}")
        return 0

    facts = pricing_to_facts(pricing)

    if dry_run:
        configs = pricing.get("configurations", [])
        config_str = ", ".join(c.get("bhk", "?") for c in configs)
        avg_psf = pricing.get("avg_price_per_sqft", "?")
        print(f"  {name:<50} WOULD-ADD ({len(facts)} facts) [{config_str}] ₹{avg_psf}/sqft")
        return len(facts)

    # Remove old pricing facts
    node["facts"] = [
        f for f in node.get("facts", [])
        if not (f.get("source", {}).get("skill_id") == "fetch_market_pricing")
    ]

    # Add new facts
    for fact in facts:
        node["facts"].append(fact.to_dict())

    atomic_write_json(node_path, node)

    configs = pricing.get("configurations", [])
    config_str = ", ".join(c.get("bhk", "?") for c in configs)
    avg_psf = pricing.get("avg_price_per_sqft", "?")
    print(f"  {name:<50} ENRICHED ({len(facts)} facts) [{config_str}] ₹{avg_psf}/sqft")
    return len(facts)


# ---------------------------------------------------------------------------
# BaseSkill wrapper for enrichment engine integration
# ---------------------------------------------------------------------------

class FetchMarketPricingSkill(BaseSkill):
    skill_id = "fetch_market_pricing"
    description = "Enrich societies with per-BHK market pricing via Gemini + Google Search"
    version = "1.0"
    output_keys = ["pricing_2bhk", "pricing_3bhk", "price_per_sqft", "configurations", "market_status"]

    def execute(self, input_data: dict) -> SkillResult:
        society_slug = input_data.get("society_slug", "")
        if not society_slug:
            logger.error("fetch_market_pricing requires society_slug")
            return SkillResult(confidence=0.0)

        node_path = SOCIETY_DIR / f"{society_slug}.json"
        if not node_path.exists():
            logger.warning("Society not found: %s", society_slug)
            return SkillResult(confidence=0.0)

        node = json.loads(node_path.read_text())
        name = node.get("name", society_slug)

        # Check cache
        cached = _get_cached(society_slug)
        if cached:
            pricing = cached
        else:
            area = get_fact_value(node, "area") or "Bengaluru"
            builder = get_fact_value(node, "builder_name") or get_fact_value(node, "rera_promoter_name") or "Unknown"
            total_units = get_fact_value(node, "rera_total_units") or get_fact_value(node, "total_units") or "Unknown"
            completion = get_fact_value(node, "rera_completion_date") or "Unknown"
            project_type = get_fact_value(node, "rera_project_sub_type") or "Apartment"
            status = get_fact_value(node, "project_status") or "unknown"

            prompt = PRICING_PROMPT.format(
                society_name=name, area=area, builder_name=builder,
                total_units=total_units, completion_date=completion,
                project_type=project_type, project_status=status,
            )

            pricing = call_gemini_pricing(prompt)
            if not pricing:
                return SkillResult(confidence=0.0, cost=SkillCost(api_calls=1))
            _save_cache(society_slug, pricing)

        if not pricing.get("found", False):
            return SkillResult(confidence=0.0, cost=SkillCost(api_calls=1))

        facts = pricing_to_facts(pricing)
        return SkillResult(
            facts=facts,
            confidence=0.7,
            cost=SkillCost(api_calls=1, estimated_usd=0.001),
        )

    def estimated_cost(self) -> SkillCost:
        return SkillCost(llm_tokens=3000, api_calls=1, estimated_usd=0.002)


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main():
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )

    parser = argparse.ArgumentParser(description="Enrich societies with market pricing via Gemini")
    parser.add_argument("--society", type=str, help="Single society slug to enrich")
    parser.add_argument("--all", action="store_true", help="Enrich all societies")
    parser.add_argument("--limit", type=int, default=10, help="Max societies to enrich in --all mode")
    parser.add_argument("--dry-run", action="store_true", help="Log actions without writing files")
    parser.add_argument("--skip-cached", action="store_true", help="Skip societies that already have pricing facts")

    args = parser.parse_args()

    if not args.society and not args.all:
        parser.error("Must specify --society <slug> or --all")

    if args.society:
        print(f"=== Market Pricing: {args.society} ===\n")
        result = enrich_society(args.society, dry_run=args.dry_run)
        if result is None:
            print("Failed.")
        else:
            print(f"\nDone: {result} facts {'would be ' if args.dry_run else ''}added.")
        return

    # --all mode
    print(f"=== Market Pricing Enrichment (limit={args.limit}) ===\n")

    # Prioritize: RERA-rooted societies with Apartment type, sorted by name
    candidates = []
    for f in sorted(SOCIETY_DIR.glob("*.json")):
        try:
            node = json.loads(f.read_text())
        except (json.JSONDecodeError, OSError):
            continue

        # Skip non-apartment projects (plots, villas, etc.)
        project_type = get_fact_value(node, "rera_project_sub_type") or ""
        if project_type and "apartment" not in project_type.lower() and "housing" not in project_type.lower():
            continue

        # Optionally skip if already has pricing
        if args.skip_cached:
            has_pricing = any(
                fact.get("source", {}).get("skill_id") == "fetch_market_pricing"
                for fact in node.get("facts", [])
            )
            if has_pricing:
                continue

        candidates.append(f.stem)

    candidates = candidates[:args.limit]
    print(f"  {len(candidates)} candidates\n")

    total_facts = 0
    enriched = 0
    failed = 0

    for slug in candidates:
        result = enrich_society(slug, dry_run=args.dry_run)
        if result is None:
            failed += 1
        elif result > 0:
            total_facts += result
            enriched += 1

        # Rate limit: 2 seconds between Gemini calls
        if not args.dry_run:
            time.sleep(2)

    print(f"\nDone:")
    print(f"  Enriched: {enriched}")
    print(f"  Failed: {failed}")
    print(f"  Total facts: {total_facts}")


if __name__ == "__main__":
    main()
