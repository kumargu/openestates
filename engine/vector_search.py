"""
VectorSearch: approximate nearest neighbor search over embeddings.

Uses numpy for now. Can be replaced with FAISS for production scale.
"""

from __future__ import annotations

import io
import logging
from typing import Any, List, Optional, Tuple, TYPE_CHECKING

try:
    import numpy as np
    HAS_NUMPY = True
except ImportError:
    HAS_NUMPY = False
    np = None  # type: ignore

if TYPE_CHECKING:
    import numpy as np

from pipeline.storage.client import StorageClient

logger = logging.getLogger(__name__)


class VectorSearch:
    """
    ANN search over stored embeddings.

    Loads all embeddings for an entity type into memory and performs
    cosine similarity search. Suitable for datasets up to ~100K vectors.
    For larger datasets, swap in FAISS.
    """

    def __init__(self, storage: StorageClient):
        if not HAS_NUMPY:
            raise ImportError("numpy is required for VectorSearch. Install it with: pip install numpy")
        self.storage = storage
        self._vectors = None  # np.ndarray when loaded
        self._ids: List[str] = []
        self._loaded_type: Optional[str] = None

    @property
    def is_loaded(self) -> bool:
        return self._vectors is not None and len(self._ids) > 0

    @property
    def size(self) -> int:
        return len(self._ids)

    def load(self, entity_type: str) -> int:
        """
        Load all embeddings for an entity type into memory.

        Args:
            entity_type: e.g., "properties", "societies"

        Returns:
            Number of vectors loaded.
        """
        prefix = f"embeddings/{entity_type}/"
        keys = self.storage.list_keys(prefix)

        vectors = []
        ids = []

        for key in keys:
            if not key.endswith(".npy"):
                continue
            raw = self.storage.get(key)
            if raw is None:
                continue
            try:
                buf = io.BytesIO(raw)
                vec = np.load(buf)
                vectors.append(vec.flatten())
                entity_id = key.split("/")[-1].replace(".npy", "")
                ids.append(entity_id)
            except Exception as e:
                logger.warning("Failed to load embedding %s: %s", key, e)

        if vectors:
            self._vectors = np.stack(vectors).astype(np.float32)
            # Pre-normalize for cosine similarity
            norms = np.linalg.norm(self._vectors, axis=1, keepdims=True)
            norms = np.maximum(norms, 1e-10)
            self._vectors = self._vectors / norms
        else:
            self._vectors = None

        self._ids = ids
        self._loaded_type = entity_type

        logger.info("Loaded %d vectors for %s", len(ids), entity_type)
        return len(ids)

    def search(
        self,
        query_vector: np.ndarray,
        top_k: int = 10,
        threshold: Optional[float] = None,
    ) -> List[Tuple[str, float]]:
        """
        Find nearest neighbors to query_vector.

        Args:
            query_vector: The query embedding.
            top_k: Maximum number of results.
            threshold: Minimum similarity score (0-1). None = no threshold.

        Returns:
            List of (entity_id, similarity) tuples, sorted by similarity descending.
        """
        if not self.is_loaded:
            logger.warning("No vectors loaded. Call load() first.")
            return []

        # Normalize query
        query = query_vector.flatten().astype(np.float32)
        norm = np.linalg.norm(query)
        if norm > 0:
            query = query / norm

        # Cosine similarity (vectors are pre-normalized)
        similarities = self._vectors @ query

        # Sort by similarity
        top_indices = np.argsort(similarities)[::-1][:top_k]

        results = []
        for idx in top_indices:
            sim = float(similarities[idx])
            if threshold is not None and sim < threshold:
                break
            results.append((self._ids[idx], sim))

        return results

    def search_by_id(
        self,
        entity_id: str,
        top_k: int = 10,
        exclude_self: bool = True,
    ) -> List[Tuple[str, float]]:
        """
        Find entities similar to a given entity by ID.

        Args:
            entity_id: ID of the query entity.
            top_k: Number of results.
            exclude_self: Whether to exclude the query entity from results.

        Returns:
            List of (entity_id, similarity) tuples.
        """
        if entity_id not in self._ids:
            logger.warning("Entity %s not found in index", entity_id)
            return []

        idx = self._ids.index(entity_id)
        query_vec = self._vectors[idx]

        results = self.search(query_vec, top_k=top_k + (1 if exclude_self else 0))

        if exclude_self:
            results = [(eid, sim) for eid, sim in results if eid != entity_id]

        return results[:top_k]
