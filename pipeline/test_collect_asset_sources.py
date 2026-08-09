import hashlib
import json
import os
import tempfile
import unittest
from io import BytesIO
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import MagicMock, patch
from urllib.error import HTTPError

from pipeline.collect_asset_sources import (
    bengaluru_metro_stations_from_overpass,
    collect_asset_sources,
    collect_bengaluru_metro_stations,
    collect_environment_groundwater_potential,
    collect_google_nearby_places,
    collect_google_places,
    collect_osm_power_infrastructure,
    collect_stormwater_drains,
    geospatial_society_inputs,
    google_nearby_collection_categories,
    groundwater_zones_from_kml,
    collect_reddit_assets,
    collect_rera_receipts,
    collect_rera_source_records,
    collect_rera_registry,
    google_society_inputs,
    reddit_society_inputs,
    request_with_rera_detail_facts,
    rera_project_detail_source_records,
)
from pipeline.skills.base import FactSource, SkillCost, SkillResult, SourcedFact
from pipeline.skills.fetch_google_review_links import (
    FetchGoogleReviewLinksSkill,
    fetch_google_places_nearby_text,
)
from pipeline.skills.search_reddit import (
    RedditSourceBlocked,
    RedditSourceInvalidResponse,
    RedditSourceUnavailable,
    fetch_reddit_threads,
    fetch_reddit_threads_with_retry,
    threads_to_skill_result,
)
from pipeline.sources.external_listings import (
    collect_external_listings,
    external_listing_source_pages,
    magicbricks_source_pages,
    squareyards_source_pages,
)
from pipeline.sources.external_images import (
    classify_media_candidate,
    skip_image_optimization,
    write_optimized_preview,
)


class CollectAssetSourcesTest(unittest.TestCase):
    def setUp(self):
        self._old_skip_image_optimization = os.environ.get(
            "OPENESTATES_SKIP_IMAGE_OPTIMIZATION"
        )
        self._old_enable_image_optimization = os.environ.get(
            "OPENESTATES_ENABLE_IMAGE_OPTIMIZATION"
        )
        self._old_skip_local_society_photo_collection = os.environ.get(
            "OPENESTATES_SKIP_LOCAL_SOCIETY_PHOTO_COLLECTION"
        )
        os.environ["OPENESTATES_SKIP_IMAGE_OPTIMIZATION"] = "1"
        os.environ["OPENESTATES_SKIP_LOCAL_SOCIETY_PHOTO_COLLECTION"] = "1"

    def tearDown(self):
        if self._old_skip_image_optimization is None:
            os.environ.pop("OPENESTATES_SKIP_IMAGE_OPTIMIZATION", None)
        else:
            os.environ["OPENESTATES_SKIP_IMAGE_OPTIMIZATION"] = (
                self._old_skip_image_optimization
            )
        if self._old_enable_image_optimization is None:
            os.environ.pop("OPENESTATES_ENABLE_IMAGE_OPTIMIZATION", None)
        else:
            os.environ["OPENESTATES_ENABLE_IMAGE_OPTIMIZATION"] = (
                self._old_enable_image_optimization
            )
        if self._old_skip_local_society_photo_collection is None:
            os.environ.pop("OPENESTATES_SKIP_LOCAL_SOCIETY_PHOTO_COLLECTION", None)
        else:
            os.environ["OPENESTATES_SKIP_LOCAL_SOCIETY_PHOTO_COLLECTION"] = (
                self._old_skip_local_society_photo_collection
            )

    def test_manual_rera_receipt_source_preserves_raw_listing_bytes(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            listing_cache = root / "listing.json"
            listing_raw = root / "listing.html"
            listing_cache.write_text(
                json.dumps({"cached_at": "2026-08-09T10:30:00Z"}),
                encoding="utf-8",
            )
            listing_raw.write_bytes(b"<html>official receipt</html>")
            with patch("pipeline.collect_asset_sources.LISTING_CACHE_PATH", listing_cache), patch(
                "pipeline.collect_asset_sources.LISTING_RAW_CACHE_PATH", listing_raw
            ):
                payload = collect_rera_receipts({"planned_at": "2026-08-09T11:00:00Z"})

        self.assertEqual(payload["snapshot_date"], "2026-08-09")
        self.assertEqual(payload["receipts"][0]["kind"], "registry_listing")
        self.assertEqual(
            bytes.fromhex(payload["receipts"][0]["body_hex"]),
            b"<html>official receipt</html>",
        )

    def test_rera_source_records_parse_the_raw_listing_with_receipt_lineage(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            listing_cache = root / "listing.json"
            listing_raw = root / "listing.html"
            listing_cache.write_text(
                json.dumps({"cached_at": "2026-08-09T10:30:00Z"}),
                encoding="utf-8",
            )
            listing_raw.write_bytes(
                b"applicationNameList.push('ACK-1');"
                b"applicationNameList2.push('PRM/KA/RERA/1251/446/PR/200811/003528');"
                b"applicationNameList3.push('Fixture Project');"
                b"applicationNameList4.push('Fixture Promoter');"
            )
            with patch("pipeline.collect_asset_sources.LISTING_CACHE_PATH", listing_cache), patch(
                "pipeline.collect_asset_sources.LISTING_RAW_CACHE_PATH", listing_raw
            ):
                payload = collect_rera_source_records({"planned_at": "2026-08-09T11:00:00Z"})

        self.assertEqual(payload["snapshot_date"], "2026-08-09")
        self.assertEqual(len(payload["records"]), 1)
        row = payload["records"][0]
        self.assertEqual(row["kind"], "registration_summary")
        self.assertTrue(row["receipt_id"].startswith("rera_receipt:sha256:"))
        receipt_id = row["receipt_id"]
        expected_capture = hashlib.sha256(
            "rera_capture.v1\n{}\n{}\n2026-08-09T10:30:00+00:00".format(
                receipt_id, "https://rera.karnataka.gov.in/viewAllProjects?language=en"
            ).encode("utf-8")
        ).hexdigest()
        self.assertEqual(row["capture_id"], "rera_capture:sha256:{}".format(expected_capture))
        self.assertEqual(
            json.loads(row["raw_value"])["project_name"], "Fixture Project"
        )

    def test_rera_source_records_scoped_run_ignores_unrelated_malformed_listing_rows(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            listing_cache = root / "listing.json"
            listing_raw = root / "listing.html"
            listing_cache.write_text(
                json.dumps({"cached_at": "2026-08-09T10:30:00Z"}),
                encoding="utf-8",
            )
            listing_raw.write_bytes(
                b"applicationNameList.push('ACK-BLANK');"
                b"applicationNameList2.push('');"
                b"applicationNameList3.push('Malformed Project');"
                b"applicationNameList4.push('Malformed Promoter');"
                b"applicationNameList.push('ACK-GODREJ');"
                b"applicationNameList2.push('PRM/KA/RERA/1251/446/PR/300924/007105');"
                b"applicationNameList3.push('GODREJ LAKESIDE ORCHARD');"
                b"applicationNameList4.push('Godrej Properties Limited');"
            )
            request = {
                "planned_at": "2026-08-09T11:00:00Z",
                "source_entities": [
                    {
                        "entity_id": "society:godrej-lakeside-orchard",
                        "name": "GODREJ LAKESIDE ORCHARD",
                        "project_key": "PRM/KA/RERA/1251/446/PR/300924/007105",
                    }
                ],
            }
            with patch("pipeline.collect_asset_sources.LISTING_CACHE_PATH", listing_cache), patch(
                "pipeline.collect_asset_sources.LISTING_RAW_CACHE_PATH", listing_raw
            ), patch(
                "pipeline.collect_asset_sources.load_scoped_rera_detail_receipts",
                return_value=[],
            ):
                payload = collect_rera_source_records(request)

        self.assertEqual(len(payload["records"]), 1)
        self.assertEqual(
            payload["records"][0]["registration_number"],
            "PRM/KA/RERA/1251/446/PR/300924/007105",
        )

    def test_rera_project_detail_records_preserve_declarations_and_qpr_inventory(self):
        def quarter(quarter, year, submitted, booked, unsold):
            return """
                <b>Quarter {quarter} ( {year} )<span> Details (Submitted on {submitted})</span></b>
                <table><thead><tr><th>Total No of Units Booked</th></tr></thead><tbody>
                <tr><td>Total</td><td>970</td><td>{booked}</td><td>{unsold}</td></tr>
                </tbody></table>
                <a href='/download_jc?DOC_ID=form-{quarter}'>Form 6 {quarter}.pdf</a>
            """.format(
                quarter=quarter,
                year=year,
                submitted=submitted,
                booked=booked,
                unsold=unsold,
            )

        detail_html = """
            <div><p>Total Number of Inventories/Flats/Villas<span>:</span></p></div>
            <div><p>698</p></div>
            <div><p>Total Carpet Area of all the Floors (Sq Mtr)<span>:</span></p></div>
            <div><p>65100</p></div>
            <div>Development <span>Details (Bifurcation of Type of Inventories/Flats/Villas)</span></div>
            <table><thead><tr>
                <th>Sl No</th><th>Type of Inventory</th><th>No. of Units</th>
                <th>Carpet Area</th><th>Balcony/Verandah Area</th><th>Open Terrace Area</th>
            </tr></thead><tbody>
                <tr><td>1</td><td>3BHK+3T</td><td>135</td><td>14832.5</td><td>1986</td><td></td></tr>
                <tr><td>2</td><td>2BHK+2T</td><td>154</td><td>11053</td><td>1695</td><td>0</td></tr>
                <tr><td></td><td>TOTAL</td><td>289</td><td>25885</td><td>3681</td><td></td></tr>
            </tbody></table>
            <div><p>Source of Water<span>:</span></p></div><div><p>Local Authority,</p></div>
            <div><p>Local Authority<span>:</span></p></div><div><p>Kodathi Grama Panchayath</p></div>
            <tr><td>At the time of Registration</td><td>01-11-2024</td><td>30-09-2030</td></tr>
        """ + quarter("Q3", "2025-26", "12-01-2026", 678, 292) + quarter(
            "Q4", "2025-26", "13-04-2026", 678, 292
        ) + quarter("Q1", "2026-27", "13-07-2026", 837, 133)
        snapshot = {
            "registration_number": "PRM/KA/RERA/1251/446/PR/300924/007105",
            "source_url": "https://rera.karnataka.gov.in/projectDetails?action=12638",
            "captured_at": "2026-08-09T10:30:00+00:00",
            "body_hex": detail_html.encode("utf-8").hex(),
        }

        rows = rera_project_detail_source_records(snapshot)
        by_kind = {}
        for row in rows:
            by_kind.setdefault(row["kind"], []).append(row)

        declaration_values = [json.loads(row["raw_value"]) for row in by_kind["promoter_declaration"]]
        self.assertEqual(
            declaration_values,
            [{"unit_count": 698}, {"total_carpet_area_sqm": 65100.0}],
        )
        inventory_values = [json.loads(row["raw_value"]) for row in by_kind["tower_inventory"]]
        self.assertEqual(
            inventory_values,
            [
                {
                    "inventory_type": "3BHK+3T",
                    "unit_count": 135,
                    "total_carpet_area_sqm": 14832.5,
                    "total_balcony_verandah_area_sqm": 1986,
                    "total_open_terrace_area_sqm": None,
                },
                {
                    "inventory_type": "2BHK+2T",
                    "unit_count": 154,
                    "total_carpet_area_sqm": 11053,
                    "total_balcony_verandah_area_sqm": 1695,
                    "total_open_terrace_area_sqm": 0,
                },
                {
                    "inventory_type": "TOTAL",
                    "unit_count": 289,
                    "total_carpet_area_sqm": 25885,
                    "total_balcony_verandah_area_sqm": 3681,
                    "total_open_terrace_area_sqm": None,
                },
            ],
        )
        self.assertEqual(
            json.loads(by_kind["water_service_declaration"][0]["raw_value"]),
            {"source": "Local Authority"},
        )
        qpr_rows = [json.loads(row["raw_value"]) for row in by_kind["quarterly_progress"]]
        self.assertEqual(
            qpr_rows,
            [
                {"quarter": "Q3", "financial_year": "2025-26", "tower_count": 1, "total_units": 970, "booked_units": 678, "unsold_units": 292},
                {"quarter": "Q4", "financial_year": "2025-26", "tower_count": 1, "total_units": 970, "booked_units": 678, "unsold_units": 292},
                {"quarter": "Q1", "financial_year": "2026-27", "tower_count": 1, "total_units": 970, "booked_units": 837, "unsold_units": 133},
            ],
        )
        self.assertEqual(len(by_kind["document_approval"]), 3)
        self.assertEqual(len(by_kind["source_warning"]), 1)

    def test_rera_detail_collection_is_scoped_and_preserves_alias_lineage(self):
        request = {
            "partition": {"parts": [["dt", "2026-07-14"]]},
            "planned_at": "2026-07-14T09:30:00Z",
            "requested_assets": ["rera_registry_monthly"],
            "source_entities": [
                {
                    "entity_id": "society:rera-raintree",
                    "alias_entity_id": "society:prestige-raintree-park",
                    "name": "Prestige Raintree Park",
                    "area": "Whitefield",
                    "city": "Bengaluru",
                    "project_key": "PRM-RAINTREE",
                    "latitude": 12.9698,
                    "longitude": 77.75,
                }
            ],
        }
        listing = SimpleNamespace(
            ack_number="ACK-1",
            registration_number="PRM-RAINTREE",
            project_name="Prestige Raintree Park",
            promoter_name="Prestige Group",
        )
        other_listing = SimpleNamespace(
            ack_number="ACK-2",
            registration_number="PRM-OTHER",
            project_name="Other Project",
            promoter_name="Other Builder",
        )
        result = SkillResult(
            facts=[
                SourcedFact(
                    key="rera_total_land_area_sqm",
                    value={"type": "Numeric", "data": 48562.3},
                    confidence=1.0,
                    source=FactSource(
                        source_type="Rera",
                        url="https://rera.karnataka.gov.in/projectViewDetails",
                        skill_id="fetch_rera",
                    ),
                    learned_at="2026-07-14T09:31:00Z",
                    display_template="Total Land Area: {value} sq m",
                    answers_preferences=["project size", "land area"],
                )
            ]
        )
        skill = MagicMock()
        skill.run.return_value = result

        output = collect_rera_registry(
            request,
            rera_fetch=lambda: ([listing, other_listing], "2026-07-10T08:00:00Z"),
            detail_skill=skill,
        )

        skill.run.assert_called_once()
        self.assertEqual(len(output["projects"]), 1)
        self.assertEqual(output["projects"][0]["registration_number"], "PRM-RAINTREE")
        detail_facts = output["detail_facts"]
        rera_facts = [
            fact for fact in detail_facts if fact["source_type"] == "Rera"
        ]
        self.assertEqual(len(rera_facts), 2)
        self.assertEqual(
            {fact["entity_id"] for fact in rera_facts},
            {
                "society:rera-raintree",
                "society:prestige-raintree-park",
            },
        )
        self.assertEqual(rera_facts[0]["value_type"], "numeric")
        self.assertEqual(rera_facts[0]["source_type"], "Rera")
        self.assertEqual(rera_facts[0]["triggered_by"], "asset_dag")
        profile_facts = [
            fact
            for fact in detail_facts
            if fact["skill_id"] == "source_entity_profile"
        ]
        self.assertEqual(
            {fact["fact_key"] for fact in profile_facts},
            {
                "title",
                "area",
                "city",
                "geo.latitude",
                "geo.longitude",
                "source_scan_selected",
            },
        )
        latitude_fact = next(
            fact for fact in profile_facts if fact["fact_key"] == "geo.latitude"
        )
        self.assertEqual(latitude_fact["value_type"], "numeric")
        self.assertEqual(
            json.loads(latitude_fact["value_json"]),
            {"type": "Numeric", "data": 12.9698},
        )
        self.assertEqual(len(output["detail_fact_annotations"]), 8)
        self.assertEqual(
            output["source_watermarks"][1],
            {
                "source": "karnataka_rera_project_details",
                "high_watermark": "2026-07-14T09:31:00Z",
            },
        )

    def test_geospatial_inputs_ignore_rera_lat_lng_when_google_missing(self):
        subjects = geospatial_society_inputs(
            {
                "source_entities": [
                    {
                        "entity_id": "society:rera-green",
                        "name": "RERA Green",
                        "area": "Kanakapura Road",
                        "city": "Bengaluru",
                    }
                ]
            },
            {
                "detail_facts": [
                    {
                        "entity_id": "society:rera-green",
                        "fact_key": "rera_lat_lng",
                        "value_json": json.dumps(
                            {"type": "Text", "data": "12.877617,77.528900"}
                        ),
                    }
                ]
            },
        )

        self.assertEqual(subjects, [])

    def test_geospatial_inputs_prefer_google_coordinate_pair_over_rera(self):
        subjects = geospatial_society_inputs(
            {
                "source_entities": [
                    {
                        "entity_id": "society:rera-green",
                        "name": "RERA Green",
                        "area": "Kanakapura Road",
                        "city": "Bengaluru",
                    }
                ]
            },
            {
                "detail_facts": [
                    {
                        "entity_id": "society:rera-green",
                        "fact_key": "rera_lat_lng",
                        "value_json": json.dumps(
                            {"type": "Text", "data": "12.814964,77.509353"}
                        ),
                    }
                ]
            },
            {
                "records": [
                    {
                        "entity_id": "society:rera-green",
                        "latitude": 12.896276,
                        "longitude": 77.5308391,
                    }
                ]
            },
        )

        self.assertEqual(len(subjects), 1)
        self.assertEqual(subjects[0]["latitude"], 12.896276)
        self.assertEqual(subjects[0]["longitude"], 77.5308391)

    def test_reddit_inputs_use_only_requested_source_entities(self):
        inputs = reddit_society_inputs(
            {
                "partition": {
                    "parts": [["subreddit", "BangaloreRealEstates"]]
                },
                "source_entities": [
                    {
                        "entity_id": "society:rera-raintree",
                        "alias_entity_id": "society:prestige-raintree-park",
                        "name": "Prestige Raintree Park",
                        "area": "Whitefield",
                        "city": "Bengaluru",
                        "project_key": "PRM-RAINTREE",
                    },
                    {
                        "entity_id": "society:rera-park-grove",
                        "alias_entity_id": "society:prestige-park-grove",
                        "name": "Prestige Park Grove",
                        "area": "Whitefield",
                        "city": "Bengaluru",
                        "project_key": "PRM-PARK-GROVE",
                    },
                ],
            },
            rera_input={
                "projects": [
                    {
                        "registration_number": "PRM-UNREQUESTED",
                        "project_name": "Unrequested Society",
                    }
                ]
            },
        )

        self.assertEqual(
            set(inputs),
            {
                "society:rera-raintree",
                "society:rera-park-grove",
            },
        )
        self.assertEqual(
            inputs["society:rera-raintree"]["query"],
            "Prestige Raintree Park Whitefield Bengaluru",
        )
        self.assertEqual(
            inputs["society:rera-raintree"]["subreddit"],
            "BangaloreRealEstates",
        )
        self.assertEqual(
            inputs["society:rera-raintree"]["alias_entity_id"],
            "society:prestige-raintree-park",
        )

    def test_reddit_transport_block_is_not_reported_as_zero_threads(self):
        error = HTTPError(
            "https://www.reddit.com/search.json",
            403,
            "Blocked",
            {},
            BytesIO(b"blocked"),
        )
        with patch("pipeline.skills.search_reddit.urlopen", side_effect=error):
            with self.assertRaisesRegex(RedditSourceBlocked, "HTTP 403"):
                fetch_reddit_threads("Prestige Raintree Park", "bangalore")

    def test_reddit_collection_is_not_supported_by_active_collector(self):
        request = {
            "partition": {
                "parts": [
                    ["dt", "2026-07-14"],
                    ["subreddit", "BangaloreRealEstates"],
                ]
            },
            "planned_at": "2026-07-14T09:30:00Z",
            "requested_assets": ["reddit_threads_daily"],
        }

        with self.assertRaisesRegex(ValueError, "unsupported source assets"):
            collect_asset_sources(request)

    def test_groundwater_kml_collection_emits_normalized_zones(self):
        kml = b"""<?xml version="1.0" encoding="UTF-8"?>
        <kml xmlns="http://www.opengis.net/kml/2.2">
          <Document>
            <Placemark>
              <ExtendedData>
                <Data name="GWATER_ID"><value>zone-1</value></Data>
                <Data name="GW_PROS"><value>Moderate</value></Data>
              </ExtendedData>
              <Polygon>
                <outerBoundaryIs>
                  <LinearRing>
                    <coordinates>
                      77.0,12.0,0 78.0,12.0,0 78.0,13.0,0 77.0,13.0,0 77.0,12.0,0
                    </coordinates>
                  </LinearRing>
                </outerBoundaryIs>
              </Polygon>
            </Placemark>
          </Document>
        </kml>"""

        zones = groundwater_zones_from_kml(kml)

        self.assertEqual(len(zones), 1)
        self.assertEqual(zones[0]["zone_id"], "zone-1")
        self.assertEqual(zones[0]["groundwater_potential_class"], "Moderate")
        self.assertEqual(zones[0]["rings"][0][0], {"latitude": 12.0, "longitude": 77.0})

    def test_collect_asset_sources_supports_groundwater_payload(self):
        request = {
            "partition": {"parts": [["dt", "2026-07-24"]]},
            "planned_at": "2026-07-24T09:30:00Z",
            "requested_assets": ["society_groundwater_potential_facts"],
        }
        kml = b"""<kml xmlns="http://www.opengis.net/kml/2.2"><Document><Placemark>
          <ExtendedData><Data name="GWATER_ID"><value>zone-1</value></Data><Data name="GW_PROS"><value>Good</value></Data></ExtendedData>
          <Polygon><outerBoundaryIs><LinearRing><coordinates>77.0,12.0,0 78.0,12.0,0 78.0,13.0,0 77.0,13.0,0 77.0,12.0,0</coordinates></LinearRing></outerBoundaryIs></Polygon>
        </Placemark></Document></kml>"""

        with patch("pipeline.collect_asset_sources.fetch_url_bytes", return_value=kml):
            output = collect_asset_sources(request)
        direct = collect_environment_groundwater_potential(
            request, fetch=lambda _url: kml
        )

        self.assertNotIn("source_failures", output)
        self.assertIn("environment_groundwater_potential", output)
        self.assertEqual(
            output["environment_groundwater_potential"]["zones"][0][
                "groundwater_potential_class"
            ],
            "Good",
        )
        self.assertEqual(direct["snapshot_date"], "2026-07-24")
        self.assertEqual(direct["zones"][0]["groundwater_potential_class"], "Good")

    def test_overpass_metro_collection_emits_station_coordinates(self):
        payload = {
            "elements": [
                {
                    "type": "node",
                    "id": 101,
                    "lat": 12.995,
                    "lon": 77.759,
                    "tags": {
                        "name": "Kadugodi Tree Park",
                        "line": "Purple Line",
                        "network": "Namma Metro",
                        "operator": "BMRCL",
                    },
                },
                {
                    "type": "way",
                    "id": 102,
                    "center": {"lat": 12.976, "lon": 77.572},
                    "tags": {
                        "name:en": "Majestic",
                        "ref": "Purple Line;Green Line",
                        "construction": "yes",
                    },
                },
            ]
        }

        stations = bengaluru_metro_stations_from_overpass(payload)

        self.assertEqual(len(stations), 2)
        self.assertEqual(stations[0]["name"], "Kadugodi Tree Park")
        self.assertEqual(stations[0]["station_id"], "node/101")
        self.assertEqual(stations[0]["latitude"], 12.995)
        self.assertEqual(stations[0]["longitude"], 77.759)
        self.assertEqual(stations[0]["lines"], ["Purple Line"])
        self.assertEqual(stations[1]["lines"], ["Purple Line", "Green Line"])
        self.assertEqual(stations[1]["operational_status"], "under_construction")

    def test_station_lines_normalize_colour_and_ignore_station_refs(self):
        from pipeline.collect_asset_sources import station_lines

        self.assertEqual(
            station_lines({"colour": "#e542de", "ref": "BYPH"}),
            ["Purple Line"],
        )
        self.assertEqual(
            station_lines({"line": "Green Line", "colour": "#00a651"}),
            ["Green Line"],
        )
        self.assertEqual(station_lines({"ref": "BSNK"}), [])

    def test_collect_asset_sources_supports_bengaluru_metro_payload(self):
        request = {
            "partition": {"parts": [["dt", "2026-07-24"]]},
            "planned_at": "2026-07-24T09:30:00Z",
            "requested_assets": ["bengaluru_metro_station_facts"],
        }
        payload = {
            "elements": [
                {
                    "type": "node",
                    "id": 101,
                    "lat": 12.995,
                    "lon": 77.759,
                    "tags": {
                        "name": "Kadugodi Tree Park",
                        "line": "Purple Line",
                    },
                }
            ]
        }

        with patch(
            "pipeline.collect_asset_sources.fetch_overpass_json",
            return_value=payload,
        ):
            output = collect_asset_sources(request)
        direct = collect_bengaluru_metro_stations(
            request, fetch=lambda _url, _query: payload
        )

        self.assertNotIn("source_failures", output)
        self.assertIn("bengaluru_metro_stations", output)
        self.assertEqual(
            output["bengaluru_metro_stations"]["stations"][0]["name"],
            "Kadugodi Tree Park",
        )
        self.assertEqual(direct["snapshot_date"], "2026-07-24")

    def test_collect_osm_power_infrastructure_uses_google_place_coordinates(self):
        request = {
            "partition": {"parts": [["dt", "2026-07-27"]]},
            "planned_at": "2026-07-27T09:00:00Z",
            "source_entities": [
                {
                    "entity_id": "society:prestige-southern-star",
                    "name": "Prestige Southern Star",
                    "area": "Akshayanagar",
                    "city": "Bengaluru",
                    "project_key": "PRM-SOUTHERN",
                }
            ],
        }
        google_places = {
            "records": [
                {
                    "entity_id": "society:prestige-southern-star",
                    "latitude": 12.9000,
                    "longitude": 77.6000,
                }
            ]
        }
        overpass = {
            "elements": [
                {
                    "type": "way",
                    "id": 12345,
                    "tags": {
                        "power": "line",
                        "voltage": "220000",
                        "name": "220 kV test line",
                    },
                    "geometry": [
                        {"lat": 12.8990, "lon": 77.5990},
                        {"lat": 12.9010, "lon": 77.6010},
                    ],
                }
            ]
        }

        output = collect_osm_power_infrastructure(
            request,
            google_places_input=google_places,
            fetch=lambda _url, _query: overpass,
        )

        self.assertEqual(output["snapshot_date"], "2026-07-27")
        self.assertEqual(len(output["records"]), 1)
        record = output["records"][0]
        self.assertEqual(record["entity_id"], "society:prestige-southern-star")
        self.assertEqual(record["osm_id"], "way/12345")
        self.assertEqual(record["voltage_kv"], 220.0)
        self.assertLess(record["distance_meters"], 5.0)
        self.assertIn("LineString", record["geometry_geojson"])

    def test_collect_osm_power_infrastructure_falls_back_to_subject_queries(self):
        request = {
            "partition": {"parts": [["dt", "2026-07-27"]]},
            "planned_at": "2026-07-27T09:00:00Z",
            "source_entities": [
                {
                    "entity_id": "society:first",
                    "name": "First Society",
                    "area": "Whitefield",
                    "city": "Bengaluru",
                    "project_key": "PRM-FIRST",
                },
                {
                    "entity_id": "society:second",
                    "name": "Second Society",
                    "area": "Whitefield",
                    "city": "Bengaluru",
                    "project_key": "PRM-SECOND",
                },
            ],
        }
        google_places = {
            "records": [
                {"entity_id": "society:first", "latitude": 12.9700, "longitude": 77.7500},
                {"entity_id": "society:second", "latitude": 12.9800, "longitude": 77.7600},
            ]
        }
        overpass = {
            "elements": [
                {
                    "type": "way",
                    "id": 222,
                    "tags": {"power": "line", "voltage": "220000"},
                    "geometry": [
                        {"lat": 12.9695, "lon": 77.7495},
                        {"lat": 12.9705, "lon": 77.7505},
                    ],
                }
            ]
        }
        calls = []

        def fetch(_url, query):
            calls.append(query)
            if len(calls) == 1:
                raise TimeoutError("combined query timed out")
            return overpass

        output = collect_osm_power_infrastructure(
            request,
            google_places_input=google_places,
            fetch=fetch,
        )

        self.assertEqual(len(calls), 3)
        self.assertEqual(len(output["records"]), 1)
        self.assertEqual(output["records"][0]["entity_id"], "society:first")
        self.assertIn("records=1", output["source_watermarks"][0]["high_watermark"])

    def test_collect_osm_power_infrastructure_fails_closed_on_partial_subject_failure(self):
        request = {
            "partition": {"parts": [["dt", "2026-07-27"]]},
            "planned_at": "2026-07-27T09:00:00Z",
            "source_entities": [
                {"entity_id": "society:first", "name": "First Society"},
                {"entity_id": "society:second", "name": "Second Society"},
            ],
        }
        google_places = {
            "records": [
                {"entity_id": "society:first", "latitude": 12.9700, "longitude": 77.7500},
                {"entity_id": "society:second", "latitude": 12.9800, "longitude": 77.7600},
            ]
        }
        calls = []

        def fetch(_url, _query):
            calls.append(True)
            raise TimeoutError("overpass unavailable")

        with self.assertRaisesRegex(ValueError, "failed for 2 of 2 subjects"):
            collect_osm_power_infrastructure(
                request,
                google_places_input=google_places,
                fetch=fetch,
            )

        self.assertEqual(len(calls), 3)

    def test_collect_stormwater_drains_emits_rajakaluve_rows(self):
        request = {
            "partition": {"parts": [["dt", "2026-07-27"]]},
            "planned_at": "2026-07-27T09:00:00Z",
            "source_entities": [
                {
                    "entity_id": "society:whitefield-test",
                    "name": "Whitefield Test",
                    "area": "Whitefield",
                    "city": "Bengaluru",
                    "project_key": "PRM-WF",
                }
            ],
        }
        google_places = {
            "records": [
                {
                    "entity_id": "society:whitefield-test",
                    "latitude": 12.9700,
                    "longitude": 77.7500,
                }
            ]
        }
        overpass = {
            "elements": [
                {
                    "type": "way",
                    "id": 987,
                    "tags": {
                        "waterway": "drain",
                        "name": "Whitefield Rajakaluve",
                    },
                    "geometry": [
                        {"lat": 12.9695, "lon": 77.7495},
                        {"lat": 12.9705, "lon": 77.7505},
                    ],
                }
            ]
        }

        output = collect_stormwater_drains(
            request,
            google_places_input=google_places,
            fetch=lambda _url, _query: overpass,
        )

        self.assertEqual(output["snapshot_date"], "2026-07-27")
        self.assertEqual(len(output["records"]), 1)
        record = output["records"][0]
        self.assertEqual(record["entity_id"], "society:whitefield-test")
        self.assertEqual(record["drain_id"], "way/987")
        self.assertEqual(record["drain_type"], "rajakaluve")
        self.assertEqual(record["hierarchy"], "primary_swd")
        self.assertLess(record["distance_meters"], 5.0)
        self.assertIn("LineString", record["geometry_geojson"])

    def test_collect_stormwater_drains_rejects_partial_subject_failure(self):
        request = {
            "partition": {"parts": [["dt", "2026-07-27"]]},
            "planned_at": "2026-07-27T09:00:00Z",
            "source_entities": [
                {
                    "entity_id": "society:overpass-fails",
                    "name": "Overpass Fails",
                    "area": "Whitefield",
                    "city": "Bengaluru",
                    "project_key": "PRM-FAIL",
                },
                {
                    "entity_id": "society:overpass-succeeds",
                    "name": "Overpass Succeeds",
                    "area": "Whitefield",
                    "city": "Bengaluru",
                    "project_key": "PRM-OK",
                },
            ],
        }
        google_places = {
            "records": [
                {
                    "entity_id": "society:overpass-fails",
                    "latitude": 12.9700,
                    "longitude": 77.7500,
                },
                {
                    "entity_id": "society:overpass-succeeds",
                    "latitude": 12.9800,
                    "longitude": 77.7600,
                },
            ]
        }
        overpass = {
            "elements": [
                {
                    "type": "way",
                    "id": 654,
                    "tags": {"waterway": "drain", "name": "Whitefield Rajakaluve"},
                    "geometry": [
                        {"lat": 12.9795, "lon": 77.7595},
                        {"lat": 12.9805, "lon": 77.7605},
                    ],
                }
            ]
        }
        calls = []

        def fetch(_url, _query):
            calls.append(_query)
            if len(calls) == 1:
                raise HTTPError("https://overpass.example", 504, "timeout", None, None)
            return overpass

        with self.assertRaisesRegex(ValueError, "unavailable for 1 of 2 subjects"):
            collect_stormwater_drains(
                request,
                google_places_input=google_places,
                fetch=fetch,
            )

    def test_collect_stormwater_drains_never_promotes_partial_rows(self):
        request = {
            "partition": {"parts": [["dt", "2026-07-27"]]},
            "planned_at": "2026-07-27T09:00:00Z",
            "source_entities": [
                {
                    "entity_id": "society:overpass-fails",
                    "name": "Overpass Fails",
                    "city": "Bengaluru",
                    "latitude": 12.9700,
                    "longitude": 77.7500,
                },
                {
                    "entity_id": "society:overpass-succeeds",
                    "name": "Overpass Succeeds",
                    "city": "Bengaluru",
                    "latitude": 12.9800,
                    "longitude": 77.7600,
                },
            ],
        }
        overpass = {
            "elements": [
                {
                    "type": "way",
                    "id": 654,
                    "tags": {"waterway": "drain", "name": "Whitefield Rajakaluve"},
                    "geometry": [
                        {"lat": 12.9795, "lon": 77.7595},
                        {"lat": 12.9805, "lon": 77.7605},
                    ],
                }
            ]
        }
        calls = []

        def fetch(_url, _query):
            calls.append(_query)
            if len(calls) == 1:
                raise HTTPError("https://overpass.example", 504, "timeout", None, None)
            return overpass

        with self.assertRaisesRegex(ValueError, "unavailable for 1 of 2 subjects"):
            collect_stormwater_drains(request, fetch=fetch)

    def test_reddit_transient_failure_retries_before_returning_empty(self):
        unavailable = RedditSourceUnavailable("temporary failure")
        with patch(
            "pipeline.skills.search_reddit.fetch_reddit_threads",
            side_effect=[unavailable, []],
        ) as fetch:
            threads = fetch_reddit_threads_with_retry(
                "Prestige Raintree Park",
                "bangalore",
                sleep=lambda _seconds: None,
            )

        self.assertEqual(threads, [])
        self.assertEqual(fetch.call_count, 2)

    def test_reddit_schema_invalid_json_is_classified(self):
        response = MagicMock()
        response.__enter__.return_value = response
        response.read.return_value = b"null"
        with patch("pipeline.skills.search_reddit.urlopen", return_value=response):
            with self.assertRaisesRegex(
                RedditSourceInvalidResponse, "unexpected response shape"
            ):
                fetch_reddit_threads("Prestige Raintree Park", "bangalore")

    def test_reddit_fact_rows_preserve_explicit_entity_id(self):
        request = {
            "partition": {
                "parts": [
                    ["dt", "2026-07-14"],
                    ["subreddit", "BangaloreRealEstates"],
                ]
            },
            "planned_at": "2026-07-14T09:30:00Z",
        }
        _, facts = collect_reddit_assets(
            request,
            society_inputs={
                "input-key": {
                    "entity_id": "society:rera-raintree",
                    "alias_entity_id": "society:prestige-raintree-park",
                    "query": "Prestige Raintree Park Whitefield",
                    "subreddit": "BangaloreRealEstates",
                }
            },
            thread_fetch=lambda _query, _subreddit: [],
            result_builder=threads_to_skill_result,
        )

        self.assertEqual(
            facts["facts"][0]["entity_id"],
            "society:rera-raintree",
        )
        self.assertEqual(
            facts["facts"][1]["entity_id"],
            "society:prestige-raintree-park",
        )

    def test_google_inputs_use_canonical_request_seeds(self):
        inputs = google_society_inputs(
            {
                "source_entities": [
                    {
                        "entity_id": "society:rera-canonical",
                        "name": "Canonical Green",
                        "area": "Whitefield",
                        "city": "Bengaluru",
                        "address": "Canonical Road",
                        "project_key": "PRM-CANONICAL",
                    }
                ]
            }
        )
        self.assertEqual(
            inputs["society:rera-canonical"]["project_key"], "PRM-CANONICAL"
        )
        self.assertEqual(
            inputs["society:rera-canonical"]["society_name"], "Canonical Green"
        )

    def test_google_inputs_hydrate_rera_address_when_rera_input_missing(self):
        with patch(
            "pipeline.collect_asset_sources.collect_rera_project_details",
            return_value=(
                [
                    {
                        "entity_id": "society:rera-godrej-air",
                        "fact_key": "rera_project_address",
                        "value_json": json.dumps(
                            {
                                "type": "Text",
                                "data": "Khatha No. 365, Hoodi Village, K.R. Puram Hobli",
                            }
                        ),
                    }
                ],
                [],
                "2026-08-01T09:00:00Z",
            ),
        ) as collect_details:
            inputs = google_society_inputs(
                {
                    "source_entities": [
                        {
                            "entity_id": "society:rera-godrej-air",
                            "alias_entity_id": "society:godrej-air",
                            "name": "Godrej Air",
                            "area": "Whitefield",
                            "city": "Bengaluru",
                            "project_key": "PRM-GODREJ-AIR",
                        }
                    ]
                }
            )

        collect_details.assert_called_once()
        self.assertEqual(
            inputs["society:rera-godrej-air"]["address"],
            "Khatha No. 365, Hoodi Village, K.R. Puram Hobli",
        )

    def test_google_source_collection_shares_rera_address_hydration(self):
        captured_inputs = []

        def capture_places(_request, society_inputs=None):
            captured_inputs.append(society_inputs)
            return {"records": [], "source_watermarks": []}

        def capture_nearby(_request, society_inputs=None):
            captured_inputs.append(society_inputs)
            return {"records": [], "source_watermarks": []}

        with patch(
            "pipeline.collect_asset_sources.collect_rera_project_details",
            return_value=(
                [
                    {
                        "entity_id": "society:rera-godrej-air",
                        "fact_key": "rera_project_address",
                        "value_json": json.dumps(
                            {
                                "type": "Text",
                                "data": "Khatha No. 365, Hoodi Village, K.R. Puram Hobli",
                            }
                        ),
                    }
                ],
                [],
                "2026-08-01T09:00:00Z",
            ),
        ) as collect_details:
            with patch(
                "pipeline.collect_asset_sources.collect_google_places",
                side_effect=capture_places,
            ):
                with patch(
                    "pipeline.collect_asset_sources.collect_google_nearby_places",
                    side_effect=capture_nearby,
                ):
                    collect_asset_sources(
                        {
                            "requested_assets": [
                                "google_places_weekly",
                                "google_nearby_places_weekly",
                            ],
                            "source_entities": [
                                {
                                    "entity_id": "society:rera-godrej-air",
                                    "name": "Godrej Air",
                                    "area": "Whitefield",
                                    "city": "Bengaluru",
                                }
                            ],
                            "planned_at": "2026-08-01T09:00:00Z",
                        }
                    )

        collect_details.assert_called_once()
        self.assertEqual(len(captured_inputs), 2)
        for inputs in captured_inputs:
            self.assertEqual(
                inputs["society:rera-godrej-air"]["address"],
                "Khatha No. 365, Hoodi Village, K.R. Puram Hobli",
            )

    def test_google_inputs_attach_rera_address_without_using_rera_coordinates(self):
        inputs = google_society_inputs(
            {
                "source_entities": [
                    {
                        "entity_id": "society:godrej-air",
                        "name": "Godrej Air",
                        "area": "Whitefield",
                        "city": "Bengaluru",
                        "project_key": "PRM-GODREJ-AIR",
                    }
                ]
            },
            {
                "detail_facts": [
                    {
                        "entity_id": "society:godrej-air",
                        "fact_key": "rera_project_address",
                        "value_json": json.dumps(
                            {
                                "type": "Text",
                                "data": "Khatha No. 365, Hoodi Village, K.R. Puram Hobli",
                            }
                        ),
                    },
                    {
                        "entity_id": "society:godrej-air",
                        "fact_key": "rera_lat_lng",
                        "value_json": json.dumps(
                            {"type": "Text", "data": "12.991,77.715"}
                        ),
                    },
                ]
            },
        )

        subject = inputs["society:godrej-air"]
        self.assertEqual(
            subject["address"],
            "Khatha No. 365, Hoodi Village, K.R. Puram Hobli",
        )
        self.assertIsNone(subject["latitude"])
        self.assertIsNone(subject["longitude"])

    def test_external_listing_collection_normalizes_magicbricks_markdown_without_confidence(self):
        output = collect_asset_sources(
            {
                "partition": {"parts": [["dt", "2026-07-16"]]},
                "planned_at": "2026-07-16T09:30:00Z",
                "requested_assets": ["external_listings_weekly"],
                "source_entities": [
                    {
                        "entity_id": "society:example-green",
                        "name": "Example Green",
                        "area": "Whitefield",
                        "project_key": "PRM-EXAMPLE-GREEN",
                        "external_listing_source_pages": [
                            {
                                "source_url": "https://www.magicbricks.com/project-example-green-for-sale-in-bangalore-pppfs",
                                "text": """
                                    ## 3 BHK Flat for Sale in Example Green, Whitefield, Bangalore

                                    Carpet Area

                                    2000 sqft

                                    Status

                                    Ready to Move

                                    Floor

                                    12 out of 20

                                    Bathroom

                                    3

                                    Read more

                                    ₹2.50 Cr

                                    ₹12,500 per sqft

                                    ## 3 BHK Flat for Sale in Example Green, Hoodi, Whitefield, Bangalore

                                    Super Area

                                    2400 sqft

                                    Bathroom

                                    4

                                    Read more

                                    ₹3.50 Cr

                                    ₹14,583 per sqft
                                """,
                            }
                        ],
                    }
                ],
            }
        )

        listing = output["external_listings_weekly"]["records"][0]
        self.assertEqual(output["external_listings_weekly"]["snapshot_date"], "2026-07-16")
        self.assertEqual(listing["entity_id"], "society:example-green")
        self.assertEqual(listing["project_key"], "PRM-EXAMPLE-GREEN")
        self.assertEqual(listing["source_name"], "MagicBricks")
        self.assertEqual(
            listing["source_url"],
            "https://www.magicbricks.com/project-example-green-for-sale-in-bangalore-pppfs",
        )
        self.assertEqual(listing["price"], 30_000_000)
        self.assertEqual(listing["price_min"], 25_000_000)
        self.assertEqual(listing["price_max"], 35_000_000)
        self.assertEqual(listing["price_display"], "₹2.5 Cr - ₹3.5 Cr")
        self.assertEqual(listing["area_sqft"], 2215)
        self.assertEqual(listing["area_sqft_min"], 2000)
        self.assertEqual(listing["area_sqft_max"], 2400)
        self.assertEqual(listing["area_display"], "2000-2400 sqft")
        self.assertEqual(listing["price_per_sqft_min"], 12500)
        self.assertEqual(listing["price_per_sqft_max"], 14583)
        self.assertEqual(listing["price_per_sqft_display"], "₹12,500-14,583 per sqft")
        self.assertEqual(listing["configuration"], "3 BHK")
        self.assertEqual(listing["area_type"], "mixed listed area")
        self.assertEqual(listing["bhk"], 3.0)
        self.assertEqual(listing["bathrooms"], 3.5)
        self.assertEqual(listing["floor"], "12 out of 20")
        self.assertEqual(listing["society"], "Example Green")
        self.assertEqual(listing["locality"], "Hoodi, Whitefield")
        self.assertEqual(listing["observed_at"], "2026-07-16T09:30:00Z")
        self.assertNotIn("confidence", listing)

    def test_external_listing_collection_normalizes_magicbricks_html(self):
        output = collect_asset_sources(
            {
                "partition": {"parts": [["dt", "2026-07-16"]]},
                "planned_at": "2026-07-16T09:30:00Z",
                "requested_assets": ["external_listings_weekly"],
                "source_entities": [
                    {
                        "entity_id": "society:example-green",
                        "name": "Example Green",
                        "area": "Whitefield",
                        "external_listing_source_pages": [
                            {
                                "source_name": "MagicBricks",
                                "source_url": "https://www.magicbricks.com/project-example-green-for-sale-in-bangalore-pppfs",
                                "html": """
                                    <html><body>
                                      <h2 class="mb-srp__card--title" title="3 BHK Flat  for Sale in  Example Green, Hoodi, Bangalore">3 BHK Flat</h2>
                                      <div class="mb-srp__card__summary--label">Carpet Area</div>
                                      <div class="mb-srp__card__summary--value">1,800 sqft</div>
                                      <div class="mb-srp__card__summary--label">Floor</div>
                                      <div class="mb-srp__card__summary--value">12 out of 20</div>
                                      <div class="mb-srp__card__summary--label">Bathroom</div>
                                      <div class="mb-srp__card__summary--value">3</div>
                                      <div class="mb-srp__card__price--amount"><span class="rupees">₹</span>2.08 Cr</div>
                                      <div class="mb-srp__card__price--size"><span class="rupees">₹</span>11556 per sqft</div>
                                    </body></html>
                                """,
                            }
                        ],
                    }
                ],
            }
        )

        listing = output["external_listings_weekly"]["records"][0]
        self.assertEqual(listing["source_name"], "MagicBricks")
        self.assertEqual(listing["price"], 20_800_000)
        self.assertEqual(listing["area_sqft"], 1800)
        self.assertEqual(listing["area_type"], "carpet")
        self.assertEqual(listing["price_per_sqft_min"], 11556)
        self.assertEqual(listing["bathrooms"], 3.0)
        self.assertEqual(listing["floor"], "12 out of 20")
        self.assertEqual(listing["locality"], "Hoodi")

    def test_external_listing_alias_page_keeps_canonical_society_name(self):
        output = collect_asset_sources(
            {
                "partition": {"parts": [["dt", "2026-07-16"]]},
                "planned_at": "2026-07-16T09:30:00Z",
                "requested_assets": ["external_listings_weekly"],
                "source_entities": [
                    {
                        "entity_id": "society:example-green",
                        "name": "Example Green Phase 2",
                        "area": "Whitefield",
                        "external_listing_source_pages": [
                            {
                                "source_name": "MagicBricks",
                                "query_society_name": "Example Green",
                                "source_url": "https://www.magicbricks.com/project-example-green-for-sale-in-bangalore-pppfs",
                                "html": """
                                    <html><body>
                                      <h2 class="mb-srp__card--title" title="3 BHK Flat for Sale in Example Green, Hoodi, Bangalore">3 BHK Flat</h2>
                                      <div class="mb-srp__card__summary--label">Super Area</div>
                                      <div class="mb-srp__card__summary--value">1,800 sqft</div>
                                      <div class="mb-srp__card__price--amount"><span class="rupees">₹</span>2.08 Cr</div>
                                    </body></html>
                                """,
                            }
                        ],
                    }
                ],
            }
        )

        listing = output["external_listings_weekly"]["records"][0]
        self.assertEqual(listing["society"], "Example Green Phase 2")
        self.assertEqual(listing["locality"], "Hoodi")

    def test_external_listing_collection_keeps_rent_separate(self):
        output = collect_asset_sources(
            {
                "partition": {"parts": [["dt", "2026-07-16"]]},
                "planned_at": "2026-07-16T09:30:00Z",
                "requested_assets": ["external_listings_weekly"],
                "source_entities": [
                    {
                        "entity_id": "society:example-green",
                        "name": "Example Green",
                        "area": "Whitefield",
                        "external_listing_source_pages": [
                            {
                                "source_url": "https://www.magicbricks.com/3-bhk-flats-for-rent-in-example-green-bangalore-pppfr",
                                "query_kind": "rent",
                                "text": """
                                    ## 3 BHK Flat for Rent in Example Green, Whitefield, Bangalore

                                    Super Area

                                    1800 sqft

                                    Bathroom

                                    3

                                    ₹95,000
                                """,
                            }
                        ],
                    }
                ],
            }
        )

        listing = output["external_listings_weekly"]["records"][0]
        self.assertEqual(listing["listing_type"], "rent")
        self.assertEqual(listing["price"], 95_000)
        self.assertEqual(listing["configuration"], "3 BHK")

    def test_external_listing_collection_records_empty_and_thin_coverage(self):
        output = collect_external_listings(
            {
                "partition": {"parts": [["dt", "2026-07-16"]]},
                "planned_at": "2026-07-16T09:30:00Z",
                "skip_external_listing_fetch": True,
                "source_entities": [
                    {
                        "entity_id": "society:example-green",
                        "name": "Example Green",
                        "area": "Whitefield",
                    }
                ],
            }
        )

        self.assertEqual(output["records"], [])
        watermarks = {
            watermark["source"]: watermark["high_watermark"]
            for watermark in output["source_watermarks"]
        }
        self.assertIn("external_listing_empty", watermarks)
        self.assertEqual(
            watermarks["external_listing_coverage"],
            "entities=1;records=0;entities_below_min=1;min_records_per_entity=4",
        )

    def test_rera_detail_configurations_feed_listing_queries_in_same_run(self):
        request = {
            "source_entities": [
                {
                    "entity_id": "society:example-green",
                    "name": "Example Green",
                    "city": "Bengaluru",
                }
            ]
        }
        rera_input = {
            "detail_facts": [
                {
                    "entity_id": "society:example-green",
                    "fact_key": "available_configurations",
                    "value_json": json.dumps(
                        {"type": "Tags", "data": ["2BHK", "3BHK"]},
                        separators=(",", ":"),
                    ),
                }
            ]
        }

        enriched = request_with_rera_detail_facts(request, rera_input)
        pages = magicbricks_source_pages(enriched["source_entities"][0], "Example Green")
        urls = [page["source_url"] for page in pages]

        self.assertTrue(any("2-bhk-flats-for-sale" in url for url in urls))
        self.assertTrue(any("3-bhk-flats-for-sale" in url for url in urls))
        self.assertTrue(any("2-bhk-flats-for-rent" in url for url in urls))
        self.assertTrue(any("3-bhk-flats-for-rent" in url for url in urls))

    def test_magicbricks_queries_are_seeded_by_rera_configurations(self):
        pages = magicbricks_source_pages(
            {
                "city": "Bengaluru",
                "available_configurations": ["2BHK", "3BHK", "4BHK"],
            },
            "Example Green",
        )

        urls = [page["source_url"] for page in pages]
        self.assertIn(
            "https://www.magicbricks.com/2-bhk-flats-for-sale-in-example-green-bangalore-pppfs",
            urls,
        )
        self.assertIn(
            "https://www.magicbricks.com/3-bhk-flats-for-rent-in-example-green-bangalore-pppfr",
            urls,
        )
        self.assertFalse(any("4-bhk-flats-for-rent" in url for url in urls))

    def test_external_listing_queries_try_base_project_for_phase_names(self):
        pages = external_listing_source_pages(
            {"city": "Bengaluru"},
            "Embassy Verde Phase 2",
        )
        urls = [page["source_url"] for page in pages]

        self.assertIn(
            "https://www.magicbricks.com/project-embassy-verde-phase-2-for-sale-in-bangalore-pppfs",
            urls,
        )
        self.assertIn(
            "https://www.magicbricks.com/project-embassy-verde-for-sale-in-bangalore-pppfs",
            urls,
        )
        self.assertIn(
            "https://www.squareyards.com/sale/resale-properties-in-embassy-verde-bangalore",
            urls,
        )

    def test_squareyards_uses_focused_project_pages(self):
        pages = squareyards_source_pages(
            {
                "city": "Bengaluru",
                "available_configurations": ["2BHK", "3BHK", "4BHK"],
            },
            "Example Green",
        )

        self.assertEqual(
            [page["source_url"] for page in pages],
            [
                "https://www.squareyards.com/sale/resale-properties-in-example-green-bangalore",
                "https://www.squareyards.com/rent/property-for-rent-in-example-green-bangalore",
            ],
        )

    def test_external_listing_collection_normalizes_squareyards_project_page(self):
        output = collect_asset_sources(
            {
                "partition": {"parts": [["dt", "2026-07-16"]]},
                "planned_at": "2026-07-16T09:30:00Z",
                "requested_assets": ["external_listings_weekly"],
                "source_entities": [
                    {
                        "entity_id": "society:example-green",
                        "name": "Example Green",
                        "area": "Whitefield",
                        "external_listing_source_pages": [
                            {
                                "source_name": "SquareYards",
                                "source_url": "https://www.squareyards.com/rent/property-for-rent-in-example-green-bangalore",
                                "query_kind": "rent",
                                "text": """
                                    Other Project
                                    ## 2 BHK Flat for Rent in Whitefield, Bangalore

                                    **₹ 90,000**/ Per Month

                                    Config 2 BHK + 2 Bath

                                    Area Built-up Area

                                     1700

                                    Sq.Ft.

                                    Example Green
                                    ## 3 BHK Flat for Rent in Whitefield, Bangalore

                                    **₹ 1.4 L**/ Per Month

                                    Config 3 BHK + 3 Bath

                                    Area Built-up Area

                                     1800

                                    Sq.Ft.
                                """,
                            }
                        ],
                    }
                ],
            }
        )

        self.assertEqual(len(output["external_listings_weekly"]["records"]), 1)
        listing = output["external_listings_weekly"]["records"][0]
        self.assertEqual(listing["source_name"], "SquareYards")
        self.assertEqual(listing["listing_type"], "rent")
        self.assertEqual(listing["price"], 140_000)
        self.assertEqual(listing["area_sqft"], 1800)
        self.assertEqual(listing["area_type"], "built-up")
        self.assertEqual(listing["bathrooms"], 3.0)

    def test_external_listing_collection_normalizes_squareyards_html(self):
        output = collect_asset_sources(
            {
                "partition": {"parts": [["dt", "2026-07-16"]]},
                "planned_at": "2026-07-16T09:30:00Z",
                "requested_assets": ["external_listings_weekly"],
                "source_entities": [
                    {
                        "entity_id": "society:example-green",
                        "name": "Example Green",
                        "area": "Whitefield",
                        "external_listing_source_pages": [
                            {
                                "source_name": "SquareYards",
                                "source_url": "https://www.squareyards.com/sale/resale-properties-in-example-green-bangalore",
                                "html": """
                                    <html><body>
                                      <article class="listing-card single-box-conversion">
                                        <span class="project-name">Example Green</span>
                                        <h2 class="heading">
                                          <span>3 BHK Flat for Sale in Hoodi, Bangalore</span>
                                        </h2>
                                        <p class="listing-price"><strong>₹ 1.4 Cr</strong></p>
                                        <dl class="listing-attributes">
                                          <div class="attribute-item"><dt><em class="icon-bed"></em>Config</dt><dd>3 BHK + 3 Bath</dd></div>
                                          <div class="attribute-item unit-drop"><dt><em class="icon-unit-size"></em>Area <small>Built-up Area</small></dt><dd><div class="unit-value avail-area" data-area="1800">1800</div></dd></div>
                                          <div class="attribute-item"><dt><em class="icon-stairs"></em>Floor</dt><dd>10th of 20 Floors</dd></div>
                                        </dl>
                                      </article>
                                    </body></html>
                                """,
                            }
                        ],
                    }
                ],
            }
        )

        listing = output["external_listings_weekly"]["records"][0]
        self.assertEqual(listing["source_name"], "SquareYards")
        self.assertEqual(listing["price"], 14_000_000)
        self.assertEqual(listing["area_sqft"], 1800)
        self.assertEqual(listing["area_type"], "built-up")
        self.assertEqual(listing["bathrooms"], 3.0)
        self.assertEqual(listing["floor"], "10th of 20 Floors")
        self.assertEqual(listing["locality"], "Hoodi")

    def test_external_listing_collection_falls_back_to_direct_html_fetch(self):
        class Response:
            status = 200

            def __enter__(self):
                return self

            def __exit__(self, *_args):
                return False

            def read(self):
                return b"""
                    <html><body>
                      <h2 class="mb-srp__card--title" title="3 BHK Flat for Sale in Example Green, Whitefield, Bangalore">3 BHK Flat</h2>
                      <div class="mb-srp__card__summary--label">Super Area</div>
                      <div class="mb-srp__card__summary--value">2,000 sqft</div>
                      <div class="mb-srp__card__price--amount"><span class="rupees">\xe2\x82\xb9</span>2.5 Cr</div>
                    </body></html>
                """

        def fake_urlopen(request, timeout):
            url = request.full_url
            if url.startswith("https://r.jina.ai/"):
                raise HTTPError(url, 403, "Forbidden", {}, BytesIO())
            return Response()

        with patch(
            "pipeline.sources.external_listings.urllib.request.urlopen",
            side_effect=fake_urlopen,
        ):
            output = collect_external_listings(
                {
                    "partition": {"parts": [["dt", "2026-07-16"]]},
                    "planned_at": "2026-07-16T09:30:00Z",
                    "source_entities": [
                        {
                            "entity_id": "society:example-green",
                            "name": "Example Green",
                            "area": "Whitefield",
                            "city": "Bengaluru",
                        }
                    ],
                }
            )

        listing = output["records"][0]
        self.assertEqual(listing["price"], 25_000_000)
        watermarks = {
            watermark["source"]: watermark["high_watermark"]
            for watermark in output["source_watermarks"]
        }
        self.assertIn("direct_fallbacks=", watermarks["external_listing_fetch_coverage"])

    def test_external_listing_collection_strips_markdown_images_from_locality(self):
        output = collect_asset_sources(
            {
                "partition": {"parts": [["dt", "2026-07-16"]]},
                "planned_at": "2026-07-16T09:30:00Z",
                "requested_assets": ["external_listings_weekly"],
                "source_entities": [
                    {
                        "entity_id": "society:example-green",
                        "name": "Example Green",
                        "area": "Whitefield",
                        "external_listing_source_pages": [
                            {
                                "source_name": "SquareYards",
                                "source_url": "https://www.squareyards.com/sale/resale-properties-in-example-green-bangalore",
                                "text": """
                                    Example Green
                                    ## 3 BHK Flat for Sale in Varthur, Bangalore _![Image 118: Location map of 3 BHK Flat for Sale in Varthur, Bangalore located at 12.929851, 77.740097](https://www.squareyards.com/assets/images/map-icon.png)_

                                    **₹ 1.4 Cr**

                                    Config 3 BHK + 3 Bath

                                    Area Built-up Area

                                     1800

                                    Sq.Ft.
                                """,
                            }
                        ],
                    }
                ],
            }
        )

        listing = output["external_listings_weekly"]["records"][0]
        self.assertEqual(listing["locality"], "Varthur")

    def test_external_image_collection_extracts_magicbricks_images(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            output = collect_asset_sources(
                {
                    "project_root": temp_dir,
                    "partition": {"parts": [["dt", "2026-07-16"]]},
                    "planned_at": "2026-07-16T09:30:00Z",
                    "requested_assets": ["external_images_weekly"],
                    "source_entities": [
                        {
                            "entity_id": "society:example-green",
                            "name": "Example Green",
                            "project_key": "PRM-EXAMPLE-GREEN",
                            "image_source_pages": [
                                {
                                    "source_name": "magicbricks",
                                    "source_page_url": "https://www.magicbricks.com/example-green",
                                    "reject_url_patterns": ["Photo_h300_w450"],
                                    "html": """
                                        <html>
                                          <img data-src="https://img.staticmb.com/mbimages/project/example-green-elevation.jpg" alt="Example Green elevation" width="1200" height="800">
                                          <img src="//img.staticmb.com/mbimages/project/example-green-clubhouse.webp" alt="Example Green clubhouse">
                                          <img src="https://img.staticmb.com/mbimages/project/Photo_h300_w450/example-green-tower.jpg" alt="Example Green tower">
                                        </html>
                                    """,
                                }
                            ],
                        }
                    ],
                }
            )

        records = output["external_images_weekly"]["records"]
        self.assertEqual(output["external_images_weekly"]["snapshot_date"], "2026-07-16")
        self.assertEqual(len(records), 3)
        elevation = next(
            record
            for record in records
            if record["image_url"].endswith("example-green-elevation.jpg")
        )
        thumbnail = next(
            record
            for record in records
            if "Photo_h300_w450" in record["image_url"]
        )
        self.assertEqual(elevation["entity_id"], "society:example-green")
        self.assertEqual(elevation["source_name"], "magicbricks")
        self.assertEqual(elevation["source_page_url"], "https://www.magicbricks.com/example-green")
        self.assertEqual(
            elevation["image_url"],
            "https://img.staticmb.com/mbimages/project/example-green-elevation.jpg",
        )
        self.assertEqual(elevation["image_kind"], "exterior")
        self.assertEqual(elevation["width"], 1200)
        self.assertEqual(elevation["height"], 800)
        self.assertEqual(elevation["storage_policy"], "link_only")
        self.assertNotIn("confidence", elevation)
        self.assertEqual(thumbnail["reject_reason"], "reject_pattern:Photo_h300_w450")
        self.assertEqual(thumbnail["allowed_slots"], [])

    def test_external_image_collection_classifies_houssed_media_slots(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            output = collect_asset_sources(
                {
                    "project_root": temp_dir,
                    "partition": {"parts": [["dt", "2026-07-16"]]},
                    "planned_at": "2026-07-16T09:30:00Z",
                    "requested_assets": ["external_images_weekly"],
                    "source_entities": [
                        {
                            "entity_id": "society:godrej-splendour",
                            "name": "Godrej Splendour",
                            "image_source_pages": [
                                {
                                    "source_name": "Houssed",
                                    "source_page_url": "https://houssed.com/bangalore/godrej-properties/godrej-splendour-2579",
                                    "html": """
                                      <img src="https://imgcdn.houssed.com/assets/Files/Projects/2579/Project%20Image/tower.webp" alt="Godrej Splendour project image" width="1200" height="675">
                                      <img src="https://imgcdn.houssed.com/assets/Files/Projects/2579/Amenities/pool.webp" alt="Amenities" width="780" height="441">
                                      <img src="https://imgcdn.houssed.com/assets/Files/Projects/2579/BHK_Configuration/floor.webp" alt="1 BHK Flat">
                                      <img src="https://imgcdn.houssed.com/assets/Files/Projects/2579/Master%20Plan/master.webp" alt="Master Plan">
                                      <img src="https://imgcdn.houssed.com/assets/Files/Developer/FirmLogos/godrej.webp" alt="Godrej logo">
                                    """,
                                }
                            ],
                        }
                    ],
                }
            )

        records = output["external_images_weekly"]["records"]
        by_kind = {record["candidate_kind"]: record for record in records}
        self.assertIn("exterior", by_kind)
        self.assertIn("amenity", by_kind)
        self.assertIn("floor_plan", by_kind)
        self.assertIn("site_plan", by_kind)
        self.assertIn("hero", by_kind["exterior"]["allowed_slots"])
        self.assertIn("gallery", by_kind["amenity"]["allowed_slots"])
        self.assertEqual(by_kind["floor_plan"]["allowed_slots"], ["floor_plan"])
        self.assertEqual(by_kind["site_plan"]["allowed_slots"], ["site_plan"])
        self.assertEqual(by_kind["logo"]["reject_reason"], "kind:logo")
        report = output["external_images_weekly"]["media_qa_report"]
        self.assertEqual(report["entities"]["society:godrej-splendour"]["candidate_count"], 5)

    def test_external_image_collection_records_source_health_on_fetch_failure(self):
        def fake_fetch(url, source_name=None):
            return None

        with tempfile.TemporaryDirectory() as temp_dir:
            with patch("pipeline.sources.external_images.fetch_page_text", side_effect=fake_fetch):
                output = collect_asset_sources(
                    {
                        "project_root": temp_dir,
                        "partition": {"parts": [["dt", "2026-07-16"]]},
                        "planned_at": "2026-07-16T09:30:00Z",
                        "requested_assets": ["external_images_weekly"],
                        "source_entities": [
                            {
                                "entity_id": "society:example-green",
                                "name": "Example Green",
                                "image_source_pages": [
                                    {
                                        "source_name": "MagicBricks",
                                        "source_page_url": "https://www.magicbricks.com/example-green",
                                    }
                                ],
                            }
                        ],
                    }
                )

        self.assertEqual(output["external_images_weekly"]["records"], [])
        self.assertEqual(
            output["external_images_weekly"]["source_health"][0]["status"],
            "fetch_failed",
        )

    def test_external_image_collection_prefers_downloaded_society_photos(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            photo_dir = (
                Path(temp_dir)
                / "frontend"
                / "public"
                / "societies"
                / "example-green"
            )
            photo_dir.mkdir(parents=True)
            (photo_dir / "1.jpg").write_bytes(b"local-photo-one")
            (photo_dir / "2.jpg").write_bytes(b"local-photo-two")

            output = collect_asset_sources(
                {
                    "project_root": temp_dir,
                    "partition": {"parts": [["dt", "2026-07-16"]]},
                    "planned_at": "2026-07-16T09:30:00Z",
                    "requested_assets": ["external_images_weekly"],
                    "source_entities": [
                        {
                            "entity_id": "society:example-green",
                            "name": "Example Green",
                            "project_key": "PRM-EXAMPLE-GREEN",
                            "image_source_pages": [
                                {
                                    "source_name": "SquareYards",
                                    "source_page_url": "https://www.squareyards.com/example-green",
                                    "html": """
                                        <img src="https://img.squareyards.com/secondaryPortal/optImages/example-green-room.jpg?aio=w-300;h-300;fill;">
                                    """,
                                }
                            ],
                        }
                    ],
                }
            )

        records = output["external_images_weekly"]["records"]
        self.assertEqual(len(records), 3)
        self.assertEqual(records[0]["image_url"], "/societies/example-green/1.jpg")
        self.assertEqual(records[0]["source_name"], "LocalSocietyPhotos")
        self.assertEqual(records[0]["storage_policy"], "static_public_asset")
        self.assertEqual(records[1]["image_url"], "/societies/example-green/2.jpg")
        self.assertEqual(records[2]["source_name"], "SquareYards")
        self.assertGreater(records[2]["rank"], records[1]["rank"])

    def test_external_image_collection_uses_name_slug_for_rera_hash_entities(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            photo_dir = (
                Path(temp_dir)
                / "frontend"
                / "public"
                / "societies"
                / "prestige-waterford"
            )
            photo_dir.mkdir(parents=True)
            (photo_dir / "1.jpg").write_bytes(b"local-photo-one")

            output = collect_asset_sources(
                {
                    "project_root": temp_dir,
                    "partition": {"parts": [["dt", "2026-07-16"]]},
                    "planned_at": "2026-07-16T09:30:00Z",
                    "requested_assets": ["external_images_weekly"],
                    "source_entities": [
                        {
                            "entity_id": "society:rera-53c0b81882e6acc6",
                            "name": "Prestige Waterford",
                            "image_source_pages": [
                                {
                                    "source_name": "dry-run",
                                    "source_page_url": "https://example.invalid/waterford",
                                    "html": "<html></html>",
                                }
                            ],
                        }
                    ],
                }
            )

        records = output["external_images_weekly"]["records"]
        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["image_url"], "/societies/prestige-waterford/1.jpg")

    def test_external_image_collection_skips_portals_when_local_gallery_is_complete(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            photo_dir = (
                Path(temp_dir)
                / "frontend"
                / "public"
                / "societies"
                / "example-green"
            )
            photo_dir.mkdir(parents=True)
            for index in range(1, 6):
                (photo_dir / "{}.jpg".format(index)).write_bytes(
                    "local-photo-{}".format(index).encode("utf-8")
                )

            output = collect_asset_sources(
                {
                    "project_root": temp_dir,
                    "partition": {"parts": [["dt", "2026-07-16"]]},
                    "planned_at": "2026-07-16T09:30:00Z",
                    "requested_assets": ["external_images_weekly"],
                    "source_entities": [
                        {
                            "entity_id": "society:example-green",
                            "name": "Example Green",
                            "image_source_pages": [
                                {
                                    "source_name": "SquareYards",
                                    "source_page_url": "https://www.squareyards.com/example-green",
                                    "html": """
                                        <img src="https://img.squareyards.com/secondaryPortal/optImages/example-green-room.jpg?aio=w-300;h-300;fill;">
                                    """,
                                }
                            ],
                        }
                    ],
                }
            )

        records = output["external_images_weekly"]["records"]
        self.assertEqual(len(records), 5)
        self.assertEqual(records[0]["source_name"], "LocalSocietyPhotos")
        self.assertTrue(
            all(record["image_url"].startswith("/societies/example-green/") for record in records)
        )

    def test_external_image_collection_can_fill_local_society_photos_from_policy(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            policy_dir = (
                Path(temp_dir) / "app" / "config" / "dag" / "crawl_policies"
            )
            policy_dir.mkdir(parents=True)
            (policy_dir / "local_society_photo_collection.json").write_text(
                json.dumps(
                    {
                        "enabled": True,
                        "target_images": 5,
                        "skip_env": "OPENESTATES_SKIP_LOCAL_SOCIETY_PHOTO_COLLECTION",
                    }
                )
            )

            def fake_fetch(**kwargs):
                photo_dir = (
                    Path(temp_dir)
                    / "frontend"
                    / "public"
                    / "societies"
                    / "example-green"
                )
                photo_dir.mkdir(parents=True)
                (photo_dir / "1.jpg").write_bytes(b"\xff\xd8\xfflocal-photo")
                return {
                    "entity_id": kwargs["entity_id"],
                    "all_photos": ["/societies/example-green/1.jpg"],
                }

            os.environ.pop("OPENESTATES_SKIP_LOCAL_SOCIETY_PHOTO_COLLECTION", None)
            with patch("pipeline.skills.fetch_images.fetch_images_for_entity", side_effect=fake_fetch) as fetch:
                output = collect_asset_sources(
                    {
                        "project_root": temp_dir,
                        "partition": {"parts": [["dt", "2026-07-16"]]},
                        "planned_at": "2026-07-16T09:30:00Z",
                        "requested_assets": ["external_images_weekly"],
                        "source_entities": [
                            {
                                "entity_id": "society:example-green",
                                "name": "Example Green",
                                "project_key": "PRM-EXAMPLE-GREEN",
                                "image_source_pages": [
                                    {
                                        "source_name": "dry-run",
                                        "source_page_url": "https://example.invalid/example-green",
                                        "html": "<html></html>",
                                    }
                                ],
                            }
                        ],
                    }
                )

        fetch.assert_called_once()
        records = output["external_images_weekly"]["records"]
        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["image_url"], "/societies/example-green/1.jpg")
        self.assertEqual(records[0]["storage_policy"], "static_public_asset")

    def test_external_image_collection_uses_magicbricks_project_page_fallback(self):
        html = """
            <html>
              <img data-src="https://img.staticmb.com/mbimages/project/example-green-tower.jpg" alt="Example Green tower">
            </html>
        """
        def fake_fetch(url, source_name=None):
            return html if "magicbricks" in url else None

        with patch("pipeline.sources.external_images.fetch_page_text", side_effect=fake_fetch) as fetch:
            output = collect_asset_sources(
                {
                    "partition": {"parts": [["dt", "2026-07-16"]]},
                    "planned_at": "2026-07-16T09:30:00Z",
                    "requested_assets": ["external_images_weekly"],
                    "source_entities": [
                        {
                            "entity_id": "society:example-green",
                            "name": "Example Green",
                            "project_key": "PRM-EXAMPLE-GREEN",
                            "city": "Bengaluru",
                        }
                    ],
                }
        )

        records = output["external_images_weekly"]["records"]
        fetch.assert_any_call(
            "https://www.magicbricks.com/project-example-green-for-sale-in-bangalore-pppfs",
            "MagicBricks",
        )
        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["source_name"], "MagicBricks")
        self.assertEqual(
            records[0]["source_page_url"],
            "https://www.magicbricks.com/project-example-green-for-sale-in-bangalore-pppfs",
        )
        self.assertEqual(records[0]["image_kind"], "exterior")

    def test_external_image_collection_uses_squareyards_project_page_fallback(self):
        html = """
            ![Image 1: Room in 3 BHK Apartment at Example Green](https://img.squareyards.com/secondaryPortal/optImages/example-green-room.jpg?aio=w-300;h-300;fill;)**Agent**
            ![Image 2](https://img.squareyards.com/connect/profilepic/agent.jpg?aio=w-32;h-32;crop;)**Agent**
            ![Image 3](https://static.squareyards.com/ui-assets/images/app-store.png)**App**
            ![Image 4](https://www.squareyards.com/assets/images/qr-code/qr-code.png)**QR**
        """

        with tempfile.TemporaryDirectory() as temp_dir:
            output = collect_asset_sources(
                {
                    "project_root": temp_dir,
                    "partition": {"parts": [["dt", "2026-07-16"]]},
                    "planned_at": "2026-07-16T09:30:00Z",
                    "requested_assets": ["external_images_weekly"],
                    "source_entities": [
                        {
                            "entity_id": "society:example-green",
                            "name": "Example Green",
                            "project_key": "PRM-EXAMPLE-GREEN",
                            "image_source_pages": [
                                {
                                    "source_name": "SquareYards",
                                    "source_page_url": "https://www.squareyards.com/rent/property-for-rent-in-example-green-bangalore",
                                    "html": html,
                                }
                            ],
                        }
                    ],
                }
            )

        records = output["external_images_weekly"]["records"]
        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["source_name"], "SquareYards")
        self.assertEqual(
            records[0]["image_url"],
            "https://img.squareyards.com/secondaryPortal/optImages/example-green-room.jpg?aio=w-300;h-300;fill;",
        )

    def test_media_classifier_rejects_watermarked_source_images(self):
        policy = {
            "classification": {
                "watermark_source_rejects": ["SquareYards"],
                "watermark_reject_patterns": ["square yards", "squareyards"],
            },
            "promotion_slots": {
                "hero": ["exterior"],
                "gallery": ["exterior"],
            },
            "reject_kinds": [],
        }

        qa = classify_media_candidate(
            image_url="https://img.squareyards.com/secondaryPortal/optImages/example-green.jpg",
            original_image_url="https://img.squareyards.com/secondaryPortal/optImages/example-green.jpg",
            alt_text="Example Green exterior",
            width=1200,
            height=800,
            source_name="SquareYards",
            source_bucket=None,
            source_page={"source_name": "SquareYards"},
            policy=policy,
            score=80.0,
        )

        self.assertEqual(qa["reject_reason"], "watermark:squareyards")
        self.assertEqual(qa["allowed_slots"], [])
        self.assertEqual(qa["quality_score"], 0.0)
        self.assertEqual(qa["relevance_score"], 0.0)

    def test_media_classifier_rejects_configured_local_content_hashes(self):
        policy = {
            "classification": {
                "rejected_content_sha256": ["abc123"],
            },
            "promotion_slots": {
                "hero": ["exterior"],
                "gallery": ["exterior"],
            },
            "reject_kinds": [],
        }

        qa = classify_media_candidate(
            image_url="/societies/example-green/1.jpg",
            original_image_url="/societies/example-green/1.jpg",
            alt_text="Example Green photo 1",
            width=1200,
            height=800,
            source_name="LocalSocietyPhotos",
            source_bucket="local_society_photo",
            source_page={},
            policy=policy,
            score=100.0,
            content_sha256="abc123",
        )

        self.assertEqual(qa["reject_reason"], "watermark:content_sha256")
        self.assertEqual(qa["allowed_slots"], [])

    def test_local_society_photos_use_provenance_for_watermark_rejection(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            policy_dir = root / "app" / "config" / "dag" / "crawl_policies"
            policy_dir.mkdir(parents=True)
            (policy_dir / "media_source_policy.json").write_text(
                json.dumps(
                    {
                        "enabled": True,
                        "classification": {
                            "watermark_source_rejects": ["SquareYards"],
                            "watermark_reject_patterns": ["squareyards"],
                        },
                        "promotion_slots": {
                            "hero": ["exterior"],
                            "gallery": ["exterior"],
                        },
                        "reject_kinds": [],
                        "sources": [],
                    }
                )
            )
            (policy_dir / "local_society_photo_collection.json").write_text(
                json.dumps({"enabled": True, "target_images": 1})
            )
            photo_dir = root / "frontend" / "public" / "societies" / "example-green"
            photo_dir.mkdir(parents=True)
            (photo_dir / "1.jpg").write_bytes(b"local-photo-one")
            metadata_dir = root / "data" / "cache" / "image_metadata"
            metadata_dir.mkdir(parents=True)
            (metadata_dir / "example-green.json").write_text(
                json.dumps(
                    {
                        "sources": [
                            {
                                "file": "1.jpg",
                                "source_page_url": "https://www.squareyards.com/example-green/project",
                                "original_image_url": "https://static.squareyards.com/resources/images/example-green.jpg",
                            }
                        ]
                    }
                )
            )

            output = collect_asset_sources(
                {
                    "project_root": temp_dir,
                    "partition": {"parts": [["dt", "2026-07-16"]]},
                    "planned_at": "2026-07-16T09:30:00Z",
                    "requested_assets": ["external_images_weekly"],
                    "source_entities": [
                        {
                            "entity_id": "society:example-green",
                            "name": "Example Green",
                        }
                    ],
                }
            )

        records = output["external_images_weekly"]["records"]
        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["image_url"], "/societies/example-green/1.jpg")
        self.assertEqual(
            records[0]["original_image_url"],
            "https://static.squareyards.com/resources/images/example-green.jpg",
        )
        self.assertEqual(records[0]["reject_reason"], "watermark:squareyards")
        self.assertEqual(records[0]["allowed_slots"], [])

    def test_external_image_optimizer_writes_webp_preview(self):
        try:
            from PIL import Image
        except Exception:
            self.skipTest("Pillow is not installed")

        os.environ.pop("OPENESTATES_SKIP_IMAGE_OPTIMIZATION", None)
        image_buffer = BytesIO()
        Image.new("RGB", (1800, 1200), color=(80, 120, 160)).save(
            image_buffer, format="JPEG"
        )

        with tempfile.TemporaryDirectory() as temp_dir:
            result = write_optimized_preview(
                project_root=Path(temp_dir),
                entity_id="society:example-green",
                image_url="https://img.example.com/example-green-elevation.jpg",
                source_page_url="https://www.magicbricks.com/example-green",
                image_bytes=image_buffer.getvalue(),
            )

            self.assertIsNotNone(result)
            assert result is not None
            self.assertTrue(result["preview_url"].startswith("/media/previews/"))
            preview_path = Path(result["preview_path"])
            self.assertTrue(preview_path.exists())
            self.assertEqual(preview_path.suffix, ".webp")
            self.assertLessEqual(result["width"], 1280)
            self.assertLessEqual(result["height"], 960)

    def test_external_image_optimization_is_disabled_until_explicitly_enabled(self):
        os.environ.pop("OPENESTATES_SKIP_IMAGE_OPTIMIZATION", None)
        os.environ.pop("OPENESTATES_ENABLE_IMAGE_OPTIMIZATION", None)

        self.assertTrue(skip_image_optimization())

        os.environ["OPENESTATES_ENABLE_IMAGE_OPTIMIZATION"] = "1"
        self.assertFalse(skip_image_optimization())

    def test_stale_google_collection_forces_skill_refresh(self):
        calls = []
        result = SkillResult(
            facts=[
                SourcedFact(
                    key="google_reviews_url",
                    value={"type": "Text", "data": "https://maps.google.com/fresh"},
                    confidence=0.8,
                    source=FactSource(source_type="Google"),
                    learned_at="2026-07-14T09:30:00Z",
                )
            ],
            confidence=0.8,
        )
        skill = SimpleNamespace(
            run=lambda _input, force=False: calls.append(force) or result
        )
        collect_google_places(
            {
                "partition": {"parts": [["dt", "2026-07-14"]]},
                "planned_at": "2026-07-14T09:30:00Z",
                "force_refresh_assets": ["google_places_weekly"],
            },
            society_inputs={
                "canonical": {
                    "entity_id": "society:rera-canonical",
                    "society_name": "Canonical Green",
                }
            },
            skill=skill,
        )
        self.assertEqual(calls, [True])

    def test_google_collection_emits_raw_place_rows_and_preserves_cached_timestamp(self):
        result = SkillResult(
            facts=[
                SourcedFact(
                    key="google_reviews_url",
                    value={"type": "Text", "data": "https://maps.google.com/example"},
                    confidence=0.84,
                    source=FactSource(source_type="Google"),
                    learned_at="2026-07-12T08:15:00Z",
                ),
                SourcedFact(
                    key="google_place_id",
                    value={"type": "Text", "data": "place-123"},
                    confidence=0.84,
                    source=FactSource(source_type="Google"),
                    learned_at="2026-07-12T08:15:00Z",
                ),
                SourcedFact(
                    key="google_place_address",
                    value={"type": "Text", "data": "Example Green, ECC Road, Bengaluru"},
                    confidence=0.84,
                    source=FactSource(source_type="Google"),
                    learned_at="2026-07-12T08:15:00Z",
                ),
                SourcedFact(
                    key="google_rating",
                    value={"type": "Numeric", "data": 4.4},
                    confidence=0.84,
                    source=FactSource(source_type="Google"),
                    learned_at="2026-07-12T08:15:00Z",
                ),
            ],
            confidence=0.84,
            cost=SkillCost(api_calls=0),
            cached=True,
        )
        skill = SimpleNamespace(run=lambda _input, force=False: result)
        output = collect_google_places(
            {
                "partition": {"parts": [["dt", "2026-07-14"]]},
                "planned_at": "2026-07-14T09:30:00Z",
            },
            society_inputs={
                "example-green": {
                    "entity_id": "society:rera-example-green",
                    "project_key": "PRM-EXAMPLE-GREEN",
                    "society_name": "Example Green",
                    "area": "Whitefield",
                    "city": "Bengaluru",
                    "address": "Whitefield, Bengaluru",
                }
            },
            skill=skill,
        )

        record = output["records"][0]
        self.assertEqual(record["entity_id"], "society:rera-example-green")
        self.assertEqual(record["project_key"], "PRM-EXAMPLE-GREEN")
        self.assertEqual(record["query"], "Example Green Whitefield Bengaluru")
        self.assertEqual(record["place_id"], "place-123")
        self.assertEqual(record["address"], "Example Green, ECC Road, Bengaluru")
        self.assertEqual(record["rating"], 4.4)
        self.assertEqual(record["fetched_at"], "2026-07-12T08:15:00Z")
        self.assertEqual(record["fetch_source"], "fetch_google_review_links_cache")

    def test_google_places_api_key_collects_precise_place_metadata(self):
        search_payload = {
            "places": [
                {
                    "id": "places/example-green",
                    "displayName": {"text": "Example Green"},
                    "formattedAddress": "Whitefield, Bengaluru",
                    "googleMapsUri": "https://maps.google.com/?cid=123",
                    "location": {"latitude": 12.97, "longitude": 77.75},
                    "primaryType": "housing_complex",
                    "types": ["housing_complex", "establishment"],
                    "rating": 4.5,
                    "userRatingCount": 812,
                }
            ]
        }
        detail_payload = {
            "id": "places/example-green",
            "displayName": {"text": "Example Green"},
            "formattedAddress": "Whitefield, Bengaluru",
            "googleMapsUri": "https://maps.google.com/?cid=123",
            "location": {"latitude": 12.97, "longitude": 77.75},
            "primaryType": "housing_complex",
            "types": ["housing_complex", "establishment"],
            "rating": 4.5,
            "userRatingCount": 812,
            "reviews": [
                {"text": {"text": "Good clubhouse and green campus."}},
                {"originalText": {"text": "Traffic can be slow at peak hours."}},
            ],
        }
        responses = [search_payload, detail_payload]

        with tempfile.TemporaryDirectory() as temp_dir:
            with patch.dict("os.environ", {"GOOGLE_PLACES_API_KEY": "test-key"}):
                with patch(
                    "pipeline.skills.fetch_google_review_links.urllib.request.urlopen",
                    side_effect=lambda *_args, **_kwargs: BytesIO(
                        json.dumps(responses.pop(0)).encode("utf-8")
                    ),
                ) as mocked_open:
                    skill = FetchGoogleReviewLinksSkill(cache_dir=Path(temp_dir))
                    output = collect_google_places(
                        {
                            "partition": {"parts": [["dt", "2026-07-14"]]},
                            "planned_at": "2026-07-14T09:30:00Z",
                        },
                        society_inputs={
                            "example-green": {
                                "entity_id": "society:rera-example-green",
                                "project_key": "PRM-EXAMPLE-GREEN",
                                "society_name": "Example Green",
                                "area": "Whitefield",
                                "city": "Bengaluru",
                            }
                        },
                        skill=skill,
                    )

        search_request = mocked_open.call_args_list[0][0][0]
        detail_request = mocked_open.call_args_list[1][0][0]
        self.assertEqual(
            search_request.full_url, "https://places.googleapis.com/v1/places:searchText"
        )
        self.assertEqual(search_request.get_header("X-goog-api-key"), "test-key")
        self.assertIn(
            "places.userRatingCount", search_request.get_header("X-goog-fieldmask")
        )
        self.assertEqual(
            detail_request.full_url,
            "https://places.googleapis.com/v1/places/example-green",
        )
        self.assertIn("reviews", detail_request.get_header("X-goog-fieldmask"))
        record = output["records"][0]
        self.assertEqual(record["place_id"], "places/example-green")
        self.assertEqual(record["reviews_url"], "https://maps.google.com/?cid=123")
        self.assertEqual(record["rating"], 4.5)
        self.assertEqual(record["review_count"], 812)
        self.assertEqual(
            record["review_snippets"],
            [
                "Good clubhouse and green campus.",
                "Traffic can be slow at peak hours.",
            ],
        )
        self.assertEqual(record["confidence"], 0.85)
        self.assertEqual(record["fetch_source"], "google_places_text_search")

    def test_google_nearby_collection_emits_raw_category_rows(self):
        self.assertNotIn("stormwater_drain", google_nearby_collection_categories())
        output = collect_google_nearby_places(
            {
                "partition": {"parts": [["dt", "2026-07-14"]]},
                "planned_at": "2026-07-14T09:30:00Z",
            },
            society_inputs={
                "example-green": {
                    "entity_id": "society:rera-example-green",
                    "project_key": "PRM-EXAMPLE-GREEN",
                    "society_name": "Example Green",
                    "area": "Whitefield",
                    "city": "Bengaluru",
                }
            },
            nearby_fetch=lambda _input, category: [
                {
                    "place_name": "Example {}".format(category),
                    "place_id": "{}-1".format(category),
                    "place_url": "https://maps.google.com/{}".format(category),
                    "distance_km": 1.2,
                    "latitude": 12.972,
                    "longitude": 77.596,
                    "rating": 4.2,
                    "review_count": 42,
                    "primary_type": category,
                    "place_types": [category],
                    "confidence": 0.8,
                    "fetched_at": "2026-07-14T09:35:00Z",
                    "fetch_source": "fixture_nearby",
                }
            ],
        )

        self.assertEqual(output["snapshot_date"], "2026-07-14")
        self.assertEqual(
            len(output["records"]), len(google_nearby_collection_categories())
        )
        school = output["records"][0]
        self.assertEqual(school["entity_id"], "society:rera-example-green")
        self.assertEqual(school["project_key"], "PRM-EXAMPLE-GREEN")
        self.assertEqual(school["query"], "schools near Example Green Whitefield Bengaluru")
        self.assertEqual(school["category"], "school")
        self.assertEqual(school["place_name"], "Example school")
        self.assertEqual(school["distance_km"], 1.2)
        self.assertEqual(school["latitude"], 12.972)
        self.assertEqual(school["longitude"], 77.596)
        self.assertEqual(school["primary_type"], "school")
        self.assertEqual(school["place_types"], ["school"])
        self.assertEqual(school["fetch_source"], "fixture_nearby")

    def test_google_nearby_collection_skips_societies_without_coordinates(self):
        calls = []

        def fake_nearby_fetch(input_data, category):
            calls.append((input_data["society_name"], category))
            if input_data["society_name"] == "Missing Coordinates":
                raise ValueError(
                    "Google nearby collection requires an accepted origin coordinate pair"
                )
            return [
                {
                    "place_name": "Example {}".format(category),
                    "place_url": "https://maps.google.com/{}".format(category),
                }
            ]

        output = collect_google_nearby_places(
            {
                "partition": {"parts": [["dt", "2026-07-14"]]},
                "planned_at": "2026-07-14T09:30:00Z",
            },
            society_inputs={
                "missing": {
                    "entity_id": "society:rera-missing",
                    "society_name": "Missing Coordinates",
                    "area": "Whitefield",
                    "city": "Bengaluru",
                },
                "valid": {
                    "entity_id": "society:rera-valid",
                    "society_name": "Valid Coordinates",
                    "area": "Whitefield",
                    "city": "Bengaluru",
                },
            },
            nearby_fetch=fake_nearby_fetch,
        )

        self.assertEqual(len(output["records"]), len(google_nearby_collection_categories()))
        self.assertEqual(
            calls,
            [("Missing Coordinates", "school")]
            + [
                ("Valid Coordinates", category)
                for category in google_nearby_collection_categories()
            ],
        )
        self.assertTrue(
            all(record["entity_id"] == "society:rera-valid" for record in output["records"])
        )

    def test_google_nearby_collection_uses_places_api_by_default(self):
        requests = []

        def fake_urlopen(request, timeout=20):
            requests.append(request)
            body = json.loads(request.data.decode("utf-8"))
            query = body["textQuery"]
            payload = {
                "places": [
                    {
                        "id": "places/origin",
                        "displayName": {"text": "Example Green"},
                        "googleMapsUri": "https://maps.google.com/?cid=origin",
                        "location": {"latitude": 12.9716, "longitude": 77.5946},
                        "types": ["residential_complex"],
                    }
                ]
            }
            if query.startswith("schools near"):
                payload["places"][0].update(
                    {
                        "id": "places/school",
                        "displayName": {"text": "Example School"},
                        "googleMapsUri": "https://maps.google.com/?cid=school",
                        "primaryType": "school",
                        "types": ["school"],
                        "rating": 4.1,
                        "userRatingCount": 120,
                    }
                )
            elif query.startswith("metro near"):
                payload["places"][0].update(
                    {
                        "displayName": {"text": "Example Metro Station"},
                        "primaryType": "subway_station",
                        "types": ["subway_station", "transit_station"],
                    }
                )
            elif query.startswith("hospitals near"):
                payload["places"][0].update(
                    {
                        "displayName": {"text": "Example Hospital"},
                        "primaryType": "hospital",
                        "types": ["hospital"],
                    }
                )
            elif query.startswith("fitness near"):
                payload["places"][0].update(
                    {
                        "displayName": {"text": "Cult Whitefield"},
                        "primaryType": "gym",
                        "types": ["gym"],
                    }
                )
            elif query.startswith("tech parks and offices near"):
                payload["places"][0].update(
                    {
                        "displayName": {"text": "Example Tech Park"},
                        "primaryType": "business_center",
                        "types": ["business_center"],
                    }
                )
            return BytesIO(json.dumps(payload).encode("utf-8"))

        with patch.dict("os.environ", {"GOOGLE_PLACES_API_KEY": "test-key"}):
            with patch(
                "pipeline.skills.fetch_google_review_links.urllib.request.urlopen",
                side_effect=fake_urlopen,
            ):
                output = collect_google_nearby_places(
                    {
                        "partition": {"parts": [["dt", "2026-07-14"]]},
                        "planned_at": "2026-07-14T09:30:00Z",
                    },
                    society_inputs={
                        "example-green": {
                            "entity_id": "society:rera-example-green",
                            "project_key": "PRM-EXAMPLE-GREEN",
                            "society_name": "Example Green",
                            "area": "Whitefield",
                            "city": "Bengaluru",
                            "latitude": 12.9716,
                            "longitude": 77.5946,
                        }
                    },
                )

        self.assertEqual(len(requests), len(google_nearby_collection_categories()))
        requests_by_query = {
            json.loads(request.data.decode("utf-8"))["textQuery"]: request
            for request in requests
        }
        school_request = json.loads(
            requests_by_query["schools near Example Green Whitefield Bengaluru"].data.decode(
                "utf-8"
            )
        )
        self.assertEqual(
            school_request["locationBias"]["circle"]["center"]["latitude"], 12.9716
        )
        self.assertEqual(
            school_request["locationBias"]["circle"]["center"]["longitude"], 77.5946
        )
        self.assertEqual(school_request["locationBias"]["circle"]["radius"], 5000)
        metro_request = json.loads(
            requests_by_query[
                "metro near Example Green Whitefield Bengaluru"
            ].data.decode("utf-8")
        )
        self.assertEqual(metro_request["locationBias"]["circle"]["radius"], 6000)
        fitness_request = json.loads(
            requests_by_query[
                "fitness near Example Green Whitefield Bengaluru"
            ].data.decode("utf-8")
        )
        self.assertEqual(fitness_request["locationBias"]["circle"]["radius"], 3500)
        tech_park_request = json.loads(
            requests_by_query[
                "tech parks and offices near Example Green Whitefield Bengaluru"
            ].data.decode("utf-8")
        )
        self.assertEqual(
            tech_park_request["locationBias"]["circle"]["radius"],
            15000,
        )
        self.assertEqual(len(output["records"]), 5)
        school = output["records"][0]
        self.assertEqual(school["query"], "schools near Example Green Whitefield Bengaluru")
        self.assertEqual(school["place_name"], "Example School")
        self.assertEqual(school["place_id"], "places/school")
        self.assertEqual(school["distance_km"], 0.0)
        self.assertEqual(school["latitude"], 12.9716)
        self.assertEqual(school["longitude"], 77.5946)
        self.assertEqual(school["rating"], 4.1)
        self.assertEqual(school["review_count"], 120)
        self.assertEqual(school["primary_type"], "school")
        self.assertEqual(school["place_types"], ["school"])
        self.assertEqual(school["fetch_source"], "google_places_text_search_nearby")

    def test_google_nearby_filters_category_mismatches_before_raw_rows(self):
        nearby = {
            "places": [
                {
                    "id": "places/apartment",
                    "displayName": {"text": "Prestige Park View"},
                    "googleMapsUri": "https://maps.google.com/?cid=apartment",
                    "location": {"latitude": 12.9720, "longitude": 77.5946},
                    "primaryType": "apartment_complex",
                    "types": ["apartment_complex", "point_of_interest"],
                },
                {
                    "id": "places/gym",
                    "displayName": {"text": "Cult Example Gym"},
                    "googleMapsUri": "https://maps.google.com/?cid=gym",
                    "location": {"latitude": 12.9730, "longitude": 77.5946},
                    "primaryType": "gym",
                    "types": ["gym"],
                },
            ]
        }

        with patch.dict("os.environ", {"GOOGLE_PLACES_API_KEY": "test-key"}):
            with patch(
                "pipeline.skills.fetch_google_review_links.fetch_google_places_text_search",
                side_effect=[nearby],
            ):
                records = fetch_google_places_nearby_text(
                    {
                        "society_name": "Example Green",
                        "city": "Bengaluru",
                        "latitude": 12.9716,
                        "longitude": 77.5946,
                    },
                    "fitness",
                )

        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["place_name"], "Cult Example Gym")
        self.assertEqual(records[0]["latitude"], 12.9730)
        self.assertEqual(records[0]["longitude"], 77.5946)

    def test_google_nearby_does_not_collect_bus_stop_as_metro(self):
        nearby = {
            "places": [
                {
                    "id": "places/bus-stop",
                    "displayName": {"text": "Varthuru Bus Stop"},
                    "googleMapsUri": "https://maps.google.com/?cid=bus-stop",
                    "location": {"latitude": 12.9720, "longitude": 77.5946},
                    "primaryType": "bus_stop",
                    "types": ["bus_stop", "transit_station", "transit_stop"],
                },
                {
                    "id": "places/metro",
                    "displayName": {"text": "Example Metro Station"},
                    "googleMapsUri": "https://maps.google.com/?cid=metro",
                    "location": {"latitude": 12.9730, "longitude": 77.5946},
                    "primaryType": "subway_station",
                    "types": ["subway_station", "transit_station"],
                },
            ]
        }

        with patch.dict("os.environ", {"GOOGLE_PLACES_API_KEY": "test-key"}):
            with patch(
                "pipeline.skills.fetch_google_review_links.fetch_google_places_text_search",
                side_effect=[nearby],
            ):
                records = fetch_google_places_nearby_text(
                    {
                        "society_name": "Example Green",
                        "city": "Bengaluru",
                        "latitude": 12.9716,
                        "longitude": 77.5946,
                    },
                    "metro",
                )

        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["place_name"], "Example Metro Station")

    def test_google_nearby_does_not_collect_site_office_as_lake(self):
        nearby = {
            "places": [
                {
                    "id": "places/site-office",
                    "displayName": {"text": "Prestige Lakeside Habitat Site Office"},
                    "googleMapsUri": "https://maps.google.com/?cid=site-office",
                    "location": {"latitude": 12.9720, "longitude": 77.5946},
                    "primaryType": "lake",
                    "types": ["lake", "point_of_interest"],
                },
                {
                    "id": "places/lake",
                    "displayName": {"text": "Varthur Lake"},
                    "googleMapsUri": "https://maps.google.com/?cid=lake",
                    "location": {"latitude": 12.9730, "longitude": 77.5946},
                    "primaryType": "lake",
                    "types": ["lake"],
                },
            ]
        }

        with patch.dict("os.environ", {"GOOGLE_PLACES_API_KEY": "test-key"}):
            with patch(
                "pipeline.skills.fetch_google_review_links.fetch_google_places_text_search",
                side_effect=[nearby],
            ):
                records = fetch_google_places_nearby_text(
                    {
                        "society_name": "Example Green",
                        "city": "Bengaluru",
                        "latitude": 12.9716,
                        "longitude": 77.5946,
                    },
                    "lake",
                )

        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["place_name"], "Varthur Lake")

    def test_collects_exact_rust_source_input_shape(self):
        request = {
            "project_root": "/tmp/openestates",
            "partition": {
                "parts": [
                    ["dt", "2026-07-14"],
                ]
            },
            "planned_at": "2026-07-14T09:30:00Z",
            "requested_assets": [
                "rera_registry_monthly",
            ],
        }
        rera_rows = [
            SimpleNamespace(
                ack_number="ACK-1",
                registration_number="PRM-1",
                project_name="Example Green",
                promoter_name="Example Builder",
            ),
            SimpleNamespace(
                ack_number="ACK-BAD",
                registration_number="",
                project_name="",
                promoter_name="Incomplete Builder",
            ),
        ]
        output = collect_asset_sources(
            request,
            rera_fetch=lambda: (rera_rows, "2026-07-10T08:00:00Z"),
        )

        project = output["rera_registry_monthly"]["projects"][0]
        self.assertEqual(len(output["rera_registry_monthly"]["projects"]), 1)
        self.assertEqual(project["registration_number"], "PRM-1")
        self.assertIsNone(project["total_land_area_sqm"])
        self.assertEqual(project["fetched_at"], "2026-07-10T08:00:00Z")
        self.assertEqual(output["rera_registry_monthly"]["snapshot_date"], "2026-07")

    def test_rejects_unknown_assets(self):
        request = {
            "requested_assets": ["unknown_fact_asset"],
            "partition": {"parts": []},
            "planned_at": "2026-07-14T09:30:00Z",
        }

        with self.assertRaisesRegex(ValueError, "unsupported source assets"):
            collect_asset_sources(request)

    def test_skill_rows_preserve_fact_provenance_and_search_annotations(self):
        from pipeline.collect_asset_sources import skill_result_rows

        result = SkillResult(
            facts=[
                SourcedFact(
                    key="google_rating",
                    value={"type": "Numeric", "data": 4.4},
                    confidence=0.82,
                    source=FactSource(
                        source_type="Google",
                        url="https://maps.google.com/example",
                        skill_id="fetch_google_review_links",
                    ),
                    learned_at="2026-07-14T09:30:00Z",
                    display_template="Google rating: {value}",
                    answers_preferences=["google rating", "reviews"],
                    scoring_hint={
                        "direction": "HigherIsBetter",
                        "weight": 1.0,
                        "thresholds": [4.4, 4.0],
                    },
                )
            ]
        )

        facts, annotations = skill_result_rows(
            "society:example",
            "fetch_google_review_links",
            "2026-07-14",
            {"society_name": "Example"},
            result,
        )

        self.assertEqual(facts[0]["value_type"], "numeric")
        self.assertEqual(facts[0]["source_url"], "https://maps.google.com/example")
        self.assertTrue(facts[0]["input_hash"].startswith("sha256:"))
        self.assertEqual(annotations[0]["scoring_direction"], "HigherIsBetter")
        self.assertEqual(annotations[0]["answers_preferences_json"], '["google rating","reviews"]')


if __name__ == "__main__":
    unittest.main()
