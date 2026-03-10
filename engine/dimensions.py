"""
Individual scoring dimensions for property evaluation.

Each dimension function takes a property dict and ScoringContext,
returns a DimensionScore with a 0-1 score and human-readable explanation.
"""

from typing import Any, Dict, Optional

from engine.types import DimensionScore, ScoringContext


def _clamp(value: float, low: float = 0.0, high: float = 1.0) -> float:
    return max(low, min(high, value))


def score_value(prop: Dict[str, Any], ctx: ScoringContext) -> DimensionScore:
    """
    Value dimension: how well-priced is the property relative to market?

    Uses price_per_sqft vs area median. Below-median = higher score.
    """
    price_sqft = prop.get("price_per_sqft", 0)
    median = ctx.area_median_price_sqft

    if not median or not price_sqft:
        return DimensionScore(
            dimension="value",
            score=0.5,
            explanation="Insufficient pricing data for value comparison.",
            raw_inputs={"price_per_sqft": price_sqft, "area_median": median},
        )

    # Score: 1.0 if 20%+ below median, 0.5 at median, 0.0 if 40%+ above
    ratio = price_sqft / median
    if ratio <= 0.8:
        score = 1.0
    elif ratio >= 1.4:
        score = 0.0
    else:
        # Linear interpolation: 0.8 -> 1.0, 1.0 -> 0.5, 1.4 -> 0.0
        if ratio <= 1.0:
            score = 1.0 - (ratio - 0.8) * 2.5  # 0.8->1.0, 1.0->0.5
        else:
            score = 0.5 - (ratio - 1.0) * 1.25  # 1.0->0.5, 1.4->0.0

    score = _clamp(score)

    if score >= 0.7:
        explanation = f"Priced at {price_sqft}/sqft vs area median {median}/sqft — good value."
    elif score >= 0.4:
        explanation = f"Priced at {price_sqft}/sqft, near the area median of {median}/sqft."
    else:
        explanation = f"Priced at {price_sqft}/sqft, above area median of {median}/sqft — premium pricing."

    return DimensionScore(
        dimension="value",
        score=score,
        explanation=explanation,
        raw_inputs={"price_per_sqft": price_sqft, "area_median": median, "ratio": round(ratio, 3)},
    )


def score_commute(prop: Dict[str, Any], ctx: ScoringContext) -> DimensionScore:
    """
    Commute dimension: how convenient is the commute?

    Uses metro_distance_mins and traffic_score.
    """
    metro_mins = prop.get("metro_distance_mins")
    traffic = prop.get("traffic_score", 0.5)
    max_commute = ctx.user.max_commute_mins

    if metro_mins is None:
        return DimensionScore(
            dimension="commute",
            score=0.5,
            explanation="No commute data available.",
        )

    # Metro accessibility: 0-10min=1.0, 10-20min=0.7-0.5, 20-30min=0.5-0.2, 30+=0.1
    if metro_mins <= 10:
        metro_score = 1.0
    elif metro_mins <= 20:
        metro_score = 1.0 - (metro_mins - 10) * 0.03
    elif metro_mins <= 30:
        metro_score = 0.7 - (metro_mins - 20) * 0.05
    else:
        metro_score = max(0.1, 0.2 - (metro_mins - 30) * 0.01)

    # Traffic penalty: high traffic_score = worse commute
    traffic_penalty = traffic * 0.2  # Up to 0.2 penalty for bad traffic

    score = _clamp(metro_score - traffic_penalty)

    # Check against user preference
    if max_commute and metro_mins > max_commute:
        explanation = f"Metro {metro_mins} mins away — exceeds your {max_commute} min preference."
        score = _clamp(score * 0.5)  # Heavy penalty
    elif metro_mins <= 15:
        explanation = f"Metro just {metro_mins} mins away — excellent connectivity."
    else:
        explanation = f"Metro {metro_mins} mins away. Traffic score: {traffic:.0%}."

    return DimensionScore(
        dimension="commute",
        score=score,
        explanation=explanation,
        raw_inputs={"metro_distance_mins": metro_mins, "traffic_score": traffic},
    )


def score_society_quality(prop: Dict[str, Any], ctx: ScoringContext) -> DimensionScore:
    """
    Society quality dimension.

    Uses society_quality_score and builder_quality_score from property data.
    """
    society_q = prop.get("society_quality_score", 0.5)
    builder_q = prop.get("builder_quality_score", 0.5)

    # Weighted blend: 60% society, 40% builder
    score = _clamp(society_q * 0.6 + builder_q * 0.4)

    if score >= 0.8:
        explanation = "Premium society with strong builder reputation."
    elif score >= 0.6:
        explanation = "Good society quality with decent builder track record."
    elif score >= 0.4:
        explanation = "Average society quality. Builder reputation is mixed."
    else:
        explanation = "Below-average society quality. Consider visiting before deciding."

    return DimensionScore(
        dimension="society_quality",
        score=score,
        explanation=explanation,
        raw_inputs={"society_quality_score": society_q, "builder_quality_score": builder_q},
    )


def score_risk(prop: Dict[str, Any], ctx: ScoringContext) -> DimensionScore:
    """
    Risk dimension: lower risk = higher score.

    Considers litigation_risk, document_completeness, waterlogging_risk.
    """
    litigation = prop.get("litigation_risk", 0.0)
    doc_complete = prop.get("document_completeness_score", 0.5)
    waterlogging = prop.get("waterlogging_risk_score", 0.0)

    # Invert risk factors (lower risk = higher score)
    litigation_score = 1.0 - litigation
    waterlogging_score = 1.0 - waterlogging

    # Blend: 40% docs, 35% litigation, 25% waterlogging
    score = _clamp(doc_complete * 0.4 + litigation_score * 0.35 + waterlogging_score * 0.25)

    issues = []
    if litigation > 0.3:
        issues.append("elevated litigation risk")
    if doc_complete < 0.7:
        issues.append("incomplete documentation")
    if waterlogging > 0.5:
        issues.append("waterlogging risk area")

    if not issues:
        explanation = "Low risk profile: clean documents, no major legal or environmental concerns."
    else:
        explanation = f"Risk factors: {', '.join(issues)}."

    return DimensionScore(
        dimension="risk",
        score=score,
        explanation=explanation,
        raw_inputs={
            "litigation_risk": litigation,
            "document_completeness": doc_complete,
            "waterlogging_risk": waterlogging,
        },
    )


def score_greenery(prop: Dict[str, Any], ctx: ScoringContext) -> DimensionScore:
    """
    Greenery and environment dimension.

    Uses greenery_score, open_space_score, sunlight_score, noise_score.
    """
    greenery = prop.get("greenery_score", 0.5)
    open_space = prop.get("open_space_score", 0.5)
    sunlight = prop.get("sunlight_score", 0.5)
    noise = prop.get("noise_score", 0.5)

    # Noise is bad, so invert it
    quiet = 1.0 - noise

    # Blend: 30% greenery, 25% open space, 25% sunlight, 20% quiet
    score = _clamp(greenery * 0.3 + open_space * 0.25 + sunlight * 0.25 + quiet * 0.2)

    highlights = []
    if greenery >= 0.7:
        highlights.append("lush green surroundings")
    if sunlight >= 0.7:
        highlights.append("good natural light")
    if noise <= 0.3:
        highlights.append("peaceful and quiet")
    if open_space >= 0.7:
        highlights.append("ample open spaces")

    if highlights:
        explanation = "Environment: " + ", ".join(highlights) + "."
    elif score >= 0.5:
        explanation = "Decent environmental quality overall."
    else:
        explanation = "Limited greenery and open space. Higher noise levels."

    return DimensionScore(
        dimension="greenery",
        score=score,
        explanation=explanation,
        raw_inputs={
            "greenery_score": greenery,
            "open_space_score": open_space,
            "sunlight_score": sunlight,
            "noise_score": noise,
        },
    )


def score_resale(prop: Dict[str, Any], ctx: ScoringContext) -> DimensionScore:
    """
    Resale potential dimension.

    Uses resale_strength_score, area trend, days_on_market, interest_level.
    """
    resale = prop.get("resale_strength_score", 0.5)
    days_on_market = prop.get("days_on_market", 30)
    interest = prop.get("interest_level", "medium")
    trend = ctx.area_trend_direction

    # Interest level modifier
    interest_bonus = {"high": 0.1, "medium": 0.0, "low": -0.1}.get(interest, 0.0)

    # Trend modifier
    trend_bonus = {"up": 0.05, "stable": 0.0, "down": -0.1}.get(trend, 0.0)

    # Days on market: <14 = hot, 14-30 = normal, 30-60 = slow, 60+ = concerning
    if days_on_market < 14:
        dom_bonus = 0.05
    elif days_on_market <= 30:
        dom_bonus = 0.0
    elif days_on_market <= 60:
        dom_bonus = -0.05
    else:
        dom_bonus = -0.1

    score = _clamp(resale + interest_bonus + trend_bonus + dom_bonus)

    parts = []
    if resale >= 0.7:
        parts.append("strong resale fundamentals")
    if trend == "up":
        parts.append("area prices trending up")
    elif trend == "down":
        parts.append("area prices declining")
    if interest == "high":
        parts.append("high buyer interest")
    if days_on_market > 60:
        parts.append(f"on market for {days_on_market} days")

    explanation = "Resale outlook: " + (", ".join(parts) if parts else "average resale potential") + "."

    return DimensionScore(
        dimension="resale",
        score=score,
        explanation=explanation,
        raw_inputs={
            "resale_strength_score": resale,
            "days_on_market": days_on_market,
            "interest_level": interest,
            "area_trend": trend,
        },
    )


def score_market_activity(prop: Dict[str, Any], ctx: ScoringContext) -> DimensionScore:
    """
    Market activity dimension: how active is buyer interest?

    Uses saves_last_7d, offers_last_7d, interest_level.
    """
    saves = prop.get("saves_last_7d", 0)
    offers = prop.get("offers_last_7d", 0)
    interest = prop.get("interest_level", "medium")

    # Normalize saves: 0-5 = low, 5-15 = moderate, 15+ = high
    if saves >= 15:
        save_score = 0.9
    elif saves >= 5:
        save_score = 0.5 + (saves - 5) * 0.04
    else:
        save_score = 0.2 + saves * 0.06

    # Offers boost
    offer_boost = min(0.2, offers * 0.1)

    score = _clamp(save_score + offer_boost)

    if offers >= 2:
        explanation = f"Active market: {saves} saves and {offers} offers in the last 7 days."
    elif saves >= 10:
        explanation = f"Good interest: {saves} saves in the last 7 days."
    elif saves >= 3:
        explanation = f"Moderate activity: {saves} saves recently."
    else:
        explanation = "Limited buyer activity so far."

    return DimensionScore(
        dimension="market_activity",
        score=score,
        explanation=explanation,
        raw_inputs={"saves_last_7d": saves, "offers_last_7d": offers, "interest_level": interest},
    )


# Registry of all dimension scorers
DIMENSION_SCORERS = {
    "value": score_value,
    "commute": score_commute,
    "society_quality": score_society_quality,
    "risk": score_risk,
    "greenery": score_greenery,
    "resale": score_resale,
    "market_activity": score_market_activity,
}

# Default weights for each dimension
DEFAULT_WEIGHTS = {
    "value": 0.25,
    "commute": 0.15,
    "society_quality": 0.15,
    "risk": 0.15,
    "greenery": 0.10,
    "resale": 0.10,
    "market_activity": 0.10,
}
