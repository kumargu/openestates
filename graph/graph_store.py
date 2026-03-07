"""
Graph Store — file-based persistence for context graphs and event log.

Stores one JSON file per buyer at data/contexts/<buyer_id>.json.
Appends events to data/events.jsonl.
"""

import datetime
import json
import os
from typing import List

from graph.context_graph import ContextGraph


class GraphStore:
    """Persists and retrieves ContextGraph objects as JSON files."""

    def __init__(self, data_dir: str):
        self.contexts_dir = os.path.join(data_dir, "contexts")
        self.events_path = os.path.join(data_dir, "events.jsonl")
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

    def log_event(self, event_type: str, data: dict):
        """Append a structured event to the JSONL event log."""
        event = {
            "event": event_type,
            "timestamp": datetime.datetime.utcnow().isoformat() + "Z",
            **data,
        }
        with open(self.events_path, "a") as f:
            f.write(json.dumps(event) + "\n")
