"""Source-neutral external listing collection.

The collector produces generic listing observations. Portal-specific adapters
such as MagicBricks, 99acres, Housing, or broker feeds should normalize into
this shape before Rust materializes the durable DAG assets.
"""

import json
import re
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple


def collect_external_listings(request: Dict[str, Any]) -> Dict[str, Any]:
    project_root = Path(request.get("project_root") or ".").resolve()
    observed_at = normalized_planned_at(request)
    snapshot_date = partition_values(request).get("dt") or observed_at[:10]
    records = []  # type: List[Dict[str, Any]]

    for input_data in request.get("source_entities", []):
        records.extend(listing_records_from_legacy_pricing(project_root, input_data, observed_at))

    records.sort(
        key=lambda record: (
            record.get("entity_id") or "",
            record.get("bhk") or 0,
            record.get("source_name") or "",
            record.get("source_url") or "",
        )
    )
    return {
        "snapshot_date": snapshot_date,
        "records": records,
        "source_watermarks": [
            {
                "source": "external_listing_local_pricing_adapter",
                "high_watermark": max(
                    (record["observed_at"] for record in records), default=observed_at
                ),
            }
        ],
    }


def listing_records_from_legacy_pricing(
    project_root: Path, input_data: Dict[str, Any], observed_at: str
) -> List[Dict[str, Any]]:
    entity_id = optional_string(input_data.get("entity_id"))
    society_name = optional_string(
        input_data.get("society_name")
        or input_data.get("name")
        or input_data.get("project_name")
    )
    if not entity_id or not society_name:
        return []

    node = legacy_society_node(project_root, entity_id, society_name)
    if not node:
        return []

    records = []  # type: List[Dict[str, Any]]
    locality = optional_string(input_data.get("area"))
    for fact in node.get("facts", []):
        key = str(fact.get("key") or "")
        if not key.startswith("pricing_"):
            continue
        payload = fact_value_data(fact)
        if not isinstance(payload, str):
            continue
        try:
            pricing = json.loads(payload)
        except ValueError:
            continue
        record = listing_record_from_pricing(
            entity_id=entity_id,
            project_key=optional_string(input_data.get("project_key")),
            society_name=society_name,
            locality=locality,
            pricing=pricing,
            fact=fact,
            observed_at=observed_at,
        )
        if record:
            records.append(record)
    return records


def listing_record_from_pricing(
    entity_id: str,
    project_key: Optional[str],
    society_name: str,
    locality: Optional[str],
    pricing: Dict[str, Any],
    fact: Dict[str, Any],
    observed_at: str,
) -> Optional[Dict[str, Any]]:
    bhk = bhk_number(pricing.get("bhk"))
    price_lakh = number_range_midpoint(pricing.get("price_range_lakh"), crore_to_lakh=True)
    area_sqft = number_range_midpoint(pricing.get("sqft_range"), crore_to_lakh=False)
    price_lakh_bounds = number_range_bounds(pricing.get("price_range_lakh"), crore_to_lakh=True)
    area_sqft_bounds = number_range_bounds(pricing.get("sqft_range"), crore_to_lakh=False)
    price_per_sqft_bounds = number_range_bounds(
        pricing.get("price_per_sqft"), crore_to_lakh=False
    )
    if bhk is None or price_lakh is None or area_sqft is None:
        return None

    source = fact.get("source") or {}
    source_url = optional_string(pricing.get("source_url")) or optional_string(source.get("url"))
    source_name = (
        optional_string(pricing.get("source_name"))
        or source.get("skill_id")
        or "external_listing"
    )
    return {
        "entity_id": entity_id,
        "project_key": project_key,
        "source_name": source_name,
        "source_url": source_url,
        "price": round(price_lakh * 100_000),
        "price_min": round(price_lakh_bounds[0] * 100_000) if price_lakh_bounds else None,
        "price_max": round(price_lakh_bounds[1] * 100_000) if price_lakh_bounds else None,
        "area_sqft": round(area_sqft),
        "area_sqft_min": round(area_sqft_bounds[0]) if area_sqft_bounds else None,
        "area_sqft_max": round(area_sqft_bounds[1]) if area_sqft_bounds else None,
        "price_per_sqft_min": price_per_sqft_bounds[0] if price_per_sqft_bounds else None,
        "price_per_sqft_max": price_per_sqft_bounds[1] if price_per_sqft_bounds else None,
        "price_display": optional_string(pricing.get("price_range_lakh")),
        "area_display": optional_string(pricing.get("sqft_range")),
        "price_per_sqft_display": optional_string(pricing.get("price_per_sqft")),
        "configuration": optional_string(pricing.get("bhk")) or "{}BHK".format(bhk),
        "area_type": optional_string(pricing.get("area_type")) or "unknown",
        "bhk": bhk,
        "bathrooms": optional_float(pricing.get("bathrooms")),
        "floor": optional_string(pricing.get("floor")),
        "society": society_name,
        "locality": locality,
        "observed_at": optional_string(fact.get("learned_at")) or observed_at,
    }


def legacy_society_node(
    project_root: Path, entity_id: str, society_name: str
) -> Optional[Dict[str, Any]]:
    candidates = []
    if entity_id.startswith("society:"):
        candidates.append(entity_id.split(":", 1)[1])
    candidates.append(slug(society_name))

    node_dir = project_root / "data" / "knowledge" / "nodes" / "society"
    for candidate in candidates:
        path = node_dir / "{}.json".format(candidate)
        if not path.exists():
            continue
        try:
            return json.loads(path.read_text())
        except (OSError, ValueError):
            continue
    return None


def fact_value_data(fact: Dict[str, Any]) -> Any:
    value = fact.get("value")
    if isinstance(value, dict):
        return value.get("data")
    return None


def bhk_number(value: Any) -> Optional[float]:
    match = re.search(r"\d+(?:\.\d+)?", str(value or ""))
    return float(match.group(0)) if match else None


def number_range_midpoint(value: Any, crore_to_lakh: bool) -> Optional[float]:
    bounds = number_range_bounds(value, crore_to_lakh)
    if not bounds:
        return None
    return (bounds[0] + bounds[1]) / 2.0


def number_range_bounds(value: Any, crore_to_lakh: bool) -> Optional[Tuple[float, float]]:
    text = optional_string(value)
    if not text:
        return None
    normalized = text.lower().replace(",", "")
    numbers = [float(part) for part in re.findall(r"\d+(?:\.\d+)?", normalized)]
    if not numbers:
        return None
    multiplier = 100.0 if crore_to_lakh and ("cr" in normalized or "crore" in normalized) else 1.0
    if len(numbers) == 1:
        value = numbers[0] * multiplier
        return value, value
    return numbers[0] * multiplier, numbers[-1] * multiplier


def optional_string(value: Any):
    if value is None:
        return None
    text = str(value).strip()
    return text or None


def optional_float(value: Any):
    if value is None or value == "":
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def slug(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", str(value or "").lower()).strip("-")


def partition_values(request: Dict[str, Any]) -> Dict[str, str]:
    partition = request.get("partition", {})
    return {str(key): str(value) for key, value in partition.get("parts", [])}


def normalized_planned_at(request: Dict[str, Any]) -> str:
    value = str(request.get("planned_at") or "").strip()
    return value or datetime.now(timezone.utc).isoformat()
