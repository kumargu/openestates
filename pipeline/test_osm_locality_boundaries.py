import json
import unittest
from pathlib import Path

from pipeline.sources.osm_locality_boundaries import (
    collect_locality_boundaries,
    locality_boundaries_from_overpass,
    overpass_query,
)


class OsmLocalityBoundariesTest(unittest.TestCase):
    def test_captured_real_relation_assembles_boundary_and_preserves_members(self):
        fixture_path = (
            Path(__file__).parent
            / "fixtures"
            / "osm_hoodi_relation_19883493.json"
        )
        payload = json.loads(fixture_path.read_text(encoding="utf-8"))

        records = locality_boundaries_from_overpass(payload)

        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["osm_id"], "relation/19883493")
        self.assertEqual(records[0]["admin_level"], "10")
        self.assertEqual(len(records[0]["members"]), 7)
        self.assertEqual(
            {member["role"] for member in records[0]["members"]}, {"outer"}
        )
        geometry = json.loads(records[0]["geometry_geojson"])
        self.assertEqual(geometry["type"], "Polygon")
        self.assertEqual(len(geometry["coordinates"]), 1)
        self.assertGreater(len(geometry["coordinates"][0]), 250)
        self.assertEqual(
            geometry["coordinates"][0][0], geometry["coordinates"][0][-1]
        )

    def test_relation_members_form_polygon_with_hole(self):
        payload = {
            "elements": [
                {
                    "type": "relation",
                    "id": 42,
                    "tags": {
                        "boundary": "administrative",
                        "admin_level": "10",
                        "name": "Fixture locality",
                    },
                    "members": [
                        {
                            "type": "way",
                            "ref": 100,
                            "role": "outer",
                            "geometry": [
                                {"lat": 12.0, "lon": 77.0},
                                {"lat": 12.0, "lon": 77.1},
                                {"lat": 12.1, "lon": 77.1},
                            ],
                        },
                        {
                            "type": "way",
                            "ref": 101,
                            "role": "outer",
                            "geometry": [
                                {"lat": 12.1, "lon": 77.1},
                                {"lat": 12.1, "lon": 77.0},
                                {"lat": 12.0, "lon": 77.0},
                            ],
                        },
                        {
                            "type": "way",
                            "ref": 102,
                            "role": "inner",
                            "geometry": [
                                {"lat": 12.02, "lon": 77.02},
                                {"lat": 12.02, "lon": 77.03},
                                {"lat": 12.03, "lon": 77.03},
                                {"lat": 12.02, "lon": 77.02},
                            ],
                        },
                    ],
                }
            ]
        }
        records = locality_boundaries_from_overpass(payload)
        self.assertEqual(len(records), 1)
        geometry = json.loads(records[0]["geometry_geojson"])
        self.assertEqual(geometry["type"], "Polygon")
        self.assertEqual(len(geometry["coordinates"]), 2)
        self.assertEqual(records[0]["osm_id"], "relation/42")
        self.assertEqual(
            records[0]["members"],
            [
                {"member_type": "way", "member_ref": "100", "role": "outer"},
                {"member_type": "way", "member_ref": "101", "role": "outer"},
                {"member_type": "way", "member_ref": "102", "role": "inner"},
            ],
        )

    def test_collection_is_scoped_and_watermarked(self):
        payload = {
            "elements": [
                {
                    "type": "way",
                    "id": 7,
                    "tags": {
                        "boundary": "administrative",
                        "admin_level": "10",
                        "name": "Test area",
                    },
                    "geometry": [
                        {"lat": 12.0, "lon": 77.0},
                        {"lat": 12.0, "lon": 77.1},
                        {"lat": 12.1, "lon": 77.1},
                        {"lat": 12.0, "lon": 77.0},
                    ],
                }
            ]
        }
        policy = {
            "broad_region": {"bbox": [12.0, 77.0, 12.2, 77.2]},
            "admin_levels": [9, 10],
            "output_mode": "body geom",
        }
        captured_query = []
        output = collect_locality_boundaries(
            "2026-09-05",
            "https://overpass.test",
            lambda _url, query: captured_query.append(query) or payload,
            policy,
        )
        self.assertEqual(len(output["boundaries"]), 1)
        self.assertEqual(len(output["source_watermarks"]), 2)
        self.assertIn("out body geom;", captured_query[0])
        self.assertNotIn("out tags geom;", captured_query[0])
        self.assertIn("(12.0,77.0,12.2,77.2)", captured_query[0])

    def test_query_rejects_unconfigured_or_unsafe_scope(self):
        with self.assertRaises(ValueError):
            overpass_query({"admin_levels": [10], "output_mode": "body geom"})


if __name__ == "__main__":
    unittest.main()
