"""
Enrichment validation — checks that the knowledge graph has real, complete data.

Usage:
    python3 -m pipeline.test_enrichment

Checks:
  1. All entities exist on disk
  2. Minimum fact counts per entity type
  3. Required facts present per type (e.g., areas must have metro_status)
  4. No empty/null fact values
  5. Embeddings present and correct dimensionality
  6. Source provenance is valid
  7. Fact confidence within expected ranges
  8. Edges are consistent (both endpoints exist)
"""

import json
import sys
from pathlib import Path
from typing import Optional

PROJECT_ROOT = Path(__file__).parent.parent
GRAPH_DIR = PROJECT_ROOT / "data" / "knowledge" / "nodes"
EDGES_PATH = PROJECT_ROOT / "data" / "knowledge" / "edges.json"

# Expected embedding dimensionality
EXPECTED_EMBEDDING_DIMS = 3072

# Minimum facts per entity type (bootstrap + enrichment)
MIN_FACTS = {
    "area": 15,       # 13 bootstrap + at least 2 enrichment
    "society": 15,     # 15 bootstrap base
    "property": 14,    # 16 bootstrap
    "builder": 0,      # builders may have 0 facts
}

# Required facts per type (must be present after enrichment)
REQUIRED_FACTS = {
    "area": [
        # Bootstrap facts
        "city", "median_price_per_sqft",
        # Area intelligence facts
        "metro_status", "traffic_reality", "waterlogging_risk",
        "price_trend", "livability_summary",
    ],
    "society": [
        # Bootstrap facts
        "area", "city", "builder_name", "summary",
    ],
    "property": [
        # Bootstrap facts
        "area", "bhk", "price", "carpet_area_sqft",
    ],
}

# Facts that SHOULD be present (warn if missing, don't fail)
EXPECTED_FACTS = {
    "area": ["school_quality", "area_vibe", "upcoming_infra"],
    "society": ["google_rating", "google_sentiment"],
}

VALID_SOURCE_TYPES = {
    "Manual",
    "Reddit",
    "Google",
    "Rera",
    "Bbmp",
    "News",
    "Computed",
    "Llm",  # Legacy KG compatibility only; active enrichment should not emit this.
}
VALID_VALUE_TYPES = {"Text", "Numeric", "Bool", "Tags", "Score"}


class TestResult:
    def __init__(self, name: str):
        self.name = name
        self.passed = 0
        self.failed = 0
        self.warned = 0
        self.messages: list[str] = []

    def ok(self, msg: str = ""):
        self.passed += 1

    def fail(self, msg: str):
        self.failed += 1
        self.messages.append(f"  FAIL: {msg}")

    def warn(self, msg: str):
        self.warned += 1
        self.messages.append(f"  WARN: {msg}")

    @property
    def status(self) -> str:
        if self.failed > 0:
            return "FAIL"
        elif self.warned > 0:
            return "WARN"
        return "PASS"

    def print(self):
        symbol = {"PASS": "✓", "WARN": "⚠", "FAIL": "✗"}[self.status]
        print(f"\n{symbol} {self.name} [{self.status}] — {self.passed} passed, {self.failed} failed, {self.warned} warnings")
        for msg in self.messages:
            print(msg)


def load_all_nodes() -> dict[str, list[dict]]:
    """Load all nodes grouped by type."""
    result = {}
    if not GRAPH_DIR.exists():
        return result
    for type_dir in sorted(GRAPH_DIR.iterdir()):
        if not type_dir.is_dir():
            continue
        ntype = type_dir.name
        nodes = []
        for path in sorted(type_dir.glob("*.json")):
            with open(path) as f:
                nodes.append(json.load(f))
        result[ntype] = nodes
    return result


def load_edges() -> list[dict]:
    if not EDGES_PATH.exists():
        return []
    with open(EDGES_PATH) as f:
        return json.load(f)


# ---------------------------------------------------------------------------
# Test functions
# ---------------------------------------------------------------------------

def test_entity_counts(nodes_by_type: dict) -> TestResult:
    """Check expected entity counts."""
    t = TestResult("Entity counts")

    expected = {"area": 5, "society": 12, "property": 20, "builder": 6}
    for ntype, expected_count in expected.items():
        actual = len(nodes_by_type.get(ntype, []))
        if actual == expected_count:
            t.ok()
        elif actual > 0:
            t.warn(f"{ntype}: expected {expected_count}, got {actual}")
        else:
            t.fail(f"{ntype}: expected {expected_count}, got 0")

    return t


def test_minimum_facts(nodes_by_type: dict) -> TestResult:
    """Check minimum fact counts per entity type."""
    t = TestResult("Minimum fact counts")

    for ntype, min_count in MIN_FACTS.items():
        for node in nodes_by_type.get(ntype, []):
            fact_count = len(node.get("facts", []))
            name = node.get("name", node.get("id", "?"))
            if fact_count >= min_count:
                t.ok()
            else:
                t.fail(f"{name} ({ntype}): {fact_count} facts < minimum {min_count}")

    return t


def test_required_facts(nodes_by_type: dict) -> TestResult:
    """Check that required facts are present."""
    t = TestResult("Required facts")

    for ntype, required_keys in REQUIRED_FACTS.items():
        for node in nodes_by_type.get(ntype, []):
            node_facts = {f["key"] for f in node.get("facts", [])}
            name = node.get("name", node.get("id", "?"))
            for key in required_keys:
                if key in node_facts:
                    t.ok()
                else:
                    t.fail(f"{name} ({ntype}): missing required fact '{key}'")

    return t


def test_expected_facts(nodes_by_type: dict) -> TestResult:
    """Check that expected enrichment facts are present (warn-only)."""
    t = TestResult("Expected enrichment facts")

    for ntype, expected_keys in EXPECTED_FACTS.items():
        for node in nodes_by_type.get(ntype, []):
            node_facts = {f["key"] for f in node.get("facts", [])}
            name = node.get("name", node.get("id", "?"))
            for key in expected_keys:
                if key in node_facts:
                    t.ok()
                else:
                    t.warn(f"{name} ({ntype}): missing expected fact '{key}'")

    return t


def test_fact_values(nodes_by_type: dict) -> TestResult:
    """Check that fact values are not empty/null and have valid structure."""
    t = TestResult("Fact value integrity")

    for ntype, nodes in nodes_by_type.items():
        for node in nodes:
            name = node.get("name", node.get("id", "?"))
            for fact in node.get("facts", []):
                key = fact.get("key", "")
                value = fact.get("value")

                # Must have a value
                if value is None:
                    t.fail(f"{name}.{key}: value is null")
                    continue

                # Value must be a dict with type and data
                if not isinstance(value, dict):
                    t.fail(f"{name}.{key}: value is not a dict: {type(value)}")
                    continue

                vtype = value.get("type")
                data = value.get("data")

                if vtype not in VALID_VALUE_TYPES:
                    t.fail(f"{name}.{key}: unknown value type '{vtype}'")
                elif data is None:
                    t.fail(f"{name}.{key}: data is null")
                elif vtype == "Text" and not str(data).strip():
                    t.warn(f"{name}.{key}: empty text value")
                elif vtype == "Tags" and (not isinstance(data, list) or len(data) == 0):
                    t.warn(f"{name}.{key}: empty tags list")
                else:
                    t.ok()

    return t


def test_source_provenance(nodes_by_type: dict) -> TestResult:
    """Check that all facts have valid source provenance."""
    t = TestResult("Source provenance")

    for ntype, nodes in nodes_by_type.items():
        for node in nodes:
            name = node.get("name", node.get("id", "?"))
            for fact in node.get("facts", []):
                key = fact.get("key", "")
                source = fact.get("source")

                if not source:
                    t.fail(f"{name}.{key}: no source")
                    continue

                source_type = source.get("source_type", "")
                if source_type not in VALID_SOURCE_TYPES:
                    t.fail(f"{name}.{key}: unknown source_type '{source_type}'")
                else:
                    t.ok()

                # Check confidence is reasonable
                confidence = fact.get("confidence", -1)
                if not (0.0 <= confidence <= 1.0):
                    t.fail(f"{name}.{key}: confidence {confidence} out of range [0,1]")
                else:
                    t.ok()

                # Check learned_at is present
                if not fact.get("learned_at"):
                    t.warn(f"{name}.{key}: no learned_at timestamp")
                else:
                    t.ok()

    return t


def test_embeddings(nodes_by_type: dict) -> TestResult:
    """Check embedding presence and dimensionality."""
    t = TestResult("Embeddings")

    # Societies and areas should have embeddings
    for ntype in ["society", "area"]:
        for node in nodes_by_type.get(ntype, []):
            name = node.get("name", node.get("id", "?"))
            emb = node.get("summary_embedding")

            if emb is None:
                t.fail(f"{name} ({ntype}): no embedding")
            elif not isinstance(emb, list):
                t.fail(f"{name} ({ntype}): embedding is not a list")
            elif len(emb) != EXPECTED_EMBEDDING_DIMS:
                t.fail(f"{name} ({ntype}): embedding dims {len(emb)} != expected {EXPECTED_EMBEDDING_DIMS}")
            elif not all(isinstance(v, (int, float)) for v in emb[:10]):
                t.fail(f"{name} ({ntype}): embedding contains non-numeric values")
            else:
                t.ok()

    # Properties MAY have embeddings (warn if missing)
    for node in nodes_by_type.get("property", []):
        name = node.get("name", node.get("id", "?"))
        emb = node.get("summary_embedding")
        if emb and len(emb) == EXPECTED_EMBEDDING_DIMS:
            t.ok()
        elif emb:
            t.fail(f"{name} (property): embedding dims {len(emb)} != {EXPECTED_EMBEDDING_DIMS}")
        else:
            t.warn(f"{name} (property): no embedding")

    return t


def test_edges(nodes_by_type: dict) -> TestResult:
    """Check edge consistency — both endpoints must exist."""
    t = TestResult("Edge consistency")

    all_ids = set()
    for nodes in nodes_by_type.values():
        for node in nodes:
            all_ids.add(node.get("id", ""))

    edges = load_edges()
    if not edges:
        t.warn("No edges found")
        return t

    valid_relations = {
        "SocietyInArea", "PropertyInSociety", "BuiltBy",
        "NearMetro", "NearSchool", "HasSignal",
    }

    for edge in edges:
        from_id = edge.get("from", "")
        to_id = edge.get("to", "")
        relation = edge.get("relation", "")

        if from_id not in all_ids:
            t.fail(f"Edge from '{from_id}' — node does not exist")
        elif to_id not in all_ids:
            t.fail(f"Edge to '{to_id}' — node does not exist")
        elif relation not in valid_relations:
            t.warn(f"Edge {from_id} -> {to_id}: unknown relation '{relation}'")
        else:
            t.ok()

    return t


def test_multi_source_enrichment(nodes_by_type: dict) -> TestResult:
    """Check that entities have facts from multiple sources (the whole point of enrichment)."""
    t = TestResult("Multi-source enrichment")

    for ntype in ["area", "society"]:
        for node in nodes_by_type.get(ntype, []):
            name = node.get("name", node.get("id", "?"))
            sources = set()
            for fact in node.get("facts", []):
                src = fact.get("source", {}).get("source_type", "")
                if src:
                    sources.add(src)

            if len(sources) >= 2:
                t.ok()
            else:
                t.warn(f"{name} ({ntype}): only {len(sources)} source(s): {sources}")

    return t


def test_no_duplicate_facts(nodes_by_type: dict) -> TestResult:
    """Check that no node has duplicate fact keys."""
    t = TestResult("No duplicate fact keys")

    for ntype, nodes in nodes_by_type.items():
        for node in nodes:
            name = node.get("name", node.get("id", "?"))
            keys = [f["key"] for f in node.get("facts", [])]
            seen = set()
            for key in keys:
                if key in seen:
                    t.fail(f"{name} ({ntype}): duplicate fact key '{key}'")
                else:
                    seen.add(key)
                    t.ok()

    return t


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    if not GRAPH_DIR.exists():
        print("ERROR: Knowledge graph not found at", GRAPH_DIR)
        print("Run the backend once to trigger migration, then run enrichment.")
        sys.exit(1)

    print("=" * 70)
    print("KNOWLEDGE GRAPH ENRICHMENT VALIDATION")
    print("=" * 70)

    nodes_by_type = load_all_nodes()

    total_nodes = sum(len(v) for v in nodes_by_type.values())
    total_facts = sum(
        len(n.get("facts", []))
        for nodes in nodes_by_type.values()
        for n in nodes
    )
    total_embeddings = sum(
        1
        for nodes in nodes_by_type.values()
        for n in nodes
        if n.get("summary_embedding")
    )
    print(f"\nGraph: {total_nodes} nodes, {total_facts} facts, {total_embeddings} embeddings\n")

    tests = [
        test_entity_counts(nodes_by_type),
        test_minimum_facts(nodes_by_type),
        test_required_facts(nodes_by_type),
        test_expected_facts(nodes_by_type),
        test_fact_values(nodes_by_type),
        test_source_provenance(nodes_by_type),
        test_embeddings(nodes_by_type),
        test_edges(nodes_by_type),
        test_multi_source_enrichment(nodes_by_type),
        test_no_duplicate_facts(nodes_by_type),
    ]

    for test in tests:
        test.print()

    # Summary
    total_passed = sum(t.passed for t in tests)
    total_failed = sum(t.failed for t in tests)
    total_warned = sum(t.warned for t in tests)
    all_pass = all(t.failed == 0 for t in tests)

    print("\n" + "=" * 70)
    if all_pass:
        print(f"ALL TESTS PASSED — {total_passed} checks ok, {total_warned} warnings")
    else:
        print(f"SOME TESTS FAILED — {total_passed} passed, {total_failed} failed, {total_warned} warnings")
    print("=" * 70)

    sys.exit(0 if all_pass else 1)


if __name__ == "__main__":
    main()
