"""Collect administrative locality polygons from OSM for offline DAG topology."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any, Callable


POLICY_PATH = (
    Path(__file__).resolve().parents[2]
    / "app"
    / "config"
    / "dag"
    / "source_adapters"
    / "openstreetmap_locality_boundaries.json"
)


def load_collection_policy(path: Path = POLICY_PATH) -> dict[str, Any]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    collection = payload.get("collection")
    if not isinstance(collection, dict):
        raise ValueError("OSM locality adapter requires collection policy")
    return collection


def overpass_query(policy: dict[str, Any]) -> str:
    region = policy.get("broad_region")
    bbox = region.get("bbox") if isinstance(region, dict) else None
    levels = policy.get("admin_levels")
    if (
        not isinstance(bbox, list)
        or len(bbox) != 4
        or not all(isinstance(value, (int, float)) for value in bbox)
    ):
        raise ValueError("OSM locality broad_region.bbox must contain four numbers")
    if not isinstance(levels, list) or not levels or not all(
        isinstance(level, int) and 1 <= level <= 12 for level in levels
    ):
        raise ValueError("OSM locality admin_levels must contain levels 1..12")
    output_mode = str(policy.get("output_mode") or "").strip().lower()
    if output_mode != "body geom":
        raise ValueError("OSM locality output_mode must be 'body geom'")
    bounds = ",".join(str(float(value)) for value in bbox)
    level_pattern = "|".join(str(level) for level in sorted(set(levels)))
    return "\n".join(
        [
            "[out:json][timeout:60];",
            "(",
            '  relation["boundary"="administrative"]["name"]'
            f'["admin_level"~"^({level_pattern})$"]({bounds});',
            '  way["boundary"="administrative"]["name"]'
            f'["admin_level"~"^({level_pattern})$"]({bounds});',
            ");",
            "out body geom;",
        ]
    )


def collect_locality_boundaries(
    snapshot_date: str,
    source_url: str,
    fetch: Callable[[str, str], dict[str, Any]],
    policy: dict[str, Any] | None = None,
) -> dict[str, Any]:
    query = overpass_query(policy or load_collection_policy())
    payload = fetch(source_url, query)
    boundaries = locality_boundaries_from_overpass(payload)
    if not boundaries:
        raise ValueError("Overpass payload produced zero usable locality boundaries")
    return {
        "snapshot_date": snapshot_date,
        "source_url": source_url,
        "boundaries": boundaries,
        "source_watermarks": [
            {
                "source": "openstreetmap_overpass_query",
                "high_watermark": "sha256:{}".format(
                    hashlib.sha256(query.encode("utf-8")).hexdigest()
                ),
            },
            {
                "source": "openstreetmap_locality_boundary_count",
                "high_watermark": str(len(boundaries)),
            },
        ],
    }


def locality_boundaries_from_overpass(payload: dict[str, Any]) -> list[dict[str, Any]]:
    elements = payload.get("elements") if isinstance(payload, dict) else None
    if not isinstance(elements, list):
        raise ValueError("Overpass response must contain an elements list")
    records: list[dict[str, Any]] = []
    for element in elements:
        if not isinstance(element, dict):
            continue
        tags = element.get("tags") or {}
        name = _text(tags.get("name:en")) or _text(tags.get("name"))
        if not name or tags.get("boundary") != "administrative":
            continue
        geometry = _element_geometry(element)
        if geometry is None:
            continue
        element_type = _text(element.get("type")) or "element"
        element_id = element.get("id")
        if element_id is None:
            continue
        records.append(
            {
                "osm_id": "{}/{}".format(element_type, element_id),
                "name": name,
                "geometry_geojson": json.dumps(geometry, separators=(",", ":")),
                "source_url": "https://www.openstreetmap.org/{}/{}".format(
                    element_type, element_id
                ),
                "admin_level": _text(tags.get("admin_level")),
                "members": _relation_members(element),
            }
        )
    records.sort(key=lambda record: (record["name"].lower(), record["osm_id"]))
    return records


def _relation_members(element: dict[str, Any]) -> list[dict[str, Any]]:
    if element.get("type") != "relation":
        return []
    members = element.get("members")
    if not isinstance(members, list):
        return []
    preserved: list[dict[str, Any]] = []
    for member in members:
        if not isinstance(member, dict) or member.get("ref") is None:
            continue
        preserved.append(
            {
                "member_type": _text(member.get("type")) or "unknown",
                "member_ref": str(member["ref"]),
                "role": _text(member.get("role")) or "",
            }
        )
    return preserved


def _element_geometry(element: dict[str, Any]) -> dict[str, Any] | None:
    if element.get("type") == "way":
        ring = _closed_ring(element.get("geometry"))
        return {"type": "Polygon", "coordinates": [ring]} if ring else None
    members = element.get("members")
    if not isinstance(members, list):
        return None
    outers = _join_rings(
        member.get("geometry")
        for member in members
        if isinstance(member, dict) and member.get("role") in ("outer", "")
    )
    inners = _join_rings(
        member.get("geometry")
        for member in members
        if isinstance(member, dict) and member.get("role") == "inner"
    )
    if not outers:
        return None
    polygons: list[list[list[list[float]]]] = [[[point for point in outer]] for outer in outers]
    for inner in inners:
        owner = next(
            (index for index, outer in enumerate(outers) if _point_in_ring(inner[0], outer)),
            None,
        )
        if owner is not None:
            polygons[owner].append(inner)
    if len(polygons) == 1:
        return {"type": "Polygon", "coordinates": polygons[0]}
    return {"type": "MultiPolygon", "coordinates": polygons}


def _join_rings(raw_segments: Any) -> list[list[list[float]]]:
    segments = [segment for raw in raw_segments if (segment := _line(raw))]
    rings: list[list[list[float]]] = []
    while segments:
        current = segments.pop(0)
        while current[0] != current[-1]:
            match_index = next(
                (
                    index
                    for index, segment in enumerate(segments)
                    if current[-1] in (segment[0], segment[-1])
                ),
                None,
            )
            if match_index is None:
                current = []
                break
            segment = segments.pop(match_index)
            if segment[-1] == current[-1]:
                segment.reverse()
            current.extend(segment[1:])
        if len(current) >= 4 and current[0] == current[-1]:
            rings.append(current)
    return rings


def _closed_ring(raw: Any) -> list[list[float]] | None:
    line = _line(raw)
    return line if line and len(line) >= 4 and line[0] == line[-1] else None


def _line(raw: Any) -> list[list[float]] | None:
    if not isinstance(raw, list):
        return None
    line: list[list[float]] = []
    for point in raw:
        if not isinstance(point, dict):
            return None
        lat, lon = point.get("lat"), point.get("lon")
        if not isinstance(lat, (int, float)) or not isinstance(lon, (int, float)):
            return None
        line.append([float(lon), float(lat)])
    return line or None


def _point_in_ring(point: list[float], ring: list[list[float]]) -> bool:
    x, y = point
    inside = False
    for left, right in zip(ring, ring[1:]):
        x1, y1 = left
        x2, y2 = right
        if (y1 > y) != (y2 > y):
            crossing = (x2 - x1) * (y - y1) / (y2 - y1) + x1
            if x < crossing:
                inside = not inside
    return inside


def _text(value: Any) -> str | None:
    text = str(value).strip() if value is not None else ""
    return text or None
