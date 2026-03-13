"""
seed_from_rera — fuzzy-match societies against RERA listing and backfill/seed.

Two modes:
  --backfill: Enrich existing 32 societies missing RERA date facts
  --seed:     Create new society nodes from RERA registry

Imports scraping infra from fetch_rera.py — no duplication.

Usage:
  python3 -m pipeline.skills.seed_from_rera --backfill
  python3 -m pipeline.skills.seed_from_rera --seed --area "Whitefield" --limit 10
"""

import argparse
import json
import logging
import os
import re
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, List, Optional, Tuple

from pipeline.skills.base import FactSource, SourcedFact
from pipeline.skills.fetch_rera import (
    ReraListingEntry,
    ReraSession,
    fetch_rera_detail,
    parse_rera_detail,
    rera_detail_to_facts,
    scrape_rera_listing,
    search_rera_project,
)

logger = logging.getLogger(__name__)

KNOWLEDGE_DIR = Path("data/knowledge/nodes")
SOCIETY_DIR = KNOWLEDGE_DIR / "society"
BUILDER_DIR = KNOWLEDGE_DIR / "builder"
AREA_DIR = KNOWLEDGE_DIR / "area"
EDGES_FILE = Path("data/knowledge/edges.json")

# Words to strip from names before tokenizing for fuzzy match
NOISE_WORDS = frozenset({
    "phase", "tower", "towers", "pvt", "ltd", "private", "limited",
    "group", "properties", "developers", "constructions", "realty",
    "real", "estate", "holdings", "infracon", "infra", "infrastructures",
    "corporation", "corp", "builder", "builders",
    "residential", "apartments", "apartment", "flats", "flat",
    "block", "wing", "plot", "plots", "layout",
    "the", "of", "at", "in", "by", "and", "&", "-", "–",
    "i", "ii", "iii", "iv", "v", "vi", "vii", "viii", "ix", "x",
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "10",
    "a", "b", "c", "d", "e",
})


# ---------------------------------------------------------------------------
# Fuzzy matching
# ---------------------------------------------------------------------------

def tokenize(name: str) -> set:
    """Tokenize a name: lowercase, split on non-alpha, remove noise."""
    tokens = set(re.findall(r'[a-z0-9]+', name.lower()))
    return tokens - NOISE_WORDS


def jaccard_similarity(a: set, b: set) -> float:
    """Jaccard token similarity: |intersection| / |union|."""
    if not a or not b:
        return 0.0
    intersection = a & b
    union = a | b
    return len(intersection) / len(union)


def fuzzy_match_society(
    society_name: str,
    listing: List[ReraListingEntry],
    threshold: float = 0.5,
) -> List[Tuple[ReraListingEntry, float]]:
    """Find RERA listing entries matching a society name above threshold.

    Returns list of (entry, score) sorted by score descending.
    """
    society_tokens = tokenize(society_name)
    if not society_tokens:
        return []

    matches = []
    for entry in listing:
        entry_tokens = tokenize(entry.project_name)
        score = jaccard_similarity(society_tokens, entry_tokens)
        if score >= threshold:
            matches.append((entry, score))

    # Sort by score descending
    matches.sort(key=lambda x: x[1], reverse=True)
    return matches


# ---------------------------------------------------------------------------
# Node helpers
# ---------------------------------------------------------------------------

def get_fact_value(node: dict, key: str) -> Optional[str]:
    """Get the latest fact value for a given key."""
    best = None
    best_version = -1
    for fact in node.get("facts", []):
        if fact["key"] == key:
            v = fact.get("version", 1)
            if v > best_version:
                best_version = v
                best = fact.get("value", {}).get("data")
    return best


def has_rera_dates(node: dict) -> bool:
    """Check if a node already has RERA date facts."""
    for fact in node.get("facts", []):
        if fact["key"] in ("rera_completion_date", "rera_start_date"):
            if fact.get("source", {}).get("source_type") == "Rera":
                return True
    return False


def remove_stale_rera_facts(node: dict) -> dict:
    """Remove existing rera_ facts from non-Rera sources (e.g., guessed by LLM).

    Keeps facts from source_type=Rera (real scrapes) and non-rera keys.
    """
    new_facts = []
    for fact in node.get("facts", []):
        if fact["key"].startswith("rera_"):
            source_type = fact.get("source", {}).get("source_type", "")
            if source_type == "Rera":
                new_facts.append(fact)
            # else: skip stale rera facts from non-Rera sources
        else:
            new_facts.append(fact)
    node["facts"] = new_facts
    return node


def atomic_write_json(path: Path, data: dict):
    """Write JSON atomically via .tmp + rename."""
    tmp_path = path.with_suffix(".json.tmp")
    tmp_path.write_text(json.dumps(data, indent=2, ensure_ascii=False))
    os.rename(tmp_path, path)


def slugify(name: str) -> str:
    """Convert a name to a URL-friendly slug."""
    slug = re.sub(r'[^a-z0-9]+', '-', name.lower()).strip('-')
    # Collapse multiple dashes
    slug = re.sub(r'-+', '-', slug)
    return slug


# ---------------------------------------------------------------------------
# Edge management
# ---------------------------------------------------------------------------

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


def add_edge(edges: list, from_id: str, to_id: str, relation: str) -> list:
    """Add an edge if it doesn't already exist."""
    if not edge_exists(edges, from_id, to_id, relation):
        edges.append({
            "from": from_id,
            "to": to_id,
            "relation": relation,
            "weight": 1.0,
            "source": {
                "source_type": "Rera",
                "url": "https://rera.karnataka.gov.in/projectViewDetails",
                "skill_id": "seed_from_rera",
            },
        })
    return edges


# ---------------------------------------------------------------------------
# Backfill mode
# ---------------------------------------------------------------------------

def run_backfill(dry_run: bool = False):
    """Backfill RERA facts for societies missing RERA dates.

    For each society without RERA date facts:
    1. Fuzzy-match against RERA listing
    2. Fetch detail page via RERA portal search (cached)
    3. Convert to SourcedFacts
    4. Write facts to society JSON
    5. Update root_source to "Rera"
    """
    print("=== RERA Backfill Mode ===\n")

    # Load RERA listing (cached ~7 days)
    print("Loading RERA listing...")
    listing = scrape_rera_listing()
    print(f"  {len(listing)} RERA projects loaded\n")

    if not listing:
        print("ERROR: No RERA listing available. Cannot proceed.", file=sys.stderr)
        return

    session = ReraSession()

    matched = 0
    failed = 0
    skipped = 0
    total_facts_added = 0

    print(f"{'Society':<45} {'RERA Match':<50} {'Score':>6} {'Action'}")
    print("-" * 120)

    for node_file in sorted(SOCIETY_DIR.glob("*.json")):
        try:
            data = json.loads(node_file.read_text())
        except (json.JSONDecodeError, OSError) as e:
            print(f"  SKIP {node_file.name}: {e}", file=sys.stderr)
            continue

        society_name = data.get("name", node_file.stem)

        # Skip if already has RERA dates
        if has_rera_dates(data):
            skipped += 1
            continue

        # Step 1: Fuzzy match against listing
        matches = fuzzy_match_society(society_name, listing, threshold=0.4)

        if not matches:
            print(f"  {society_name:<45} {'NO MATCH':<50} {'':>6} SKIP")
            failed += 1
            continue

        best_entry, best_score = matches[0]

        # Log all candidates above threshold
        if len(matches) > 1:
            for entry, score in matches[:3]:
                marker = " <--" if entry == best_entry else ""
                logger.info(
                    "  Candidate: %s (%.2f)%s",
                    entry.project_name, score, marker,
                )

        if best_score < 0.5:
            # Below confidence threshold — log but skip
            print(f"  {society_name:<45} {best_entry.project_name:<50} {best_score:>6.2f} LOW-SCORE")
            failed += 1
            continue

        # Step 2: Search RERA portal for this project to get numeric ID + details
        try:
            search_result = search_rera_project(session, best_entry.project_name)
        except Exception as e:
            print(f"  {society_name:<45} {best_entry.project_name:<50} {best_score:>6.2f} SEARCH-ERROR: {e}")
            failed += 1
            continue

        if not search_result:
            print(f"  {society_name:<45} {best_entry.project_name:<50} {best_score:>6.2f} NOT-FOUND-ON-PORTAL")
            failed += 1
            continue

        # Step 3: Fetch detail page
        try:
            detail_html = fetch_rera_detail(session, search_result.numeric_id)
        except Exception as e:
            print(f"  {society_name:<45} {best_entry.project_name:<50} {best_score:>6.2f} DETAIL-ERROR: {e}")
            failed += 1
            continue

        # Step 4: Parse and convert to facts
        detail = parse_rera_detail(detail_html, search_result)
        facts = rera_detail_to_facts(detail)

        if not facts:
            print(f"  {society_name:<45} {best_entry.project_name:<50} {best_score:>6.2f} NO-FACTS")
            failed += 1
            continue

        if dry_run:
            print(f"  {society_name:<45} {best_entry.project_name:<50} {best_score:>6.2f} WOULD-ADD ({len(facts)} facts)")
            matched += 1
            total_facts_added += len(facts)
            continue

        # Step 5: Remove stale rera facts, add new ones
        data = remove_stale_rera_facts(data)
        for fact in facts:
            data.setdefault("facts", []).append(fact.to_dict())

        # Step 6: Update root_source
        data["root_source"] = "Rera"

        # Step 7: Atomic write
        atomic_write_json(node_file, data)

        matched += 1
        total_facts_added += len(facts)
        print(f"  {society_name:<45} {best_entry.project_name:<50} {best_score:>6.2f} ENRICHED ({len(facts)} facts)")

    print()
    print(f"Backfill complete:")
    print(f"  Matched & enriched: {matched}")
    print(f"  Failed/no match:    {failed}")
    print(f"  Skipped (has RERA): {skipped}")
    print(f"  Total facts added:  {total_facts_added}")


# ---------------------------------------------------------------------------
# Seed mode
# ---------------------------------------------------------------------------

def _infer_area_from_address(address: str) -> Optional[str]:
    """Try to infer area name from RERA project address."""
    # Common Bangalore area names to match against
    known_areas = [
        "Whitefield", "Sarjapur", "Marathahalli", "Bellandur", "HSR Layout",
        "Koramangala", "Hebbal", "Thanisandra", "Hoodi", "Panathur",
        "Varthur", "Haralur", "Electronic City", "Yelahanka", "Devanahalli",
        "Kanakapura", "Bannerghatta", "JP Nagar", "Jayanagar", "Rajajinagar",
        "Malleshwaram", "Indiranagar", "Domlur", "Ulsoor", "KR Puram",
        "Mahadevapura", "Kadugodi", "Budigere", "Bagalur", "Hennur",
        "Brookefield", "Kundalahalli", "Harlur",
    ]
    address_lower = address.lower()
    for area in known_areas:
        if area.lower() in address_lower:
            return area
    return None


def run_seed(area_filter: str, limit: int = 10, dry_run: bool = False):
    """Seed new society nodes from RERA registry for a given area.

    For RERA projects matching the area filter that aren't already in the KG:
    1. Create new society node
    2. Create/update builder node
    3. Add RERA facts
    4. Create edges
    """
    print(f"=== RERA Seed Mode: area={area_filter}, limit={limit} ===\n")

    # Load existing society slugs for dedup
    existing_slugs = set()
    for f in SOCIETY_DIR.glob("*.json"):
        existing_slugs.add(f.stem)

    # Also build a set of existing society names (lowercased) for name-level dedup
    existing_names_lower = set()
    for f in SOCIETY_DIR.glob("*.json"):
        try:
            data = json.loads(f.read_text())
            existing_names_lower.add(data.get("name", "").lower())
        except (json.JSONDecodeError, OSError):
            pass

    # Load RERA listing
    print("Loading RERA listing...")
    listing = scrape_rera_listing()
    print(f"  {len(listing)} RERA projects loaded\n")

    if not listing:
        print("ERROR: No RERA listing available.", file=sys.stderr)
        return

    # Filter listing by area (search in project_name for area substring)
    area_lower = area_filter.lower()
    candidates = []
    for entry in listing:
        name_lower = entry.project_name.lower()
        promoter_lower = entry.promoter_name.lower()
        # Check if the area name appears in the project name or promoter
        # We'll also check the detail page address later
        if area_lower in name_lower or area_lower in promoter_lower:
            # Skip if already exists
            slug = slugify(entry.project_name)
            if slug in existing_slugs:
                continue
            if entry.project_name.lower() in existing_names_lower:
                continue
            candidates.append(entry)

    print(f"  {len(candidates)} candidates after area filter + dedup (limit {limit})\n")

    # Also do a fuzzy check: for each candidate, make sure it doesn't closely
    # match an existing society
    filtered_candidates = []
    for entry in candidates:
        entry_tokens = tokenize(entry.project_name)
        is_dupe = False
        for f in SOCIETY_DIR.glob("*.json"):
            try:
                data = json.loads(f.read_text())
                existing_tokens = tokenize(data.get("name", ""))
                if jaccard_similarity(entry_tokens, existing_tokens) > 0.7:
                    is_dupe = True
                    break
            except (json.JSONDecodeError, OSError):
                pass
        if not is_dupe:
            filtered_candidates.append(entry)

    candidates = filtered_candidates[:limit]
    print(f"  {len(candidates)} candidates after fuzzy dedup\n")

    if not candidates:
        print("No new societies to seed.")
        return

    session = ReraSession()
    edges = load_edges()
    created = 0

    print(f"{'RERA Project':<55} {'Promoter':<35} {'Action'}")
    print("-" * 110)

    for entry in candidates:
        # Search RERA portal for detail
        try:
            search_result = search_rera_project(session, entry.project_name)
        except Exception as e:
            print(f"  {entry.project_name:<55} {entry.promoter_name:<35} SEARCH-ERROR: {e}")
            continue

        if not search_result:
            print(f"  {entry.project_name:<55} {entry.promoter_name:<35} NOT-FOUND")
            continue

        # Fetch detail
        try:
            detail_html = fetch_rera_detail(session, search_result.numeric_id)
        except Exception as e:
            print(f"  {entry.project_name:<55} {entry.promoter_name:<35} DETAIL-ERROR: {e}")
            continue

        detail = parse_rera_detail(detail_html, search_result)
        facts = rera_detail_to_facts(detail)

        if not facts:
            print(f"  {entry.project_name:<55} {entry.promoter_name:<35} NO-FACTS")
            continue

        # Infer area from address or project name
        area_name = _infer_area_from_address(detail.project_address or "")
        if not area_name:
            area_name = area_filter  # Fall back to user-provided area

        # Create society node
        society_slug = slugify(entry.project_name)
        society_id = f"society:{society_slug}"

        # Build base facts (name, area, city, builder)
        base_source = FactSource(
            source_type="Rera",
            url="https://rera.karnataka.gov.in/projectViewDetails",
            skill_id="seed_from_rera",
        )

        base_facts = [
            SourcedFact(
                key="name",
                value={"type": "Text", "data": entry.project_name},
                confidence=1.0,
                source=base_source,
            ).to_dict(),
            SourcedFact(
                key="area",
                value={"type": "Text", "data": area_name},
                confidence=0.8,
                source=base_source,
            ).to_dict(),
            SourcedFact(
                key="city",
                value={"type": "Text", "data": "Bengaluru"},
                confidence=1.0,
                source=base_source,
            ).to_dict(),
            SourcedFact(
                key="builder_name",
                value={"type": "Text", "data": entry.promoter_name},
                confidence=1.0,
                source=base_source,
            ).to_dict(),
        ]

        society_node = {
            "id": society_id,
            "node_type": "Society",
            "name": entry.project_name,
            "root_source": "Rera",
            "facts": base_facts + [f.to_dict() for f in facts],
        }

        if dry_run:
            print(f"  {entry.project_name:<55} {entry.promoter_name:<35} WOULD-CREATE ({len(facts)} RERA facts)")
            created += 1
            continue

        # Write society node
        society_path = SOCIETY_DIR / f"{society_slug}.json"
        if society_path.exists():
            print(f"  {entry.project_name:<55} {entry.promoter_name:<35} ALREADY-EXISTS")
            continue

        atomic_write_json(society_path, society_node)

        # Create/check builder node
        builder_slug = slugify(entry.promoter_name)
        builder_id = f"builder:{builder_slug}"
        builder_path = BUILDER_DIR / f"{builder_slug}.json"

        if not builder_path.exists():
            builder_node = {
                "id": builder_id,
                "node_type": "Builder",
                "name": entry.promoter_name,
                "root_source": "Rera",
                "facts": [
                    SourcedFact(
                        key="name",
                        value={"type": "Text", "data": entry.promoter_name},
                        confidence=1.0,
                        source=base_source,
                    ).to_dict(),
                ],
            }
            BUILDER_DIR.mkdir(parents=True, exist_ok=True)
            atomic_write_json(builder_path, builder_node)

        # Create area node if missing
        area_slug = slugify(area_name)
        area_id = f"area:{area_slug}"
        area_path = AREA_DIR / f"{area_slug}.json"

        if not area_path.exists():
            area_node = {
                "id": area_id,
                "node_type": "Area",
                "name": area_name,
                "root_source": "Rera",
                "facts": [
                    SourcedFact(
                        key="name",
                        value={"type": "Text", "data": area_name},
                        confidence=1.0,
                        source=base_source,
                    ).to_dict(),
                    SourcedFact(
                        key="city",
                        value={"type": "Text", "data": "Bengaluru"},
                        confidence=1.0,
                        source=base_source,
                    ).to_dict(),
                ],
            }
            AREA_DIR.mkdir(parents=True, exist_ok=True)
            atomic_write_json(area_path, area_node)

        # Add edges
        edges = add_edge(edges, society_id, area_id, "SocietyInArea")
        edges = add_edge(edges, society_id, builder_id, "BuiltBy")

        created += 1
        print(f"  {entry.project_name:<55} {entry.promoter_name:<35} CREATED ({len(facts)} facts)")

    # Save edges
    if not dry_run and created > 0:
        save_edges(edges)
        print(f"\n  Edges saved ({len(edges)} total)")

    print(f"\nSeed complete:")
    print(f"  Created: {created}")
    print(f"  From {len(candidates)} candidates")


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main():
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )

    parser = argparse.ArgumentParser(description="RERA-based society seeding and backfill")
    parser.add_argument("--backfill", action="store_true", help="Backfill RERA facts for existing societies")
    parser.add_argument("--seed", action="store_true", help="Seed new society nodes from RERA")
    parser.add_argument("--area", type=str, default="", help="Area filter for seed mode")
    parser.add_argument("--limit", type=int, default=10, help="Max new societies to create in seed mode")
    parser.add_argument("--dry-run", action="store_true", help="Log actions without writing files")

    args = parser.parse_args()

    if not args.backfill and not args.seed:
        parser.error("Must specify --backfill or --seed")

    if args.backfill:
        run_backfill(dry_run=args.dry_run)

    if args.seed:
        if not args.area:
            parser.error("--seed requires --area")
        run_seed(area_filter=args.area, limit=args.limit, dry_run=args.dry_run)


if __name__ == "__main__":
    main()
