"""Load manual RedditTheme POC facts for issue #2 societies (skip-crawl path)."""

import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Set, Tuple

PROJECT_ROOT = Path(__file__).resolve().parents[2]
POC_PATH = PROJECT_ROOT / "data" / "validation" / "reddit_poc_society_signals.json"
TAXONOMY_PATH = PROJECT_ROOT / "app" / "config" / "dag" / "concern_taxonomy.json"
ADAPTER_PATH = PROJECT_ROOT / "app" / "config" / "dag" / "source_adapters" / "reddit_theme.json"


def load_concern_taxonomy_keys() -> Set[str]:
    payload = json.loads(TAXONOMY_PATH.read_text(encoding="utf-8"))
    keys: Set[str] = set()
    for bucket in payload.get("buckets", []):
        for leaf in bucket.get("leaves", []):
            fact_key = str(leaf.get("fact_key") or "").strip()
            if fact_key:
                keys.add(fact_key)
    return keys


def load_poc_payload() -> Dict[str, Any]:
    return json.loads(POC_PATH.read_text(encoding="utf-8"))


def collect_reddit_poc_fact_rows(
    snapshot_date: str,
) -> Tuple[List[Dict[str, Any]], List[Dict[str, Any]]]:
    """Return collector-shaped fact + annotation rows from the POC JSON file."""
    payload = load_poc_payload()
    adapter = json.loads(ADAPTER_PATH.read_text(encoding="utf-8"))
    allowed_keys = load_concern_taxonomy_keys()
    max_confidence = float(payload.get("max_confidence") or adapter.get("max_confidence") or 0.45)
    derived_value = str(adapter.get("derived_value") or "mentioned")
    source_type = str(payload.get("source_type") or adapter.get("source_type") or "RedditTheme")
    skill_id = str(adapter.get("skill_id") or "reddit_resident_facts")
    learned_at = datetime.now(timezone.utc).isoformat()
    input_hash = hashlib.sha256(
        json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    run_id = "collector-reddit-poc-{}".format(snapshot_date)

    facts: List[Dict[str, Any]] = []
    annotations: List[Dict[str, Any]] = []
    for entry in payload.get("facts", []):
        if not isinstance(entry, dict):
            continue
        entity_id = str(entry.get("entity_id") or "").strip()
        fact_key = str(entry.get("fact_key") or "").strip()
        if not entity_id or not fact_key:
            continue
        if fact_key not in allowed_keys:
            raise ValueError("POC fact_key not in concern_taxonomy: {}".format(fact_key))

        value = str(entry.get("value") or derived_value).strip() or derived_value
        if len(value) > 64:
            raise ValueError(
                "POC fact value too long for compliance (use derived tokens only): {}".format(
                    fact_key
                )
            )

        confidence = float(entry.get("confidence") or max_confidence)
        confidence = min(confidence, max_confidence)
        value_json = json.dumps({"type": "Text", "data": value}, separators=(",", ":"))

        facts.append(
            {
                "entity_id": entity_id,
                "fact_key": fact_key,
                "value_type": "text",
                "value_json": value_json,
                "confidence": confidence,
                "source_type": source_type,
                "source_url": entry.get("source_url"),
                "model": None,
                "skill_id": skill_id,
                "triggered_by": "reddit_poc_import",
                "learned_at": learned_at,
                "run_id": run_id,
                "input_hash": "sha256:{}".format(input_hash),
            }
        )
        annotations.append(
            {
                "entity_id": entity_id,
                "fact_key": fact_key,
                "display_template": None,
                "answers_preferences_json": "[]",
                "scoring_direction": None,
                "scoring_weight": None,
                "scoring_thresholds_json": "[]",
            }
        )

    return facts, annotations
