"""Collect OSM structures within an already resolved society polygon."""

from __future__ import annotations

import hashlib
import json
import math
import os
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Dict, Iterable, List, Optional, Sequence, Tuple

from pipeline.sources.osm_access_corridors import relation_polygons
from pipeline.sources.overpass_transport import OverpassTransport


Coordinate = Tuple[float, float]  # longitude, latitude
Bounds = Tuple[float, float, float, float]  # south, west, north, east
Polygon = Tuple[List[Coordinate], List[List[Coordinate]]]

CONFIG_PATH = (
    Path(__file__).resolve().parents[2]
    / "app"
    / "config"
    / "dag"
    / "osm_society_structures.json"
)


@dataclass(frozen=True)
class StructureTile:
    tile_id: str
    bounds: Bounds
    query_bounds: Bounds


def load_structure_config(path: Path = CONFIG_PATH) -> Dict[str, Any]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    collector = payload.get("collector")
    if not isinstance(collector, dict):
        raise ValueError("OSM society structures require collector configuration")
    return collector


def collect_society_structures(
    society_entity_id: str,
    boundary_osm_id: str,
    boundary_geometry_geojson: str,
    planned_at: str,
    fetch: Optional[Callable[[str, str], Dict[str, Any]]] = None,
    collector: Optional[Dict[str, Any]] = None,
) -> Dict[str, Any]:
    """Collect resumable tile-shaped structure observations for one society."""
    config = dict(collector or load_structure_config())
    polygons = parse_polygons(boundary_geometry_geojson)
    if not polygons:
        raise ValueError("society structure collection requires polygon geometry")
    tiles = plan_structure_tiles(polygons, boundary_osm_id, config)
    source_url = _collector_url(config)
    transport = OverpassTransport(fetch=fetch, policy=config.get("transport_policy"))
    records: Dict[str, Dict[str, Any]] = {}
    coverage = []
    consecutive_failures = 0
    stopped = False

    for tile in tiles:
        query = structure_overpass_query(
            tile.query_bounds,
            int(config.get("query_timeout_seconds") or 45),
        )
        query_hash = hashlib.sha256(query.encode("utf-8")).hexdigest()
        if stopped:
            coverage.append(
                _coverage_row(
                    tile,
                    query,
                    query_hash,
                    "deferred_after_circuit_breaker",
                    retryable=True,
                )
            )
            continue
        try:
            payload = transport.request(source_url, query)
            elements = payload.get("elements")
            if not isinstance(elements, list):
                raise ValueError("Overpass response must contain an elements list")
            accepted = 0
            for element in elements:
                record = structure_record(
                    society_entity_id,
                    boundary_osm_id,
                    polygons,
                    element,
                    tile.tile_id,
                    planned_at,
                    config,
                )
                if record is None:
                    continue
                accepted += 1
                osm_id = record["osm_id"]
                existing = records.get(osm_id)
                if existing is None:
                    records[osm_id] = record
                elif tile.tile_id not in existing["coverage_tile_ids"]:
                    existing["coverage_tile_ids"].append(tile.tile_id)
            coverage.append(
                _coverage_row(
                    tile,
                    query,
                    query_hash,
                    "complete" if accepted else "complete_empty",
                    element_count=len(elements),
                    accepted_structure_count=accepted,
                    response_json_bytes=len(
                        json.dumps(payload, separators=(",", ":")).encode("utf-8")
                    ),
                )
            )
            consecutive_failures = 0
        except Exception as error:
            consecutive_failures += 1
            coverage.append(
                _coverage_row(
                    tile,
                    query,
                    query_hash,
                    "failed",
                    error=f"{type(error).__name__}: {error}",
                    retryable=True,
                )
            )
            stopped = consecutive_failures >= int(
                config.get("max_consecutive_failed_tiles") or 2
            )

    ordered_records = sorted(records.values(), key=lambda row: row["osm_id"])
    for record in ordered_records:
        record["coverage_tile_ids"].sort()
    complete = all(row["status"].startswith("complete") for row in coverage)
    boundary_hash = hashlib.sha256(
        canonical_geometry(boundary_geometry_geojson).encode("utf-8")
    ).hexdigest()
    return {
        "snapshot_date": planned_at[:10],
        "collection_status": "complete" if complete else "incomplete",
        "society_entity_id": society_entity_id,
        "boundary_osm_id": boundary_osm_id,
        "boundary_geometry_hash": boundary_hash,
        "planner_version": 1,
        "coverage": coverage,
        "records": ordered_records,
        "source_watermarks": [
            {
                "source": str(
                    config.get("source_id") or "openstreetmap_society_structures"
                ),
                "high_watermark": (
                    f"boundary_sha256:{boundary_hash};tiles={len(coverage)};"
                    f"structures={len(ordered_records)}"
                ),
            }
        ],
    }


def plan_structure_tiles(
    polygons: Sequence[Polygon],
    boundary_osm_id: str,
    collector: Dict[str, Any],
) -> List[StructureTile]:
    south, west, north, east = polygon_bounds(polygons)
    center_latitude = (south + north) / 2.0
    tile_meters = float(collector.get("tile_size_meters") or 450.0)
    overlap_meters = float(collector.get("tile_overlap_meters") or 20.0)
    latitude_step = tile_meters / 111_320.0
    longitude_step = tile_meters / max(
        1.0, 111_320.0 * math.cos(math.radians(center_latitude))
    )
    latitude_overlap = overlap_meters / 111_320.0
    longitude_overlap = overlap_meters / max(
        1.0, 111_320.0 * math.cos(math.radians(center_latitude))
    )
    rows = max(1, math.ceil((north - south) / latitude_step))
    columns = max(1, math.ceil((east - west) / longitude_step))
    tiles = []
    boundary_slug = boundary_osm_id.replace("/", "-")
    for row in range(rows):
        tile_south = south + row * latitude_step
        tile_north = min(north, tile_south + latitude_step)
        for column in range(columns):
            tile_west = west + column * longitude_step
            tile_east = min(east, tile_west + longitude_step)
            bounds = (tile_south, tile_west, tile_north, tile_east)
            if not polygon_intersects_bounds(polygons, bounds):
                continue
            tiles.append(
                StructureTile(
                    tile_id=f"{boundary_slug}:r{row:03d}:c{column:03d}",
                    bounds=bounds,
                    query_bounds=(
                        tile_south - latitude_overlap,
                        tile_west - longitude_overlap,
                        tile_north + latitude_overlap,
                        tile_east + longitude_overlap,
                    ),
                )
            )
    maximum = int(collector.get("max_tiles_per_society") or 64)
    if len(tiles) > maximum:
        raise ValueError(
            f"society polygon requires {len(tiles)} structure tiles; maximum is {maximum}"
        )
    return tiles


def structure_overpass_query(bounds: Bounds, timeout_seconds: int) -> str:
    south, west, north, east = bounds
    bbox = f"{south:.7f},{west:.7f},{north:.7f},{east:.7f}"
    return (
        f"[out:json][timeout:{timeout_seconds}];\n(\n"
        f'  way["building"]({bbox});\n'
        f'  relation["type"="multipolygon"]["building"]({bbox});\n'
        f'  way["building:part"]({bbox});\n'
        f'  relation["type"="multipolygon"]["building:part"]({bbox});\n'
        ");\nout body center geom;"
    )


def structure_record(
    society_entity_id: str,
    boundary_osm_id: str,
    society_polygons: Sequence[Polygon],
    element: Any,
    tile_id: str,
    planned_at: str,
    collector: Dict[str, Any],
) -> Optional[Dict[str, Any]]:
    if not isinstance(element, dict) or element.get("type") not in {"way", "relation"}:
        return None
    tags = element.get("tags") if isinstance(element.get("tags"), dict) else {}
    if not tags.get("building") and not tags.get("building:part"):
        return None
    geometry = element_geometry(element)
    if geometry is None:
        return None
    structure_polygons = geometry_polygons(geometry)
    if not geometries_intersect(society_polygons, structure_polygons):
        return None
    osm_id = f"{element['type']}/{element.get('id')}"
    selected_tags = {
        key: str(tags[key])
        for key in collector.get("structure_tag_keys") or []
        if tags.get(key) not in (None, "")
    }
    longitude, latitude = geometry_center(structure_polygons)
    return {
        "society_entity_id": society_entity_id,
        "boundary_osm_id": boundary_osm_id,
        "osm_id": osm_id,
        "name": _optional_text(tags.get("name")),
        "ref": _optional_text(tags.get("ref")),
        "building": _optional_text(tags.get("building")),
        "building_part": _optional_text(tags.get("building:part")),
        "building_levels": _optional_float(tags.get("building:levels")),
        "height_meters": _height_meters(tags.get("height")),
        "latitude": latitude,
        "longitude": longitude,
        "geometry_geojson": json.dumps(geometry, separators=(",", ":"), sort_keys=True),
        "source_tags": selected_tags,
        "source_url": f"https://www.openstreetmap.org/{osm_id}",
        "confidence": float(collector.get("confidence") or 0.8),
        "fetched_at": planned_at,
        "fetch_source": str(
            collector.get("fetch_source") or "overpass_society_structure_snapshot"
        ),
        "coverage_tile_ids": [tile_id],
    }


def parse_polygons(geometry_geojson: str) -> List[Polygon]:
    return geometry_polygons(json.loads(geometry_geojson))


def geometry_polygons(geometry: Dict[str, Any]) -> List[Polygon]:
    kind = geometry.get("type")
    values = geometry.get("coordinates")
    if kind == "Polygon":
        values = [values]
    if kind not in {"Polygon", "MultiPolygon"} or not isinstance(values, list):
        return []
    polygons = []
    for polygon in values:
        if not isinstance(polygon, list) or not polygon:
            continue
        rings = [_coordinate_ring(ring) for ring in polygon]
        rings = [ring for ring in rings if len(ring) >= 4]
        if rings:
            polygons.append((rings[0], rings[1:]))
    return polygons


def element_geometry(element: Dict[str, Any]) -> Optional[Dict[str, Any]]:
    if element.get("type") == "way":
        ring = _coordinate_ring(
            [
                [point.get("lon"), point.get("lat")]
                for point in element.get("geometry") or []
                if isinstance(point, dict)
            ]
        )
        if len(ring) < 4 or ring[0] != ring[-1]:
            return None
        return {"type": "Polygon", "coordinates": [[list(point) for point in ring]]}
    polygons = relation_polygons(element)
    if not polygons:
        return None
    return (
        {"type": "Polygon", "coordinates": polygons[0]}
        if len(polygons) == 1
        else {"type": "MultiPolygon", "coordinates": polygons}
    )


def polygon_bounds(polygons: Sequence[Polygon]) -> Bounds:
    points = [point for outer, _holes in polygons for point in outer]
    if not points:
        raise ValueError("polygon has no exterior coordinates")
    return (
        min(point[1] for point in points),
        min(point[0] for point in points),
        max(point[1] for point in points),
        max(point[0] for point in points),
    )


def polygon_intersects_bounds(polygons: Sequence[Polygon], bounds: Bounds) -> bool:
    south, west, north, east = bounds
    box = [
        (west, south),
        (east, south),
        (east, north),
        (west, north),
        (west, south),
    ]
    return geometries_intersect(polygons, [(box, [])])


def geometries_intersect(left: Sequence[Polygon], right: Sequence[Polygon]) -> bool:
    for left_outer, left_holes in left:
        for right_outer, right_holes in right:
            if any(_point_in_polygon(point, left_outer, left_holes) for point in right_outer):
                return True
            if any(_point_in_polygon(point, right_outer, right_holes) for point in left_outer):
                return True
            if _rings_intersect(left_outer, right_outer):
                return True
    return False


def geometry_center(polygons: Sequence[Polygon]) -> Coordinate:
    points = [point for outer, _holes in polygons for point in outer[:-1]]
    return (
        sum(point[0] for point in points) / len(points),
        sum(point[1] for point in points) / len(points),
    )


def canonical_geometry(value: str) -> str:
    return json.dumps(json.loads(value), separators=(",", ":"), sort_keys=True)


def _coverage_row(
    tile: StructureTile,
    query: str,
    query_hash: str,
    status: str,
    *,
    element_count: int = 0,
    accepted_structure_count: int = 0,
    response_json_bytes: int = 0,
    error: Optional[str] = None,
    retryable: bool = False,
) -> Dict[str, Any]:
    row = {
        "tile_id": tile.tile_id,
        "bounds": list(tile.bounds),
        "query_bounds": list(tile.query_bounds),
        "query": query,
        "query_hash": query_hash,
        "status": status,
        "element_count": element_count,
        "accepted_structure_count": accepted_structure_count,
        "response_json_bytes": response_json_bytes,
        "retryable": retryable,
    }
    if error:
        row["error"] = error
    return row


def _collector_url(collector: Dict[str, Any]) -> str:
    env_key = _optional_text(collector.get("overpass_url_env"))
    if env_key and _optional_text(os.environ.get(env_key)):
        return str(os.environ[env_key])
    return str(collector.get("default_overpass_url"))


def _coordinate_ring(values: Iterable[Any]) -> List[Coordinate]:
    ring = []
    for value in values:
        if not isinstance(value, (list, tuple)) or len(value) < 2:
            continue
        try:
            point = (float(value[0]), float(value[1]))
        except (TypeError, ValueError):
            continue
        if not ring or ring[-1] != point:
            ring.append(point)
    return ring


def _point_in_polygon(
    point: Coordinate,
    outer: Sequence[Coordinate],
    holes: Sequence[Sequence[Coordinate]],
) -> bool:
    return _point_in_ring(point, outer) and not any(
        _point_in_ring(point, hole) for hole in holes
    )


def _point_in_ring(point: Coordinate, ring: Sequence[Coordinate]) -> bool:
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


def _rings_intersect(
    left: Sequence[Coordinate], right: Sequence[Coordinate]
) -> bool:
    return any(
        _segments_intersect(a, b, c, d)
        for a, b in zip(left, left[1:])
        for c, d in zip(right, right[1:])
    )


def _segments_intersect(
    a: Coordinate, b: Coordinate, c: Coordinate, d: Coordinate
) -> bool:
    def side(p: Coordinate, q: Coordinate, r: Coordinate) -> float:
        return (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])

    ab_c, ab_d = side(a, b, c), side(a, b, d)
    cd_a, cd_b = side(c, d, a), side(c, d, b)
    if ab_c * ab_d > 0.0 or cd_a * cd_b > 0.0:
        return False
    return not (
        max(a[0], b[0]) < min(c[0], d[0])
        or max(c[0], d[0]) < min(a[0], b[0])
        or max(a[1], b[1]) < min(c[1], d[1])
        or max(c[1], d[1]) < min(a[1], b[1])
    )


def _optional_text(value: Any) -> Optional[str]:
    text = str(value).strip() if value is not None else ""
    return text or None


def _optional_float(value: Any) -> Optional[float]:
    try:
        return float(value) if value not in (None, "") else None
    except (TypeError, ValueError):
        return None


def _height_meters(value: Any) -> Optional[float]:
    text = _optional_text(value)
    if not text:
        return None
    match = re.fullmatch(r"\s*([0-9]+(?:\.[0-9]+)?)\s*(?:m|meter|meters|metre|metres)?\s*", text.lower())
    return _optional_float(match.group(1)) if match else None
