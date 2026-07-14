import unittest
from types import SimpleNamespace

from pipeline.collect_asset_sources import collect_asset_sources, collect_reddit_assets
from pipeline.skills.base import FactSource, SkillResult, SourcedFact
from pipeline.skills.search_reddit import threads_to_skill_result


class CollectAssetSourcesTest(unittest.TestCase):
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
