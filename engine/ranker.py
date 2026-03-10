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
from engine.vector_search import VectorSearch

logger = logging.getLogger(__name__)


class Ranker:
    """
    Combines PropertyScorer (dimensional scores) with optional
    VectorSearch (semantic similarity) for final ranking.

    The ranker:
    1. Scores all properties across dimensions
    2. Optionally boosts with vector similarity (for NL search)
    3. Computes final score = (1 - vector_weight) * composite + vector_weight * similarity
    4. Sorts and explains rankings
    """

    def __init__(
        self,
        scorer: PropertyScorer,
        vector_search: Optional[VectorSearch] = None,
        vector_weight: float = 0.3,
    ):
        """
        Args:
            scorer: PropertyScorer instance for dimensional scoring.
            vector_search: Optional VectorSearch for semantic boosting.
            vector_weight: Weight of vector similarity in final score (0-1).
                           Only applied when a query vector is provided.
        """
        self.scorer = scorer
        self.vector_search = vector_search
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

        # Step 2: Optionally apply vector similarity
        if query_vector is not None and self.vector_search and self.vector_search.is_loaded:
            sim_results = self.vector_search.search(query_vector, top_k=len(properties))
            sim_map = {eid: sim for eid, sim in sim_results}

            for sp in scored:
                sim = sim_map.get(sp.property_id, 0.0)
                sp.vector_similarity = sim
                # Blend: dimensional score + vector similarity
                sp.final_score = (
                    (1 - self.vector_weight) * sp.composite_score
                    + self.vector_weight * sim
                )
        else:
            # No vector search: final = composite
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
