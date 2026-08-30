import json
import unittest

from pipeline.sources.osm_access_corridors import (
    access_corridor_record,
    access_roads_overpass_query,
    collect_access_corridor_records,
)


class OsmAccessCorridorTests(unittest.TestCase):
    def test_routes_on_osm_geometry_and_keeps_address_abbreviation(self):
        subject = {
            "entity_id": "society:waterford",
            "name": "Waterford",
            "address": "Waterford, Whitefield, Bengaluru",
            "latitude": 12.9810,
            "longitude": 77.7410,
        }
        station = {
            "station_id": "node/metro",
            "name": "Tree Park",
            "latitude": 12.9830,
            "longitude": 77.7440,
            "operational_status": "operational",
        }
        payload = {
            "elements": [
                {
                    "type": "way",
                    "id": 10,
                    "tags": {
                        "highway": "tertiary",
                        "name": "Ecumenical Christian Center Road",
                        "alt_name": "ECC Road",
                    },
                    "geometry": [
                        {"lat": 12.9810, "lon": 77.7410},
                        {"lat": 12.9820, "lon": 77.7420},
                        {"lat": 12.9830, "lon": 77.7430},
                    ],
                },
                {
                    "type": "way",
                    "id": 11,
                    "tags": {"highway": "footway"},
                    "geometry": [
                        {"lat": 12.9830, "lon": 77.7430},
                        {"lat": 12.9830, "lon": 77.7440},
                    ],
                },
            ]
        }

        record = access_corridor_record(
            subject,
            station,
            payload,
            "overpass query",
            {"confidence": 0.8},
            "2026-08-30T12:00:00Z",
            100.0,
        )

        self.assertIsNotNone(record)
        self.assertEqual(record["frontage_road_name"], "ECC Road")
        self.assertEqual(record["frontage_way_id"], "10")
        self.assertEqual(record["route_way_ids"], ["10", "11"])
        frontage_geometry = json.loads(record["frontage_geometry_geojson"])
        self.assertEqual(
            frontage_geometry["coordinates"],
            [[77.741, 12.981], [77.742, 12.982], [77.743, 12.983]],
        )
        self.assertGreater(record["frontage_distance_meters"], 0)
        geometry = json.loads(record["geometry_geojson"])
        self.assertEqual(geometry["type"], "LineString")
        self.assertEqual(geometry["coordinates"][0], [77.741, 12.981])
        self.assertEqual(geometry["coordinates"][-1], [77.744, 12.983])
        self.assertGreater(record["distance_meters"], 0)

    def test_does_not_draw_a_centroid_connector_when_road_is_too_far(self):
        record = access_corridor_record(
            {
                "entity_id": "society:one",
                "latitude": 12.9000,
                "longitude": 77.6000,
            },
            {
                "station_id": "node/metro",
                "name": "Metro",
                "latitude": 12.9100,
                "longitude": 77.6100,
            },
            {
                "elements": [
                    {
                        "type": "way",
                        "id": 12,
                        "tags": {"highway": "residential"},
                        "geometry": [
                            {"lat": 12.9050, "lon": 77.6050},
                            {"lat": 12.9100, "lon": 77.6100},
                        ],
                    }
                ]
            },
            "overpass query",
            {},
            "2026-08-30T12:00:00Z",
            100.0,
        )

        self.assertIsNone(record)

    def test_collection_selects_nearest_operational_station(self):
        captured = []

        def fetch(_url, query):
            captured.append(query)
            return {
                "elements": [
                    {
                        "type": "way",
                        "id": 20,
                        "tags": {"highway": "residential", "name": "Access Road"},
                        "geometry": [
                            {"lat": 12.9800, "lon": 77.7400},
                            {"lat": 12.9820, "lon": 77.7420},
                        ],
                    }
                ]
            }

        records, hashes = collect_access_corridor_records(
            [
                {
                    "entity_id": "society:one",
                    "latitude": 12.9800,
                    "longitude": 77.7400,
                }
            ],
            [
                {
                    "station_id": "future",
                    "name": "Future stop",
                    "latitude": 12.9805,
                    "longitude": 77.7405,
                    "operational_status": "under_construction",
                },
                {
                    "station_id": "open",
                    "name": "Open stop",
                    "latitude": 12.9820,
                    "longitude": 77.7420,
                    "operational_status": "operational",
                },
            ],
            fetch,
            "https://overpass.example/api",
            {"max_snap_distance_meters": 50},
            "2026-08-30T12:00:00Z",
        )

        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["destination_name"], "Open stop")
        self.assertEqual(len(hashes), 1)
        self.assertEqual(len(captured), 1)

    def test_collection_selects_the_shortest_street_route_not_air_distance(self):
        def fetch(_url, _query):
            return {
                "elements": [
                    {
                        "type": "way",
                        "id": 30,
                        "tags": {"highway": "residential", "name": "Long Road"},
                        "geometry": [
                            {"lat": 0.0, "lon": 0.0},
                            {"lat": 0.01, "lon": 0.0},
                            {"lat": 0.01, "lon": 0.001},
                            {"lat": 0.0, "lon": 0.001},
                        ],
                    },
                    {
                        "type": "way",
                        "id": 31,
                        "tags": {"highway": "residential", "name": "Direct Road"},
                        "geometry": [
                            {"lat": 0.0, "lon": 0.0},
                            {"lat": 0.002, "lon": 0.0},
                        ],
                    },
                ]
            }

        records, _ = collect_access_corridor_records(
            [{"entity_id": "society:one", "latitude": 0.0, "longitude": 0.0}],
            [
                {
                    "station_id": "near-by-air",
                    "name": "Near by air",
                    "latitude": 0.0,
                    "longitude": 0.001,
                    "operational_status": "operational",
                },
                {
                    "station_id": "shorter-route",
                    "name": "Shorter route",
                    "latitude": 0.002,
                    "longitude": 0.0,
                    "operational_status": "operational",
                },
            ],
            fetch,
            "https://overpass.example/api",
            {"max_snap_distance_meters": 25, "station_candidate_limit": 2},
            "2026-08-30T12:00:00Z",
        )

        self.assertEqual(records[0]["destination_name"], "Shorter route")
        self.assertLess(records[0]["distance_meters"], 250)

    def test_overpass_query_is_config_driven(self):
        query = access_roads_overpass_query(
            (12.9, 77.5, 13.0, 77.6), ["primary", "residential"], 45
        )

        self.assertIn('[timeout:45]', query)
        self.assertIn("primary|residential", query)


if __name__ == "__main__":
    unittest.main()
