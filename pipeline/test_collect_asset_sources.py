import unittest
from io import BytesIO
from types import SimpleNamespace
from unittest.mock import MagicMock, patch
from urllib.error import HTTPError

from pipeline.collect_asset_sources import (
    collect_asset_sources,
    collect_google_places,
    collect_reddit_assets,
    google_society_inputs,
    reddit_society_inputs,
)
from pipeline.skills.base import FactSource, SkillCost, SkillResult, SourcedFact
from pipeline.skills.search_reddit import (
    RedditSourceBlocked,
    RedditSourceInvalidResponse,
    RedditSourceUnavailable,
    fetch_reddit_threads,
    fetch_reddit_threads_with_retry,
    threads_to_skill_result,
)


class CollectAssetSourcesTest(unittest.TestCase):
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

    def test_reddit_collection_rejects_missing_entity_scope(self):
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

        with self.assertRaisesRegex(ValueError, "requires scoped source_entities"):
            collect_asset_sources(request)

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
        reddit_collect = lambda reddit_request, society_inputs=None: collect_reddit_assets(
            reddit_request,
            society_inputs=society_inputs
            or {
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
