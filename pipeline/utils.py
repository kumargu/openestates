"""Shared utilities for the OpenEstates pipeline."""

import json
import logging
import os
import urllib.request

logger = logging.getLogger(__name__)


def reload_backend(base_url: str = "http://localhost:4000") -> bool:
    """POST to /api/admin/reload-knowledge to hot-reload the graph.

    Returns True if reload succeeded, False otherwise.
    """
    token = os.environ.get("ADMIN_TOKEN", "dev")
    try:
        req = urllib.request.Request(
            f"{base_url}/api/admin/reload-knowledge",
            method="POST",
            headers={"X-Admin-Token": token, "Content-Type": "application/json"},
            data=b"{}",
        )
        resp = urllib.request.urlopen(req, timeout=5)
        data = json.loads(resp.read().decode())
        nodes = data.get("nodes_loaded", "?")
        facts = data.get("facts_loaded", "?")
        logger.info("Backend reloaded: %s nodes, %s facts", nodes, facts)
        return True
    except Exception as e:
        logger.info("Backend reload skipped (may not be running): %s", e)
        return False
