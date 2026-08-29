"""Typed contracts for deterministic structural-plate scenes."""

from __future__ import annotations

import hashlib
import json
import math
from dataclasses import asdict, dataclass
from typing import Any


SCENE_SCHEMA_VERSION = 1
ALLOWED_GEOMETRY_SOURCES = {
    "osm_open_data",
    "reviewed_boundary",
}
MAX_BOUNDARY_POINTS = 10_000
MAX_BOUNDARY_SPAN_M = 20_000
MIN_BOUNDARY_AREA_M2 = 1
MAX_FRAME_EDGE = 4_096
MAX_FRAME_PIXELS = 16_777_216
METRES_PER_DEGREE_LATITUDE = 111_320


class SceneContractError(ValueError):
    """A structural scene crossed an invalid or unsafe boundary."""


def canonical_hash(payload: dict[str, Any]) -> str:
    encoded = json.dumps(
        payload,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=True,
    ).encode()
    return hashlib.sha256(encoded).hexdigest()


def _validate_coordinate(point: tuple[float, float]) -> None:
    latitude, longitude = point
    if (
        not all(math.isfinite(value) for value in point)
        or not -90 <= latitude <= 90
        or not -180 <= longitude <= 180
    ):
        raise SceneContractError("invalid_geographic_coordinate")


def validate_boundary(boundary: tuple[tuple[float, float], ...]) -> None:
    if not 3 <= len(boundary) <= MAX_BOUNDARY_POINTS:
        raise SceneContractError("invalid_scene_boundary_size")
    for point in boundary:
        _validate_coordinate(point)
    if len(set(boundary)) < 3:
        raise SceneContractError("degenerate_scene_boundary")
    center_latitude = sum(point[0] for point in boundary) / len(boundary)
    origin_latitude, origin_longitude = boundary[0]
    longitude_scale = METRES_PER_DEGREE_LATITUDE * math.cos(
        math.radians(center_latitude)
    )
    projected = tuple(
        (
            (longitude - origin_longitude) * longitude_scale,
            (latitude - origin_latitude) * METRES_PER_DEGREE_LATITUDE,
        )
        for latitude, longitude in boundary
    )
    twice_area_m2 = abs(
        sum(
            x * projected[(index + 1) % len(projected)][1]
            - projected[(index + 1) % len(projected)][0] * y
            for index, (x, y) in enumerate(projected)
        )
    )
    if twice_area_m2 < MIN_BOUNDARY_AREA_M2 * 2:
        raise SceneContractError("degenerate_scene_boundary")
    latitude_span_m = (
        max(point[0] for point in boundary)
        - min(point[0] for point in boundary)
    ) * METRES_PER_DEGREE_LATITUDE
    longitude_span_m = (
        max(point[1] for point in boundary)
        - min(point[1] for point in boundary)
    ) * METRES_PER_DEGREE_LATITUDE * math.cos(math.radians(center_latitude))
    if max(latitude_span_m, longitude_span_m) > MAX_BOUNDARY_SPAN_M:
        raise SceneContractError("scene_boundary_too_large")


@dataclass(frozen=True)
class EvidenceSource:
    source_kind: str
    source_url: str
    license: str
    attribution: str
    retrieved_at: str
    source_ref: str | None = None

    def validate(self, *, geometry: bool = False) -> None:
        required = (
            self.source_kind,
            self.source_url,
            self.license,
            self.attribution,
            self.retrieved_at,
        )
        if not all(isinstance(value, str) and value.strip() for value in required):
            raise SceneContractError("incomplete_evidence_source")
        normalized_kind = (
            self.source_kind.strip().lower().replace("-", "_").replace(" ", "_")
        )
        if geometry and normalized_kind not in ALLOWED_GEOMETRY_SOURCES:
            raise SceneContractError("geometry_source_not_allowed")


@dataclass(frozen=True)
class SceneCamera:
    camera_id: str
    eye: tuple[float, float, float]
    target: tuple[float, float, float]
    vertical_fov_degrees: float
    image_width: int
    image_height: int
    derivation: str

    def validate(self) -> None:
        _validate_coordinate(self.eye[:2])
        _validate_coordinate(self.target[:2])
        if not all(math.isfinite(value) for value in (*self.eye, *self.target)):
            raise SceneContractError("invalid_camera_coordinate")
        if self.eye == self.target:
            raise SceneContractError("camera_eye_equals_target")
        if not 1 <= self.vertical_fov_degrees < 180:
            raise SceneContractError("invalid_camera_fov")
        if (
            self.image_width <= 0
            or self.image_height <= 0
            or self.image_width > MAX_FRAME_EDGE
            or self.image_height > MAX_FRAME_EDGE
            or self.image_width * self.image_height > MAX_FRAME_PIXELS
        ):
            raise SceneContractError("invalid_frame_dimensions")
        if not self.camera_id or not self.derivation:
            raise SceneContractError("incomplete_camera")


@dataclass(frozen=True)
class SceneBuilding:
    building_id: str
    footprint: tuple[tuple[float, float], ...]
    role: str
    source: EvidenceSource
    confidence: float
    height_m: float | None = None
    floors: int | None = None
    height_source: dict[str, Any] | None = None

    def validate(self) -> None:
        if not self.building_id or not self.role:
            raise SceneContractError("incomplete_building")
        if len(self.footprint) < 3:
            raise SceneContractError("building_footprint_too_small")
        for point in self.footprint:
            _validate_coordinate(point)
        if not 0 <= self.confidence <= 1:
            raise SceneContractError("invalid_building_confidence")
        if self.height_m is not None and (
            not math.isfinite(self.height_m) or self.height_m <= 0
        ):
            raise SceneContractError("invalid_building_height")
        if self.floors is not None and self.floors <= 0:
            raise SceneContractError("invalid_building_floors")
        self.source.validate(geometry=True)


@dataclass(frozen=True)
class SceneFeature:
    feature_id: str
    kind: str
    geometry_kind: str
    geometry: tuple[tuple[float, float], ...]
    source: EvidenceSource
    confidence: float
    width_m: float | None = None

    def validate(self) -> None:
        if not self.feature_id or not self.kind or not self.geometry_kind:
            raise SceneContractError("incomplete_feature")
        if not self.geometry:
            raise SceneContractError("empty_feature_geometry")
        for point in self.geometry:
            _validate_coordinate(point)
        if not 0 <= self.confidence <= 1:
            raise SceneContractError("invalid_feature_confidence")
        if self.width_m is not None and (
            not math.isfinite(self.width_m) or self.width_m <= 0
        ):
            raise SceneContractError("invalid_feature_width")
        self.source.validate(geometry=True)


@dataclass(frozen=True)
class StructuralScene:
    property_id: str
    camera: SceneCamera
    boundary: tuple[tuple[float, float], ...]
    boundary_source: EvidenceSource
    buildings: tuple[SceneBuilding, ...]
    features: tuple[SceneFeature, ...]
    schema_version: int = SCENE_SCHEMA_VERSION

    def validate(self) -> None:
        if self.schema_version != SCENE_SCHEMA_VERSION:
            raise SceneContractError("unsupported_scene_schema")
        if not self.property_id:
            raise SceneContractError("incomplete_scene_identity")
        validate_boundary(self.boundary)
        self.boundary_source.validate(geometry=True)
        self.camera.validate()
        object_ids = {"site-boundary"}
        for item in (*self.buildings, *self.features):
            item.validate()
            object_id = (
                item.building_id
                if isinstance(item, SceneBuilding)
                else item.feature_id
            )
            if object_id in object_ids:
                raise SceneContractError("duplicate_scene_object_id")
            object_ids.add(object_id)

    def to_payload(self) -> dict[str, Any]:
        self.validate()
        return asdict(self)

    @property
    def scene_hash(self) -> str:
        return canonical_hash(self.to_payload())

    @property
    def camera_hash(self) -> str:
        return canonical_hash(asdict(self.camera))
