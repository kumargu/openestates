"""Collect sourced OSM boundaries, entrances, and public approach corridors."""

from __future__ import annotations

import hashlib
import json
import math
import re
from typing import Any, Callable, Dict, Iterable, List, Optional, Tuple

Coordinate = Tuple[float, float]


def collect_society_access_records(
    subjects: List[Dict[str, Any]],
    fetch: Callable[[str, str], Dict[str, Any]],
    source_url: str,
    collector: Dict[str, Any],
    planned_at: str,
) -> Tuple[List[Dict[str, Any]], List[str]]:
    """Collect one independently sourced society-access record per subject."""
    records: List[Dict[str, Any]] = []
    query_hashes: List[str] = []
    failures: List[str] = []
    padding_meters = float(collector.get("bbox_padding_meters") or 1_650.0)
    timeout_seconds = int(collector.get("query_timeout_seconds") or 60)
    highway_values = _string_list(collector.get("eligible_highway_values"))
    boundary_landuse_values = _string_list(collector.get("boundary_landuse_values"))
    for subject in subjects:
        query = society_access_overpass_query(
            _padded_bbox([subject], padding_meters),
            highway_values,
            boundary_landuse_values,
            timeout_seconds,
        )
        query_hashes.append(hashlib.sha256(query.encode("utf-8")).hexdigest())
        try:
            record = society_access_record(
                subject,
                fetch(source_url, query),
                query,
                collector,
                planned_at,
            )
            if record is not None:
                records.append(record)
        except Exception as error:
            failures.append(f"{subject['entity_id']}: {error}")
    if failures and (
        len(failures) == len(subjects)
        or not bool(collector.get("allow_partial_subject_failures", False))
    ):
        raise ValueError(
            "OSM society access collection failed for "
            f"{len(failures)} of {len(subjects)} subjects: {'; '.join(failures[:5])}"
        )
    records.sort(key=lambda row: row["entity_id"])
    return records, query_hashes


def society_access_overpass_query(
    bbox: Tuple[float, float, float, float],
    highway_values: List[str],
    boundary_landuse_values: List[str],
    timeout_seconds: int,
) -> str:
    """Build one local query whose result families do not depend on each other."""
    road_pattern = "|".join(sorted(set(highway_values))) or (
        "motorway|trunk|primary|secondary|tertiary|unclassified|residential|"
        "service|living_street"
    )
    boundary_pattern = "|".join(sorted(set(boundary_landuse_values)))
    south, west, north, east = bbox
    boundary_query = (
        f'  way["landuse"~"^({boundary_pattern})$"]["name"]'
        f"({south:.7f},{west:.7f},{north:.7f},{east:.7f});\n"
        if boundary_pattern else ""
    )
    return (
        f"[out:json][timeout:{timeout_seconds}];\n(\n"
        f'  way["highway"~"^({road_pattern})$"]'
        f"({south:.7f},{west:.7f},{north:.7f},{east:.7f});\n"
        f"{boundary_query}"
        f'  node["barrier"="gate"]({south:.7f},{west:.7f},{north:.7f},{east:.7f});\n'
        f'  node["entrance"~"^(main|yes)$"]'
        f"({south:.7f},{west:.7f},{north:.7f},{east:.7f});\n"
        ");\nout tags center geom;"
    )


def society_access_record(
    subject: Dict[str, Any],
    payload: Dict[str, Any],
    query: str,
    collector: Dict[str, Any],
    planned_at: str,
) -> Optional[Dict[str, Any]]:
    """Project only source geometry; absent evidence remains absent."""
    origin = (float(subject["latitude"]), float(subject["longitude"]))
    boundary = _society_boundary(payload, str(subject.get("name") or ""))
    boundary_points = boundary[3] if boundary else []
    roads = _eligible_roads(payload, collector)
    road = _select_frontage_road(subject, roads, boundary_points, origin, collector)
    entrance = _select_entrance(subject, payload, boundary_points, road, collector)
    if boundary is None and road is None and entrance is None:
        return None
    access_id = hashlib.sha256(
        f"{subject['entity_id']}:{boundary and boundary[0]}:{road and road['id']}:"
        f"{entrance and entrance['id']}".encode("utf-8")
    ).hexdigest()[:16]
    record: Dict[str, Any] = {
        "entity_id": subject["entity_id"],
        "project_key": subject.get("project_key"),
        "query": query,
        "access_id": access_id,
        "subject_latitude": origin[0],
        "subject_longitude": origin[1],
        "confidence": float(collector.get("confidence") or 0.78),
        "fetched_at": planned_at,
        "fetch_source": str(
            collector.get("fetch_source") or "overpass_society_access_snapshot"
        ),
    }
    if boundary:
        record.update(
            boundary_name=boundary[1],
            boundary_way_id=boundary[0],
            boundary_geometry_geojson=boundary[2],
        )
    if road:
        focus = entrance["coordinate"] if entrance else origin
        source_points = road["points"]
        direction = _road_direction(road["tags"])
        legal_points = list(reversed(source_points)) if direction == "oneway_reverse" else source_points
        bounded = _bounded_line(
            legal_points,
            focus,
            float(collector.get("max_corridor_meters") or 1_500.0),
        )
        if direction == "two_way":
            bounded = _orient_two_way(bounded, focus)
        record.update(
            approach_road_name=road["name"],
            approach_way_id=road["id"],
            approach_distance_meters=_line_length_m(bounded),
            approach_geometry_geojson=_line_geojson(bounded),
            approach_source_geometry_geojson=_line_geojson(source_points),
            approach_direction=direction,
            approach_association_method=road["association_method"],
            source_url=f"https://www.openstreetmap.org/way/{road['id']}",
        )
    if entrance:
        entrance_coordinate = entrance["coordinate"]
        record.update(
            entrance_id=entrance["id"],
            entrance_latitude=entrance_coordinate[0],
            entrance_longitude=entrance_coordinate[1],
            entrance_status=entrance["status"],
            entrance_association_method=entrance["association_method"],
            entrance_source_url=entrance["source_url"],
        )
    return record


def _eligible_roads(payload: Dict[str, Any], collector: Dict[str, Any]) -> List[Dict[str, Any]]:
    allowed = set(_string_list(collector.get("eligible_highway_values")))
    denied_highways = set(_string_list(collector.get("denied_highway_values")) or [
        "footway", "path", "steps", "pedestrian", "cycleway", "bridleway",
    ])
    denied_access = set(_string_list(collector.get("denied_access_values")) or ["no", "private"])
    roads = []
    for element in payload.get("elements") or []:
        if not isinstance(element, dict) or element.get("type") != "way":
            continue
        tags = element.get("tags") if isinstance(element.get("tags"), dict) else {}
        highway = str(tags.get("highway") or "").lower()
        if not highway or highway in denied_highways or (allowed and highway not in allowed):
            continue
        access_values = {
            str(tags.get(key) or "").lower()
            for key in ("access", "vehicle", "motor_vehicle", "motorcar")
        }
        if access_values & denied_access:
            continue
        points = _element_points(element)
        if len(points) < 2:
            continue
        name = (
            _optional_string(tags.get("short_name"))
            or _optional_string(tags.get("alt_name"))
            or _optional_string(tags.get("name"))
            or str(tags.get("ref") or "Public road")
        )
        roads.append({"id": str(element.get("id") or ""), "name": name, "points": points, "tags": tags})
    return roads


def _select_frontage_road(
    subject: Dict[str, Any],
    roads: List[Dict[str, Any]],
    boundary: List[Coordinate],
    origin: Coordinate,
    collector: Dict[str, Any],
) -> Optional[Dict[str, Any]]:
    address_road = _frontage_road_from_address(
        str(subject.get("address") or ""),
        _string_list(collector.get("road_suffixes")) or ["Road", "Street", "Marg"],
    )
    reference = boundary or [origin]
    candidates = []
    for road in roads:
        distance = _polylines_distance_m(road["points"], reference)
        matched = bool(address_road and _road_names_match(address_road, road["name"]))
        candidates.append((not matched, distance, road["name"], road))
    if not candidates:
        return None
    not_address_match, distance, _name, selected = min(candidates)
    if distance > float(collector.get("max_frontage_distance_meters") or 120.0):
        return None
    selected = dict(selected)
    selected["association_method"] = "address_match" if not not_address_match else "boundary_proximity"
    return selected


def _select_entrance(
    subject: Dict[str, Any],
    payload: Dict[str, Any],
    boundary: List[Coordinate],
    road: Optional[Dict[str, Any]],
    collector: Dict[str, Any],
) -> Optional[Dict[str, Any]]:
    official = _official_entrance(subject)
    if official:
        return official
    if not boundary or road is None:
        return None
    boundary_limit = float(collector.get("max_entrance_boundary_distance_meters") or 35.0)
    road_limit = float(collector.get("max_entrance_road_distance_meters") or 30.0)
    candidates = []
    for element in payload.get("elements") or []:
        if not isinstance(element, dict) or element.get("type") != "node":
            continue
        tags = element.get("tags") if isinstance(element.get("tags"), dict) else {}
        if tags.get("barrier") != "gate" and tags.get("entrance") not in {"main", "yes"}:
            continue
        coordinate = _coordinate(element)
        if coordinate is None:
            continue
        boundary_distance = _point_to_line_m(coordinate, boundary)
        road_distance = _point_to_line_m(coordinate, road["points"])
        if boundary_distance <= boundary_limit and road_distance <= road_limit:
            candidates.append((boundary_distance + road_distance, str(element.get("id") or ""), coordinate))
    if not candidates:
        return None
    _distance, node_id, coordinate = min(candidates)
    return {
        "id": f"node/{node_id}",
        "coordinate": coordinate,
        "status": "inferred",
        "association_method": "osm_gate_boundary_public_road",
        "source_url": f"https://www.openstreetmap.org/node/{node_id}",
    }


def _official_entrance(subject: Dict[str, Any]) -> Optional[Dict[str, Any]]:
    if not (subject.get("entrance_reviewed") or subject.get("entrance_official")):
        return None
    try:
        coordinate = (float(subject["entrance_latitude"]), float(subject["entrance_longitude"]))
    except (KeyError, TypeError, ValueError):
        return None
    source = _optional_string(subject.get("entrance_source_url"))
    if not _valid_coordinate(coordinate) or not source:
        return None
    return {
        "id": str(subject.get("entrance_id") or "reviewed"),
        "coordinate": coordinate,
        "status": "verified",
        "association_method": "reviewed_coordinates",
        "source_url": source,
    }


def _society_boundary(payload: Dict[str, Any], subject_name: str) -> Optional[Tuple[str, str, str, List[Coordinate]]]:
    normalized_subject = _normalized_name(subject_name)
    if not normalized_subject:
        return None
    for element in payload.get("elements") or []:
        if not isinstance(element, dict) or element.get("type") != "way":
            continue
        tags = element.get("tags") if isinstance(element.get("tags"), dict) else {}
        name = _optional_string(tags.get("name"))
        points = _element_points(element)
        if not name or _normalized_name(name) != normalized_subject:
            continue
        if len(points) < 4 or points[0] != points[-1]:
            continue
        geometry = json.dumps(
            {"type": "Polygon", "coordinates": [[[lon, lat] for lat, lon in points]]},
            separators=(",", ":"), sort_keys=True,
        )
        return str(element.get("id") or ""), name, geometry, points
    return None


def _road_direction(tags: Dict[str, Any]) -> str:
    value = str(tags.get("oneway") or "").lower()
    if value == "-1":
        return "oneway_reverse"
    if value in {"yes", "1", "true"}:
        return "oneway_forward"
    return "two_way"


def _orient_two_way(points: List[Coordinate], focus: Coordinate) -> List[Coordinate]:
    if len(points) < 2:
        return points
    along = _nearest_along_distance(points, focus)
    return points if along >= _line_length_m(points) - along else list(reversed(points))


def _bounded_line(points: List[Coordinate], focus: Coordinate, cap_meters: float) -> List[Coordinate]:
    if len(points) < 2 or _line_length_m(points) <= cap_meters:
        return list(points)
    cumulative = _cumulative_distances(points)
    focus_distance = _nearest_along_distance(points, focus)
    start = max(0.0, min(focus_distance - cap_meters / 2.0, cumulative[-1] - cap_meters))
    end = min(cumulative[-1], start + cap_meters)
    output = [_point_at_distance(points, cumulative, start)]
    output.extend(point for point, distance in zip(points, cumulative) if start < distance < end)
    output.append(_point_at_distance(points, cumulative, end))
    return _dedupe_adjacent(output)


def _nearest_along_distance(points: List[Coordinate], focus: Coordinate) -> float:
    cumulative = _cumulative_distances(points)
    return min(
        ((_distance_m(point, focus), cumulative[index]) for index, point in enumerate(points)),
        default=(math.inf, 0.0),
    )[1]


def _point_at_distance(points: List[Coordinate], cumulative: List[float], distance: float) -> Coordinate:
    for index in range(1, len(points)):
        if cumulative[index] < distance:
            continue
        span = cumulative[index] - cumulative[index - 1]
        if span <= 0:
            return points[index]
        ratio = (distance - cumulative[index - 1]) / span
        left, right = points[index - 1], points[index]
        return (left[0] + (right[0] - left[0]) * ratio, left[1] + (right[1] - left[1]) * ratio)
    return points[-1]


def _polylines_distance_m(left: List[Coordinate], right: List[Coordinate]) -> float:
    return min(
        min(_point_to_line_m(point, right) for point in left),
        min(_point_to_line_m(point, left) for point in right),
    )


def _point_to_line_m(point: Coordinate, line: List[Coordinate]) -> float:
    if not line:
        return math.inf
    if len(line) == 1:
        return _distance_m(point, line[0])
    latitude_scale = 110_570.0
    longitude_scale = 111_320.0 * max(0.1, math.cos(math.radians(point[0])))
    best = math.inf
    for left, right in zip(line, line[1:]):
        ax = (left[1] - point[1]) * longitude_scale
        ay = (left[0] - point[0]) * latitude_scale
        bx = (right[1] - point[1]) * longitude_scale
        by = (right[0] - point[0]) * latitude_scale
        dx, dy = bx - ax, by - ay
        denominator = dx * dx + dy * dy
        ratio = 0.0 if denominator == 0 else max(0.0, min(1.0, -(ax * dx + ay * dy) / denominator))
        best = min(best, math.hypot(ax + ratio * dx, ay + ratio * dy))
    return best


def _line_geojson(points: List[Coordinate]) -> str:
    return json.dumps(
        {"type": "LineString", "coordinates": [[lon, lat] for lat, lon in points]},
        separators=(",", ":"), sort_keys=True,
    )


def _line_length_m(points: List[Coordinate]) -> float:
    return sum(_distance_m(left, right) for left, right in zip(points, points[1:]))


def _cumulative_distances(points: List[Coordinate]) -> List[float]:
    output = [0.0]
    for left, right in zip(points, points[1:]):
        output.append(output[-1] + _distance_m(left, right))
    return output


def _element_points(element: Dict[str, Any]) -> List[Coordinate]:
    return [
        coordinate for point in element.get("geometry") or []
        if isinstance(point, dict) and (coordinate := _coordinate(point)) is not None
    ]


def _frontage_road_from_address(address: str, suffixes: List[str]) -> Optional[str]:
    suffix_pattern = "|".join(re.escape(value) for value in suffixes if value)
    if not address.strip() or not suffix_pattern:
        return None
    pattern = re.compile(
        rf"(?:^|,|;)\s*([A-Za-z0-9][A-Za-z0-9 .'-]*?\s(?:{suffix_pattern}))(?=\s*(?:,|;|$))",
        re.IGNORECASE,
    )
    matches = [match.group(1).strip() for match in pattern.finditer(address)]
    return matches[-1] if matches else None


def _road_names_match(left: str, right: str) -> bool:
    left_words = _road_words(left)
    right_words = _road_words(right)
    if not left_words or not right_words:
        return False
    left_core = left_words[:-1] if left_words[-1] in {"road", "street", "marg"} else left_words
    right_core = right_words[:-1] if right_words[-1] in {"road", "street", "marg"} else right_words
    return (
        "".join(left_core) == "".join(right_core)
        or "".join(left_core) == "".join(word[0] for word in right_core)
        or "".join(right_core) == "".join(word[0] for word in left_core)
    )


def _road_words(value: str) -> List[str]:
    return re.findall(r"[a-z0-9]+", value.lower())


def _normalized_name(value: str) -> str:
    return " ".join(re.findall(r"[a-z0-9]+", value.lower()))


def _padded_bbox(points: Iterable[Dict[str, Any]], padding_meters: float) -> Tuple[float, float, float, float]:
    points = list(points)
    latitudes = [float(point["latitude"]) for point in points]
    longitudes = [float(point["longitude"]) for point in points]
    center_latitude = sum(latitudes) / len(latitudes)
    latitude_padding = padding_meters / 111_320.0
    longitude_padding = padding_meters / (111_320.0 * max(0.1, math.cos(math.radians(center_latitude))))
    return (
        min(latitudes) - latitude_padding, min(longitudes) - longitude_padding,
        max(latitudes) + latitude_padding, max(longitudes) + longitude_padding,
    )


def _distance_m(left: Coordinate, right: Coordinate) -> float:
    radius = 6_371_000.0
    lat1, lat2 = math.radians(left[0]), math.radians(right[0])
    dlat = lat2 - lat1
    dlon = math.radians(right[1] - left[1])
    value = math.sin(dlat / 2.0) ** 2 + math.cos(lat1) * math.cos(lat2) * math.sin(dlon / 2.0) ** 2
    return radius * 2.0 * math.atan2(math.sqrt(value), math.sqrt(max(0.0, 1.0 - value)))


def _coordinate(point: Dict[str, Any]) -> Optional[Coordinate]:
    try:
        coordinate = (round(float(point["lat"]), 7), round(float(point["lon"]), 7))
    except (KeyError, TypeError, ValueError):
        return None
    return coordinate if _valid_coordinate(coordinate) else None


def _valid_coordinate(coordinate: Coordinate) -> bool:
    return all(math.isfinite(value) for value in coordinate) and -90 <= coordinate[0] <= 90 and -180 <= coordinate[1] <= 180


def _dedupe_adjacent(points: List[Coordinate]) -> List[Coordinate]:
    output: List[Coordinate] = []
    for point in points:
        if not output or point != output[-1]:
            output.append(point)
    return output


def _string_list(value: Any) -> List[str]:
    return [str(item).strip() for item in value if str(item).strip()] if isinstance(value, list) else []


def _optional_string(value: Any) -> Optional[str]:
    text = str(value or "").strip()
    return text or None
