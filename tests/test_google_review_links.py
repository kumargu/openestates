import os
import unittest
from unittest.mock import patch

from pipeline.skills.fetch_google_review_links import (
    FetchGoogleReviewLinksSkill,
    best_place_result,
    build_place_query,
    fallback_search_result,
    google_maps_search_url,
    place_to_skill_result,
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
                "rating": "4.4",
                "reviews": "1,234",
            },
            api_calls=1,
        )

        by_key = {fact.key: fact for fact in result.facts}
        self.assertEqual(
            by_key["google_reviews_url"].value["data"],
            "https://www.google.com/maps/place/prestige",
        )
        self.assertEqual(by_key["google_place_id"].value["data"], "ChIJ123")
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
        env = {k: v for k, v in os.environ.items() if k not in {"SERPAPI_API_KEY", "SERPAPI_KEY"}}
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
        env = {k: v for k, v in os.environ.items() if k not in {"SERPAPI_API_KEY", "SERPAPI_KEY"}}
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
        env = {k: v for k, v in os.environ.items() if k not in {"SERPAPI_API_KEY", "SERPAPI_KEY"}}
        with patch.dict(os.environ, env, clear=True):
            fallback_key = skill._cache_key(input_data)
        with patch.dict(os.environ, {"SERPAPI_API_KEY": "test-key"}, clear=True):
            serpapi_key = skill._cache_key(input_data)

        self.assertNotEqual(fallback_key, serpapi_key)


if __name__ == "__main__":
    unittest.main()
