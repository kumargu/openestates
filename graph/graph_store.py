"""
Graph Store — file-based persistence for context graphs.

Stores one JSON file per buyer at data/contexts/<buyer_id>.json.
"""

import json
import os

from graph.context_graph import ContextGraph


class GraphStore:
    """Persists and retrieves ContextGraph objects as JSON files."""

    def __init__(self, data_dir: str):
        self.contexts_dir = os.path.join(data_dir, "contexts")
        os.makedirs(self.contexts_dir, exist_ok=True)

    def _path(self, buyer_id: str) -> str:
        return os.path.join(self.contexts_dir, f"{buyer_id}.json")

    def load(self, buyer_id: str) -> ContextGraph:
        """Load context graph for a buyer. Returns empty graph if none exists."""
        path = self._path(buyer_id)
        if os.path.exists(path):
            with open(path) as f:
                return ContextGraph.from_dict(json.load(f))
        return ContextGraph(buyer_id=buyer_id)

    def save(self, graph: ContextGraph):
        """Save context graph to disk."""
        path = self._path(graph.buyer_id)
        with open(path, "w") as f:
            json.dump(graph.to_dict(), f, indent=2)

    def exists(self, buyer_id: str) -> bool:
        return os.path.exists(self._path(buyer_id))
