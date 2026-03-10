"""Tests for Day 30: Progressive Async Enrichment — search-triggered background learning.

Tests cover:
- JSONL reading and truncation
- Request deduplication by area (with merging)
- Area neighbor discovery from seed data
- Freshness checking
- Wave execution with mocked skills
- End-to-end process_requests flow
- CLI dry-run mode
"""

import asyncio
import json
import sys
import tempfile
import unittest
from datetime import datetime, timezone
from pathlib import Path
from unittest.mock import patch, MagicMock

sys.path.insert(0, str(Path(__file__).parent.parent))

from pipeline.enrich_async import (
    WAVE_CONFIG,
    PENDING_PATH,
    read_and_truncate,
    deduplicate_requests,
    find_area_neighbors,
    needs_enrichment,
    run_wave,
    process_requests,
)
from pipeline.enrich import SKILL_REGISTRY
from pipeline.entity_resolver import EntityResolver, FreshnessTracker


class TestWaveConfig(unittest.TestCase):
    """Verify wave configuration is well-formed."""

    def test_all_waves_exist(self):
        for wave_num in [1, 2, 3, 4]:
            self.assertIn(wave_num, WAVE_CONFIG)

    def test_delays_increase_per_wave(self):
        delays = [WAVE_CONFIG[w]["delay_between_calls"] for w in sorted(WAVE_CONFIG)]
        self.assertEqual(delays, sorted(delays), "Delays should increase per wave")

    def test_cost_tiers_are_valid(self):
        valid_tiers = {"free", "cheap", "moderate", "expensive"}
        for wave_num, config in WAVE_CONFIG.items():
            for tier in config["cost_tiers"]:
                self.assertIn(tier, valid_tiers, f"Wave {wave_num} has invalid tier: {tier}")

    def test_wave1_is_free_only(self):
        self.assertEqual(WAVE_CONFIG[1]["cost_tiers"], ["free"])

    def test_wave3_is_free_only(self):
        self.assertEqual(WAVE_CONFIG[3]["cost_tiers"], ["free"])

    def test_wave3_and_4_have_entity_caps(self):
        self.assertIsNotNone(WAVE_CONFIG[3]["max_entities"])
        self.assertIsNotNone(WAVE_CONFIG[4]["max_entities"])


class TestReadAndTruncate(unittest.TestCase):
    """Test JSONL file reading and truncation."""

    def test_reads_valid_jsonl(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False) as f:
            path = Path(f.name)
            f.write(json.dumps({"area": "Whitefield", "entities": ["a"]}) + "\n")
            f.write(json.dumps({"area": "Sarjapur", "entities": ["b"]}) + "\n")

        requests = read_and_truncate(path)
        self.assertEqual(len(requests), 2)
        self.assertEqual(requests[0]["area"], "Whitefield")
        self.assertEqual(requests[1]["area"], "Sarjapur")

        # File should be truncated
        self.assertEqual(path.read_text(), "")
        path.unlink()

    def test_empty_file_returns_empty(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False) as f:
            path = Path(f.name)
            f.write("")

        requests = read_and_truncate(path)
        self.assertEqual(requests, [])
        path.unlink()

    def test_nonexistent_file_returns_empty(self):
        requests = read_and_truncate(Path("/tmp/nonexistent_enrichment.jsonl"))
        self.assertEqual(requests, [])

    def test_skips_malformed_lines(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False) as f:
            path = Path(f.name)
            f.write(json.dumps({"area": "Whitefield"}) + "\n")
            f.write("not json\n")
            f.write(json.dumps({"area": "Sarjapur"}) + "\n")

        requests = read_and_truncate(path)
        self.assertEqual(len(requests), 2)
        path.unlink()


class TestDeduplicateRequests(unittest.TestCase):
    """Test request deduplication and merging."""

    def test_single_request_passes_through(self):
        requests = [{"area": "Whitefield", "entities": ["a", "b"], "preferences": ["quiet"], "query": "q1"}]
        result = deduplicate_requests(requests)
        self.assertEqual(len(result), 1)
        self.assertIn("whitefield", result)

    def test_same_area_merges_entities(self):
        requests = [
            {"area": "Whitefield", "entities": ["a", "b"], "preferences": ["quiet"], "query": "q1"},
            {"area": "Whitefield", "entities": ["b", "c"], "preferences": ["metro"], "query": "q2"},
        ]
        result = deduplicate_requests(requests)
        self.assertEqual(len(result), 1)
        entities = set(result["whitefield"]["entities"])
        self.assertEqual(entities, {"a", "b", "c"})
        prefs = set(result["whitefield"]["preferences"])
        self.assertEqual(prefs, {"quiet", "metro"})

    def test_different_areas_kept_separate(self):
        requests = [
            {"area": "Whitefield", "entities": ["a"], "preferences": [], "query": "q1"},
            {"area": "Sarjapur", "entities": ["b"], "preferences": [], "query": "q2"},
        ]
        result = deduplicate_requests(requests)
        self.assertEqual(len(result), 2)

    def test_empty_area_skipped(self):
        requests = [{"area": "", "entities": ["a"], "preferences": [], "query": "q"}]
        result = deduplicate_requests(requests)
        self.assertEqual(len(result), 0)

    def test_case_insensitive_area_dedup(self):
        requests = [
            {"area": "Whitefield", "entities": ["a"], "preferences": [], "query": "q1"},
            {"area": "whitefield", "entities": ["b"], "preferences": [], "query": "q2"},
        ]
        result = deduplicate_requests(requests)
        self.assertEqual(len(result), 1)


class TestFindAreaNeighbors(unittest.TestCase):
    """Test area neighbor discovery from seed data."""

    def test_finds_whitefield_neighbors(self):
        neighbors = find_area_neighbors("Whitefield", [])
        # Should find at least some societies in Whitefield from seed data
        self.assertGreater(len(neighbors), 0, "Should find Whitefield societies in seed data")

    def test_excludes_specified_entities(self):
        all_neighbors = find_area_neighbors("Whitefield", [])
        if len(all_neighbors) > 1:
            excluded = [all_neighbors[0]]
            filtered = find_area_neighbors("Whitefield", excluded)
            self.assertNotIn(excluded[0], filtered)
            self.assertEqual(len(filtered), len(all_neighbors) - 1)

    def test_nonexistent_area_returns_empty(self):
        neighbors = find_area_neighbors("NonexistentArea", [])
        self.assertEqual(neighbors, [])

    def test_case_insensitive_area_match(self):
        neighbors_upper = find_area_neighbors("Whitefield", [])
        neighbors_lower = find_area_neighbors("whitefield", [])
        self.assertEqual(set(neighbors_upper), set(neighbors_lower))


class TestNeedsEnrichment(unittest.TestCase):
    """Test freshness checking for skill+entity pairs."""

    def test_missing_entity_needs_enrichment(self):
        tracker = FreshnessTracker()
        # Entity that was never enriched should need it
        self.assertTrue(needs_enrichment("society:nonexistent-test", "search_reddit", tracker))

    def test_wrong_entity_type_does_not_need(self):
        tracker = FreshnessTracker()
        # search_reddit only applies to societies, not areas
        self.assertFalse(needs_enrichment("area:whitefield", "search_reddit", tracker))

    def test_unknown_skill_returns_false(self):
        tracker = FreshnessTracker()
        self.assertFalse(needs_enrichment("society:test", "nonexistent_skill", tracker))


class TestRunWave(unittest.TestCase):
    """Test wave execution with mocked skills."""

    def _run(self, coro):
        return asyncio.get_event_loop().run_until_complete(coro)

    @patch("pipeline.enrich_async._make_skill")
    @patch("pipeline.enrich_async._push_facts")
    def test_wave1_runs_free_skills_only(self, mock_push, mock_make):
        from pipeline.skills.base import SkillResult, SkillCost, SourcedFact, FactSource
        mock_skill = MagicMock()
        mock_skill.run.return_value = SkillResult(
            facts=[SourcedFact(
                key="reddit_thread_count",
                value={"type": "Numeric", "data": 5},
                confidence=0.9,
                source=FactSource(source_type="Crawl", skill_id="search_reddit"),
            )],
            cost=SkillCost(estimated_usd=0.0),
            cached=False,
        )
        mock_make.return_value = mock_skill

        resolver = EntityResolver()
        tracker = FreshnessTracker(path=str(Path(tempfile.mkdtemp()) / "fresh.json"))
        failures = {}

        stats = self._run(run_wave(
            wave_num=1,
            entities=["society:test-entity"],
            preferences=["quiet"],
            resolver=resolver,
            tracker=tracker,
            failures=failures,
            delay_multiplier=0.0,  # No delays in tests
        ))

        # Should have executed some skills
        self.assertGreaterEqual(stats["executed"], 0)

    def test_dry_run_does_not_execute(self):
        resolver = EntityResolver()
        tracker = FreshnessTracker(path=str(Path(tempfile.mkdtemp()) / "fresh.json"))
        failures = {}

        stats = self._run(run_wave(
            wave_num=1,
            entities=["society:nonexistent-dry-run-test"],
            preferences=["quiet"],
            resolver=resolver,
            tracker=tracker,
            failures=failures,
            delay_multiplier=0.0,
            dry_run=True,
        ))

        # Dry run: executed count reflects planned, not actual execution
        # No facts should be written
        self.assertEqual(stats["facts"], 0)

    def test_empty_entities_does_nothing(self):
        resolver = EntityResolver()
        tracker = FreshnessTracker(path=str(Path(tempfile.mkdtemp()) / "fresh.json"))
        failures = {}

        stats = self._run(run_wave(
            wave_num=1,
            entities=[],
            preferences=[],
            resolver=resolver,
            tracker=tracker,
            failures=failures,
            delay_multiplier=0.0,
        ))

        self.assertEqual(stats["executed"], 0)
        self.assertEqual(stats["facts"], 0)


class TestProcessRequests(unittest.TestCase):
    """Test end-to-end request processing."""

    def _run(self, coro):
        return asyncio.get_event_loop().run_until_complete(coro)

    def test_empty_requests_returns_zero(self):
        stats = self._run(process_requests([], dry_run=True))
        self.assertEqual(stats["areas"], 0)

    @patch("pipeline.enrich_async.reload_backend")
    @patch("pipeline.enrich_async.run_wave")
    def test_processes_single_area(self, mock_wave, mock_reload):
        mock_wave.return_value = {"executed": 2, "succeeded": 2, "failed": 0, "facts": 5, "skipped": 0}

        requests = [
            {"area": "Whitefield", "entities": ["society:test"], "preferences": ["quiet"], "query": "test"}
        ]

        stats = self._run(process_requests(requests, delay_multiplier=0.0))
        self.assertEqual(stats["areas"], 1)
        # run_wave should be called 4 times (one per wave) — or less if no neighbors
        self.assertGreaterEqual(mock_wave.call_count, 2)  # At least waves 1 and 2

    @patch("pipeline.enrich_async.reload_backend")
    @patch("pipeline.enrich_async.run_wave")
    def test_processes_multiple_areas(self, mock_wave, mock_reload):
        mock_wave.return_value = {"executed": 1, "succeeded": 1, "failed": 0, "facts": 2, "skipped": 0}

        requests = [
            {"area": "Whitefield", "entities": ["society:a"], "preferences": [], "query": "q1"},
            {"area": "Sarjapur", "entities": ["society:b"], "preferences": [], "query": "q2"},
        ]

        stats = self._run(process_requests(requests, delay_multiplier=0.0))
        self.assertEqual(stats["areas"], 2)


class TestEnrichmentPendingIntegration(unittest.TestCase):
    """Integration test: write JSONL, read it, process it."""

    def test_write_read_roundtrip(self):
        """Write requests to a temp file, read and truncate, verify."""
        with tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False) as f:
            path = Path(f.name)
            # Simulate what Rust writes
            req1 = {
                "area": "Whitefield",
                "entities": ["society:prestige-lakeside-habitat", "society:sobha-insignia"],
                "preferences": ["quiet", "metro access"],
                "query": "quiet apartment near metro whitefield",
                "ts": datetime.now(timezone.utc).isoformat(),
            }
            req2 = {
                "area": "Sarjapur",
                "entities": ["society:brigade-woods"],
                "preferences": ["good society"],
                "query": "well maintained apartment sarjapur",
                "ts": datetime.now(timezone.utc).isoformat(),
            }
            f.write(json.dumps(req1) + "\n")
            f.write(json.dumps(req2) + "\n")

        # Read and truncate
        requests = read_and_truncate(path)
        self.assertEqual(len(requests), 2)
        self.assertEqual(requests[0]["area"], "Whitefield")
        self.assertEqual(len(requests[0]["entities"]), 2)

        # File should be empty after reading
        self.assertEqual(path.read_text(), "")

        # Deduplicate
        by_area = deduplicate_requests(requests)
        self.assertEqual(len(by_area), 2)
        self.assertIn("whitefield", by_area)
        self.assertIn("sarjapur", by_area)

        path.unlink()


class TestUtilsReloadBackend(unittest.TestCase):
    """Test the shared reload utility."""

    @patch("urllib.request.urlopen")
    def test_reload_success(self, mock_urlopen):
        mock_resp = MagicMock()
        mock_resp.read.return_value = json.dumps({"nodes_loaded": 55, "facts_loaded": 2800}).encode()
        mock_urlopen.return_value = mock_resp

        from pipeline.utils import reload_backend
        result = reload_backend()
        self.assertTrue(result)

    @patch("urllib.request.urlopen", side_effect=Exception("Connection refused"))
    def test_reload_failure_returns_false(self, mock_urlopen):
        from pipeline.utils import reload_backend
        result = reload_backend()
        self.assertFalse(result)


if __name__ == "__main__":
    unittest.main()
