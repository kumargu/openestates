from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from typing import Any

from film.illustrated.osm_neighborhood import (
    OsmNeighborhoodError,
    parse_osm_neighborhood,
    query_url_for_boundary,
)
from film.illustrated.render_structural_plate import build_scene
from film.illustrated.scene_models import EvidenceSource, SceneContractError


BOUNDARY = (
    (12.9795, 77.7395),
    (12.9795, 77.7415),
    (12.9815, 77.7415),
    (12.9815, 77.7395),
)
OSM_XML = b"""<?xml version="1.0" encoding="UTF-8"?>
<osm version="0.6">
  <bounds minlat="12.979" minlon="77.739" maxlat="12.982" maxlon="77.742"/>
  <node id="1" lat="12.9800" lon="77.7400"/>
  <node id="2" lat="12.9800" lon="77.7410"/>
  <node id="3" lat="12.9810" lon="77.7410"/>
  <node id="4" lat="12.9810" lon="77.7400"/>
  <node id="5" lat="12.9792" lon="77.7392"/>
  <node id="6" lat="12.9818" lon="77.7418"/>
  <node id="7" lat="12.9812" lon="77.7405">
    <tag k="railway" v="station"/>
    <tag k="station" v="subway"/>
  </node>
  <way id="10" timestamp="2026-08-20T00:00:00Z">
    <nd ref="1"/><nd ref="2"/><nd ref="3"/><nd ref="4"/><nd ref="1"/>
    <tag k="building" v="apartments"/>
    <tag k="building:levels" v="12"/>
  </way>
  <way id="20" timestamp="2026-08-20T00:00:00Z">
    <nd ref="5"/><nd ref="6"/>
    <tag k="highway" v="residential"/>
  </way>
</osm>
"""


def request() -> dict[str, Any]:
    return {
        "schema_version": 1,
        "property_id": "property:test",
        "boundary": [list(point) for point in BOUNDARY],
        "boundary_source": {
            "source_kind": "reviewed_boundary",
            "source_url": "https://example.com/boundary",
            "license": "CC-BY-4.0",
            "attribution": "Test fixture",
            "retrieved_at": "2026-08-20T00:00:00Z",
        },
        "camera": {
            "camera_id": "overview",
            "eye": [12.978, 77.738, 300],
            "target": [12.9805, 77.7405, 20],
            "vertical_fov_degrees": 35,
            "image_width": 640,
            "image_height": 360,
            "derivation": "test camera",
        },
    }


class StructuralPlateTest(unittest.TestCase):
    def test_osm_snapshot_becomes_typed_scene_geometry(self) -> None:
        neighborhood = parse_osm_neighborhood(
            OSM_XML,
            BOUNDARY,
            "https://api.openstreetmap.org/api/0.6/map?bbox=test",
        )

        self.assertEqual(neighborhood.subject_building_count, 1)
        building = neighborhood.buildings[0]
        self.assertEqual(building.building_id, "osm-way-10")
        self.assertEqual(building.floors, 12)
        self.assertEqual(building.height_m, 36)
        self.assertEqual(building.source.license, "ODbL-1.0")
        self.assertTrue(
            any(feature.kind == "road" for feature in neighborhood.features)
        )
        self.assertTrue(
            any(
                feature.kind == "metro_station"
                for feature in neighborhood.features
            )
        )

    def test_offline_request_uses_cached_osm_without_network(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            cache_path = Path(directory) / "snapshot.osm"
            cache_path.write_bytes(OSM_XML)
            cache_path.with_suffix(".osm.json").write_text(
                json.dumps(
                    {
                        "query_url": query_url_for_boundary(BOUNDARY),
                        "retrieved_at": "2026-08-20T00:00:00Z",
                    }
                )
            )

            scene = build_scene(
                request(),
                osm_cache=cache_path,
                offline=True,
                refresh_osm=False,
            )

        self.assertEqual(scene.property_id, "property:test")
        self.assertEqual(len(scene.buildings), 1)
        self.assertEqual(scene.camera.image_width, 640)
        self.assertEqual(len(scene.scene_hash), 64)

    def test_provider_imagery_cannot_be_geometry_authority(self) -> None:
        source = EvidenceSource(
            source_kind="google_imagery",
            source_url="https://example.com/provider",
            license="restricted",
            attribution="Provider",
            retrieved_at="2026-08-20T00:00:00Z",
        )

        with self.assertRaisesRegex(
            SceneContractError,
            "geometry_source_not_allowed",
        ):
            source.validate(geometry=True)

    def test_offline_mode_fails_when_snapshot_is_absent(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(
                ValueError,
                "offline_osm_snapshot_missing",
            ):
                build_scene(
                    request(),
                    osm_cache=Path(directory) / "missing.osm",
                    offline=True,
                    refresh_osm=False,
                )

    def test_offline_mode_rejects_snapshot_for_another_boundary(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            cache_path = Path(directory) / "snapshot.osm"
            cache_path.write_bytes(OSM_XML)
            cache_path.with_suffix(".osm.json").write_text(
                json.dumps(
                    {
                        "query_url": "https://api.openstreetmap.org/wrong",
                        "retrieved_at": "2026-08-20T00:00:00Z",
                    }
                )
            )

            with self.assertRaisesRegex(
                OsmNeighborhoodError,
                "osm_cache_boundary_mismatch",
            ):
                build_scene(
                    request(),
                    osm_cache=cache_path,
                    offline=True,
                    refresh_osm=False,
                )

    def test_invalid_frame_size_fails_before_reading_osm(self) -> None:
        oversized = request()
        camera = oversized["camera"]
        self.assertIsInstance(camera, dict)
        camera["image_width"] = 10_000

        with self.assertRaisesRegex(
            SceneContractError,
            "invalid_frame_dimensions",
        ):
            build_scene(
                oversized,
                osm_cache=Path("/does/not/exist.osm"),
                offline=False,
                refresh_osm=False,
            )

    def test_unknown_request_fields_are_rejected_before_io(self) -> None:
        unknown = request()
        unknown["provider_image"] = "must-not-enter-geometry"

        with self.assertRaisesRegex(
            ValueError,
            "invalid_structural_plate_request",
        ):
            build_scene(
                unknown,
                osm_cache=Path("/does/not/exist.osm"),
                offline=False,
                refresh_osm=False,
            )

    def test_continent_scale_boundary_is_rejected_before_io(self) -> None:
        oversized = request()
        oversized["boundary"] = [
            [0, 0],
            [0, 1],
            [1, 1],
            [1, 0],
        ]

        with self.assertRaisesRegex(
            SceneContractError,
            "scene_boundary_too_large",
        ):
            build_scene(
                oversized,
                osm_cache=Path("/does/not/exist.osm"),
                offline=False,
                refresh_osm=False,
            )

    def test_collinear_boundary_is_rejected_before_io(self) -> None:
        degenerate = request()
        degenerate["boundary"] = [
            [12.98, 77.74],
            [12.981, 77.741],
            [12.982, 77.742],
        ]

        with self.assertRaisesRegex(
            SceneContractError,
            "degenerate_scene_boundary",
        ):
            build_scene(
                degenerate,
                osm_cache=Path("/does/not/exist.osm"),
                offline=False,
                refresh_osm=False,
            )


if __name__ == "__main__":
    unittest.main()
