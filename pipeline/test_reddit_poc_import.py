"""Tests for Reddit POC import compliance and taxonomy validation."""

import unittest

from pipeline.skills.reddit_poc_import import collect_reddit_poc_fact_rows, load_concern_taxonomy_keys


class RedditPocImportTests(unittest.TestCase):
    def test_poc_facts_use_taxonomy_keys_and_derived_values(self):
        facts, annotations = collect_reddit_poc_fact_rows("2026-07-19")
        self.assertGreaterEqual(len(facts), 10)
        self.assertEqual(len(facts), len(annotations))
        allowed = load_concern_taxonomy_keys()
        for fact in facts:
            self.assertIn(fact["fact_key"], allowed)
            self.assertEqual(fact["source_type"], "RedditTheme")
            self.assertLessEqual(fact["confidence"], 0.45)
            payload = fact["value_json"]
            self.assertIn('"data":"mentioned"', payload.replace(" ", ""))


if __name__ == "__main__":
    unittest.main()
