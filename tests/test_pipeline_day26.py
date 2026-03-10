"""Tests for Day 26 pipeline: entity resolver, RERA scraper, area intelligence, orchestrator."""

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch, MagicMock

# Add project root to path
sys.path.insert(0, str(Path(__file__).parent.parent))


class TestEntityResolver(unittest.TestCase):
    """Test the EntityResolver class."""

    def setUp(self):
        """Create a resolver with the real KG data."""
        from pipeline.entity_resolver import EntityResolver
        self.resolver = EntityResolver()

    def test_loads_existing_nodes(self):
        """Resolver should load all existing KG nodes at init."""
        # _name_to_id is the internal lookup table built from KG nodes
        self.assertGreater(len(self.resolver._name_to_id), 0)

    def test_resolve_society_exact(self):
        """Exact society name resolution."""
        result = self.resolver.resolve_society("Prestige Lakeside Habitat")
        self.assertEqual(result, "society:prestige-lakeside-habitat")

    def test_resolve_society_case_insensitive(self):
        """Society resolution should be case-insensitive."""
        result = self.resolver.resolve_society("PRESTIGE LAKESIDE HABITAT")
        self.assertEqual(result, "society:prestige-lakeside-habitat")

    def test_resolve_area(self):
        """Area name resolution."""
        result = self.resolver.resolve_area("Whitefield")
        self.assertEqual(result, "area:whitefield")

    def test_resolve_builder(self):
        """Builder name resolution."""
        result = self.resolver.resolve_builder("Sobha Limited")
        # Should resolve to existing builder node
        self.assertTrue(result.startswith("builder:"))
        self.assertIn("sobha", result)

    def test_resolve_builder_with_prefix(self):
        """Builder resolution should handle M/s. prefix."""
        result = self.resolver.resolve_builder("M/s. SOBHA LIMITED")
        self.assertTrue(result.startswith("builder:"))
        self.assertIn("sobha", result)

    def test_resolve_html_entities(self):
        """Should handle HTML entities like &amp;."""
        normalized = self.resolver._normalize("Bhavani Builders &amp; Developers")
        self.assertIn("&", normalized)  # Should decode &amp;
        self.assertNotIn("&amp;", normalized)

    def test_alias_registration(self):
        """Aliases should be remembered and persisted."""
        with tempfile.TemporaryDirectory() as tmpdir:
            aliases_path = Path(tmpdir) / "aliases.json"
            aliases_path.write_text("{}")

            from pipeline.entity_resolver import EntityResolver
            r = EntityResolver()
            r.aliases_path = aliases_path
            r.register_alias("Test Project", "society:test-project")
            r.save()

            # Reload and verify
            loaded = json.loads(aliases_path.read_text())
            self.assertIn("test project", loaded)  # Should be normalized

    def test_rera_mapping(self):
        """RERA ACK -> entity mapping should work."""
        self.resolver.register_rera_mapping(
            "ACK/KA/RERA/1251/446/PR/090822/006221", "society:sobha-insignia"
        )
        result = self.resolver.lookup_by_rera("ACK/KA/RERA/1251/446/PR/090822/006221")
        self.assertEqual(result, "society:sobha-insignia")

    def test_rera_mapping_missing(self):
        """Looking up an unregistered RERA ACK should return None."""
        result = self.resolver.lookup_by_rera("ACK/NONEXISTENT/12345")
        self.assertIsNone(result)

    def test_fuzzy_match(self):
        """Fuzzy matching should find partial name matches."""
        result = self.resolver.resolve_society("Prestige Lakeside")
        self.assertEqual(result, "society:prestige-lakeside-habitat")

    def test_get_all_entities(self):
        """Should return all entities of a given type."""
        societies = self.resolver.get_all_entities("society")
        self.assertGreater(len(societies), 0)
        for s in societies:
            self.assertTrue(s.startswith("society:"))

    def test_get_name_reverse_lookup(self):
        """get_name should return display name for a canonical ID."""
        # Resolve to populate the internal mappings
        self.resolver.resolve_society("Prestige Lakeside Habitat")
        name = self.resolver.get_name("society:prestige-lakeside-habitat")
        self.assertIsNotNone(name)
        self.assertIn("Prestige", name)

    def test_slugify(self):
        """Slugify should produce clean URL-friendly slugs."""
        slug = self.resolver._slugify("Prestige Lakeside Habitat")
        self.assertEqual(slug, "prestige-lakeside-habitat")

    def test_slugify_with_special_chars(self):
        """Slugify should handle special characters."""
        slug = self.resolver._slugify("Sobha's Dream Acres - Phase 2")
        # Should be lowercase, hyphens only, no special chars
        self.assertRegex(slug, r'^[a-z0-9-]+$')

    def test_normalize_strips_suffixes(self):
        """Normalize should strip common suffixes like 'Private Limited'."""
        result = self.resolver._normalize("Sobha Limited")
        self.assertEqual(result, "sobha")

    def test_normalize_strips_pvt_ltd(self):
        """Normalize should strip 'Pvt. Ltd.' suffix."""
        result = self.resolver._normalize("Brigade Enterprises Pvt. Ltd.")
        self.assertNotIn("pvt", result)
        self.assertNotIn("ltd", result)


class TestFreshnessTracker(unittest.TestCase):
    """Test the FreshnessTracker class."""

    def test_new_entity_is_stale(self):
        """An entity never enriched should be stale."""
        from pipeline.entity_resolver import FreshnessTracker
        tracker = FreshnessTracker(path="/tmp/test_freshness_nonexistent.json")
        self.assertTrue(tracker.is_stale("society:test", "rera"))

    def test_mark_fresh_then_check(self):
        """After marking fresh, entity should not be stale."""
        from pipeline.entity_resolver import FreshnessTracker
        tracker = FreshnessTracker(path="/tmp/test_freshness_nonexistent.json")
        tracker.mark_fresh("society:test", "rera", {"count": 5})
        self.assertFalse(tracker.is_stale("society:test", "rera", ttl_days=1))

    def test_zero_ttl_is_always_stale(self):
        """With TTL of 0, even freshly marked entities are stale."""
        from pipeline.entity_resolver import FreshnessTracker
        tracker = FreshnessTracker(path="/tmp/test_freshness_nonexistent.json")
        tracker.mark_fresh("society:test", "rera")
        self.assertTrue(tracker.is_stale("society:test", "rera", ttl_days=0))

    def test_mark_fresh_stores_metadata(self):
        """mark_fresh should store additional metadata."""
        from pipeline.entity_resolver import FreshnessTracker
        tracker = FreshnessTracker(path="/tmp/test_freshness_nonexistent.json")
        tracker.mark_fresh("society:test", "rera", {"thread_count": 5})
        entry = tracker.data["society:test"]["rera"]
        self.assertEqual(entry["thread_count"], 5)
        self.assertIn("last_fetched", entry)

    def test_get_stale_entities(self):
        """get_stale_entities should return entities needing refresh."""
        from pipeline.entity_resolver import FreshnessTracker
        tracker = FreshnessTracker(path="/tmp/test_freshness_nonexistent.json")
        tracker.mark_fresh("society:a", "rera")
        tracker.mark_fresh("society:b", "rera")
        # With ttl_days=0, both should be stale
        stale = tracker.get_stale_entities("society", "rera", ttl_days=0)
        self.assertEqual(len(stale), 2)

    def test_get_never_enriched(self):
        """get_never_enriched should find entities without a source record."""
        from pipeline.entity_resolver import FreshnessTracker
        tracker = FreshnessTracker(path="/tmp/test_freshness_nonexistent.json")
        tracker.mark_fresh("society:a", "rera")
        all_ids = ["society:a", "society:b", "society:c"]
        never = tracker.get_never_enriched(all_ids, "rera")
        self.assertIn("society:b", never)
        self.assertIn("society:c", never)
        self.assertNotIn("society:a", never)

    def test_persistence_roundtrip(self):
        """Tracker data should survive save + reload."""
        from pipeline.entity_resolver import FreshnessTracker
        with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as f:
            path = f.name

        try:
            tracker = FreshnessTracker(path=path)
            tracker.mark_fresh("society:test", "rera", {"count": 42})
            tracker.save()

            tracker2 = FreshnessTracker(path=path)
            self.assertFalse(tracker2.is_stale("society:test", "rera", ttl_days=1))
            self.assertEqual(tracker2.data["society:test"]["rera"]["count"], 42)
        finally:
            os.unlink(path)


class TestFetchReraSkill(unittest.TestCase):
    """Test the RERA scraper skill structure (without hitting the network)."""

    def test_import(self):
        """fetch_rera should import cleanly."""
        from pipeline.skills.fetch_rera import FetchReraSkill
        skill = FetchReraSkill()
        self.assertEqual(skill.skill_id, "fetch_rera")

    def test_skill_version(self):
        """Should be v2.0 (replacing verify_rera v1.0)."""
        from pipeline.skills.fetch_rera import FetchReraSkill
        skill = FetchReraSkill()
        self.assertEqual(skill.version, "2.0")

    def test_empty_project_name(self):
        """Should return empty result for empty project name."""
        from pipeline.skills.fetch_rera import FetchReraSkill
        skill = FetchReraSkill()
        result = skill.execute({"project_name": ""})
        self.assertEqual(result.confidence, 0.0)

    def test_missing_project_name(self):
        """Should return empty result when project_name key is absent."""
        from pipeline.skills.fetch_rera import FetchReraSkill
        skill = FetchReraSkill()
        result = skill.execute({})
        self.assertEqual(result.confidence, 0.0)

    def test_listing_entry_dataclass(self):
        """ReraListingEntry should be a proper dataclass."""
        from pipeline.skills.fetch_rera import ReraListingEntry
        entry = ReraListingEntry(
            ack_number="ACK/KA/RERA/1251/446/PR/090822/006221",
            registration_number="PRM/KA/RERA/1251/446/PR/030922/005212",
            project_name="Sobha Insignia",
            promoter_name="SOBHA LIMITED",
        )
        self.assertEqual(entry.project_name, "Sobha Insignia")
        self.assertEqual(entry.ack_number, "ACK/KA/RERA/1251/446/PR/090822/006221")

    def test_rera_detail_dataclass(self):
        """ReraProjectDetail should have all expected fields."""
        from pipeline.skills.fetch_rera import ReraProjectDetail
        detail = ReraProjectDetail()
        # Defaults should be falsy/zero
        self.assertFalse(detail.land_litigation)
        self.assertEqual(detail.complaints_count, 0)
        self.assertEqual(detail.builder_revocations, 0)
        self.assertEqual(detail.builder_other_rera_projects, 0)
        self.assertIsNone(detail.total_units)
        self.assertIsNone(detail.total_project_cost_inr)
        self.assertEqual(detail.project_name, "")

    def test_rera_detail_dataclass_full(self):
        """ReraProjectDetail should accept all fields."""
        from pipeline.skills.fetch_rera import ReraProjectDetail
        detail = ReraProjectDetail(
            ack_number="ACK123",
            registration_number="PRM123",
            project_name="Test Project",
            promoter_name="Test Builder",
            status="APPROVED",
            completion_date="31/03/2026",
            total_units=100,
            total_project_cost_inr=500000000.0,
            land_cost_inr=100000000.0,
            construction_cost_inr=400000000.0,
            complaints_count=5,
            complaints_resolved=3,
            escrow_bank="HDFC Bank",
            land_litigation=False,
            builder_other_rera_projects=50,
            builder_revocations=1,
            builder_states=["Karnataka", "Tamil Nadu"],
        )
        self.assertEqual(detail.total_units, 100)
        self.assertEqual(detail.builder_states, ["Karnataka", "Tamil Nadu"])

    def test_fact_production(self):
        """rera_detail_to_facts should produce self-describing facts."""
        from pipeline.skills.fetch_rera import ReraProjectDetail, rera_detail_to_facts
        detail = ReraProjectDetail(
            registration_number="PRM/KA/RERA/1251/446/PR/030922/005212",
            project_name="Sobha Insignia",
            promoter_name="SOBHA LIMITED",
            status="APPROVED",
            completion_date="31/03/2026",
            total_units=33,
            total_project_cost_inr=762385229.0,
            land_cost_inr=148670485.0,
            construction_cost_inr=613714745.0,
            complaints_count=20,
            complaints_resolved=19,
            escrow_bank="Axis Bank",
            land_litigation=False,
            builder_other_rera_projects=104,
            builder_revocations=2,
            builder_states=["Maharashtra", "Gujarat", "Tamil Nadu"],
        )
        facts = rera_detail_to_facts(detail)

        # Should produce multiple facts
        self.assertGreater(len(facts), 10)

        # All facts should have confidence 1.0 (government source)
        for fact in facts:
            self.assertEqual(fact.confidence, 1.0)

        # All facts should have source_type "Rera"
        for fact in facts:
            self.assertEqual(fact.source.source_type, "Rera")

        # Check specific facts exist
        fact_keys = [f.key for f in facts]
        self.assertIn("rera_registered", fact_keys)
        self.assertIn("rera_number", fact_keys)
        self.assertIn("rera_status", fact_keys)
        self.assertIn("rera_total_project_cost", fact_keys)
        self.assertIn("rera_complaints_count", fact_keys)
        self.assertIn("rera_escrow_bank", fact_keys)
        self.assertIn("rera_land_litigation", fact_keys)
        self.assertIn("rera_builder_projects_count", fact_keys)
        self.assertIn("rera_builder_revocations", fact_keys)
        self.assertIn("rera_builder_states", fact_keys)
        self.assertIn("rera_cost_per_unit", fact_keys)

        # Check display_templates are set
        for fact in facts:
            self.assertIsNotNone(
                fact.display_template,
                f"Missing display_template for {fact.key}"
            )

    def test_fact_production_minimal(self):
        """Facts should still be produced with minimal detail input."""
        from pipeline.skills.fetch_rera import ReraProjectDetail, rera_detail_to_facts
        detail = ReraProjectDetail()  # All defaults
        facts = rera_detail_to_facts(detail)
        # Should at least have rera_registered, rera_number, rera_ack_number,
        # rera_complaints_count, rera_builder_revocations, rera_portal_url
        self.assertGreater(len(facts), 3)
        fact_keys = [f.key for f in facts]
        self.assertIn("rera_registered", fact_keys)
        self.assertIn("rera_portal_url", fact_keys)

    def test_fact_to_dict_serializable(self):
        """All produced facts should be JSON-serializable via to_dict()."""
        from pipeline.skills.fetch_rera import ReraProjectDetail, rera_detail_to_facts
        detail = ReraProjectDetail(
            registration_number="PRM123",
            status="APPROVED",
            total_project_cost_inr=100000000.0,
            complaints_count=5,
        )
        facts = rera_detail_to_facts(detail)
        for fact in facts:
            d = fact.to_dict()
            # Should not raise
            serialized = json.dumps(d, default=str)
            self.assertIsInstance(serialized, str)

    def test_search_result_dataclass(self):
        """ReraSearchResult should be importable and usable."""
        from pipeline.skills.fetch_rera import ReraSearchResult
        result = ReraSearchResult(
            ack_number="ACK123",
            registration_number="PRM123",
            project_name="Test",
            promoter_name="Builder",
            status="APPROVED",
            district="Bangalore Urban",
            taluk="North",
            project_type="Residential",
            approved_on="01/01/2023",
            completion_date="31/12/2025",
            original_completion_date="31/12/2024",
            numeric_id="12345",
        )
        self.assertEqual(result.numeric_id, "12345")

    def test_estimated_cost(self):
        """Estimated cost should report zero USD (free government source)."""
        from pipeline.skills.fetch_rera import FetchReraSkill
        skill = FetchReraSkill()
        cost = skill.estimated_cost()
        self.assertEqual(cost.estimated_usd, 0.0)
        self.assertGreater(cost.api_calls, 0)


class TestLearnAreaSkill(unittest.TestCase):
    """Test the enhanced learn_area skill."""

    def test_import(self):
        """learn_area should import cleanly."""
        from pipeline.skills.learn_area import LearnAreaSkill
        skill = LearnAreaSkill()
        self.assertEqual(skill.skill_id, "learn_area")

    def test_version_v2(self):
        """Should be v2.0 (Reddit-first)."""
        from pipeline.skills.learn_area import LearnAreaSkill
        skill = LearnAreaSkill()
        self.assertEqual(skill.version, "2.0")

    def test_fetch_area_threads_function_exists(self):
        """Should have the area-focused Reddit fetcher."""
        from pipeline.skills.learn_area import fetch_area_reddit_threads
        # Just verify the function exists and is callable
        self.assertTrue(callable(fetch_area_reddit_threads))

    def test_fetch_thread_comments_function_exists(self):
        """Should have the comment fetcher."""
        from pipeline.skills.learn_area import fetch_thread_comments
        self.assertTrue(callable(fetch_thread_comments))

    def test_empty_area_name(self):
        """Should return empty result for empty area name."""
        from pipeline.skills.learn_area import LearnAreaSkill
        skill = LearnAreaSkill()
        result = skill.execute({"area_name": ""})
        self.assertEqual(result.confidence, 0.0)

    def test_is_base_skill_subclass(self):
        """LearnAreaSkill should inherit from BaseSkill."""
        from pipeline.skills.learn_area import LearnAreaSkill
        from pipeline.skills.base import BaseSkill
        self.assertTrue(issubclass(LearnAreaSkill, BaseSkill))


class TestVerifyReraDeleted(unittest.TestCase):
    """Verify that verify_rera.py was deleted."""

    def test_file_not_exists(self):
        """verify_rera.py should not exist."""
        path = Path(__file__).parent.parent / "pipeline" / "skills" / "verify_rera.py"
        self.assertFalse(path.exists(), "verify_rera.py should have been deleted")

    def test_import_fails(self):
        """Importing verify_rera should fail."""
        with self.assertRaises((ModuleNotFoundError, ImportError)):
            import pipeline.skills.verify_rera


class TestOrchestrator(unittest.TestCase):
    """Test the pipeline orchestrator."""

    def test_import(self):
        """orchestrate module should import cleanly."""
        import pipeline.orchestrate

    def test_main_function_exists(self):
        """Should have a main() function."""
        from pipeline.orchestrate import main
        self.assertTrue(callable(main))

    def test_enrich_entity_function_exists(self):
        """Should have enrich_entity function."""
        from pipeline.orchestrate import enrich_entity
        self.assertTrue(callable(enrich_entity))

    def test_enrich_society_function_exists(self):
        """Should have enrich_society function."""
        from pipeline.orchestrate import enrich_society
        self.assertTrue(callable(enrich_society))

    def test_enrich_area_function_exists(self):
        """Should have enrich_area function."""
        from pipeline.orchestrate import enrich_area
        self.assertTrue(callable(enrich_area))

    def test_enrich_builder_function_exists(self):
        """Should have enrich_builder function."""
        from pipeline.orchestrate import enrich_builder
        self.assertTrue(callable(enrich_builder))

    def test_enrich_entity_dispatches_by_type(self):
        """enrich_entity should dispatch based on entity type prefix."""
        from pipeline.orchestrate import enrich_entity
        from pipeline.entity_resolver import EntityResolver, FreshnessTracker

        resolver = EntityResolver()
        tracker = FreshnessTracker(path="/tmp/test_orch_freshness.json")

        # Should not raise for unknown entity (just logs a warning)
        with patch("pipeline.orchestrate.logger") as mock_logger:
            enrich_entity("unknown:test", resolver, tracker)
            mock_logger.warning.assert_called()


class TestSourcedFactContract(unittest.TestCase):
    """Verify that all skills produce facts matching the SourcedFact contract."""

    def test_rera_facts_have_answers_preferences(self):
        """RERA facts that affect search should have answers_preferences."""
        from pipeline.skills.fetch_rera import ReraProjectDetail, rera_detail_to_facts
        detail = ReraProjectDetail(
            registration_number="TEST",
            status="APPROVED",
            total_project_cost_inr=100000000.0,
            complaints_count=5,
        )
        facts = rera_detail_to_facts(detail)

        # Key searchable facts should have answers_preferences
        searchable_keys = [
            "rera_registered", "rera_status",
            "rera_total_project_cost", "rera_complaints_count",
        ]
        for fact in facts:
            if fact.key in searchable_keys:
                self.assertIsNotNone(
                    fact.answers_preferences,
                    f"Fact {fact.key} should have answers_preferences for search matching"
                )
                self.assertGreater(len(fact.answers_preferences), 0)

    def test_rera_facts_source_has_skill_id(self):
        """Every RERA fact's source should reference the skill that produced it."""
        from pipeline.skills.fetch_rera import ReraProjectDetail, rera_detail_to_facts
        detail = ReraProjectDetail(registration_number="TEST")
        facts = rera_detail_to_facts(detail)
        for fact in facts:
            self.assertEqual(fact.source.skill_id, "fetch_rera")

    def test_rera_facts_scoring_hints_valid(self):
        """Facts with scoring_hint should have valid direction values."""
        from pipeline.skills.fetch_rera import ReraProjectDetail, rera_detail_to_facts
        detail = ReraProjectDetail(
            registration_number="TEST",
            status="APPROVED",
            completion_date="31/12/2026",
            original_completion_date="31/12/2024",
            total_project_cost_inr=100000000.0,
            complaints_count=10,
            land_litigation=False,
            builder_other_rera_projects=50,
            builder_revocations=1,
        )
        facts = rera_detail_to_facts(detail)
        valid_directions = {"HigherIsBetter", "LowerIsBetter", "TextMatch"}
        for fact in facts:
            if fact.scoring_hint:
                self.assertIn(
                    fact.scoring_hint.get("direction"),
                    valid_directions,
                    f"Invalid scoring direction for {fact.key}: {fact.scoring_hint}"
                )
                self.assertGreater(
                    fact.scoring_hint.get("weight", 0), 0,
                    f"Scoring weight should be positive for {fact.key}"
                )

    def test_sourced_fact_to_dict_contract(self):
        """SourcedFact.to_dict() should produce a dict matching the expected schema."""
        from pipeline.skills.base import SourcedFact, FactSource
        fact = SourcedFact(
            key="test_key",
            value={"type": "Text", "data": "hello"},
            confidence=0.9,
            source=FactSource(source_type="Rera", skill_id="test"),
            display_template="Test: {value}",
            answers_preferences=["test query"],
            scoring_hint={"direction": "HigherIsBetter", "weight": 1.0},
        )
        d = fact.to_dict()
        self.assertEqual(d["key"], "test_key")
        self.assertEqual(d["value"]["type"], "Text")
        self.assertEqual(d["confidence"], 0.9)
        self.assertEqual(d["source"]["source_type"], "Rera")
        self.assertEqual(d["display_template"], "Test: {value}")
        self.assertEqual(d["answers_preferences"], ["test query"])
        self.assertIn("learned_at", d)


class TestBaseSkillInfrastructure(unittest.TestCase):
    """Test the BaseSkill caching and run mechanics."""

    def test_skill_result_to_dict(self):
        """SkillResult.to_dict() should be JSON-serializable."""
        from pipeline.skills.base import SkillResult, SkillCost
        result = SkillResult(confidence=0.9, cost=SkillCost(api_calls=3))
        d = result.to_dict()
        serialized = json.dumps(d)
        self.assertIn("confidence", serialized)

    def test_cache_key_deterministic(self):
        """Same input should produce same cache key."""
        from pipeline.skills.fetch_rera import FetchReraSkill
        skill = FetchReraSkill()
        key1 = skill._cache_key({"project_name": "Test"})
        key2 = skill._cache_key({"project_name": "Test"})
        self.assertEqual(key1, key2)

    def test_cache_key_varies_with_input(self):
        """Different input should produce different cache key."""
        from pipeline.skills.fetch_rera import FetchReraSkill
        skill = FetchReraSkill()
        key1 = skill._cache_key({"project_name": "Test A"})
        key2 = skill._cache_key({"project_name": "Test B"})
        self.assertNotEqual(key1, key2)


if __name__ == "__main__":
    unittest.main(verbosity=2)
