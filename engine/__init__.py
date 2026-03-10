"""
OpenEstates scoring and ranking engine.

Provides multi-dimensional property scoring, ranking with explanations,
and vector similarity search for context-based discovery.
"""

from engine.types import ScoringContext, ScoredProperty, RankingResult, UserPreferences
from engine.scorer import PropertyScorer
from engine.ranker import Ranker

__all__ = [
    "ScoringContext",
    "ScoredProperty",
    "RankingResult",
    "UserPreferences",
    "PropertyScorer",
    "Ranker",
]
