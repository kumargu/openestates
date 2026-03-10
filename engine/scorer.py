"""
PropertyScorer: multi-dimensional property scoring.

Scores a property across all dimensions and produces a composite score
with explanations for transparency.
"""

import logging
from typing import Any, Dict, List

from engine.dimensions import DEFAULT_WEIGHTS, DIMENSION_SCORERS
from engine.types import DimensionScore, ScoredProperty, ScoringContext

logger = logging.getLogger(__name__)


class PropertyScorer:
    """
    Multi-dimensional property scorer.

    Evaluates properties across dimensions (value, commute, society_quality,
    risk, greenery, resale, market_activity) and produces a weighted
    composite score.

    Each dimension returns a 0-1 score with a human-readable explanation,
    supporting the transparency promise.
    """

    def __init__(self, weights: Dict[str, float] = None):
        """
        Args:
            weights: Optional dimension weight overrides.
                     Keys are dimension names, values are weights (0-1).
                     Missing dimensions use defaults.
        """
        self.weights = dict(DEFAULT_WEIGHTS)
        if weights:
            self.weights.update(weights)

        # Normalize weights to sum to 1.0
        total = sum(self.weights.values())
        if total > 0:
            self.weights = {k: v / total for k, v in self.weights.items()}

    def score_property(
        self,
        prop: Dict[str, Any],
        ctx: ScoringContext,
    ) -> ScoredProperty:
        """
        Score a single property across all dimensions.

        Args:
            prop: Property data dict (from seed JSON or storage).
            ctx: ScoringContext with user preferences and market data.

        Returns:
            ScoredProperty with dimension scores and composite score.
        """
        # Apply any weight overrides from context
        weights = dict(self.weights)
        if ctx.weights:
            weights.update(ctx.weights)
            total = sum(weights.values())
            if total > 0:
                weights = {k: v / total for k, v in weights.items()}

        # Score each dimension
        dimension_scores: List[DimensionScore] = []
        composite = 0.0

        for dim_name, scorer_fn in DIMENSION_SCORERS.items():
            try:
                dim_score = scorer_fn(prop, ctx)
                dimension_scores.append(dim_score)
                weight = weights.get(dim_name, 0.0)
                composite += dim_score.score * weight
            except Exception as e:
                logger.warning("Error scoring dimension %s for %s: %s", dim_name, prop.get("id"), e)
                dimension_scores.append(DimensionScore(
                    dimension=dim_name,
                    score=0.5,
                    explanation=f"Could not evaluate: {e}",
                ))
                composite += 0.5 * weights.get(dim_name, 0.0)

        # Clamp composite
        composite = max(0.0, min(1.0, composite))

        # Generate transparency tags
        tags = self._generate_tags(dimension_scores, prop)

        return ScoredProperty(
            property_id=prop.get("id", "unknown"),
            property_data=prop,
            dimension_scores=dimension_scores,
            composite_score=composite,
            final_score=composite,  # Will be adjusted by ranker if vector similarity is used
            transparency_tags=tags,
        )

    def score_batch(
        self,
        properties: List[Dict[str, Any]],
        ctx: ScoringContext,
    ) -> List[ScoredProperty]:
        """Score a batch of properties."""
        return [self.score_property(p, ctx) for p in properties]

    def _generate_tags(
        self,
        scores: List[DimensionScore],
        prop: Dict[str, Any],
    ) -> List[str]:
        """Generate transparency tags based on scores."""
        tags = []
        score_map = {s.dimension: s.score for s in scores}

        if score_map.get("value", 0) >= 0.7:
            tags.append("good_value")
        elif score_map.get("value", 0) <= 0.3:
            tags.append("premium_priced")

        if score_map.get("commute", 0) >= 0.8:
            tags.append("great_connectivity")

        if score_map.get("society_quality", 0) >= 0.8:
            tags.append("premium_society")

        if score_map.get("risk", 0) >= 0.9:
            tags.append("low_risk")
        elif score_map.get("risk", 0) <= 0.4:
            tags.append("elevated_risk")

        if score_map.get("greenery", 0) >= 0.7:
            tags.append("green_and_peaceful")

        if score_map.get("resale", 0) >= 0.8:
            tags.append("strong_resale")

        if score_map.get("market_activity", 0) >= 0.8:
            tags.append("high_demand")

        if prop.get("possession_status") == "ready":
            tags.append("ready_to_move")

        return tags
