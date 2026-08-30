"""Collect deterministic street-network paths from societies to transit stops."""

from __future__ import annotations

import hashlib
import heapq
import json
import math
import re
from typing import Any, Callable, Dict, Iterable, List, Optional, Tuple


Coordinate = Tuple[float, float]
Graph = Dict[Coordinate, List[Tuple[Coordinate, float, str, Optional[str]]]]


def collect_access_corridor_records(
    subjects: List[Dict[str, Any]],
    stations: List[Dict[str, Any]],
    fetch: Callable[[str, str], Dict[str, Any]],
    source_url: str,
    collector: Dict[str, Any],
    planned_at: str,
) -> Tuple[List[Dict[str, Any]], List[str]]:
    """Fetch and route one nearest-station corridor per society.

    Routes are computed offline from OSM street geometry. The returned line
    begins and ends on the street graph; it never invents a straight connector
    from a society centroid to a station.
    """

    usable_stations = [station for station in stations if _usable_station(station)]
    if not usable_stations:
        raise ValueError("OSM access corridor collection requires transit stations")

    records: List[Dict[str, Any]] = []
    query_hashes: List[str] = []
    failures: List[str] = []
    max_station_distance = float(collector.get("max_station_distance_meters") or 5000.0)
    station_candidate_limit = int(collector.get("station_candidate_limit") or 4)
    max_snap_distance = float(collector.get("max_snap_distance_meters") or 300.0)
    padding_meters = float(collector.get("bbox_padding_meters") or 450.0)
    highway_values = _string_list(collector.get("highway_values"))
    timeout_seconds = int(collector.get("query_timeout_seconds") or 60)

    for subject in subjects:
        station_candidates = _station_candidates(
            subject,
            usable_stations,
            max_station_distance,
            station_candidate_limit,
        )
        if not station_candidates:
            continue
        query = access_roads_overpass_query(
            _padded_bbox((subject, *station_candidates), padding_meters),
            highway_values,
            timeout_seconds,
        )
        query_hashes.append(hashlib.sha256(query.encode("utf-8")).hexdigest())
        try:
            payload = fetch(source_url, query)
            routed_candidates = [
                record
                for station in station_candidates
                if (
                    record := access_corridor_record(
                        subject,
                        station,
                        payload,
                        query,
                        collector,
                        planned_at,
                        max_snap_distance,
                    )
                )
                is not None
            ]
            if routed_candidates:
                records.append(
                    min(
                        routed_candidates,
                        key=lambda record: (
                            float(record["distance_meters"])
                            + float(record["origin_snap_distance_meters"])
                            + float(record["destination_snap_distance_meters"]),
                            str(record["destination_name"]),
                        ),
                    )
                )
        except Exception as error:
            failures.append(f"{subject['entity_id']}: {error}")

    if failures and (
        len(failures) == len(subjects)
        or not bool(collector.get("allow_partial_subject_failures", False))
    ):
        raise ValueError(
            "OSM access corridor collection failed for "
            f"{len(failures)} of {len(subjects)} subjects: {'; '.join(failures[:5])}"
        )
    records.sort(key=lambda row: (row["entity_id"], row["destination_name"]))
    return records, query_hashes


def access_roads_overpass_query(
    bbox: Tuple[float, float, float, float],
    highway_values: List[str],
    timeout_seconds: int,
) -> str:
    pattern = "|".join(sorted(set(highway_values))) or (
        "motorway|trunk|primary|secondary|tertiary|unclassified|residential|"
        "service|living_street|pedestrian|footway|path|steps"
    )
    south, west, north, east = bbox
    return (
        f"[out:json][timeout:{timeout_seconds}];\n"
        "(\n"
        f'  way["highway"~"^({pattern})$"]'
        f"({south:.7f},{west:.7f},{north:.7f},{east:.7f});\n"
        ");\n"
        "out tags geom;"
    )


def access_corridor_record(
    subject: Dict[str, Any],
    station: Dict[str, Any],
    payload: Dict[str, Any],
    query: str,
    collector: Dict[str, Any],
    planned_at: str,
    max_snap_distance: float,
) -> Optional[Dict[str, Any]]:
    graph, edge_names, way_ids = _street_graph(payload)
    if not graph:
        return None
    origin = (float(subject["latitude"]), float(subject["longitude"]))
    destination = (float(station["latitude"]), float(station["longitude"]))
    origin_node, origin_snap_m = _nearest_graph_node(graph, origin)
    destination_node, destination_snap_m = _nearest_graph_node(graph, destination)
    if (
        origin_node is None
        or destination_node is None
        or origin_snap_m > max_snap_distance
        or destination_snap_m > max_snap_distance
    ):
        return None
    route = _shortest_path(graph, origin_node, destination_node)
    if route is None:
        return None
    points, distance_meters, route_edge_keys = route
    route_names = _ordered_unique(
        edge_names.get(edge_key)
        for edge_key in route_edge_keys
        if edge_names.get(edge_key)
    )
    route_way_ids = _ordered_unique(
        way_ids.get(edge_key)
        for edge_key in route_edge_keys
        if way_ids.get(edge_key)
    )
    address_road = _frontage_road_from_address(
        str(subject.get("address") or ""),
        _string_list(collector.get("road_suffixes")) or ["Road", "Street", "Marg"],
    )
    frontage_road = _matched_frontage_name(address_road, route_names)
    destination_name = str(station["name"]).strip()
    corridor_key = hashlib.sha256(
        f"{subject['entity_id']}:{station.get('station_id') or destination_name}:{points}".encode(
            "utf-8"
        )
    ).hexdigest()[:16]
    frontage_way_id = next(
        (
            way_ids.get(edge_key)
            for edge_key in route_edge_keys
            if frontage_road
            and edge_names.get(edge_key)
            and _road_names_match(frontage_road, str(edge_names[edge_key]))
        ),
        None,
    )
    frontage_points = _way_points(payload, frontage_way_id)
    frontage_distance_meters = (
        sum(_distance_m(left, right) for left, right in zip(frontage_points, frontage_points[1:]))
        if len(frontage_points) >= 2
        else None
    )
    source_way_id = frontage_way_id or (route_way_ids[0] if route_way_ids else None)
    source_way_url = (
        f"https://www.openstreetmap.org/way/{source_way_id}"
        if source_way_id is not None
        else source_url
    )
    return {
        "entity_id": subject["entity_id"],
        "project_key": subject.get("project_key"),
        "query": query,
        "corridor_id": corridor_key,
        "destination_station_id": str(station.get("station_id") or destination_name),
        "destination_name": destination_name,
        "destination_latitude": destination[0],
        "destination_longitude": destination[1],
        "frontage_road_name": frontage_road,
        "frontage_way_id": frontage_way_id,
        "frontage_distance_meters": frontage_distance_meters,
        "frontage_geometry_geojson": (
            json.dumps(
                {
                    "type": "LineString",
                    "coordinates": [
                        [longitude, latitude] for latitude, longitude in frontage_points
                    ],
                },
                separators=(",", ":"),
                sort_keys=True,
            )
            if len(frontage_points) >= 2
            else None
        ),
        "road_names": route_names,
        "route_way_ids": [str(value) for value in route_way_ids],
        "distance_meters": distance_meters,
        "origin_snap_distance_meters": origin_snap_m,
        "destination_snap_distance_meters": destination_snap_m,
        "subject_latitude": origin[0],
        "subject_longitude": origin[1],
        "geometry_geojson": json.dumps(
            {
                "type": "LineString",
                "coordinates": [[longitude, latitude] for latitude, longitude in points],
            },
            separators=(",", ":"),
            sort_keys=True,
        ),
        "source_url": source_way_url,
        "confidence": float(collector.get("confidence") or 0.78),
        "fetched_at": planned_at,
        "fetch_source": str(
            collector.get("fetch_source") or "overpass_access_corridor_snapshot"
        ),
    }


def _street_graph(
    payload: Dict[str, Any],
) -> Tuple[Graph, Dict[Tuple[Coordinate, Coordinate], Optional[str]], Dict[Tuple[Coordinate, Coordinate], str]]:
    graph: Graph = {}
    edge_names: Dict[Tuple[Coordinate, Coordinate], Optional[str]] = {}
    way_ids: Dict[Tuple[Coordinate, Coordinate], str] = {}
    for element in payload.get("elements") or []:
        if not isinstance(element, dict) or element.get("type") != "way":
            continue
        tags = element.get("tags") if isinstance(element.get("tags"), dict) else {}
        if str(tags.get("access") or "").lower() in {"no"}:
            continue
        geometry = element.get("geometry") or []
        points = []
        for point in geometry:
            coordinate = _coordinate(point) if isinstance(point, dict) else None
            if coordinate is not None:
                points.append(coordinate)
        # Prefer OSM's buyer-facing short/alternate label when the canonical
        # name is a spelled-out expansion (for example, "ECC Road").
        road_name = (
            _optional_string(tags.get("short_name"))
            or _optional_string(tags.get("alt_name"))
            or _optional_string(tags.get("name"))
        )
        way_id = str(element.get("id") or "")
        for left, right in zip(points, points[1:]):
            if left == right:
                continue
            distance = _distance_m(left, right)
            graph.setdefault(left, []).append((right, distance, way_id, road_name))
            graph.setdefault(right, []).append((left, distance, way_id, road_name))
            edge_names[(left, right)] = road_name
            edge_names[(right, left)] = road_name
            way_ids[(left, right)] = way_id
            way_ids[(right, left)] = way_id
    return graph, edge_names, way_ids


def _way_points(payload: Dict[str, Any], way_id: Optional[str]) -> List[Coordinate]:
    """Return the complete source-way geometry, not the routed subset using it."""

    if not way_id:
        return []
    for element in payload.get("elements") or []:
        if (
            isinstance(element, dict)
            and element.get("type") == "way"
            and str(element.get("id") or "") == str(way_id)
        ):
            return [
                coordinate
                for point in element.get("geometry") or []
                if isinstance(point, dict)
                and (coordinate := _coordinate(point)) is not None
            ]
    return []


def _shortest_path(
    graph: Graph, origin: Coordinate, destination: Coordinate
) -> Optional[Tuple[List[Coordinate], float, List[Tuple[Coordinate, Coordinate]]]]:
    distances = {origin: 0.0}
    previous: Dict[Coordinate, Tuple[Coordinate, Tuple[Coordinate, Coordinate]]] = {}
    queue: List[Tuple[float, Coordinate]] = [(0.0, origin)]
    while queue:
        distance, node = heapq.heappop(queue)
        if distance > distances.get(node, math.inf):
            continue
        if node == destination:
            break
        for neighbor, edge_distance, _way_id, _name in graph.get(node, []):
            candidate = distance + edge_distance
            if candidate >= distances.get(neighbor, math.inf):
                continue
            distances[neighbor] = candidate
            previous[neighbor] = (node, (node, neighbor))
            heapq.heappush(queue, (candidate, neighbor))
    if destination not in distances:
        return None
    points = [destination]
    edge_keys: List[Tuple[Coordinate, Coordinate]] = []
    cursor = destination
    while cursor != origin:
        parent, edge_key = previous[cursor]
        edge_keys.append(edge_key)
        points.append(parent)
        cursor = parent
    points.reverse()
    edge_keys.reverse()
    return points, distances[destination], edge_keys


def _nearest_graph_node(
    graph: Graph, point: Coordinate
) -> Tuple[Optional[Coordinate], float]:
    best = None
    best_distance = math.inf
    for candidate in graph:
        distance = _distance_m(point, candidate)
        if distance < best_distance:
            best = candidate
            best_distance = distance
    return best, best_distance


def _station_candidates(
    subject: Dict[str, Any],
    stations: List[Dict[str, Any]],
    max_distance_meters: float,
    limit: int,
) -> List[Dict[str, Any]]:
    origin = (float(subject["latitude"]), float(subject["longitude"]))
    candidates = sorted(
        (
            (_distance_m(origin, (float(station["latitude"]), float(station["longitude"]))), station)
            for station in stations
        ),
        key=lambda item: (item[0], str(item[1].get("name") or "")),
    )
    return [
        station
        for distance, station in candidates
        if distance <= max_distance_meters
    ][:max(1, limit)]


def _usable_station(station: Dict[str, Any]) -> bool:
    try:
        latitude = float(station["latitude"])
        longitude = float(station["longitude"])
    except (KeyError, TypeError, ValueError):
        return False
    status = str(station.get("operational_status") or "operational").lower()
    return (
        bool(str(station.get("name") or "").strip())
        and math.isfinite(latitude)
        and math.isfinite(longitude)
        and status == "operational"
    )


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


def _matched_frontage_name(
    address_road: Optional[str], route_names: List[str]
) -> Optional[str]:
    if address_road:
        for route_name in route_names:
            if _road_names_match(address_road, route_name):
                return address_road
    return route_names[0] if route_names else address_road


def _road_names_match(left: str, right: str) -> bool:
    left_words = _road_words(left)
    right_words = _road_words(right)
    if not left_words or not right_words:
        return False
    if left_words == right_words:
        return True
    left_core = left_words[:-1] if left_words[-1] in {"road", "street", "marg"} else left_words
    right_core = right_words[:-1] if right_words[-1] in {"road", "street", "marg"} else right_words
    left_initials = "".join(word[0] for word in left_core)
    right_initials = "".join(word[0] for word in right_core)
    return (
        "".join(left_core) == "".join(right_core)
        or "".join(left_core) == right_initials
        or "".join(right_core) == left_initials
    )


def _road_words(value: str) -> List[str]:
    return re.findall(r"[a-z0-9]+", value.lower())


def _padded_bbox(
    points: Iterable[Dict[str, Any]], padding_meters: float
) -> Tuple[float, float, float, float]:
    points = list(points)
    latitudes = [float(point["latitude"]) for point in points]
    longitudes = [float(point["longitude"]) for point in points]
    center_latitude = sum(latitudes) / len(latitudes)
    latitude_padding = padding_meters / 111_320.0
    longitude_padding = padding_meters / (
        111_320.0 * max(0.1, math.cos(math.radians(center_latitude)))
    )
    return (
        min(latitudes) - latitude_padding,
        min(longitudes) - longitude_padding,
        max(latitudes) + latitude_padding,
        max(longitudes) + longitude_padding,
    )


def _distance_m(left: Coordinate, right: Coordinate) -> float:
    radius = 6_371_000.0
    lat1 = math.radians(left[0])
    lat2 = math.radians(right[0])
    dlat = lat2 - lat1
    dlon = math.radians(right[1] - left[1])
    value = (
        math.sin(dlat / 2.0) ** 2
        + math.cos(lat1) * math.cos(lat2) * math.sin(dlon / 2.0) ** 2
    )
    return radius * 2.0 * math.atan2(math.sqrt(value), math.sqrt(max(0.0, 1.0 - value)))


def _coordinate(point: Dict[str, Any]) -> Optional[Coordinate]:
    try:
        latitude = round(float(point["lat"]), 7)
        longitude = round(float(point["lon"]), 7)
    except (KeyError, TypeError, ValueError):
        return None
    if not (-90 <= latitude <= 90 and -180 <= longitude <= 180):
        return None
    return latitude, longitude


def _ordered_unique(values: Iterable[Any]) -> List[Any]:
    seen = set()
    output = []
    for value in values:
        if value in seen:
            continue
        seen.add(value)
        output.append(value)
    return output


def _string_list(value: Any) -> List[str]:
    if not isinstance(value, list):
        return []
    return [str(item).strip() for item in value if str(item).strip()]


def _optional_string(value: Any) -> Optional[str]:
    text = str(value or "").strip()
    return text or None
