import json
import unittest

from pipeline.sources.osm_locality_boundaries import (
    collect_locality_boundaries,
    locality_boundaries_from_overpass,
)


class OsmLocalityBoundariesTest(unittest.TestCase):
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
                            "role": "outer",
                            "geometry": [
                                {"lat": 12.0, "lon": 77.0},
                                {"lat": 12.0, "lon": 77.1},
                                {"lat": 12.1, "lon": 77.1},
                            ],
                        },
                        {
                            "role": "outer",
                            "geometry": [
                                {"lat": 12.1, "lon": 77.1},
                                {"lat": 12.1, "lon": 77.0},
                                {"lat": 12.0, "lon": 77.0},
                            ],
                        },
                        {
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
        output = collect_locality_boundaries(
            "2026-09-05", "https://overpass.test", lambda _url, _query: payload
        )
        self.assertEqual(len(output["boundaries"]), 1)
        self.assertEqual(len(output["source_watermarks"]), 2)


if __name__ == "__main__":
    unittest.main()
