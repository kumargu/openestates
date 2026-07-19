"""Tests for Reddit concern signal classification and fact emission."""

import unittest

from pipeline.skills.reddit_resident_facts import threads_to_concern_facts
from pipeline.skills.reddit_theme_classifier import classify_text


class RedditResidentFactsTests(unittest.TestCase):
    def test_classifier_maps_waterlogging_terms_to_taxonomy_key(self):
        keys = classify_text(
            "Approach road has waterlogging every monsoon near the gate."
        )
        self.assertIn("risk.approach_road_waterlogging", keys)

    def test_facts_emit_taxonomy_keys_without_raw_text_values(self):
        result = threads_to_concern_facts(
            {"triggered_by": "test"},
            [
                {
                    "title": "Tanker dependency is painful in summer",
                    "selftext": "We rely on water tankers twice a week.",
                    "url": "https://reddit.com/r/test/comments/abc",
                }
            ],
        )
        fact_keys = [fact.key for fact in result.facts]
        self.assertIn("operating.tanker_dependence", fact_keys)
        for fact in result.facts:
            value = fact.value or {}
            payload = str(value.get("data") or "")
            self.assertNotIn("tanker dependency", payload.lower())
            self.assertEqual(fact.source.source_type, "RedditTheme")
            self.assertLessEqual(fact.confidence, 0.45)

    def test_empty_threads_emit_no_facts(self):
        result = threads_to_concern_facts({}, [])
        self.assertEqual(result.facts, [])


if __name__ == "__main__":
    unittest.main()
