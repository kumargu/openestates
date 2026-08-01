import json
import os
import tempfile
import unittest
from io import BytesIO
from pathlib import Path
from unittest.mock import patch

from pipeline.skills.fetch_google_review_links import (
    FetchGoogleReviewLinksSkill,
    best_place_result,
    build_place_query,
    fallback_search_result,
    google_maps_search_url,
    place_to_skill_result,
    place_query_variants,
    resolve_google_project_place,
    strip_project_phase_suffix,
)


class GoogleReviewLinkSkillTests(unittest.TestCase):
    def test_build_place_query_uses_name_area_city(self):
        query = build_place_query(
            {
                "society_name": "Prestige Lakeside Habitat",
                "area": "Whitefield",
                "city": "Bengaluru",
            }
        )
        self.assertEqual(query, "Prestige Lakeside Habitat Whitefield Bengaluru")

    def test_phase_suffix_stripping_preserves_base_project_name(self):
        examples = {
            "SUMADHURA EDITION PHASE-I": "SUMADHURA EDITION",
            "SUMADHURA EDITION PHASE II": "SUMADHURA EDITION",
            "FOLIUM BY SUMADHURA PHASE-I/II/III/IV": "FOLIUM BY SUMADHURA",
            "PURSUIT OF A RADICAL RHAPSODY PHASE 2": "PURSUIT OF A RADICAL RHAPSODY",
            "Assetz Marq Phase 3B": "Assetz Marq",
            "Sobha Windsor Phase 1 Wing 1 and 2": "Sobha Windsor",
        }

        for value, expected in examples.items():
            with self.subTest(value=value):
                self.assertEqual(strip_project_phase_suffix(value), expected)

    def test_place_query_variants_are_exact_then_phase_then_address(self):
        variants = place_query_variants(
            {
                "society_name": "Sumadhura Edition Phase I",
                "area": "Whitefield",
                "city": "Bengaluru",
                "address": "Hoodi Village, K.R. Puram Hobli, Bangalore East",
            }
        )

        self.assertEqual(
            [variant["query"] for variant in variants],
            [
                "Sumadhura Edition Phase I Whitefield Bengaluru",
                "Sumadhura Edition Whitefield Bengaluru",
                "Sumadhura Edition Phase I Bengaluru",
                "Sumadhura Edition Bengaluru",
                "Sumadhura Edition Phase I Hoodi Village, K.R. Puram Hobli, Bangalore East",
                "Sumadhura Edition Hoodi Village, K.R. Puram Hobli, Bangalore East",
                "Sumadhura Edition Phase I Whitefield Bengaluru Hoodi Village, K.R. Puram Hobli, Bangalore East",
                "Sumadhura Edition Whitefield Bengaluru Hoodi Village, K.R. Puram Hobli, Bangalore East",
                "Sumadhura Edition Phase I Bengaluru Hoodi Village, K.R. Puram Hobli, Bangalore East",
                "Sumadhura Edition Bengaluru Hoodi Village, K.R. Puram Hobli, Bangalore East",
            ],
        )

    def test_maps_url_supports_place_id(self):
        url = google_maps_search_url("Test Society Bengaluru", "ChIJ Test")
        self.assertEqual(
            url,
            "https://www.google.com/maps/search/?api=1&query=Test+Society+Bengaluru&query_place_id=ChIJ+Test",
        )

    def test_fallback_writes_low_confidence_navigable_link(self):
        result = fallback_search_result("Prestige Lakeside Habitat Whitefield Bengaluru")
        self.assertEqual(len(result.facts), 1)
        fact = result.facts[0]
        self.assertEqual(fact.key, "google_reviews_url")
        self.assertEqual(fact.source.source_type, "Google")
        self.assertEqual(fact.source.skill_id, "fetch_google_review_links")
        self.assertIn("google.com/maps/search", fact.value["data"])
        self.assertEqual(result.cost.api_calls, 0)
        self.assertLess(fact.confidence, 0.5)

    def test_place_payload_emits_link_rating_count_and_place_id(self):
        result = place_to_skill_result(
            "Prestige Lakeside Habitat Whitefield Bengaluru",
            {
                "title": "Prestige Lakeside Habitat",
                "place_id": "ChIJ123",
                "link": "https://www.google.com/maps/place/prestige",
                "address": "Varthur Road, Whitefield, Bengaluru",
                "rating": "4.4",
                "reviews": "1,234",
            },
            api_calls=1,
            fetch_source="google_places_text_search",
        )

        by_key = {fact.key: fact for fact in result.facts}
        self.assertEqual(
            by_key["google_reviews_url"].value["data"],
            "https://www.google.com/maps/place/prestige",
        )
        self.assertEqual(by_key["google_place_id"].value["data"], "ChIJ123")
        self.assertEqual(
            by_key["google_place_address"].value["data"],
            "Varthur Road, Whitefield, Bengaluru",
        )
        self.assertEqual(by_key["google_rating"].value["data"], 4.4)
        self.assertEqual(by_key["google_review_count"].value["data"], 1234)
        self.assertEqual(result.cost.api_calls, 1)

    def test_best_place_result_prefers_place_results(self):
        payload = {
            "place_results": {"title": "Exact", "place_id": "exact"},
            "local_results": [{"title": "Other", "place_id": "other"}],
        }
        self.assertEqual(best_place_result(payload)["place_id"], "exact")

    def test_skill_does_not_call_network_without_api_key(self):
        env = {
            k: v
            for k, v in os.environ.items()
            if k not in {"GOOGLE_PLACES_API_KEY", "SERPAPI_API_KEY", "SERPAPI_KEY"}
        }
        with patch.dict(os.environ, env, clear=True):
            result = FetchGoogleReviewLinksSkill().execute(
                {
                    "society_name": "Prestige Lakeside Habitat",
                    "area": "Whitefield",
                    "city": "Bengaluru",
                }
            )

        self.assertEqual(result.cost.api_calls, 0)
        self.assertEqual(result.facts[0].key, "google_reviews_url")

    def test_existing_place_id_yields_precise_maps_link_without_api_call(self):
        env = {
            k: v
            for k, v in os.environ.items()
            if k not in {"GOOGLE_PLACES_API_KEY", "SERPAPI_API_KEY", "SERPAPI_KEY"}
        }
        with patch.dict(os.environ, env, clear=True):
            result = FetchGoogleReviewLinksSkill().execute(
                {
                    "society_name": "Prestige Lakeside Habitat",
                    "area": "Whitefield",
                    "city": "Bengaluru",
                    "google_place_id": "ChIJ123",
                }
            )

        by_key = {fact.key: fact for fact in result.facts}
        self.assertEqual(result.cost.api_calls, 0)
        self.assertIn("query_place_id=ChIJ123", by_key["google_reviews_url"].value["data"])
        self.assertEqual(by_key["google_place_id"].value["data"], "ChIJ123")

    def test_cache_key_changes_when_serpapi_is_available(self):
        skill = FetchGoogleReviewLinksSkill()
        input_data = {"society_name": "Prestige Lakeside Habitat"}
        env = {
            k: v
            for k, v in os.environ.items()
            if k not in {"GOOGLE_PLACES_API_KEY", "SERPAPI_API_KEY", "SERPAPI_KEY"}
        }
        with patch.dict(os.environ, env, clear=True):
            fallback_key = skill._cache_key(input_data)
        with patch.dict(os.environ, {"SERPAPI_API_KEY": "test-key"}, clear=True):
            serpapi_key = skill._cache_key(input_data)

        self.assertNotEqual(fallback_key, serpapi_key)

    def test_resolution_accepts_residential_place_with_secondary_agency_type(self):
        result = resolve_google_project_place(
            {
                "places": [
                    {
                        "id": "ChIJ-godrej",
                        "displayName": {"text": "Godrej Splendour, Whitefield Bangalore"},
                        "formattedAddress": "Whitefield, Bengaluru, Karnataka",
                        "location": {"latitude": 13.0120239, "longitude": 77.7470451},
                        "types": [
                            "apartment_building",
                            "real_estate_agency",
                            "point_of_interest",
                            "establishment",
                        ],
                    }
                ]
            },
            {
                "name": "Godrej Splendour",
                "area": "Whitefield",
                "city": "Bengaluru",
            },
        )

        self.assertEqual(result["status"], "accepted")

    def test_resolution_ignores_standalone_numeric_name_suffix(self):
        result = resolve_google_project_place(
            {
                "places": [
                    {
                        "id": "places/provident-capella",
                        "displayName": {"text": "Provident Capella"},
                        "formattedAddress": "Samethanahalli, Bengaluru",
                        "location": {"latitude": 12.9927, "longitude": 77.8060},
                        "types": ["general_contractor", "establishment"],
                    }
                ]
            },
            {
                "name": "Provident Capella 1",
                "area": "Whitefield",
                "city": "Bengaluru",
            },
        )

        self.assertEqual(result["status"], "accepted")

    def test_resolution_rejects_pure_agency_place(self):
        result = resolve_google_project_place(
            {
                "places": [
                    {
                        "id": "ChIJ-agency",
                        "displayName": {"text": "Godrej Splendour Sales Office"},
                        "formattedAddress": "Whitefield, Bengaluru, Karnataka",
                        "location": {"latitude": 13.0120239, "longitude": 77.7470451},
                        "types": [
                            "real_estate_agency",
                            "point_of_interest",
                            "establishment",
                        ],
                    }
                ]
            },
            {
                "name": "Godrej Splendour",
                "area": "Whitefield",
                "city": "Bengaluru",
            },
        )

        self.assertEqual(result["status"], "rejected")
        self.assertIn("rejected_place_type", result["reasons"])

    def test_resolution_prefers_project_over_block_or_tower_candidate(self):
        result = resolve_google_project_place(
            {
                "places": [
                    {
                        "id": "places/block",
                        "displayName": {"text": "D Block Vaswani Exquisite"},
                        "formattedAddress": "ITPL Main Road, Whitefield, Bengaluru",
                        "location": {"latitude": 12.9892, "longitude": 77.7236},
                        "types": ["apartment_building", "establishment"],
                    },
                    {
                        "id": "places/project",
                        "displayName": {"text": "Vaswani Exquisite"},
                        "formattedAddress": "ITPL Main Road, Whitefield, Bengaluru",
                        "location": {"latitude": 12.9894, "longitude": 77.7240},
                        "types": [
                            "apartment_building",
                            "condominium_complex",
                            "establishment",
                        ],
                    },
                ]
            },
            {
                "name": "Vaswani Exquisite",
                "area": "Whitefield",
                "city": "Bengaluru",
            },
        )

        self.assertEqual(result["status"], "accepted")
        self.assertEqual(result["place"]["id"], "places/project")

    def test_resolution_prefers_specific_residential_place_over_street_address(self):
        result = resolve_google_project_place(
            {
                "places": [
                    {
                        "id": "places/street-address",
                        "displayName": {"text": "Vaswani Exquisite"},
                        "formattedAddress": "Vaswani Exquisite, ITPL Main Road, Bengaluru",
                        "location": {"latitude": 12.9895, "longitude": 77.7239},
                        "types": ["premise", "street_address"],
                    },
                    {
                        "id": "places/project",
                        "displayName": {"text": "Vaswani Exquisite"},
                        "formattedAddress": "ITPL Main Road, Whitefield, Bengaluru",
                        "location": {"latitude": 12.9894, "longitude": 77.7240},
                        "types": [
                            "apartment_building",
                            "condominium_complex",
                            "establishment",
                        ],
                    },
                ]
            },
            {
                "name": "Vaswani Exquisite",
                "area": "Whitefield",
                "city": "Bengaluru",
            },
        )

        self.assertEqual(result["status"], "accepted")
        self.assertEqual(result["place"]["id"], "places/project")

    def test_resolution_rejects_experience_centre_when_project_place_exists(self):
        result = resolve_google_project_place(
            {
                "places": [
                    {
                        "id": "places/office",
                        "displayName": {
                            "text": "Sumadhura Solace & Sumadhura Edition - Experience Centre"
                        },
                        "formattedAddress": "Thubarahalli, Whitefield, Bengaluru",
                        "location": {"latitude": 12.9562, "longitude": 77.7252},
                        "primaryType": "corporate_office",
                        "types": [
                            "corporate_office",
                            "establishment",
                            "point_of_interest",
                        ],
                    },
                    {
                        "id": "places/project",
                        "displayName": {"text": "Sumadhura Solace"},
                        "formattedAddress": "Thubarahalli, Whitefield, Bengaluru",
                        "location": {"latitude": 12.9554, "longitude": 77.7238},
                        "primaryType": "apartment_building",
                        "types": ["apartment_building", "establishment"],
                    },
                ]
            },
            {
                "name": "Sumadhura Solace",
                "area": "Whitefield",
                "city": "Bengaluru",
            },
        )

        self.assertEqual(result["status"], "accepted")
        self.assertEqual(result["place"]["id"], "places/project")

    def test_google_places_tries_phase_stripped_query_after_exact_rejection(self):
        exact_payload = {"places": []}
        phase_payload = {
            "places": [
                {
                    "id": "places/sumadhura-edition",
                    "displayName": {"text": "Sumadhura Edition"},
                    "formattedAddress": "Whitefield, Bengaluru",
                    "googleMapsUri": "https://maps.google.com/?cid=edition",
                    "location": {"latitude": 12.97, "longitude": 77.75},
                    "primaryType": "housing_complex",
                    "types": ["housing_complex", "establishment"],
                    "rating": 4.3,
                    "userRatingCount": 420,
                }
            ]
        }
        detail_payload = dict(phase_payload["places"][0])
        responses = [exact_payload, phase_payload, detail_payload]

        with tempfile.TemporaryDirectory() as temp_dir:
            with patch.dict("os.environ", {"GOOGLE_PLACES_API_KEY": "test-key"}):
                with patch(
                    "pipeline.skills.fetch_google_review_links.urllib.request.urlopen",
                    side_effect=lambda *_args, **_kwargs: BytesIO(
                        json.dumps(responses.pop(0)).encode("utf-8")
                    ),
                ) as mocked_open:
                    result = FetchGoogleReviewLinksSkill(cache_dir=Path(temp_dir)).execute(
                        {
                            "society_name": "Sumadhura Edition Phase I",
                            "area": "Whitefield",
                            "city": "Bengaluru",
                        }
                    )

        request_bodies = [
            json.loads(call_args[0][0].data.decode("utf-8"))
            for call_args in mocked_open.call_args_list[:2]
        ]
        self.assertEqual(
            [body["textQuery"] for body in request_bodies],
            [
                "Sumadhura Edition Phase I Whitefield Bengaluru",
                "Sumadhura Edition Whitefield Bengaluru",
            ],
        )
        by_key = {fact.key: fact for fact in result.facts}
        self.assertEqual(
            by_key["google_place_id"].value["data"],
            "places/sumadhura-edition",
        )
        self.assertEqual(result.cost.api_calls, 3)


if __name__ == "__main__":
    unittest.main()
