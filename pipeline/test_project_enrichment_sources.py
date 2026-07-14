import io
import json
import unittest
from unittest.mock import patch
from urllib.error import HTTPError

from pipeline.sources.project_enrichment import (
    collect_metro_stations,
    collect_prestige_inventory,
    fetch_overpass_stations,
    parse_price_inr,
)


class ProjectEnrichmentSourceTest(unittest.TestCase):
    def setUp(self):
        self.request = {
            "planned_at": "2026-07-14T12:00:00Z",
            "partition": {"parts": []},
        }

    def test_prestige_inventory_selects_exact_project_and_normalizes_values(self):
        projects = [
            {
                "ProjectID": "2212",
                "ProjectName": "Prestige Park Grove",
                "Project_slug": "prestige-park-grove",
                "CityText": "Bangalore",
                "ProjectStatus": "Sold Out",
                "Size": "71.41 Acres",
                "DisplayPrice": "₹ 3.6* Crore Onwards",
                "bedroomdisplaytext": "1, 2, 3, 4 BHK",
                "total_unit": 3713,
                "LatLong": {"coordinates": [12.9698196, 77.7499721]},
                "location_url_link": "https://maps.example/park-grove",
                "Address": "Whitefield",
            },
            {
                "ProjectID": "2216",
                "ProjectName": "The Willows @ Prestige Park Grove",
                "Project_slug": "the-willows",
            },
        ]
        result = collect_prestige_inventory(
            self.request,
            {
                "society:rera-park-grove": {
                    "entity_id": "society:rera-park-grove",
                    "project_key": "PRM-1",
                    "society_name": "Prestige Park Grove",
                }
            },
            fetch_projects=lambda _name: projects,
        )

        self.assertEqual(len(result["records"]), 1)
        record = result["records"][0]
        self.assertEqual(record["source_project_name"], "Prestige Park Grove")
        self.assertEqual(record["land_area_acres"], 71.41)
        self.assertEqual(record["starting_price_inr"], 36_000_000)
        self.assertEqual(record["bhk_options"], ["1", "2", "3", "4"])
        self.assertEqual(record["total_units"], 3713)
        self.assertIn("prestige-park-grove", record["source_url"])

    def test_metro_collection_keeps_only_namma_metro_and_marks_future_station(self):
        payload = {
            "osm3s": {"timestamp_osm_base": "2026-07-14T11:59:00Z"},
            "elements": [
                {
                    "type": "node",
                    "id": 1,
                    "lat": 12.98,
                    "lon": 77.75,
                    "tags": {
                        "name": "Kadugodi Tree Park",
                        "network": "Namma Metro",
                        "operator": "Bangalore Metro Rail Corporation Limited",
                    },
                },
                {
                    "type": "node",
                    "id": 2,
                    "lat": 12.97,
                    "lon": 77.74,
                    "tags": {
                        "name": "Future Station",
                        "network": "Namma Metro",
                        "start_date": "2027-01-01",
                    },
                },
                {
                    "type": "node",
                    "id": 3,
                    "lat": 12.96,
                    "lon": 77.73,
                    "tags": {"name": "Heavy Rail", "network": "Indian Railways"},
                },
            ],
        }
        result = collect_metro_stations(
            self.request, fetch_payload=lambda: payload
        )

        self.assertEqual(len(result["records"]), 2)
        statuses = {record["name"]: record["status"] for record in result["records"]}
        self.assertEqual(statuses["Kadugodi Tree Park"], "operational")
        self.assertEqual(statuses["Future Station"], "non_operational")

    def test_metro_collection_rejects_empty_snapshot(self):
        with self.assertRaisesRegex(ValueError, "no usable Namma Metro stations"):
            collect_metro_stations(
                self.request,
                fetch_payload=lambda: {"elements": []},
            )

    def test_overpass_collection_falls_back_after_gateway_failure(self):
        payload = {"elements": [{"type": "node", "id": 1}]}
        gateway_timeout = HTTPError(
            "https://overpass-api.de/api/interpreter",
            504,
            "Gateway Timeout",
            {},
            None,
        )
        fallback_response = io.BytesIO(json.dumps(payload).encode("utf-8"))
        with patch(
            "pipeline.sources.project_enrichment.urlopen",
            side_effect=[gateway_timeout, fallback_response],
        ) as mocked_open:
            self.assertEqual(fetch_overpass_stations(), payload)
        self.assertEqual(mocked_open.call_count, 2)

    def test_price_parser_supports_crore_and_lakh(self):
        self.assertEqual(parse_price_inr("3.7 Cr Onwards"), 37_000_000)
        self.assertEqual(parse_price_inr("₹ 85 Lakh"), 8_500_000)


if __name__ == "__main__":
    unittest.main()
