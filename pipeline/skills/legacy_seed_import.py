"""Import data/seed JSON as low-confidence LegacySeed skill facts."""

from __future__ import annotations

import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

PROJECT_ROOT = Path(__file__).resolve().parents[2]
ADAPTER_PATH = PROJECT_ROOT / "app" / "config" / "dag" / "source_adapters" / "legacy_seed.json"
DEFAULT_CONFIDENCE = 0.25
SOURCE_TYPE = "LegacySeed"
SKILL_ID = "legacy_seed_import"


def load_adapter() -> Dict[str, Any]:
    with ADAPTER_PATH.open(encoding="utf-8") as handle:
        return json.load(handle)


def transform_entity_id(raw_id: str, prefix: str, transform: Optional[str]) -> str:
    value = str(raw_id or "").strip()
    if transform == "strip_soc_prefix" and value.startswith("soc-"):
        value = value[4:]
    elif transform == "strip_area_prefix" and value.startswith("area-"):
        value = value[5:]
    return "{}{}".format(prefix, value)


def value_payload(value: Any, value_type: str) -> Optional[Dict[str, Any]]:
    if value is None:
        return None
    if value_type == "numeric":
        try:
            number = float(value)
        except (TypeError, ValueError):
            return None
        if not number and number != 0:
            return None
        return {"type": "Numeric", "data": number}
    if value_type == "bool":
        return {"type": "Bool", "data": bool(value)}
    if value_type == "tags":
        if not isinstance(value, list):
            return None
        tags = [str(item).strip() for item in value if str(item).strip()]
        if not tags:
            return None
        return {"type": "Tags", "data": tags}
    text = str(value).strip()
    if not text:
        return None
    return {"type": "Text", "data": text}


def collect_legacy_seed_facts(snapshot_date: str) -> Dict[str, Any]:
    adapter = load_adapter()
    confidence = float(adapter.get("default_confidence", DEFAULT_CONFIDENCE))
    skill_id = str(adapter.get("skill_id", SKILL_ID))
    source = str(adapter.get("source_id", "legacy_seed"))
    learned_at = datetime.now(timezone.utc).isoformat()
    input_hash = hashlib.sha256(
        json.dumps(adapter, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    run_id = "collector-{}-{}".format(skill_id, snapshot_date)

    facts: List[Dict[str, Any]] = []
    annotations: List[Dict[str, Any]] = []

    for mapping in adapter.get("maps", []):
        input_path = PROJECT_ROOT / str(mapping["input_path"])
        if not input_path.exists():
            continue
        records = json.loads(input_path.read_text(encoding="utf-8"))
        if not isinstance(records, list):
            continue

        entity_prefix = str(mapping.get("entity_id_prefix", ""))
        entity_field = str(mapping.get("entity_id_field", "id"))
        transform = mapping.get("entity_id_transform")

        for record in records:
            if not isinstance(record, dict):
                continue
            raw_entity_id = record.get(entity_field)
            if not raw_entity_id:
                continue
            entity_id = transform_entity_id(
                str(raw_entity_id), entity_prefix, transform
            )

            for field in mapping.get("fields", []):
                source_field = field.get("from")
                fact_key = field.get("fact_key")
                value_type = str(field.get("value_type", "text"))
                if not source_field or not fact_key:
                    continue
                if source_field not in record:
                    continue
                payload = value_payload(record.get(source_field), value_type)
                if payload is None:
                    continue

                facts.append(
                    {
                        "entity_id": entity_id,
                        "fact_key": fact_key,
                        "value_type": value_type,
                        "value_json": json.dumps(payload, separators=(",", ":")),
                        "confidence": confidence,
                        "source_type": SOURCE_TYPE,
                        "source_url": None,
                        "model": None,
                        "skill_id": skill_id,
                        "triggered_by": "bootstrap_import",
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

    return {
        "source": source,
        "snapshot_date": snapshot_date,
        "facts": facts,
        "fact_annotations": annotations,
        "source_watermarks": [
            {
                "source": source,
                "observed_at": learned_at,
                "record_count": len(facts),
            }
        ],
    }
