"""
Ranker: combines dimensional scores with vector similarity for final ranking.

Produces ranked results with explanations for why each property ranks
where it does — core to the transparency promise.
"""

from __future__ import annotations

import logging
import time
from typing import Any, Dict, List, Optional, TYPE_CHECKING

try:
    import numpy as np
except ImportError:
    np = None  # type: ignore

if TYPE_CHECKING:
    import numpy as np

from engine.scorer import PropertyScorer
from engine.types import RankingResult, ScoredProperty, ScoringContext

logger = logging.getLogger(__name__)


class Ranker:
    """
    Combines PropertyScorer (dimensional scores) for final ranking.

    Vector search has moved to Rust (backend/src/search/semantic.rs).
    This ranker now does dimensional scoring only.

    The ranker:
    1. Scores all properties across dimensions
    2. Sorts and explains rankings
    """

    def __init__(
        self,
        scorer: PropertyScorer,
        vector_weight: float = 0.3,
    ):
        """
        Args:
            scorer: PropertyScorer instance for dimensional scoring.
            vector_weight: Weight of vector similarity in final score (0-1).
                           Only applied when a query vector is provided.
        """
        self.scorer = scorer
        self.vector_weight = vector_weight

    def rank(
        self,
        properties: List[Dict[str, Any]],
        ctx: ScoringContext,
        query_vector: Optional[np.ndarray] = None,
        limit: Optional[int] = None,
    ) -> RankingResult:
        """
        Rank properties by combined dimensional + vector score.

        Args:
            properties: List of property dicts.
            ctx: ScoringContext with user preferences and market data.
            query_vector: Optional embedding of user's NL query.
            limit: Max results to return. None = all.

        Returns:
            RankingResult with ranked, explained results.
        """
        start = time.monotonic()

        # Step 1: Score all properties across dimensions
        scored = self.scorer.score_batch(properties, ctx)

        # Vector search now handled by Rust backend (semantic.rs).
        # Python ranker scores dimensionally only.
        for sp in scored:
            sp.final_score = sp.composite_score

        # Step 3: Sort by final score descending
        scored.sort(key=lambda s: s.final_score, reverse=True)

        # Step 4: Assign ranks and generate explanations
        for i, sp in enumerate(scored):
            sp.rank = i + 1
            sp.rank_explanation = self._explain_rank(sp, scored, i)

        # Apply limit
        if limit:
            scored = scored[:limit]

        elapsed = (time.monotonic() - start) * 1000

        return RankingResult(
            query=ctx.user.natural_language_query,
            context=ctx,
            results=scored,
            total_scored=len(properties),
            scoring_time_ms=round(elapsed, 2),
        )

    def _explain_rank(
        self,
        sp: ScoredProperty,
        all_scored: List[ScoredProperty],
        index: int,
    ) -> str:
        """Generate a human-readable explanation for why this property ranks here."""
        # Find top 2 strongest and weakest dimensions
        dims_sorted = sorted(sp.dimension_scores, key=lambda d: d.score, reverse=True)
        strengths = [d for d in dims_sorted[:2] if d.score >= 0.6]
        weaknesses = [d for d in dims_sorted[-2:] if d.score < 0.4]

        parts = []

        if strengths:
            strength_names = [s.dimension.replace("_", " ") for s in strengths]
            parts.append(f"Ranks well on {' and '.join(strength_names)}")

        if weaknesses:
            weak_names = [w.dimension.replace("_", " ") for w in weaknesses]
            parts.append(f"weaker on {' and '.join(weak_names)}")

        if sp.vector_similarity and sp.vector_similarity > 0.7:
            parts.append("strong match to your search query")

        if not parts:
            parts.append("Balanced scores across all dimensions")

        explanation = ". ".join(parts) + "."
        explanation = explanation[0].upper() + explanation[1:]

        # Add relative position context
        if index == 0:
            explanation = f"Top match. {explanation}"
        elif index < 3:
            explanation = f"Near top. {explanation}"

        return explanation
