import unittest
from types import SimpleNamespace

from pipeline.collect_asset_sources import (
    collect_asset_sources,
    collect_google_places,
    collect_reddit_assets,
    google_society_inputs,
)
from pipeline.skills.base import FactSource, SkillCost, SkillResult, SourcedFact
from pipeline.skills.search_reddit import threads_to_skill_result


class CollectAssetSourcesTest(unittest.TestCase):
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
        self.assertEqual(record["rating"], 4.4)
        self.assertEqual(record["fetched_at"], "2026-07-12T08:15:00Z")
        self.assertEqual(record["fetch_source"], "fetch_google_review_links_cache")

    def test_collects_exact_rust_source_input_shape(self):
        request = {
            "project_root": "/tmp/openestates",
            "partition": {
                "parts": [
                    ["dt", "2026-07-14"],
                    ["subreddit", "BangaloreRealEstates"],
                ]
            },
            "planned_at": "2026-07-14T09:30:00Z",
            "requested_assets": [
                "rera_registry_monthly",
                "reddit_threads_daily",
                "reddit_resident_facts",
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
        reddit_threads = [
            {
                "id": "abc123",
                "subreddit": "BangaloreRealEstates",
                "title": "Example Green resident review",
                "url": "https://www.reddit.com/r/BangaloreRealEstates/comments/abc123/example/",
                "score": 12,
                "num_comments": 4,
                "created_utc": 1784021000,
                "selftext": "Quiet and green.",
            }
        ]
        reddit_collect = lambda reddit_request: collect_reddit_assets(
            reddit_request,
            society_inputs={
                "example-green": {
                    "query": "Example Green Whitefield",
                    "subreddit": "BangaloreRealEstates",
                }
            },
            thread_fetch=lambda _query, _subreddit: reddit_threads,
            result_builder=threads_to_skill_result,
        )

        output = collect_asset_sources(
            request,
            rera_fetch=lambda: (rera_rows, "2026-07-10T08:00:00Z"),
            reddit_collect=reddit_collect,
        )

        project = output["rera_registry_monthly"]["projects"][0]
        self.assertEqual(len(output["rera_registry_monthly"]["projects"]), 1)
        self.assertEqual(project["registration_number"], "PRM-1")
        self.assertIsNone(project["total_land_area_sqm"])
        self.assertEqual(project["fetched_at"], "2026-07-10T08:00:00Z")
        self.assertEqual(output["rera_registry_monthly"]["snapshot_date"], "2026-07")

        reddit = output["reddit_threads_daily"]
        self.assertEqual(reddit["snapshot_date"], "2026-07-14")
        self.assertEqual(reddit["subreddit"], "BangaloreRealEstates")
        self.assertEqual(reddit["records"][0]["thread_id"], "abc123")
        self.assertEqual(reddit["records"][0]["fetch_source"], "reddit_public_json_search")
        self.assertEqual(output["reddit_resident_facts"]["source"], "reddit")
        self.assertEqual(
            output["reddit_resident_facts"]["facts"][0]["source_url"],
            reddit["records"][0]["url"],
        )

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
