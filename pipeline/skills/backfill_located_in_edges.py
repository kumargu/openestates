"""
backfill_located_in_edges — pure computation skill (no LLM, zero cost).

For each society that has an area fact but no SocietyInArea edge in edges.json,
match the area name to existing area nodes and create the missing edge.

Usage: python3 -m pipeline.skills.backfill_located_in_edges
"""

import json
import os
import re
import sys
from pathlib import Path
from typing import Dict, List, Optional, Tuple

KNOWLEDGE_DIR = Path("data/knowledge/nodes")
SOCIETY_DIR = KNOWLEDGE_DIR / "society"
AREA_DIR = KNOWLEDGE_DIR / "area"
EDGES_FILE = Path("data/knowledge/edges.json")


def slugify(name: str) -> str:
    """Slugify a name: lowercase, non-alphanum to dashes, collapse multiples, strip.

    Normalizes em-dashes (U+2014) and en-dashes (U+2013) to regular hyphens
    before the regex so that e.g. 'Marathahalli\u2013Whitefield Road' produces
    'marathahalli-whitefield-road' (matching the existing node filename).
    """
    normalized = name.replace('\u2014', '-').replace('\u2013', '-')
    slug = re.sub(r'[^a-z0-9]+', '-', normalized.lower()).strip('-')
    return re.sub(r'-+', '-', slug)


def tokenize(name: str) -> set:
    """Tokenize a name: lowercase, split on non-alpha."""
    return set(re.findall(r'[a-z0-9]+', name.lower()))


def jaccard_similarity(a: set, b: set) -> float:
    """Jaccard token similarity: |intersection| / |union|."""
    if not a or not b:
        return 0.0
    return len(a & b) / len(a | b)


def get_fact_value(node: dict, key: str) -> Optional[str]:
    """Get the latest fact value for a given key (highest version)."""
    best = None
    best_version = -1
    for fact in node.get("facts", []):
        if fact["key"] == key:
            v = fact.get("version", 1)
            if v > best_version:
                best_version = v
                best = fact.get("value", {}).get("data")
    return best


def load_edges() -> list:
    """Load existing edges."""
    if EDGES_FILE.exists():
        return json.loads(EDGES_FILE.read_text())
    return []


def save_edges(edges: list):
    """Save edges atomically."""
    tmp = EDGES_FILE.with_suffix(".json.tmp")
    tmp.write_text(json.dumps(edges, indent=2, ensure_ascii=False))
    os.rename(tmp, EDGES_FILE)


def edge_exists(edges: list, from_id: str, to_id: str, relation: str) -> bool:
    """Check if an edge already exists."""
    for e in edges:
        if e["from"] == from_id and e["to"] == to_id and e["relation"] == relation:
            return True
    return False


def build_area_index() -> Dict[str, Tuple[str, str]]:
    """Build a mapping from area slug to (node_id, name) for all area nodes."""
    index: Dict[str, Tuple[str, str]] = {}
    if not AREA_DIR.exists():
        return index
    for f in AREA_DIR.glob("*.json"):
        try:
            data = json.loads(f.read_text())
            node_id = data.get("id", f"area:{f.stem}")
            name = data.get("name", f.stem)
            index[f.stem] = (node_id, name)
        except (json.JSONDecodeError, OSError):
            pass
    return index


def fuzzy_match_area(
    area_name: str,
    area_index: Dict[str, Tuple[str, str]],
    threshold: float = 0.5,
) -> Optional[Tuple[str, str, float]]:
    """Find the best matching area node for a given area name.

    First tries direct slug match, then falls back to Jaccard fuzzy match.
    Returns (area_node_id, area_name, score) or None.
    """
    # Direct slug match
    slug = slugify(area_name)
    if slug in area_index:
        node_id, name = area_index[slug]
        return (node_id, name, 1.0)

    # Fuzzy match: Jaccard on tokens
    query_tokens = tokenize(area_name)
    if not query_tokens:
        return None

    best_match = None
    best_score = 0.0

    for area_slug, (node_id, name) in area_index.items():
        candidate_tokens = tokenize(name)
        score = jaccard_similarity(query_tokens, candidate_tokens)
        if score > best_score:
            best_score = score
            best_match = (node_id, name, score)

    if best_match and best_match[2] >= threshold:
        return best_match
    return None


def run():
    """Backfill missing SocietyInArea edges by matching society area facts to area nodes."""
    print("=== Backfill SocietyInArea Edges ===\n")

    # Load area index
    area_index = build_area_index()
    print(f"  {len(area_index)} area nodes loaded\n")

    if not area_index:
        print("No area nodes found. Nothing to do.")
        return

    # Load edges and find societies that already have SocietyInArea edges
    edges = load_edges()
    existing_located_in: set = set()
    for e in edges:
        if e.get("relation") == "SocietyInArea":
            existing_located_in.add(e["from"])

    print(f"  {len(existing_located_in)} societies already have SocietyInArea edges\n")

    # Process each society
    new_edges = 0
    unmatched = 0
    already_has = 0
    no_area_fact = 0

    print(f"{'Society':<50} {'Area Fact':<30} {'Matched To':<30} {'Score':>6} {'Action'}")
    print("-" * 140)

    for node_file in sorted(SOCIETY_DIR.glob("*.json")):
        try:
            data = json.loads(node_file.read_text())
        except (json.JSONDecodeError, OSError) as e:
            print(f"  SKIP {node_file.name}: {e}", file=sys.stderr)
            continue

        society_id = data.get("id", f"society:{node_file.stem}")
        society_name = data.get("name", node_file.stem)

        # Skip if already has SocietyInArea edge
        if society_id in existing_located_in:
            already_has += 1
            continue

        # Get area fact
        area_name = get_fact_value(data, "area")
        if not area_name:
            # Also check node-level fields
            area_name = data.get("area")
        if not area_name:
            no_area_fact += 1
            continue

        # Match to area nodes
        match = fuzzy_match_area(area_name, area_index)

        if not match:
            print(f"  {society_name:<50} {area_name:<30} {'NO MATCH':<30} {'':>6} SKIP")
            unmatched += 1
            continue

        area_node_id, matched_name, score = match

        # Add edge
        if not edge_exists(edges, society_id, area_node_id, "SocietyInArea"):
            edges.append({
                "from": society_id,
                "to": area_node_id,
                "relation": "SocietyInArea",
                "weight": 1.0,
                "source": {
                    "source_type": "Computed",
                    "skill_id": "backfill_located_in_edges",
                },
            })
            new_edges += 1
            tag = "DIRECT" if score == 1.0 else "FUZZY"
            print(f"  {society_name:<50} {area_name:<30} {matched_name:<30} {score:>6.2f} ADDED ({tag})")
        else:
            already_has += 1

    # Save edges
    if new_edges > 0:
        save_edges(edges)
        print(f"\n  Edges saved ({len(edges)} total)")

    print(f"\nBackfill complete:")
    print(f"  New SocietyInArea edges created: {new_edges}")
    print(f"  Already had edges:               {already_has}")
    print(f"  No area fact:                    {no_area_fact}")
    print(f"  Unmatched societies:             {unmatched}")

    # Final count
    total_sia = sum(1 for e in edges if e["relation"] == "SocietyInArea")
    print(f"\n  Total SocietyInArea edges now:   {total_sia}")


def test():
    """Run unit tests for slugify and fuzzy_match_area."""
    passed = 0
    failed = 0

    def assert_eq(actual, expected, label):
        nonlocal passed, failed
        if actual == expected:
            passed += 1
            print(f"  PASS: {label}")
        else:
            failed += 1
            print(f"  FAIL: {label} — expected {expected!r}, got {actual!r}")

    def assert_true(val, label):
        nonlocal passed, failed
        if val:
            passed += 1
            print(f"  PASS: {label}")
        else:
            failed += 1
            print(f"  FAIL: {label} — expected truthy, got {val!r}")

    print("=== Tests: slugify ===\n")

    # Basic slugification
    assert_eq(slugify("Sarjapur Road"), "sarjapur-road", "basic slugify")

    # Em-dash normalization (U+2014)
    assert_eq(slugify("Marathahalli\u2014Whitefield Road"), "marathahalli-whitefield-road",
              "em-dash (U+2014) normalized to hyphen")

    # En-dash normalization (U+2013)
    assert_eq(slugify("Marathahalli\u2013Whitefield Road"), "marathahalli-whitefield-road",
              "en-dash (U+2013) normalized to hyphen")

    # Mixed special characters
    assert_eq(slugify("HSR Layout (Sector 2)"), "hsr-layout-sector-2",
              "parens and spaces normalized")

    # Already a slug
    assert_eq(slugify("hsr-layout"), "hsr-layout", "already a slug passes through")

    print("\n=== Tests: fuzzy_match_area ===\n")

    # Build a test area index
    area_index = {
        "whitefield": ("area:whitefield", "Whitefield"),
        "sarjapur-road": ("area:sarjapur-road", "Sarjapur Road"),
        "hsr-layout": ("area:hsr-layout", "HSR Layout"),
        "marathahalli-whitefield-road": ("area:marathahalli-whitefield-road", "Marathahalli-Whitefield Road"),
    }

    # Direct slug match
    result = fuzzy_match_area("Whitefield", area_index)
    assert_true(result is not None, "direct match finds Whitefield")
    assert_eq(result[0], "area:whitefield", "direct match returns correct node_id")
    assert_eq(result[2], 1.0, "direct match score is 1.0")

    # Em-dash slug match
    result2 = fuzzy_match_area("Marathahalli\u2014Whitefield Road", area_index)
    assert_true(result2 is not None, "em-dash area matches via slug normalization")
    assert_eq(result2[0], "area:marathahalli-whitefield-road", "em-dash match returns correct node_id")

    # Non-matching input
    result3 = fuzzy_match_area("Indiranagar", area_index)
    assert_true(result3 is None, "non-matching area returns None")

    # Fuzzy match via Jaccard
    result4 = fuzzy_match_area("Sarjapur Main Road", area_index, threshold=0.4)
    assert_true(result4 is not None, "fuzzy match finds Sarjapur Road for 'Sarjapur Main Road'")
    assert_eq(result4[0], "area:sarjapur-road", "fuzzy match returns correct node_id")
    assert_true(result4[2] < 1.0, "fuzzy match score is less than 1.0")

    print(f"\n=== Results: {passed} passed, {failed} failed ===")
    return failed == 0


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--test":
        success = test()
        sys.exit(0 if success else 1)
    run()
