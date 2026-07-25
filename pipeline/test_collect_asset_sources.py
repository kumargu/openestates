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
    groundwater_zones_from_kml,
    collect_reddit_assets,
    collect_rera_registry,
    google_society_inputs,
    reddit_society_inputs,
    request_with_rera_detail_facts,
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
    magicbricks_source_pages,
    squareyards_source_pages,
)
from pipeline.sources.external_images import skip_image_optimization, write_optimized_preview


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
                                    "html": """
                                        <html>
                                          <img data-src="https://img.staticmb.com/mbimages/project/example-green-elevation.jpg" alt="Example Green elevation" width="1200" height="800">
                                          <img src="//img.staticmb.com/mbimages/project/example-green-clubhouse.webp" alt="Example Green clubhouse">
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
        self.assertEqual(len(records), 2)
        self.assertEqual(records[0]["entity_id"], "society:example-green")
        self.assertEqual(records[0]["source_name"], "magicbricks")
        self.assertEqual(records[0]["source_page_url"], "https://www.magicbricks.com/example-green")
        self.assertEqual(
            records[0]["image_url"],
            "https://img.staticmb.com/mbimages/project/example-green-elevation.jpg",
        )
        self.assertEqual(records[0]["image_kind"], "exterior")
        self.assertEqual(records[0]["width"], 1200)
        self.assertEqual(records[0]["height"], 800)
        self.assertEqual(records[0]["storage_policy"], "link_only")
        self.assertNotIn("confidence", records[0])

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
        self.assertEqual(len(output["records"]), 5)
        school = output["records"][0]
        self.assertEqual(school["entity_id"], "society:rera-example-green")
        self.assertEqual(school["project_key"], "PRM-EXAMPLE-GREEN")
        self.assertEqual(school["query"], "school near Example Green Whitefield Bengaluru")
        self.assertEqual(school["category"], "school")
        self.assertEqual(school["place_name"], "Example school")
        self.assertEqual(school["distance_km"], 1.2)
        self.assertEqual(school["latitude"], 12.972)
        self.assertEqual(school["longitude"], 77.596)
        self.assertEqual(school["primary_type"], "school")
        self.assertEqual(school["place_types"], ["school"])
        self.assertEqual(school["fetch_source"], "fixture_nearby")

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
            if query.startswith("school near"):
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
            elif query.startswith("metro station near"):
                payload["places"][0].update(
                    {
                        "displayName": {"text": "Example Metro Station"},
                        "primaryType": "subway_station",
                        "types": ["subway_station", "transit_station"],
                    }
                )
            elif query.startswith("hospital near"):
                payload["places"][0].update(
                    {
                        "displayName": {"text": "Example Hospital"},
                        "primaryType": "hospital",
                        "types": ["hospital"],
                    }
                )
            elif query.startswith("gym fitness near"):
                payload["places"][0].update(
                    {
                        "displayName": {"text": "Cult Whitefield"},
                        "primaryType": "gym",
                        "types": ["gym"],
                    }
                )
            elif query.startswith("tech park office near"):
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
                        }
                    },
                )

        self.assertEqual(len(requests), 6)
        school_request = json.loads(requests[1].data.decode("utf-8"))
        self.assertEqual(
            school_request["locationBias"]["circle"]["center"]["latitude"], 12.9716
        )
        self.assertEqual(
            school_request["locationBias"]["circle"]["center"]["longitude"], 77.5946
        )
        self.assertEqual(school_request["locationBias"]["circle"]["radius"], 5000)
        metro_request = json.loads(requests[2].data.decode("utf-8"))
        self.assertEqual(metro_request["locationBias"]["circle"]["radius"], 6000)
        fitness_request = json.loads(requests[4].data.decode("utf-8"))
        self.assertEqual(fitness_request["locationBias"]["circle"]["radius"], 3500)
        tech_park_request = json.loads(requests[5].data.decode("utf-8"))
        self.assertEqual(
            tech_park_request["locationBias"]["circle"]["radius"],
            15000,
        )
        self.assertEqual(len(output["records"]), 5)
        school = output["records"][0]
        self.assertEqual(school["query"], "school near Example Green Whitefield Bengaluru")
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
                    "displayName": {"text": "Example Gym"},
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
        self.assertEqual(records[0]["place_name"], "Example Gym")
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
