"""Acquire and normalize an OSM neighbourhood for illustrated-film rendering."""

from __future__ import annotations

import json
import math
import re
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Callable
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import Request, urlopen

from .scene_models import (
    EvidenceSource,
    SceneBuilding,
    SceneFeature,
    validate_boundary,
)


OSM_MAP_URL = "https://api.openstreetmap.org/api/0.6/map"
USER_AGENT = "OpenEstates illustrated property-film renderer"
DEFAULT_RADIUS_M = 500.0
MAX_RADIUS_M = 5_000.0
DEFAULT_TIMEOUT_SECONDS = 60.0
METRES_PER_DEGREE_LATITUDE = 111_320.0
DEFAULT_BUILDING_HEIGHT_M = 10.0
FLOOR_HEIGHT_M = 3.0
OSM_LICENSE = "ODbL-1.0"
OSM_ATTRIBUTION = "© OpenStreetMap contributors"
WATER_TAGS = {
    ("natural", "water"),
    ("landuse", "reservoir"),
    ("landuse", "basin"),
    ("waterway", "riverbank"),
}
GREEN_LANDUSES = {"forest", "grass", "meadow", "recreation_ground", "village_green"}
GREEN_LEISURE = {"garden", "park"}
METRO_RAILWAYS = {"subway", "light_rail"}
METRO_WIDTH_M = 6.0
METRO_STATION_DIAMETER_M = 24.0
ROAD_WIDTH_M = {
    "motorway": 14.0,
    "trunk": 12.0,
    "primary": 10.0,
    "secondary": 9.0,
    "tertiary": 8.0,
    "residential": 6.0,
    "unclassified": 5.0,
    "service": 4.0,
    "living_street": 4.0,
    "pedestrian": 3.0,
    "cycleway": 2.0,
    "footway": 1.8,
    "path": 1.5,
}


class OsmNeighborhoodError(RuntimeError):
    """The OSM neighbourhood could not be acquired or normalized."""


@dataclass(frozen=True)
class OsmNeighborhood:
    """Renderer-ready OSM objects around one project boundary."""

    buildings: tuple[SceneBuilding, ...]
    features: tuple[SceneFeature, ...]
    query_url: str
    retrieved_at: str

    @property
    def subject_building_count(self) -> int:
        return sum(building.role == "subject" for building in self.buildings)


FetchFn = Callable[[Request, float], bytes]


def _bounding_box(
    boundary: tuple[tuple[float, float], ...],
    radius_m: float,
) -> tuple[float, float, float, float]:
    validate_boundary(boundary)
    if not math.isfinite(radius_m) or not 0 < radius_m <= MAX_RADIUS_M:
        raise OsmNeighborhoodError("invalid_osm_query_radius")
    center_latitude = sum(point[0] for point in boundary) / len(boundary)
    latitude_margin = radius_m / METRES_PER_DEGREE_LATITUDE
    longitude_margin = latitude_margin / max(
        math.cos(math.radians(center_latitude)),
        1e-6,
    )
    bbox = (
        min(point[1] for point in boundary) - longitude_margin,
        min(point[0] for point in boundary) - latitude_margin,
        max(point[1] for point in boundary) + longitude_margin,
        max(point[0] for point in boundary) + latitude_margin,
    )
    if not (-180 <= bbox[0] < bbox[2] <= 180 and -90 <= bbox[1] < bbox[3] <= 90):
        raise OsmNeighborhoodError("invalid_osm_query_bounds")
    return bbox


def _query_url(bbox: tuple[float, float, float, float]) -> str:
    encoded_bbox = ",".join(f"{value:.7f}" for value in bbox)
    return f"{OSM_MAP_URL}?{urlencode({'bbox': encoded_bbox})}"


def query_url_for_boundary(
    boundary: tuple[tuple[float, float], ...],
    radius_m: float = DEFAULT_RADIUS_M,
) -> str:
    """Return the exact OSM map URL associated with a cached snapshot."""
    return _query_url(_bounding_box(boundary, radius_m))


def _default_fetch(request: Request, timeout_seconds: float) -> bytes:
    try:
        with urlopen(request, timeout=timeout_seconds) as response:
            return response.read()
    except (HTTPError, URLError, TimeoutError) as error:
        raise OsmNeighborhoodError(f"OSM neighbourhood fetch failed: {error}") from error


def _point_in_polygon(
    point: tuple[float, float],
    polygon: tuple[tuple[float, float], ...],
) -> bool:
    latitude, longitude = point
    inside = False
    for index, start in enumerate(polygon):
        end = polygon[(index + 1) % len(polygon)]
        if (start[1] > longitude) == (end[1] > longitude):
            continue
        latitude_at_edge = (
            (end[0] - start[0])
            * (longitude - start[1])
            / (end[1] - start[1])
            + start[0]
        )
        if latitude < latitude_at_edge:
            inside = not inside
    return inside


def _tags(element: ET.Element) -> dict[str, str]:
    return {
        tag.attrib["k"]: tag.attrib["v"]
        for tag in element.findall("tag")
        if {"k", "v"} <= tag.attrib.keys()
    }


def _ring(
    way: ET.Element,
    nodes: dict[str, tuple[float, float]],
) -> tuple[tuple[float, float], ...]:
    points = tuple(
        nodes[reference]
        for node in way.findall("nd")
        if (reference := node.attrib.get("ref")) in nodes
    )
    if len(points) > 1 and points[0] == points[-1]:
        return points[:-1]
    return points


def _way_refs(way: ET.Element) -> list[str]:
    return [
        reference
        for node in way.findall("nd")
        if (reference := node.attrib.get("ref")) is not None
    ]


def _joined_rings(
    ways: list[ET.Element],
    nodes: dict[str, tuple[float, float]],
) -> tuple[tuple[tuple[float, float], ...], ...]:
    """Join ordered OSM relation members into closed polygon rings."""
    chains = [_way_refs(way) for way in ways]
    chains = [chain for chain in chains if len(chain) >= 2]
    rings: list[tuple[tuple[float, float], ...]] = []
    while chains:
        chain = chains.pop(0)
        while chain[0] != chain[-1]:
            match_index = next(
                (
                    index
                    for index, candidate in enumerate(chains)
                    if chain[-1] in {candidate[0], candidate[-1]}
                ),
                None,
            )
            if match_index is None:
                break
            candidate = chains.pop(match_index)
            if candidate[-1] == chain[-1]:
                candidate.reverse()
            chain.extend(candidate[1:])
        points = tuple(nodes[reference] for reference in chain if reference in nodes)
        if len(points) > 1 and points[0] == points[-1]:
            points = points[:-1]
        if len(points) >= 3:
            rings.append(points)
    return tuple(rings)


def _number(value: str | None) -> float | None:
    if value is None:
        return None
    match = re.match(r"^\s*(\d+(?:\.\d+)?)", value)
    if match is None:
        return None
    parsed = float(match.group(1))
    return parsed if math.isfinite(parsed) and parsed > 0 else None


def _height_metres(value: str | None) -> float | None:
    if value is None:
        return None
    match = re.match(
        r"^\s*(\d+(?:\.\d+)?)\s*(m|metres?|meters?|ft|feet|foot|'|cm)?\s*$",
        value,
        flags=re.IGNORECASE,
    )
    if match is None:
        return None
    parsed = float(match.group(1))
    unit = (match.group(2) or "m").lower()
    if unit in {"ft", "feet", "foot", "'"}:
        parsed *= 0.3048
    elif unit == "cm":
        parsed /= 100
    return parsed if math.isfinite(parsed) and parsed > 0 else None


def _orientation(
    start: tuple[float, float],
    end: tuple[float, float],
    point: tuple[float, float],
) -> float:
    return (end[1] - start[1]) * (point[0] - start[0]) - (
        end[0] - start[0]
    ) * (point[1] - start[1])


def _segments_intersect(
    left_start: tuple[float, float],
    left_end: tuple[float, float],
    right_start: tuple[float, float],
    right_end: tuple[float, float],
) -> bool:
    if (
        max(left_start[0], left_end[0]) < min(right_start[0], right_end[0])
        or max(right_start[0], right_end[0]) < min(left_start[0], left_end[0])
        or max(left_start[1], left_end[1]) < min(right_start[1], right_end[1])
        or max(right_start[1], right_end[1]) < min(left_start[1], left_end[1])
    ):
        return False
    left_a = _orientation(left_start, left_end, right_start)
    left_b = _orientation(left_start, left_end, right_end)
    right_a = _orientation(right_start, right_end, left_start)
    right_b = _orientation(right_start, right_end, left_end)
    return (left_a <= 0 <= left_b or left_b <= 0 <= left_a) and (
        right_a <= 0 <= right_b or right_b <= 0 <= right_a
    )


def _polygons_intersect(
    left: tuple[tuple[float, float], ...],
    right: tuple[tuple[float, float], ...],
) -> bool:
    if any(_point_in_polygon(point, right) for point in left):
        return True
    if any(_point_in_polygon(point, left) for point in right):
        return True
    return any(
        _segments_intersect(
            left[index],
            left[(index + 1) % len(left)],
            right[other_index],
            right[(other_index + 1) % len(right)],
        )
        for index in range(len(left))
        for other_index in range(len(right))
    )


def _floors(tags: dict[str, str]) -> int | None:
    value = _number(tags.get("building:levels"))
    return max(1, round(value)) if value is not None else None


def _building_height(
    tags: dict[str, str],
    source_ref: str,
) -> tuple[float, int | None, dict[str, object]]:
    explicit_height = _height_metres(tags.get("height"))
    floors = _floors(tags)
    if explicit_height is not None:
        return explicit_height, floors, {
            "source_kind": "osm_height",
            "source_ref": source_ref,
        }
    if floors is not None:
        return floors * FLOOR_HEIGHT_M, floors, {
            "source_kind": "derived_from_osm_levels",
            "source_ref": source_ref,
            "floor_height_m": FLOOR_HEIGHT_M,
        }
    return DEFAULT_BUILDING_HEIGHT_M, None, {
        "source_kind": "illustrative_context_default",
        "height_m": DEFAULT_BUILDING_HEIGHT_M,
    }


def _source(
    source_ref: str,
    query_url: str,
    retrieved_at: str,
) -> EvidenceSource:
    return EvidenceSource(
        source_kind="osm_open_data",
        source_url=f"https://www.openstreetmap.org/{source_ref}",
        source_ref=f"{source_ref}; query={query_url}",
        license=OSM_LICENSE,
        attribution=OSM_ATTRIBUTION,
        retrieved_at=retrieved_at,
    )


def _snapshot_time(root: ET.Element) -> str:
    timestamps = [
        timestamp
        for element in root
        if (timestamp := element.attrib.get("timestamp")) is not None
    ]
    return max(timestamps) if timestamps else "1970-01-01T00:00:00Z"


def parse_osm_neighborhood(
    payload: bytes,
    boundary: tuple[tuple[float, float], ...],
    query_url: str,
    retrieved_at: str | None = None,
) -> OsmNeighborhood:
    """Convert OSM map XML into buildings and ground features."""
    try:
        root = ET.fromstring(payload)
    except ET.ParseError as error:
        raise OsmNeighborhoodError(f"OSM returned invalid XML: {error}") from error
    nodes = {
        node.attrib["id"]: (
            float(node.attrib["lat"]),
            float(node.attrib["lon"]),
        )
        for node in root.findall("node")
        if {"id", "lat", "lon"} <= node.attrib.keys()
    }
    observed_at = retrieved_at or _snapshot_time(root)
    buildings: list[SceneBuilding] = []
    features: list[SceneFeature] = []
    ways_by_id = {
        way.attrib["id"]: way
        for way in root.findall("way")
        if "id" in way.attrib
    }
    relation_member_way_ids: set[str] = set()

    bounds = root.find("bounds")
    if bounds is not None:
        context_extent = (
            (float(bounds.attrib["minlat"]), float(bounds.attrib["minlon"])),
            (float(bounds.attrib["minlat"]), float(bounds.attrib["maxlon"])),
            (float(bounds.attrib["maxlat"]), float(bounds.attrib["maxlon"])),
            (float(bounds.attrib["maxlat"]), float(bounds.attrib["minlon"])),
        )
        features.append(
            SceneFeature(
                feature_id="osm-query-extent",
                kind="context_ground",
                geometry_kind="polygon",
                geometry=context_extent,
                source=EvidenceSource(
                    source_kind="osm_open_data",
                    source_url=query_url,
                    source_ref="query bounding box",
                    license=OSM_LICENSE,
                    attribution=OSM_ATTRIBUTION,
                    retrieved_at=observed_at,
                ),
                confidence=1.0,
            )
        )

    for node in root.findall("node"):
        node_id = node.attrib.get("id")
        if node_id is None or node_id not in nodes:
            continue
        tags = _tags(node)
        is_metro_station = (
            tags.get("railway") in {"station", "stop"}
            and (
                tags.get("station") in METRO_RAILWAYS
                or tags.get("subway") == "yes"
                or tags.get("light_rail") == "yes"
            )
        )
        if not is_metro_station:
            continue
        source_ref = f"node/{node_id}"
        features.append(
            SceneFeature(
                feature_id=f"osm-node-{node_id}",
                kind="metro_station",
                geometry_kind="point",
                geometry=(nodes[node_id],),
                source=_source(source_ref, query_url, observed_at),
                confidence=0.95,
                width_m=METRO_STATION_DIAMETER_M,
            )
        )

    for relation in root.findall("relation"):
        relation_id = relation.attrib.get("id")
        tags = _tags(relation)
        if (
            relation_id is None
            or tags.get("type") != "multipolygon"
            or "building" not in tags
        ):
            continue
        members = [
            member
            for member in relation.findall("member")
            if member.attrib.get("type") == "way"
        ]
        relation_member_way_ids.update(
            reference
            for member in members
            if (reference := member.attrib.get("ref")) is not None
        )
        outer_ways = [
            ways_by_id[reference]
            for member in members
            if member.attrib.get("role") in {None, "", "outer"}
            and (reference := member.attrib.get("ref")) in ways_by_id
        ]
        inner_ways = [
            ways_by_id[reference]
            for member in members
            if member.attrib.get("role") == "inner"
            and (reference := member.attrib.get("ref")) in ways_by_id
        ]
        outer_rings = _joined_rings(outer_ways, nodes)
        inner_rings = _joined_rings(inner_ways, nodes)
        source_ref = f"relation/{relation_id}"
        source = _source(source_ref, query_url, observed_at)
        height_m, floors, height_source = _building_height(tags, source_ref)
        for ring_index, outer_ring in enumerate(outer_rings):
            holes = tuple(
                inner_ring
                for inner_ring in inner_rings
                if _point_in_polygon(inner_ring[0], outer_ring)
            )
            buildings.append(
                SceneBuilding(
                    building_id=f"osm-relation-{relation_id}-{ring_index}",
                    footprint=outer_ring,
                    inner_rings=holes,
                    role=(
                        "subject"
                        if _polygons_intersect(outer_ring, boundary)
                        else "context"
                    ),
                    source=source,
                    confidence=0.9 if floors is not None or tags.get("height") else 0.75,
                    height_m=height_m,
                    floors=floors,
                    height_source=height_source,
                )
            )

    for way in root.findall("way"):
        way_id = way.attrib.get("id")
        if way_id is None or way_id in relation_member_way_ids:
            continue
        tags = _tags(way)
        points = _ring(way, nodes)
        source_ref = f"way/{way_id}"
        source = _source(source_ref, query_url, observed_at)

        if "building" in tags and len(points) >= 3:
            height_m, floors, height_source = _building_height(tags, source_ref)
            buildings.append(
                SceneBuilding(
                    building_id=f"osm-way-{way_id}",
                    footprint=points,
                    role="subject" if _polygons_intersect(points, boundary) else "context",
                    source=source,
                    confidence=0.9 if floors is not None else 0.75,
                    height_m=height_m,
                    floors=floors,
                    height_source=height_source,
                )
            )
            continue

        railway = tags.get("railway")
        if railway in METRO_RAILWAYS and len(points) >= 2:
            features.append(
                SceneFeature(
                    feature_id=f"osm-way-{way_id}",
                    kind="metro",
                    geometry_kind="line",
                    geometry=points,
                    source=source,
                    confidence=0.95,
                    width_m=METRO_WIDTH_M,
                )
            )
            continue

        highway = tags.get("highway")
        if highway is not None and len(points) >= 2:
            features.append(
                SceneFeature(
                    feature_id=f"osm-way-{way_id}",
                    kind="road",
                    geometry_kind="line",
                    geometry=points,
                    source=source,
                    confidence=0.9,
                    width_m=ROAD_WIDTH_M.get(highway, 4.0),
                )
            )
            continue

        is_water = any(tags.get(key) == value for key, value in WATER_TAGS)
        if is_water and len(points) >= 3:
            features.append(
                SceneFeature(
                    feature_id=f"osm-way-{way_id}",
                    kind="water",
                    geometry_kind="polygon",
                    geometry=points,
                    source=source,
                    confidence=0.9,
                )
            )
            continue

        is_green = (
            tags.get("landuse") in GREEN_LANDUSES
            or tags.get("leisure") in GREEN_LEISURE
        )
        if is_green and len(points) >= 3:
            features.append(
                SceneFeature(
                    feature_id=f"osm-way-{way_id}",
                    kind="green",
                    geometry_kind="polygon",
                    geometry=points,
                    source=source,
                    confidence=0.8,
                )
            )

    return OsmNeighborhood(
        buildings=tuple(sorted(buildings, key=lambda item: item.building_id)),
        features=tuple(sorted(features, key=lambda item: item.feature_id)),
        query_url=query_url,
        retrieved_at=observed_at,
    )


def load_osm_neighborhood(
    boundary: tuple[tuple[float, float], ...],
    cache_path: Path,
    *,
    radius_m: float = DEFAULT_RADIUS_M,
    timeout_seconds: float = DEFAULT_TIMEOUT_SECONDS,
    refresh: bool = False,
    offline: bool = False,
    now: datetime | None = None,
    fetch_fn: FetchFn | None = None,
) -> OsmNeighborhood:
    """Load cached OSM XML or fetch it once, then return renderer-ready objects."""
    bbox = _bounding_box(boundary, radius_m)
    query_url = _query_url(bbox)
    metadata_path = cache_path.with_suffix(cache_path.suffix + ".json")
    retrieved_at: str | None = None
    should_fetch = refresh or not cache_path.exists()

    if not should_fetch:
        try:
            metadata = json.loads(metadata_path.read_text())
        except (OSError, json.JSONDecodeError) as error:
            if offline:
                raise OsmNeighborhoodError("osm_cache_metadata_missing") from error
            should_fetch = True
        else:
            cached_retrieved_at = metadata.get("retrieved_at")
            if (
                metadata.get("query_url") != query_url
                or not isinstance(cached_retrieved_at, str)
                or not cached_retrieved_at
            ):
                if offline:
                    raise OsmNeighborhoodError("osm_cache_boundary_mismatch")
                should_fetch = True
            else:
                retrieved_at = cached_retrieved_at

    if should_fetch:
        if offline:
            raise OsmNeighborhoodError("offline_osm_snapshot_missing")
        request = Request(
            query_url,
            headers={"User-Agent": USER_AGENT, "Accept": "application/xml"},
        )
        payload = (fetch_fn or _default_fetch)(request, timeout_seconds)
        fetched_at = (now or datetime.now(timezone.utc)).astimezone(timezone.utc)
        retrieved_at = fetched_at.isoformat()
        cache_path.parent.mkdir(parents=True, exist_ok=True)
        cache_path.write_bytes(payload)
        metadata_path.write_text(
            json.dumps(
                {"query_url": query_url, "retrieved_at": retrieved_at},
                indent=2,
                sort_keys=True,
            )
            + "\n"
        )
    else:
        payload = cache_path.read_bytes()

    return parse_osm_neighborhood(
        payload,
        boundary,
        query_url,
        retrieved_at=retrieved_at,
    )
