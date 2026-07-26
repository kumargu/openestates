"""
Pure market-pricing fact normalization.

This module converts already-sourced marketplace pricing payloads into
SourcedFacts. It does not fetch data and does not call an LLM. Crawlers can use
this after collecting prices from deterministic sources such as MagicBricks,
Housing, 99acres, SquareYards, or NoBroker.
"""

import logging
import re
from typing import List, Optional, Tuple

from pipeline.skills.base import FactSource, SourcedFact

logger = logging.getLogger(__name__)

PRICE_MATH_TOLERANCE = 0.35


def pricing_to_facts(pricing: dict) -> List[SourcedFact]:
    """Convert sourced marketplace pricing data to self-describing SourcedFacts."""
    primary_source_name = _clean_text(pricing.get("primary_source_name")) or "Marketplace"
    primary_source_url = _primary_source_url(pricing)
    source = FactSource(
        source_type="Google",
        url=primary_source_url,
        skill_id="market_pricing_facts",
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
        bhk = cfg.get("bhk", "").lower().replace(" ", "")
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
            skill_id="market_pricing_facts",
        )
        if not source.url:
            source.url = source_url
        config_confidence = 0.75 if _is_magicbricks(source_name, source_url) else 0.65
        accepted_config_list.append(cfg.get("bhk", bhk))

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
            display_template=f"{cfg.get('bhk', bhk)}: {sqft} sq ft - Rs {price_range} Lakh",
            answers_preferences=[
                bhk, f"{bhk} price", f"{bhk} flat",
                f"under {_upper_bound_crore(price_range)} crore",
            ],
            scoring_hint={"direction": "TextMatch", "weight": 2.0},
        ))

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

    avg_psf = pricing.get("avg_price_per_sqft")
    if avg_psf and source.url and accepted_config_list:
        try:
            avg_psf_num = float(avg_psf) if isinstance(avg_psf, str) else avg_psf
            display_psf = f"Rs {avg_psf_num:,.0f}/sq ft (market rate)"
        except (ValueError, TypeError):
            avg_psf_num = avg_psf
            display_psf = f"Rs {avg_psf}/sq ft (market rate)"
        facts.append(SourcedFact(
            key="price_per_sqft",
            value={"type": "Numeric", "data": avg_psf_num},
            confidence=0.7,
            source=source,
            display_template=display_psf,
            answers_preferences=["price per sqft", "rate", "affordable", "expensive", "budget"],
            scoring_hint={"direction": "LowerIsBetter", "weight": 1.5},
        ))

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

    comparables = pricing.get("comparable_projects", [])
    if comparables and source.url and accepted_config_list:
        facts.append(SourcedFact(
            key="comparable_projects",
            value={"type": "List", "data": comparables},
            confidence=0.6,
            source=source,
            display_template=f"Similar: {', '.join(comparables[:3])}",
        ))

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
