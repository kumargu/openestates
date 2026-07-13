"""Tests for Day 28: Unified Enrichment Engine."""

import json
import sys
import tempfile
import unittest
from datetime import datetime, timezone, timedelta
from pathlib import Path
from unittest.mock import patch, MagicMock

sys.path.insert(0, str(Path(__file__).parent.parent))

from pipeline.enrich import (
    SKILL_REGISTRY,
    FRESHNESS_SOURCE_MAP,
    COST_TIER_ORDER,
    WorkItem,
    ExecutionStats,
    detect_gaps,
    execute,
    is_backed_off,
    record_failure,
    record_success,
    load_failures,
    save_failures,
    print_plan,
    print_summary,
    _make_skill,
    _build_input,
)
from pipeline.entity_resolver import EntityResolver, FreshnessTracker
from pipeline.skills.base import BaseSkill


class TestSkillRegistry(unittest.TestCase):
    """Verify the skill registry is well-formed."""

    def test_all_skills_have_required_fields(self):
        required = {"node_types", "pool", "max_age_days", "cost_tier", "priority", "depends_on"}
        for skill_id, config in SKILL_REGISTRY.items():
            with self.subTest(skill=skill_id):
                self.assertTrue(required.issubset(config.keys()),
                                f"{skill_id} missing: {required - config.keys()}")

    def test_dependencies_reference_valid_skills(self):
        for skill_id, config in SKILL_REGISTRY.items():
            for dep in config["depends_on"]:
                self.assertIn(dep, SKILL_REGISTRY,
                              f"{skill_id} depends on unknown skill: {dep}")

    def test_no_circular_dependencies(self):
        """No skill can transitively depend on itself."""
        def _deps(sid, visited=None):
            if visited is None:
                visited = set()
            if sid in visited:
                return True  # cycle
            visited.add(sid)
            for dep in SKILL_REGISTRY[sid]["depends_on"]:
                if _deps(dep, visited.copy()):
                    return True
            return False

        for skill_id in SKILL_REGISTRY:
            self.assertFalse(_deps(skill_id), f"Circular dependency involving {skill_id}")

    def test_cost_tiers_are_valid(self):
        for skill_id, config in SKILL_REGISTRY.items():
            self.assertIn(config["cost_tier"], COST_TIER_ORDER,
                          f"{skill_id} has invalid cost_tier: {config['cost_tier']}")

    def test_every_skill_has_freshness_mapping(self):
        for skill_id in SKILL_REGISTRY:
            self.assertIn(skill_id, FRESHNESS_SOURCE_MAP,
                          f"{skill_id} missing from FRESHNESS_SOURCE_MAP")

    def test_llm_skills_are_not_registered(self):
        removed = {
            "fetch_google_reviews",
            "learn_society",
            "learn_area",
            "score_society",
            "embed_entity",
            "fetch_market_pricing",
            "rank_for_intent",
            "discover_properties",
        }
        self.assertTrue(removed.isdisjoint(SKILL_REGISTRY.keys()))

    def test_registry_has_no_llm_or_embedding_pools(self):
        for skill_id, config in SKILL_REGISTRY.items():
            self.assertNotIn(config["pool"], {"llm", "embedding"}, skill_id)


class TestOutputKeys(unittest.TestCase):
    """Verify all registered skills declare output_keys."""

    def test_all_skills_have_output_keys(self):
        for skill_id in SKILL_REGISTRY:
            with self.subTest(skill=skill_id):
                skill = _make_skill(skill_id)
                self.assertIsInstance(skill.output_keys, list)
                self.assertGreater(len(skill.output_keys), 0,
                                   f"{skill_id} has empty output_keys")

    def test_output_keys_are_strings(self):
        for skill_id in SKILL_REGISTRY:
            skill = _make_skill(skill_id)
            for key in skill.output_keys:
                self.assertIsInstance(key, str, f"{skill_id} has non-string output_key: {key}")

    def test_base_skill_default_output_keys(self):
        self.assertEqual(BaseSkill.output_keys, [])


class TestFailureTracking(unittest.TestCase):
    """Test the failure backoff system."""

    def test_no_backoff_under_threshold(self):
        failures = {"reddit": {"consecutive_failures": 2}}
        self.assertFalse(is_backed_off(failures, "reddit"))

    def test_backoff_after_three_failures(self):
        failures = {}
        record_failure(failures, "reddit")
        record_failure(failures, "reddit")
        record_failure(failures, "reddit")
        self.assertEqual(failures["reddit"]["consecutive_failures"], 3)
        self.assertTrue(is_backed_off(failures, "reddit"))

    def test_success_resets_failures(self):
        failures = {"reddit": {"consecutive_failures": 5, "backoff_until": "2099-01-01T00:00:00+00:00"}}
        record_success(failures, "reddit")
        self.assertEqual(failures["reddit"]["consecutive_failures"], 0)
        self.assertFalse(is_backed_off(failures, "reddit"))

    def test_unknown_pool_not_backed_off(self):
        self.assertFalse(is_backed_off({}, "nonexistent"))

    def test_expired_backoff_not_active(self):
        past = (datetime.now(timezone.utc) - timedelta(hours=1)).isoformat()
        failures = {"reddit": {"consecutive_failures": 5, "backoff_until": past}}
        self.assertFalse(is_backed_off(failures, "reddit"))

    def test_save_and_load_failures(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "failures.json"
            with patch("pipeline.enrich.FAILURES_PATH", path):
                failures = {"rera": {"consecutive_failures": 2}}
                save_failures(failures)
                loaded = load_failures()
                self.assertEqual(loaded["rera"]["consecutive_failures"], 2)


class TestGapDetection(unittest.TestCase):
    """Test gap detection with real KG data."""

    def setUp(self):
        self.resolver = EntityResolver()
        self.tracker = FreshnessTracker()

    def test_detects_missing_skills(self):
        """Should find work for entities that have gaps in any skill."""
        work = detect_gaps(self.resolver, self.tracker,
                           node_filter="society:sobha-insignia")
        # There should always be some deterministic work for stale/missing sources.
        self.assertGreater(len(work), 0)
        skill_ids = set(w.skill_id for w in work)
        # At least one skill should be detected as needing work
        all_skills = set(SKILL_REGISTRY.keys())
        self.assertTrue(skill_ids.issubset(all_skills),
                        f"Unknown skills in work: {skill_ids - all_skills}")

    def test_respects_cost_tier_filter(self):
        work = detect_gaps(self.resolver, self.tracker,
                           node_filter="society:sobha-insignia",
                           cost_tier_max="free")
        for item in work:
            self.assertEqual(item.cost_tier, "free")

    def test_respects_type_filter(self):
        work = detect_gaps(self.resolver, self.tracker, type_filter="area")
        for item in work:
            self.assertTrue(item.node_id.startswith("area:"))

    def test_work_items_sorted_by_priority(self):
        work = detect_gaps(self.resolver, self.tracker,
                           node_filter="society:sobha-insignia")
        priorities = [w.priority for w in work]
        self.assertEqual(priorities, sorted(priorities))

    def test_force_mode_marks_all(self):
        """Force mode should generate work even for fresh entities."""
        # Mark entity as fresh
        self.tracker.mark_fresh("society:sobha-insignia", "reddit", {"fact_count": 4})
        work_normal = detect_gaps(self.resolver, self.tracker,
                                  node_filter="society:sobha-insignia")
        work_forced = detect_gaps(self.resolver, self.tracker,
                                  node_filter="society:sobha-insignia", force=True)
        # Forced should have at least as many items
        self.assertGreaterEqual(len(work_forced), len(work_normal))
        # All forced items should say "forced"
        for item in work_forced:
            self.assertEqual(item.reason, "forced")

    def test_identify_gaps_depends_on_source_fetches(self):
        work = detect_gaps(self.resolver, self.tracker,
                           node_filter="society:sobha-insignia")
        gap_items = [w for w in work if w.skill_id == "identify_gaps"]
        if gap_items:
            deps = SKILL_REGISTRY["identify_gaps"]["depends_on"]
            self.assertEqual(set(deps), {"search_reddit", "fetch_rera"})


class TestBuildInput(unittest.TestCase):
    """Test input construction for each skill."""

    def setUp(self):
        self.resolver = EntityResolver()

    def test_search_reddit_input(self):
        inp = _build_input("search_reddit", "society:sobha-insignia", self.resolver)
        self.assertIn("query", inp)
        self.assertEqual(inp["subreddit"], "bangalore")

    def test_fetch_rera_input(self):
        inp = _build_input("fetch_rera", "society:prestige-lakeside-habitat", self.resolver)
        self.assertIn("project_name", inp)
        self.assertIn("entity_id", inp)

    def test_fetch_images_input(self):
        inp = _build_input("fetch_images", "society:sobha-insignia", self.resolver)
        self.assertEqual(inp["entity_type"], "society")
        self.assertIn("name", inp)
        self.assertIn("area", inp)

    def test_identify_gaps_input(self):
        inp = _build_input("identify_gaps", "society:sobha-insignia", self.resolver)
        self.assertEqual(inp["society_id"], "soc-sobha-insignia")


class TestExecution(unittest.TestCase):
    """Test execution engine with mocked skills."""

    def setUp(self):
        self.resolver = EntityResolver()
        self.tracker = FreshnessTracker(
            path=str(Path(tempfile.mkdtemp()) / "freshness.json")
        )

    @patch("pipeline.enrich._make_skill")
    @patch("pipeline.enrich._push_facts")
    @patch("pipeline.enrich.save_failures")
    @patch("pipeline.enrich.load_failures", return_value={})
    def test_basic_execution(self, mock_load_fail, mock_save_fail, mock_push, mock_make):
        from pipeline.skills.base import SkillResult, SkillCost, SourcedFact, FactSource
        mock_skill = MagicMock()
        mock_skill.run.return_value = SkillResult(
            facts=[SourcedFact(
                key="test_key",
                value={"type": "Text", "data": "test"},
                confidence=0.8,
                source=FactSource(source_type="Test", skill_id="search_reddit"),
            )],
            cost=SkillCost(estimated_usd=0.0),
            cached=False,
        )
        mock_make.return_value = mock_skill

        work = [WorkItem(
            node_id="society:test",
            skill_id="search_reddit",
            reason="missing",
            priority=1,
            cost_tier="free",
        )]

        stats = execute(work, self.resolver, self.tracker)
        self.assertEqual(stats.executed, 1)
        self.assertEqual(stats.succeeded, 1)
        self.assertEqual(stats.failed, 0)
        self.assertEqual(stats.facts_written, 1)

    @patch("pipeline.enrich._make_skill")
    @patch("pipeline.enrich._push_facts")
    @patch("pipeline.enrich.save_failures")
    @patch("pipeline.enrich.load_failures", return_value={})
    def test_budget_cap_stops_execution(self, mock_load_fail, mock_save_fail, mock_push, mock_make):
        from pipeline.skills.base import SkillResult, SkillCost
        mock_skill = MagicMock()
        mock_skill.run.return_value = SkillResult(
            facts=[],
            cost=SkillCost(estimated_usd=0.50),
            cached=False,
        )
        mock_make.return_value = mock_skill

        work = [
            WorkItem("society:a", "search_reddit", "missing", 1, "free"),
            WorkItem("society:b", "search_reddit", "missing", 1, "free"),
            WorkItem("society:c", "search_reddit", "missing", 1, "free"),
        ]

        stats = execute(work, self.resolver, self.tracker, budget=0.60)
        # Should execute first two ($0.50 + $0.50 = $1.00 > $0.60, stops after 2nd)
        self.assertLessEqual(stats.executed, 2)

    @patch("pipeline.enrich._make_skill")
    @patch("pipeline.enrich._push_facts")
    @patch("pipeline.enrich.save_failures")
    @patch("pipeline.enrich.load_failures", return_value={})
    def test_failure_is_tracked(self, mock_load_fail, mock_save_fail, mock_push, mock_make):
        mock_skill = MagicMock()
        mock_skill.run.side_effect = RuntimeError("RERA portal down")
        mock_make.return_value = mock_skill

        work = [WorkItem("society:test", "fetch_rera", "missing", 1, "free")]
        stats = execute(work, self.resolver, self.tracker)
        self.assertEqual(stats.failed, 1)
        self.assertEqual(len(stats.errors), 1)
        self.assertIn("RERA portal down", stats.errors[0])

    @patch("pipeline.enrich._make_skill")
    @patch("pipeline.enrich._push_facts")
    @patch("pipeline.enrich.save_failures")
    @patch("pipeline.enrich.load_failures")
    def test_backed_off_pool_is_skipped(self, mock_load_fail, mock_save_fail, mock_push, mock_make):
        future = (datetime.now(timezone.utc) + timedelta(hours=24)).isoformat()
        mock_load_fail.return_value = {
            "rera": {"consecutive_failures": 5, "backoff_until": future}
        }

        work = [WorkItem("society:test", "fetch_rera", "missing", 1, "free")]
        stats = execute(work, self.resolver, self.tracker)
        self.assertEqual(stats.skipped_backoff, 1)
        self.assertEqual(stats.executed, 0)
        mock_make.assert_not_called()

    @patch("pipeline.enrich._make_skill")
    @patch("pipeline.enrich._push_facts")
    @patch("pipeline.enrich.save_failures")
    @patch("pipeline.enrich.load_failures", return_value={})
    def test_deferred_items_run_in_second_pass(self, mock_load_fail, mock_save_fail, mock_push, mock_make):
        """Items whose dependencies complete in pass 1 should run in pass 2."""
        from pipeline.skills.base import SkillResult, SkillCost
        mock_skill = MagicMock()
        mock_skill.run.return_value = SkillResult(facts=[], cost=SkillCost(), cached=False)
        mock_make.return_value = mock_skill

        work = [
            WorkItem("society:x", "search_reddit", "missing", 1, "free"),
            WorkItem("society:x", "identify_gaps", "missing", 4, "free"),
        ]

        stats = execute(work, self.resolver, self.tracker)
        # Both should execute: search_reddit in pass 1, identify_gaps in pass 2
        self.assertEqual(stats.executed, 2)


class TestMakeSkill(unittest.TestCase):
    """Verify skill instantiation works for all registered skills."""

    def test_all_registered_skills_instantiate(self):
        for skill_id in SKILL_REGISTRY:
            with self.subTest(skill=skill_id):
                skill = _make_skill(skill_id)
                self.assertEqual(skill.skill_id, skill_id)

    def test_unknown_skill_raises(self):
        with self.assertRaises(ValueError):
            _make_skill("nonexistent_skill")


class TestPrintOutput(unittest.TestCase):
    """Test that output formatting doesn't crash."""

    def test_print_plan_empty(self):
        # Should not raise
        print_plan([])

    def test_print_plan_with_items(self):
        work = [
            WorkItem("society:test", "search_reddit", "missing", 1, "free"),
            WorkItem("society:test", "identify_gaps", "stale (15 days)", 4, "free"),
        ]
        print_plan(work)

    def test_print_summary(self):
        import time
        stats = ExecutionStats(
            executed=5, succeeded=3, cached=1, failed=1,
            facts_written=42, total_cost=0.12, start_time=time.time() - 30,
            errors=["society:x/fetch_rera: timeout"],
        )
        print_summary(stats)


class TestRepopulateAfterFactRemoval(unittest.TestCase):
    """Integration test: remove facts from a node, verify the engine re-fills them."""

    TARGET = "society:sobha-insignia"
    NODE_PATH = Path("data/knowledge/nodes/society/sobha-insignia.json")

    def setUp(self):
        self.assertTrue(self.NODE_PATH.exists(), "Node file must exist for this test")
        self.original_data = json.loads(self.NODE_PATH.read_text())
        self.original_facts = self.original_data.get("facts", [])

    def tearDown(self):
        # Restore original node file so test is idempotent
        tmp = self.NODE_PATH.with_suffix(".tmp")
        tmp.write_text(json.dumps(self.original_data, indent=2, default=str))
        tmp.rename(self.NODE_PATH)

    def test_removes_reddit_facts_then_refills(self):
        """Remove reddit facts, wipe freshness, run enrichment, verify they come back."""
        # 1. Confirm reddit facts exist before removal
        reddit_keys = {"reddit_thread_count", "reddit_total_score",
                       "reddit_total_comments", "reddit_threads"}
        original_reddit_facts = [f for f in self.original_facts if f["key"] in reddit_keys]
        self.assertGreater(len(original_reddit_facts), 0,
                           "Expected reddit facts to exist before test")

        # 2. Remove reddit facts from the node file
        stripped_facts = [f for f in self.original_facts if f["key"] not in reddit_keys]
        stripped_data = dict(self.original_data)
        stripped_data["facts"] = stripped_facts
        tmp = self.NODE_PATH.with_suffix(".tmp")
        tmp.write_text(json.dumps(stripped_data, indent=2, default=str))
        tmp.rename(self.NODE_PATH)

        # Verify removal
        check = json.loads(self.NODE_PATH.read_text())
        check_keys = {f["key"] for f in check.get("facts", [])}
        for rk in reddit_keys:
            self.assertNotIn(rk, check_keys, f"{rk} should be removed")

        # 3. Wipe freshness for reddit so engine sees it as stale
        tracker = FreshnessTracker()
        if self.TARGET in tracker.data and "reddit" in tracker.data[self.TARGET]:
            del tracker.data[self.TARGET]["reddit"]
            tracker.save()

        # 4. Run gap detection — should find search_reddit missing
        resolver = EntityResolver()
        tracker = FreshnessTracker()  # reload
        work = detect_gaps(resolver, tracker,
                           node_filter=self.TARGET, cost_tier_max="free")
        reddit_items = [w for w in work if w.skill_id == "search_reddit"]
        self.assertEqual(len(reddit_items), 1, "Should detect search_reddit as missing")
        self.assertEqual(reddit_items[0].node_id, self.TARGET)

        # 5. Execute enrichment with deterministic mocked Reddit output.
        work = reddit_items
        with patch("pipeline.enrich._make_skill") as mock_make:
            from pipeline.skills.base import SkillResult, SkillCost, SourcedFact, FactSource
            mock_skill = MagicMock()
            mock_skill.run.return_value = SkillResult(
                facts=[SourcedFact(
                    key="reddit_thread_count",
                    value={"type": "Numeric", "data": 1},
                    confidence=0.7,
                    source=FactSource(source_type="Reddit", skill_id="search_reddit"),
                )],
                cost=SkillCost(api_calls=1),
            )
            mock_make.return_value = mock_skill
            stats = execute(work, resolver, tracker, force=False)
        self.assertGreater(stats.executed, 0)
        self.assertEqual(stats.failed, 0)

        # 6. Verify reddit facts are back
        refilled = json.loads(self.NODE_PATH.read_text())
        refilled_keys = {f["key"] for f in refilled.get("facts", [])}
        self.assertIn("reddit_thread_count", refilled_keys,
                       "reddit_thread_count should be repopulated")

    def test_removes_rera_facts_then_refills(self):
        """Remove rera facts, wipe freshness, run enrichment, verify they come back."""
        rera_keys = {"rera_registered", "rera_number", "rera_status"}
        original_rera_facts = [f for f in self.original_facts if f["key"] in rera_keys]
        self.assertGreater(len(original_rera_facts), 0,
                           "Expected rera facts to exist before test")

        # Remove rera facts
        stripped_facts = [f for f in self.original_facts if f["key"] not in rera_keys]
        stripped_data = dict(self.original_data)
        stripped_data["facts"] = stripped_facts
        tmp = self.NODE_PATH.with_suffix(".tmp")
        tmp.write_text(json.dumps(stripped_data, indent=2, default=str))
        tmp.rename(self.NODE_PATH)

        # Wipe freshness
        tracker = FreshnessTracker()
        if self.TARGET in tracker.data and "rera" in tracker.data[self.TARGET]:
            del tracker.data[self.TARGET]["rera"]
            tracker.save()

        # Detect gaps
        resolver = EntityResolver()
        tracker = FreshnessTracker()
        work = detect_gaps(resolver, tracker,
                           node_filter=self.TARGET, cost_tier_max="free")
        rera_items = [w for w in work if w.skill_id == "fetch_rera"]
        self.assertEqual(len(rera_items), 1, "Should detect fetch_rera as missing")

        # Execute with deterministic mocked RERA output.
        work = rera_items
        with patch("pipeline.enrich._make_skill") as mock_make:
            from pipeline.skills.base import SkillResult, SkillCost, SourcedFact, FactSource
            mock_skill = MagicMock()
            mock_skill.run.return_value = SkillResult(
                facts=[SourcedFact(
                    key="rera_registered",
                    value={"type": "Bool", "data": True},
                    confidence=1.0,
                    source=FactSource(source_type="Rera", skill_id="fetch_rera"),
                )],
                cost=SkillCost(api_calls=1),
            )
            mock_make.return_value = mock_skill
            stats = execute(work, resolver, tracker, force=False)
        self.assertEqual(stats.failed, 0)

        # Verify rera facts are back
        refilled = json.loads(self.NODE_PATH.read_text())
        refilled_keys = {f["key"] for f in refilled.get("facts", [])}
        self.assertIn("rera_registered", refilled_keys,
                       "rera_registered should be repopulated")


if __name__ == "__main__":
    unittest.main()
