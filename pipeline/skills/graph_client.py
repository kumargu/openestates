"""
GraphClient — pushes skill results into the Rust knowledge graph via HTTP API.

This is the bridge between Python skill execution and the Rust backend.
"""

import json
import logging
from typing import Any, Dict, List, Optional
from urllib.request import Request, urlopen
from urllib.parse import quote

logger = logging.getLogger(__name__)

DEFAULT_API_BASE = "http://localhost:4000"


class GraphClient:
    """Client for interacting with the knowledge graph API."""

    def __init__(self, api_base: str = DEFAULT_API_BASE):
        self.api_base = api_base.rstrip("/")

    def get_stats(self) -> dict:
        """Get graph stats."""
        return self._get("/api/knowledge/stats")

    def get_node(self, node_id: str) -> Optional[dict]:
        """Get a node by ID."""
        try:
            return self._get(f"/api/knowledge/nodes/{quote(node_id, safe='')}")
        except Exception:
            return None

    def list_nodes(self, node_type: Optional[str] = None) -> List[dict]:
        """List nodes, optionally filtered by type."""
        path = "/api/knowledge/nodes"
        if node_type:
            path += f"?type={node_type}"
        return self._get(path)

    def add_facts(self, node_id: str, facts: List[dict]) -> dict:
        """Push facts to a node via POST /api/knowledge/nodes/{id}/facts."""
        path = f"/api/knowledge/nodes/{quote(node_id, safe='')}/facts"
        return self._post(path, facts)

    def push_skill_result(self, node_id: str, skill_result) -> Optional[dict]:
        """Push a SkillResult's facts to the graph."""
        if not skill_result.facts:
            logger.info("No facts to push for %s", node_id)
            return None

        facts = [f.to_dict() for f in skill_result.facts]

        try:
            result = self.add_facts(node_id, facts)
            logger.info("Pushed %d facts to %s: %s", len(facts), node_id, result)
            return result
        except Exception as e:
            logger.error("Failed to push facts to %s: %s", node_id, e)
            return None

    def _get(self, path: str):
        url = f"{self.api_base}{path}"
        req = Request(url, headers={"Accept": "application/json"})
        with urlopen(req, timeout=10) as resp:
            return json.loads(resp.read().decode())

    def _post(self, path: str, data) -> dict:
        url = f"{self.api_base}{path}"
        payload = json.dumps(data, default=str).encode()
        req = Request(
            url,
            data=payload,
            headers={
                "Content-Type": "application/json",
                "Accept": "application/json",
            },
            method="POST",
        )
        with urlopen(req, timeout=10) as resp:
            return json.loads(resp.read().decode())
