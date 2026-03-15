"""
Phase 0 validation harness for OpenEstates.

Loads 10 validation property IDs and checks each against:
  - Seed data completeness
  - KG society node enrichment (RERA, builder, pricing, reviews)
  - Property KG node existence and embeddings
  - Trust badge eligibility
  - Provenance chain integrity
  - Seller matching (if applicable)
  - Search readiness

Outputs structured report to stdout and JSON to data/validation/phase0_baseline.json.
Exit code 0 if no FAILs, 1 if any FAILs.
"""

from __future__ import annotations

import json
import os
import sys
from collections import Counter
from datetime import datetime
from pathlib import Path
from typing import Optional, List, Dict

# Resolve project root (3 levels up from this file)
ROOT = Path(__file__).resolve().parent.parent.parent
DATA = ROOT / "data"


def load_json(path: Path):
    """Load JSON file, return None if missing or invalid."""
    try:
        with open(path) as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return None


class CheckResult:
    """A single check result: PASS, WARN, or FAIL."""

    def __init__(self, status: str, check_name: str, message: str, details: dict | None = None):
        self.status = status  # "PASS", "WARN", "FAIL"
        self.check_name = check_name
        self.message = message
        self.details = details or {}

    def to_dict(self):
        return {
            "status": self.status,
            "check": self.check_name,
            "message": self.message,
            "details": self.details,
        }


class PropertyValidator:
    """Validates a single property across all Phase 0 criteria."""

    def __init__(self, prop_id: str, properties: list, societies: list, sellers: list):
        self.prop_id = prop_id
        self.properties = {p["id"]: p for p in properties}
        self.societies = {s["id"]: s for s in societies}
        self.sellers = sellers
        self.results: list[CheckResult] = []

    def check(self, status: str, name: str, message: str, details: dict | None = None):
        self.results.append(CheckResult(status, name, message, details))

    def run_all(self) -> list[dict]:
        self.check_seed_data()
        self.check_kg_society()
        self.check_market_pricing()
        self.check_reviews()
        self.check_property_kg_node()
        self.check_trust_badges()
        self.check_provenance()
        self.check_seller_matching()
        self.check_search_readiness()
        return [r.to_dict() for r in self.results]

    # --- Seed Data ---
    def check_seed_data(self):
        prop = self.properties.get(self.prop_id)
        if not prop:
            self.check("FAIL", "seed_exists", f"Property {self.prop_id} not found in properties.json")
            return

        required_fields = ["id", "title", "area", "city", "bhk", "price", "carpet_area_sqft", "hero_image"]
        missing = [f for f in required_fields if not prop.get(f)]
        if missing:
            self.check("FAIL", "seed_required_fields", f"Missing required fields: {missing}", {"missing": missing})
        else:
            self.check("PASS", "seed_required_fields", "All required fields present")

        # Hero image file exists (paths are relative to frontend/public, e.g. /societies/slug/1.jpg)
        hero = prop.get("hero_image", "")
        if hero:
            # Try frontend/public as base (hero paths start with /)
            hero_path = ROOT / "frontend" / "public" / hero.lstrip("/")
            if hero_path.exists():
                self.check("PASS", "seed_hero_image_exists", f"Hero image file exists: {hero}")
            else:
                self.check("WARN", "seed_hero_image_exists", f"Hero image path set but file missing: {hero}")
        else:
            self.check("FAIL", "seed_hero_image_exists", "No hero_image set")

        # Society mapping
        society_id = prop.get("society_id", "")
        if society_id and society_id in self.societies:
            self.check("PASS", "seed_society_mapping", f"Society {society_id} found in societies.json")
        elif society_id:
            self.check("WARN", "seed_society_mapping", f"society_id={society_id} not found in societies.json")
        else:
            self.check("FAIL", "seed_society_mapping", "No society_id set")

    @staticmethod
    def _society_slug(society_id: str) -> str:
        """Derive KG file slug from society_id (strip 'soc-' prefix if present)."""
        if society_id.startswith("soc-"):
            return society_id[4:]
        return society_id

    # --- KG Society Node ---
    def check_kg_society(self):
        prop = self.properties.get(self.prop_id)
        if not prop:
            self.check("FAIL", "kg_society_exists", "Cannot check: property not in seed data")
            return

        society_id = prop.get("society_id", "")
        slug = self._society_slug(society_id)
        kg_path = DATA / "knowledge" / "nodes" / "society" / f"{slug}.json"
        node = load_json(kg_path)

        if not node:
            self.check("FAIL", "kg_society_exists", f"KG node missing: {kg_path}")
            return

        self.check("PASS", "kg_society_exists", f"KG society node exists with {len(node.get('facts', []))} facts")
        facts = node.get("facts", [])
        fact_keys = {f["key"] for f in facts}

        # RERA facts
        rera_keys = {"rera_registered", "rera_number", "rera_ack_number"}
        found_rera = rera_keys & fact_keys
        if len(found_rera) >= 2:
            self.check("PASS", "kg_society_rera", f"RERA facts present: {sorted(found_rera)}")
        elif found_rera:
            self.check("WARN", "kg_society_rera", f"Partial RERA: {sorted(found_rera)}, missing: {sorted(rera_keys - found_rera)}")
        else:
            self.check("FAIL", "kg_society_rera", "No RERA facts found")

        # Builder name
        if "builder_name" in fact_keys:
            self.check("PASS", "kg_society_builder", "builder_name fact present")
        else:
            self.check("WARN", "kg_society_builder", "No builder_name fact in KG node")

        # Project status
        if "project_status" in fact_keys:
            self.check("PASS", "kg_society_project_status", "project_status fact present")
        else:
            self.check("WARN", "kg_society_project_status", "No project_status fact")

        # Total units (accept either total_units or rera_total_units)
        if "total_units" in fact_keys or "rera_total_units" in fact_keys:
            matched_key = "total_units" if "total_units" in fact_keys else "rera_total_units"
            self.check("PASS", "kg_society_total_units", f"{matched_key} fact present")
        else:
            self.check("WARN", "kg_society_total_units", "No total_units or rera_total_units fact")

    # --- Market Pricing ---
    def check_market_pricing(self):
        prop = self.properties.get(self.prop_id)
        if not prop:
            self.check("FAIL", "market_pricing", "Cannot check: property not in seed data")
            return

        society_id = prop.get("society_id", "")
        kg_path = DATA / "knowledge" / "nodes" / "society" / f"{self._society_slug(society_id)}.json"
        node = load_json(kg_path)

        if not node:
            self.check("WARN", "market_pricing", "Cannot check: KG society node missing")
            return

        facts = node.get("facts", [])
        fact_keys = {f["key"] for f in facts}

        pricing_keys = {k for k in fact_keys if k.startswith("pricing_") or k == "price_per_sqft" or k == "configurations"}
        if len(pricing_keys) >= 2:
            self.check("PASS", "market_pricing", f"Pricing facts present: {sorted(pricing_keys)}")
        elif pricing_keys:
            self.check("WARN", "market_pricing", f"Sparse pricing data: {sorted(pricing_keys)}")
        else:
            self.check("WARN", "market_pricing", "No pricing_* facts in KG society node")

    # --- Reviews ---
    def check_reviews(self):
        prop = self.properties.get(self.prop_id)
        if not prop:
            self.check("FAIL", "reviews", "Cannot check: property not in seed data")
            return

        society_id = prop.get("society_id", "")
        kg_path = DATA / "knowledge" / "nodes" / "society" / f"{self._society_slug(society_id)}.json"
        node = load_json(kg_path)

        if not node:
            self.check("WARN", "reviews", "Cannot check: KG society node missing")
            return

        facts = node.get("facts", [])
        fact_keys = {f["key"] for f in facts}

        has_reddit = any(k.startswith("reddit_") for k in fact_keys)
        has_sentiment = "google_sentiment" in fact_keys or "sentiment" in fact_keys

        if has_reddit and has_sentiment:
            self.check("PASS", "reviews", "Reddit + sentiment facts present")
        elif has_reddit:
            self.check("WARN", "reviews", "Reddit facts present but no Google sentiment")
        elif has_sentiment:
            self.check("WARN", "reviews", "Sentiment present but no Reddit facts")
        else:
            self.check("WARN", "reviews", "No review/sentiment facts found")

    # --- Property KG Node ---
    def check_property_kg_node(self):
        kg_path = DATA / "knowledge" / "nodes" / "property" / f"{self.prop_id}.json"
        node = load_json(kg_path)

        if not node:
            self.check("WARN", "property_kg_exists", f"Property KG node missing: {kg_path.name}")
            return

        facts = node.get("facts", [])
        fact_keys = {f["key"] for f in facts}
        self.check("PASS", "property_kg_exists", f"Property KG node exists with {len(facts)} facts")

        # Embedding computed
        if "embedding_computed" in fact_keys:
            self.check("PASS", "property_kg_embedding", "embedding_computed fact present")
        else:
            self.check("WARN", "property_kg_embedding", "No embedding_computed fact")

        # Summary embedding vector
        if node.get("summary_embedding"):
            self.check("PASS", "property_kg_embedding_vector", "summary_embedding vector present")
        else:
            self.check("WARN", "property_kg_embedding_vector", "No summary_embedding vector on property node")

    # --- Trust Badges ---
    def check_trust_badges(self):
        prop = self.properties.get(self.prop_id)
        if not prop:
            self.check("FAIL", "trust_badges", "Cannot check: property not in seed data")
            return

        society_id = prop.get("society_id", "")
        kg_path = DATA / "knowledge" / "nodes" / "society" / f"{self._society_slug(society_id)}.json"
        node = load_json(kg_path)

        if not node:
            self.check("WARN", "trust_badges_rera", "Cannot check: KG society node missing")
            return

        facts = node.get("facts", [])
        fact_keys = {f["key"] for f in facts}

        # RERA badge eligibility: rera_registered = true AND root_source = rera
        rera_reg_facts = [f for f in facts if f["key"] == "rera_registered"]
        root_source = node.get("root_source")

        if rera_reg_facts:
            val = rera_reg_facts[0].get("value", {})
            is_registered = val.get("data") in (True, "true", "True", "yes")
            if is_registered and root_source in ("Rera", "rera"):
                self.check("PASS", "trust_badges_rera", "RERA badge eligible: registered + rera root_source")
            elif is_registered:
                self.check("WARN", "trust_badges_rera", f"RERA registered but root_source={root_source}")
            else:
                self.check("WARN", "trust_badges_rera", "rera_registered fact present but value is not true")
        else:
            self.check("WARN", "trust_badges_rera", "No rera_registered fact for badge eligibility")

    # --- Provenance ---
    def check_provenance(self):
        prop = self.properties.get(self.prop_id)
        if not prop:
            self.check("FAIL", "provenance", "Cannot check: property not in seed data")
            return

        society_id = prop.get("society_id", "")
        kg_path = DATA / "knowledge" / "nodes" / "society" / f"{self._society_slug(society_id)}.json"
        node = load_json(kg_path)

        if not node:
            self.check("WARN", "provenance", "Cannot check: KG society node missing")
            return

        facts = node.get("facts", [])
        missing_source = 0
        missing_skill_id = 0
        missing_learned_at = 0
        duplicate_keys = []

        key_counts = Counter(f["key"] for f in facts)
        for key, count in key_counts.items():
            if count > 1:
                duplicate_keys.append(f"{key}(x{count})")

        for fact in facts:
            source = fact.get("source", {})
            if not source.get("source_type"):
                missing_source += 1
            if not source.get("skill_id"):
                missing_skill_id += 1
            if not fact.get("learned_at"):
                missing_learned_at += 1

        issues = []
        if missing_source:
            issues.append(f"{missing_source} facts missing source_type")
        if missing_skill_id:
            issues.append(f"{missing_skill_id} facts missing skill_id")
        if missing_learned_at:
            issues.append(f"{missing_learned_at} facts missing learned_at")

        if issues:
            self.check("WARN", "provenance_completeness", f"Provenance gaps: {'; '.join(issues)}",
                        {"missing_source": missing_source, "missing_skill_id": missing_skill_id,
                         "missing_learned_at": missing_learned_at})
        else:
            self.check("PASS", "provenance_completeness", f"All {len(facts)} facts have complete provenance")

        if duplicate_keys:
            self.check("WARN", "provenance_duplicates", f"Duplicate fact keys: {', '.join(duplicate_keys)}",
                        {"duplicates": duplicate_keys})
        else:
            self.check("PASS", "provenance_duplicates", "No duplicate fact keys")

    # --- Seller Matching ---
    def check_seller_matching(self):
        prop = self.properties.get(self.prop_id)
        if not prop:
            self.check("FAIL", "seller_matching", "Cannot check: property not in seed data")
            return

        seller_id = prop.get("seller_id")
        if not seller_id:
            self.check("PASS", "seller_matching", "No seller_id — seller checks skipped (expected for some properties)")
            return

        # Find the seller
        seller = None
        for s in self.sellers:
            if s["id"] == seller_id:
                seller = s
                break

        if not seller:
            self.check("FAIL", "seller_exists", f"seller_id={seller_id} not found in sellers.json")
            return

        self.check("PASS", "seller_exists", f"Seller {seller_id} found")

        # Property in seller's property_ids
        if self.prop_id in seller.get("property_ids", []):
            self.check("PASS", "seller_property_link", f"Property {self.prop_id} in seller's property_ids")
        else:
            self.check("FAIL", "seller_property_link", f"Property {self.prop_id} NOT in seller {seller_id}'s property_ids")

        # Seller has property_prompt
        if seller.get("has_property_prompt") and seller.get("property_prompt"):
            self.check("PASS", "seller_prompt", "Seller has property_prompt")
        else:
            self.check("WARN", "seller_prompt", "Seller missing property_prompt")

    # --- Search Readiness ---
    def check_search_readiness(self):
        # Check property KG node for embedding
        prop_kg_path = DATA / "knowledge" / "nodes" / "property" / f"{self.prop_id}.json"
        prop_node = load_json(prop_kg_path)

        has_embedding = False
        if prop_node:
            fact_keys = {f["key"] for f in prop_node.get("facts", [])}
            has_embedding = "embedding_computed" in fact_keys or bool(prop_node.get("summary_embedding"))

        if has_embedding:
            self.check("PASS", "search_embedding", "Embedding computed for search")
        else:
            self.check("WARN", "search_embedding", "No embedding — property may not appear in semantic search")

        # Check society KG for self-describing scoring facts (answers_preferences)
        prop = self.properties.get(self.prop_id)
        if not prop:
            return

        society_id = prop.get("society_id", "")
        kg_path = DATA / "knowledge" / "nodes" / "society" / f"{self._society_slug(society_id)}.json"
        node = load_json(kg_path)

        if not node:
            self.check("WARN", "search_scoring_facts", "Cannot check: KG society node missing")
            return

        facts = node.get("facts", [])
        facts_with_answers = [f for f in facts if f.get("answers_preferences")]
        facts_with_scoring = [f for f in facts if f.get("scoring_hint")]

        if facts_with_answers:
            self.check("PASS", "search_answers_preferences",
                        f"{len(facts_with_answers)} facts have answers_preferences (self-describing)")
        else:
            self.check("WARN", "search_answers_preferences",
                        "No facts with answers_preferences — relies on legacy fallback scoring")

        if facts_with_scoring:
            self.check("PASS", "search_scoring_hints",
                        f"{len(facts_with_scoring)} facts have scoring_hint")
        else:
            self.check("WARN", "search_scoring_hints",
                        "No facts with scoring_hint — all scoring uses legacy fallback")


def run_validation():
    """Run Phase 0 validation on the selected properties."""
    # Load validation set
    val_set_path = ROOT / "pipeline" / "validation" / "validation_set.json"
    prop_ids = load_json(val_set_path)
    if not prop_ids:
        print("FATAL: Could not load pipeline/validation/validation_set.json")
        return 1

    # Load data
    properties = load_json(DATA / "seed" / "properties.json") or []
    societies = load_json(DATA / "seed" / "societies.json") or []
    sellers = load_json(DATA / "sellers" / "sellers.json") or []

    print(f"Phase 0 Validation — {len(prop_ids)} properties")
    print(f"Data loaded: {len(properties)} properties, {len(societies)} societies, {len(sellers)} sellers")
    print("=" * 80)

    all_results = {}
    total_pass = 0
    total_warn = 0
    total_fail = 0

    for prop_id in prop_ids:
        validator = PropertyValidator(prop_id, properties, societies, sellers)
        results = validator.run_all()
        all_results[prop_id] = results

        passes = sum(1 for r in results if r["status"] == "PASS")
        warns = sum(1 for r in results if r["status"] == "WARN")
        fails = sum(1 for r in results if r["status"] == "FAIL")
        total_pass += passes
        total_warn += warns
        total_fail += fails

        # Property header
        prop = {p["id"]: p for p in properties}.get(prop_id, {})
        area = prop.get("area", "?")
        builder = prop.get("builder_name", "?")
        print(f"\n{prop_id}")
        print(f"  Area: {area} | Builder: {builder}")
        print(f"  Results: {passes} PASS, {warns} WARN, {fails} FAIL")

        for r in results:
            icon = {"PASS": "+", "WARN": "~", "FAIL": "!"}[r["status"]]
            print(f"    [{icon}] {r['status']:4s} {r['check']}: {r['message']}")

    # Summary
    print("\n" + "=" * 80)
    print(f"TOTAL: {total_pass} PASS, {total_warn} WARN, {total_fail} FAIL")
    print(f"Properties validated: {len(prop_ids)}")

    # Gap analysis: most common warns/fails
    gap_counter = Counter()
    for prop_id, results in all_results.items():
        for r in results:
            if r["status"] in ("WARN", "FAIL"):
                gap_counter[f"[{r['status']}] {r['check']}"] += 1

    if gap_counter:
        print("\nTop gaps (by frequency):")
        for gap, count in gap_counter.most_common(15):
            print(f"  {count}/{len(prop_ids)} — {gap}")

    # Write JSON report
    output_dir = DATA / "validation"
    output_dir.mkdir(parents=True, exist_ok=True)
    report = {
        "timestamp": datetime.utcnow().isoformat() + "Z",
        "validation_set": prop_ids,
        "summary": {
            "total_properties": len(prop_ids),
            "total_checks": total_pass + total_warn + total_fail,
            "pass": total_pass,
            "warn": total_warn,
            "fail": total_fail,
        },
        "gaps": [
            {"gap": gap, "count": count}
            for gap, count in gap_counter.most_common()
        ],
        "results": all_results,
    }

    report_path = output_dir / "phase0_baseline.json"
    with open(report_path, "w") as f:
        json.dump(report, f, indent=2)
    print(f"\nJSON report written to {report_path.relative_to(ROOT)}")

    return 1 if total_fail > 0 else 0


if __name__ == "__main__":
    sys.exit(run_validation())
