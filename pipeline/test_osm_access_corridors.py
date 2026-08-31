import json
import unittest

from pipeline.sources.osm_access_corridors import (
    collect_society_access_records,
    society_access_overpass_query,
    society_access_record,
)


def subject(**overrides):
    return {
        "entity_id": "society:waterford",
        "name": "Waterford",
        "address": "Waterford, ECC Road, Bengaluru",
        "latitude": 12.9810,
        "longitude": 77.7410,
        **overrides,
    }


def boundary():
    return {
        "type": "way",
        "id": 12,
        "tags": {"landuse": "residential", "name": "Waterford"},
        "geometry": [
            {"lat": 12.9805, "lon": 77.7405},
            {"lat": 12.9805, "lon": 77.7415},
            {"lat": 12.9815, "lon": 77.7415},
            {"lat": 12.9815, "lon": 77.7405},
            {"lat": 12.9805, "lon": 77.7405},
        ],
    }


def road(way_id=10, tags=None, points=None):
    return {
        "type": "way",
        "id": way_id,
        "tags": {"highway": "tertiary", "name": "ECC Road", **(tags or {})},
        "geometry": points or [
            {"lat": 12.9800, "lon": 77.7404},
            {"lat": 12.9810, "lon": 77.7405},
            {"lat": 12.9820, "lon": 77.7406},
        ],
    }


def gate(node_id=50):
    return {
        "type": "node",
        "id": node_id,
        "tags": {"barrier": "gate", "entrance": "main"},
        "lat": 12.9810,
        "lon": 77.7405,
    }


class OsmSocietyAccessTests(unittest.TestCase):
    def test_no_metro_is_required_for_boundary_road_and_explicit_gate(self):
        records, hashes = collect_society_access_records(
            [subject()],
            lambda _url, _query: {"elements": [road(), boundary(), gate()]},
            "https://overpass.example/api",
            {
                "eligible_highway_values": ["tertiary", "residential"],
                "max_frontage_distance_meters": 120,
            },
            "2026-08-30T12:00:00Z",
        )
        self.assertEqual(len(records), 1)
        self.assertEqual(len(hashes), 1)
        record = records[0]
        self.assertEqual(record["approach_road_name"], "ECC Road")
        self.assertEqual(record["entrance_status"], "inferred")
        self.assertEqual(record["entrance_id"], "node/50")
        self.assertEqual(json.loads(record["boundary_geometry_geojson"])["type"], "Polygon")

    def test_private_and_pedestrian_shortcuts_are_rejected(self):
        payload = {
            "elements": [
                boundary(),
                road(1, {"access": "private"}),
                road(2, {"highway": "footway"}),
            ]
        }
        record = society_access_record(
            subject(address=""), payload, "query", {}, "2026-08-30T12:00:00Z"
        )
        self.assertIsNotNone(record)
        self.assertNotIn("approach_geometry_geojson", record)
        self.assertNotIn("entrance_status", record)

    def test_reverse_oneway_is_oriented_legally(self):
        record = society_access_record(
            subject(),
            {"elements": [boundary(), road(tags={"oneway": "-1"})]},
            "query",
            {"eligible_highway_values": ["tertiary"]},
            "2026-08-30T12:00:00Z",
        )
        coordinates = json.loads(record["approach_geometry_geojson"])["coordinates"]
        self.assertEqual(record["approach_direction"], "oneway_reverse")
        self.assertEqual(coordinates[0], [77.7406, 12.982])
        self.assertEqual(coordinates[-1], [77.7404, 12.98])

    def test_two_way_corridor_uses_longer_lead_in_and_keeps_continuation(self):
        record = society_access_record(
            subject(),
            {"elements": [boundary(), road(), gate()]},
            "query",
            {"eligible_highway_values": ["tertiary"]},
            "2026-08-30T12:00:00Z",
        )
        coordinates = json.loads(record["approach_geometry_geojson"])["coordinates"]
        entrance = [record["entrance_longitude"], record["entrance_latitude"]]
        self.assertEqual(record["approach_direction"], "two_way")
        self.assertNotEqual(coordinates[0], entrance)
        self.assertNotEqual(coordinates[-1], entrance)
        self.assertIn(entrance, coordinates)

    def test_missing_gate_never_invents_an_entrance(self):
        record = society_access_record(
            subject(), {"elements": [boundary(), road()]}, "query", {}, "2026-08-30T12:00:00Z"
        )
        self.assertNotIn("entrance_latitude", record)
        self.assertNotIn("entrance_status", record)

    def test_reviewed_coordinates_are_verified(self):
        record = society_access_record(
            subject(
                entrance_reviewed=True,
                entrance_latitude=12.9810,
                entrance_longitude=77.7405,
                entrance_source_url="https://example.test/review/entrance",
            ),
            {"elements": [boundary(), road()]},
            "query",
            {},
            "2026-08-30T12:00:00Z",
        )
        self.assertEqual(record["entrance_status"], "verified")

    def test_overpass_query_requests_each_evidence_family(self):
        query = society_access_overpass_query(
            (12.9, 77.5, 13.0, 77.6), ["primary", "residential"], ["residential"], 45
        )
        self.assertIn('[timeout:45]', query)
        self.assertIn('way["highway"~"^(primary|residential)$"]', query)
        self.assertIn('way["landuse"~"^(residential)$"]["name"]', query)
        self.assertIn('node["barrier"="gate"]', query)
        self.assertIn('node["entrance"~"^(main|yes)$"]', query)


if __name__ == "__main__":
    unittest.main()
