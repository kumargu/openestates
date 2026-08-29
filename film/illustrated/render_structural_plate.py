"""Render deterministic OSM structural controls from a reviewed camera request."""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
from typing import Any

from .osm_neighborhood import load_osm_neighborhood
from .scene_models import EvidenceSource, SceneCamera, StructuralScene
from .three_render import render_scene


def _mapping(payload: object, error: str) -> dict[str, Any]:
    if not isinstance(payload, dict):
        raise ValueError(error)
    return payload


def _keys(
    payload: dict[str, Any],
    required: set[str],
    error: str,
    optional: set[str] | None = None,
) -> None:
    keys = set(payload)
    if not required <= keys or keys - required - (optional or set()):
        raise ValueError(error)


def _text(payload: object, error: str) -> str:
    if not isinstance(payload, str) or not payload.strip():
        raise ValueError(error)
    return payload


def _number(payload: object, error: str) -> float:
    if isinstance(payload, bool) or not isinstance(payload, (int, float)):
        raise ValueError(error)
    value = float(payload)
    if not math.isfinite(value):
        raise ValueError(error)
    return value


def _integer(payload: object, error: str) -> int:
    if isinstance(payload, bool) or not isinstance(payload, int):
        raise ValueError(error)
    return payload


def _point(payload: object) -> tuple[float, float]:
    if not isinstance(payload, list) or len(payload) != 2:
        raise ValueError("invalid_boundary_point")
    return (
        _number(payload[0], "invalid_boundary_point"),
        _number(payload[1], "invalid_boundary_point"),
    )


def _camera_point(payload: object) -> tuple[float, float, float]:
    if not isinstance(payload, list) or len(payload) != 3:
        raise ValueError("invalid_camera_point")
    return (
        _number(payload[0], "invalid_camera_point"),
        _number(payload[1], "invalid_camera_point"),
        _number(payload[2], "invalid_camera_point"),
    )


def _boundary(payload: object) -> tuple[tuple[float, float], ...]:
    if not isinstance(payload, list):
        raise ValueError("invalid_boundary")
    return tuple(_point(point) for point in payload)


def _source(payload: object) -> EvidenceSource:
    source = _mapping(payload, "invalid_boundary_source")
    _keys(
        source,
        {
            "source_kind",
            "source_url",
            "license",
            "attribution",
            "retrieved_at",
        },
        "invalid_boundary_source",
        {"source_ref"},
    )
    return EvidenceSource(
        source_kind=_text(source.get("source_kind"), "invalid_boundary_source"),
        source_url=_text(source.get("source_url"), "invalid_boundary_source"),
        license=_text(source.get("license"), "invalid_boundary_source"),
        attribution=_text(source.get("attribution"), "invalid_boundary_source"),
        retrieved_at=_text(source.get("retrieved_at"), "invalid_boundary_source"),
        source_ref=(
            _text(source["source_ref"], "invalid_boundary_source")
            if source.get("source_ref") is not None
            else None
        ),
    )


def _camera(payload: object) -> SceneCamera:
    camera = _mapping(payload, "invalid_camera")
    _keys(
        camera,
        {
            "camera_id",
            "eye",
            "target",
            "vertical_fov_degrees",
            "image_width",
            "image_height",
            "derivation",
        },
        "invalid_camera",
    )
    return SceneCamera(
        camera_id=_text(camera.get("camera_id"), "invalid_camera"),
        eye=_camera_point(camera.get("eye")),
        target=_camera_point(camera.get("target")),
        vertical_fov_degrees=_number(
            camera.get("vertical_fov_degrees"),
            "invalid_camera",
        ),
        image_width=_integer(camera.get("image_width"), "invalid_camera"),
        image_height=_integer(camera.get("image_height"), "invalid_camera"),
        derivation=_text(camera.get("derivation"), "invalid_camera"),
    )


def build_scene(
    request: dict[str, Any],
    *,
    osm_cache: Path,
    offline: bool,
    refresh_osm: bool,
) -> StructuralScene:
    """Combine reviewed boundary/camera inputs with one cached OSM snapshot."""
    schema_version = request.get("schema_version")
    if type(schema_version) is not int or schema_version != 1:
        raise ValueError("unsupported_structural_plate_request")
    _keys(
        request,
        {
            "schema_version",
            "property_id",
            "boundary",
            "boundary_source",
            "camera",
        },
        "invalid_structural_plate_request",
    )
    property_id = _text(request.get("property_id"), "invalid_property_id")
    boundary = _boundary(request.get("boundary"))
    boundary_source = _source(request.get("boundary_source"))
    camera = _camera(request.get("camera"))
    identity = StructuralScene(
        property_id=property_id,
        camera=camera,
        boundary=boundary,
        boundary_source=boundary_source,
        buildings=(),
        features=(),
    )
    identity.validate()
    if offline and not osm_cache.is_file():
        raise ValueError("offline_osm_snapshot_missing")
    neighborhood = load_osm_neighborhood(
        boundary,
        osm_cache,
        refresh=refresh_osm,
        offline=offline,
    )
    scene = StructuralScene(
        property_id=property_id,
        camera=camera,
        boundary=boundary,
        boundary_source=boundary_source,
        buildings=neighborhood.buildings,
        features=neighborhood.features,
    )
    scene.validate()
    return scene


def run(
    *,
    request_path: Path,
    osm_cache: Path,
    output_dir: Path,
    offline: bool,
    refresh_osm: bool,
) -> dict[str, object]:
    request = json.loads(request_path.read_text())
    if not isinstance(request, dict):
        raise ValueError("invalid_structural_plate_request")
    scene = build_scene(
        request,
        osm_cache=osm_cache,
        offline=offline,
        refresh_osm=refresh_osm,
    )
    rendered = render_scene(scene, output_dir)
    receipt = {
        "schema_version": 1,
        "property_id": scene.property_id,
        "scene_hash": scene.scene_hash,
        "camera_hash": scene.camera_hash,
        "osm_cache": str(osm_cache.resolve()),
        "subject_building_count": sum(
            building.role == "subject" for building in scene.buildings
        ),
        "context_building_count": sum(
            building.role == "context" for building in scene.buildings
        ),
        "controls": {
            kind: str(path.resolve())
            for kind, path in sorted(rendered.control_paths.items())
        },
    }
    receipt_path = output_dir / "structural_plate_receipt.json"
    receipt_path.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    return receipt


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Render clay, depth, semantic, and contour controls from OSM.",
    )
    parser.add_argument("--request", type=Path, required=True)
    parser.add_argument("--osm-cache", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--refresh-osm", action="store_true")
    args = parser.parse_args()
    if args.offline and args.refresh_osm:
        parser.error("--offline and --refresh-osm cannot be combined")
    receipt = run(
        request_path=args.request,
        osm_cache=args.osm_cache,
        output_dir=args.out,
        offline=args.offline,
        refresh_osm=args.refresh_osm,
    )
    print(json.dumps(receipt, sort_keys=True))


if __name__ == "__main__":
    main()
