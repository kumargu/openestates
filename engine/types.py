"""
Type definitions for the scoring and ranking engine.

All types use pydantic for validation and serialization.
"""

from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field


class UserPreferences(BaseModel):
    """User's stated preferences for property search."""
    budget_min: Optional[float] = None
    budget_max: Optional[float] = None
    bhk: Optional[int] = None
    preferred_areas: List[str] = []
    property_type: str = "apartment"
    listing_type: Optional[str] = None  # resale, new_launch, rental
    max_commute_mins: Optional[int] = None
    commute_destination: Optional[str] = None
    # Soft preferences (0-1 importance)
    greenery_importance: float = 0.5
    quiet_importance: float = 0.5
    resale_importance: float = 0.5
    society_quality_importance: float = 0.5
    safety_importance: float = 0.5
    natural_language_query: Optional[str] = None


class ScoringContext(BaseModel):
    """
    Full context for scoring a property.

    Combines user preferences with area/market data needed
    for contextual scoring.
    """
    user: UserPreferences
    area_median_price_sqft: Optional[float] = None
    area_trend_direction: Optional[str] = None
    city_median_price_sqft: Optional[float] = None
    # Dimension weight overrides (0-1). If None, uses defaults.
    weights: Dict[str, float] = {}


class DimensionScore(BaseModel):
    """Score for a single scoring dimension."""
    dimension: str
    score: float = Field(ge=0.0, le=1.0)
    explanation: str = ""
    raw_inputs: Dict[str, Any] = {}


class ScoredProperty(BaseModel):
    """A property with all its dimension scores and final composite score."""
    property_id: str
    property_data: Dict[str, Any] = {}
    dimension_scores: List[DimensionScore] = []
    composite_score: float = Field(default=0.0, ge=0.0, le=1.0)
    vector_similarity: Optional[float] = None
    final_score: float = Field(default=0.0, ge=0.0, le=1.0)
    rank: int = 0
    rank_explanation: str = ""
    transparency_tags: List[str] = []


class RankingResult(BaseModel):
    """Result of ranking a set of properties."""
    query: Optional[str] = None
    context: ScoringContext
    results: List[ScoredProperty] = []
    total_scored: int = 0
    scoring_time_ms: float = 0.0
