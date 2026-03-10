"""
discover.py — Discover real properties and generate seed-compatible data.

Takes location queries, calls Gemini to find real properties, and writes:
1. Knowledge graph nodes (data/knowledge/nodes/)
2. Seed-format files (data/seed/) for backend consumption

Usage:
    python3 -m pipeline.discover "Kadugodi metro station"
    python3 -m pipeline.discover "Whitefield" "Sarjapur Road" "Bellandur"
    python3 -m pipeline.discover --all-areas
"""
from __future__ import annotations

import json
import logging
import os
import re
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    datefmt="%H:%M:%S",
)
logger = logging.getLogger(__name__)

# Project root
PROJECT_ROOT = Path(__file__).parent.parent
NODES_DIR = PROJECT_ROOT / "data" / "knowledge" / "nodes"
EDGES_FILE = PROJECT_ROOT / "data" / "knowledge" / "edges.json"
SEED_DIR = PROJECT_ROOT / "data" / "seed"

# Default locations to discover (Bengaluru focus)
DEFAULT_LOCATIONS = [
    "Whitefield, near ITPL",
    "Sarjapur Road, near Wipro junction",
    "Bellandur, near Outer Ring Road",
    "HSR Layout",
    "Electronic City Phase 1",
    "Hebbal, near Manyata Tech Park",
    "Marathahalli",
    "Koramangala",
]


def slugify(text: str) -> str:
    text = text.lower().strip()
    text = re.sub(r'[^a-z0-9\s-]', '', text)
    text = re.sub(r'[\s-]+', '-', text)
    return text.strip('-')


def load_node(node_type: str, slug: str) -> dict | None:
    """Load a node from per-entity storage."""
    path = NODES_DIR / node_type / f"{slug}.json"
    if path.exists():
        return json.loads(path.read_text())
    return None


def save_node(node: dict) -> None:
    """Save a node to per-entity storage with atomic write."""
    node_id = node["id"]
    # Parse "type:slug" format
    parts = node_id.split(":", 1)
    if len(parts) != 2:
        logger.error("Invalid node_id format: %s", node_id)
        return
    node_type = parts[0]
    slug = parts[1]

    dir_path = NODES_DIR / node_type
    dir_path.mkdir(parents=True, exist_ok=True)

    path = dir_path / f"{slug}.json"
    tmp_path = path.with_suffix(".tmp")
    tmp_path.write_text(json.dumps(node, indent=2, default=str))
    tmp_path.rename(path)


def load_edges() -> list:
    if EDGES_FILE.exists():
        return json.loads(EDGES_FILE.read_text())
    return []


def save_edges(edges: list) -> None:
    EDGES_FILE.parent.mkdir(parents=True, exist_ok=True)
    tmp = EDGES_FILE.with_suffix(".tmp")
    tmp.write_text(json.dumps(edges, indent=2, default=str))
    tmp.rename(EDGES_FILE)


def create_property_node(prop_data: dict, facts: list, city: str) -> dict:
    """Create a knowledge graph node for a discovered property."""
    slug = prop_data["slug"]
    node_id = f"property:{prop_data['base_id']}"
    now = datetime.now(timezone.utc).isoformat()

    # Filter facts for this property (by checking key patterns)
    return {
        "id": node_id,
        "node_type": "Property",
        "name": prop_data["project_name"],
        "facts": facts,
        "summary_embedding": None,
        "created_at": now,
        "updated_at": now,
    }


def create_society_node(prop_data: dict, city: str) -> dict:
    """Create a society node from discovered property data."""
    society_name = prop_data.get("society_name") or prop_data.get("project_name") or "Unknown"
    slug = slugify(society_name)
    node_id = f"society:{slug}"
    now = datetime.now(timezone.utc).isoformat()

    source = {
        "source_type": "Google",
        "model": "gemini-2.5-flash",
        "skill_id": "discover_properties",
    }

    facts = [
        {"key": "name", "value": {"type": "Text", "data": society_name},
         "confidence": 0.8, "source": source, "learned_at": now, "version": 1},
        {"key": "area", "value": {"type": "Text", "data": prop_data["area"]},
         "confidence": 0.8, "source": source, "learned_at": now, "version": 1},
        {"key": "city", "value": {"type": "Text", "data": city},
         "confidence": 0.8, "source": source, "learned_at": now, "version": 1},
        {"key": "builder_name", "value": {"type": "Text", "data": prop_data["builder_name"]},
         "confidence": 0.8, "source": source, "learned_at": now, "version": 1},
    ]

    if prop_data.get("total_units_approx"):
        facts.append({
            "key": "total_units", "value": {"type": "Numeric", "data": float(prop_data["total_units_approx"])},
            "confidence": 0.6, "source": source, "learned_at": now, "version": 1,
        })

    if prop_data.get("year_built_or_expected"):
        facts.append({
            "key": "year_built", "value": {"type": "Numeric", "data": float(prop_data["year_built_or_expected"])},
            "confidence": 0.6, "source": source, "learned_at": now, "version": 1,
        })

    summary_parts = []
    if prop_data.get("key_highlights"):
        summary_parts.extend(prop_data["key_highlights"])
    if prop_data.get("configurations"):
        summary_parts.append(f"Configurations: {', '.join(prop_data['configurations'])}")
    if summary_parts:
        facts.append({
            "key": "summary", "value": {"type": "Text", "data": ". ".join(summary_parts)},
            "confidence": 0.6, "source": source, "learned_at": now, "version": 1,
        })

    return {
        "id": node_id,
        "node_type": "Society",
        "name": society_name,
        "facts": facts,
        "summary_embedding": None,
        "created_at": now,
        "updated_at": now,
    }


def create_area_node(area_name: str, city: str) -> dict:
    """Create a minimal area node (will be enriched by learn_area later)."""
    slug = slugify(area_name)
    node_id = f"area:{slug}"
    now = datetime.now(timezone.utc).isoformat()

    source = {"source_type": "Manual", "skill_id": "discover_properties"}

    return {
        "id": node_id,
        "node_type": "Area",
        "name": area_name,
        "facts": [
            {"key": "city", "value": {"type": "Text", "data": city},
             "confidence": 1.0, "source": source, "learned_at": now, "version": 1},
            {"key": "name", "value": {"type": "Text", "data": area_name},
             "confidence": 1.0, "source": source, "learned_at": now, "version": 1},
        ],
        "summary_embedding": None,
        "created_at": now,
        "updated_at": now,
    }


def create_builder_node(builder_name: str, city: str) -> dict:
    """Create a minimal builder node."""
    slug = slugify(builder_name)
    node_id = f"builder:{slug}"
    now = datetime.now(timezone.utc).isoformat()

    source = {"source_type": "Manual", "skill_id": "discover_properties"}

    return {
        "id": node_id,
        "node_type": "Builder",
        "name": builder_name,
        "facts": [
            {"key": "name", "value": {"type": "Text", "data": builder_name},
             "confidence": 1.0, "source": source, "learned_at": now, "version": 1},
        ],
        "summary_embedding": None,
        "created_at": now,
        "updated_at": now,
    }


def node_to_seed_property(prop_data: dict, prop_id: str, idx: int, city: str) -> dict:
    """Convert discovered property data to seed-format Property JSON."""
    area = prop_data.get("area") or "Unknown"
    area_slug = slugify(area)
    society_name = prop_data.get("society_name") or prop_data.get("project_name") or "Unknown"
    society_slug = slugify(society_name)
    builder = prop_data.get("builder_name") or "Unknown Builder"

    # Parse carpet area range like "1100-1800 sqft"
    carpet_min, carpet_max = 1000, 1500
    carpet_range = prop_data.get("carpet_area_range") or ""
    if carpet_range:
        nums = re.findall(r'\d+', carpet_range)
        if len(nums) >= 2:
            carpet_min, carpet_max = int(nums[0]), int(nums[1])
        elif len(nums) == 1:
            carpet_min = carpet_max = int(nums[0])

    # Parse configs to find BHK options
    configs = prop_data.get("configurations") or []
    bhk_options = []
    for c in configs:
        nums = re.findall(r'(\d+)\s*[bB][hH][kK]', c)
        if nums:
            bhk_options.append(int(nums[0]))
    if not bhk_options:
        bhk_options = [3]  # default

    price_min = (prop_data.get("price_range_min_lakhs") or 80) * 100000
    price_max = (prop_data.get("price_range_max_lakhs") or 150) * 100000
    price_psf = prop_data.get("price_per_sqft_approx") or 8000
    total_floors = prop_data.get("total_floors") or 15
    possession = prop_data.get("possession_status", "under_construction")
    highlights = prop_data.get("key_highlights", [])

    # Generate one property per BHK config
    properties = []
    for bhk_idx, bhk in enumerate(bhk_options):
        # Scale carpet area and price by BHK
        scale = bhk / 3.0  # normalize around 3BHK
        carpet = int(carpet_min + (carpet_max - carpet_min) * (bhk_idx / max(len(bhk_options) - 1, 1)))
        if carpet == 0:
            carpet = int((carpet_min + carpet_max) / 2)
        super_built = int(carpet * 1.25)
        price = int(price_min + (price_max - price_min) * (bhk_idx / max(len(bhk_options) - 1, 1)))
        if price == 0:
            price = int((price_min + price_max) / 2)

        actual_psf = int(price / super_built) if super_built > 0 else price_psf

        sub_id = f"{prop_id}-{bhk}bhk"
        title = f"{bhk} BHK in {prop_data['project_name']}"

        possession_label = {
            "ready_to_move": "Ready to Move",
            "under_construction": "Under Construction",
            "new_launch": "New Launch",
        }.get(possession, possession.replace("_", " ").title())

        # Build transparency tags from available data
        tags = []
        if prop_data.get("rera_number"):
            tags.append("RERA Verified")
        if possession == "ready_to_move":
            tags.append("Ready to Move")
        if price_psf and price_psf < 8000:
            tags.append("Value Buy")
        if prop_data.get("distance_from_location_km") and prop_data["distance_from_location_km"] <= 2:
            tags.append("Prime Location")
        if not tags:
            tags = ["Discovered via Search"]

        description = f"{bhk} BHK apartment in {prop_data['project_name']} by {builder}, {area}."
        if highlights:
            description += " " + highlights[0]

        properties.append({
            "id": sub_id,
            "title": title,
            "area": area,
            "area_id": f"area-{area_slug}",
            "city": city,
            "society_id": f"soc-{society_slug}",
            "builder_name": builder,
            "property_type": "Apartment",
            "listing_type": "Resale" if possession == "ready_to_move" else "New",
            "bhk": bhk,
            "price": price,
            "price_per_sqft": actual_psf,
            "carpet_area_sqft": carpet,
            "super_builtup_sqft": super_built,
            "floor": min(5, total_floors),
            "total_floors": total_floors,
            "facing": "East",
            "possession_status": possession_label,
            "metro_distance_mins": int((prop_data.get("distance_from_location_km") or 3) * 5),
            "maintenance_cost_monthly": int(carpet * 4),
            "society_quality_score": 0.5,
            "builder_quality_score": 0.5,
            "document_completeness_score": 0.7 if prop_data.get("rera_number") else 0.3,
            "litigation_risk": 0.1,
            "noise_score": 0.5,
            "sunlight_score": 0.5,
            "airport_noise_score": 0.2,
            "waterlogging_risk_score": 0.3,
            "traffic_score": 0.5,
            "days_on_market": 30,
            "greenery_score": 0.5,
            "open_space_score": 0.5,
            "resale_strength_score": 0.5,
            "interest_level": "moderate",
            "saves_last_7d": 5,
            "offers_last_7d": 2,
            "images": [],
            "hero_image": "",
            "description_summary": description,
            "transparency_tags": tags,
            "source_reference": prop_data.get("source_url", "Discovered via Gemini Search"),
        })

    return properties


def node_to_seed_society(prop_data: dict, city: str) -> dict:
    """Convert discovered property data to seed-format Society JSON."""
    society_name = prop_data.get("society_name") or prop_data.get("project_name") or "Unknown"
    slug = slugify(society_name)
    area = prop_data.get("area") or "Unknown"
    builder = prop_data.get("builder_name") or "Unknown"
    highlights = prop_data.get("key_highlights") or []

    return {
        "id": f"soc-{slug}",
        "name": society_name,
        "area": area,
        "city": city,
        "builder_name": builder,
        "year_built": prop_data.get("year_built_or_expected") or 2023,
        "total_units": prop_data.get("total_units_approx") or 0,
        "summary": ". ".join(highlights) if highlights else f"{society_name} by {builder} in {area}.",
        "maintenance_sentiment": "Unknown — needs enrichment",
        "livability_sentiment": "Unknown — needs enrichment",
        "common_positives": [],
        "common_complaints": [],
        "review_summary": "Not yet enriched. Run learn_society skill to populate.",
        "future_google_place_name": f"{society_name}, {area}, {city}",
        "future_google_place_id": None,
        "future_review_enrichment_status": "pending",
    }


def node_to_seed_area(area_name: str, city: str) -> dict:
    """Create a minimal seed-format area profile matching Rust AreaProfile struct."""
    slug = slugify(area_name)
    return {
        "id": f"area-{slug}",
        "name": area_name,
        "city": city,
        "median_price_per_sqft": 0,
        "price_range_per_sqft": {"low": 0, "high": 0},
        "trend_direction": "stable",
        "trend_summary": "Not yet enriched. Run learn_area skill to populate.",
        "metro_access_summary": "Not yet enriched.",
        "airport_noise_summary": "Not assessed.",
        "traffic_summary": "Not yet enriched.",
        "waterlogging_summary": "Not yet enriched.",
        "livability_summary": "Not yet enriched.",
        "externality_tags": [],
        "infrastructure_tags": [],
        "reddit_signals": {
            "decision_drivers": [],
            "recurring_concerns": [],
            "sentiment_label": "neutral",
            "last_updated": datetime.now().strftime("%Y-%m-%d"),
        },
        "community_notes": "Area discovered via property search. Needs enrichment.",
        "sample_size": 0,
        "last_updated": datetime.now().strftime("%Y-%m-%d"),
    }


def discover_and_write(locations: list[str], city: str = "Bengaluru") -> dict:
    """
    Main orchestration: discover properties for given locations and write all data.

    Returns stats dict.
    """
    from pipeline.skills.discover_properties import DiscoverPropertiesSkill

    skill = DiscoverPropertiesSkill()

    all_properties = []  # seed format
    all_societies = {}   # dedup by slug
    all_areas = {}       # dedup by slug
    all_kg_nodes = []    # knowledge graph nodes
    all_edges = load_edges()
    existing_edge_set = {(e.get("from", ""), e.get("to", ""), e.get("edge_type", "")) for e in all_edges}

    total_discovered = 0
    total_api_calls = 0
    total_cost = 0.0

    for location in locations:
        logger.info("=== Discovering properties near: %s ===", location)

        result = skill.run({
            "location": location,
            "city": city,
            "triggered_by": "discover.py",
        })

        total_api_calls += result.cost.api_calls
        total_cost += result.cost.estimated_usd

        if not result.new_nodes:
            logger.warning("No properties found for '%s'", location)
            continue

        logger.info("Found %d properties near %s", len(result.new_nodes), location)
        total_discovered += len(result.new_nodes)

        for i, prop_data in enumerate(result.new_nodes):
            area = prop_data.get("area", "Unknown")
            area_slug = slugify(area)
            builder = prop_data.get("builder_name", "Unknown")
            builder_slug = slugify(builder)
            society_name = prop_data.get("society_name") or prop_data.get("project_name") or "Unknown"
            society_slug = slugify(society_name)

            # Create area node if new
            if area_slug not in all_areas:
                area_node = load_node("area", area_slug)
                if not area_node:
                    area_node = create_area_node(area, city)
                    save_node(area_node)
                    logger.info("  Created area node: %s", area)
                all_areas[area_slug] = area
                # Seed format
                if not any(a["id"] == f"area-{area_slug}" for a in all_areas.values() if isinstance(a, dict)):
                    pass  # We'll build seed areas from the all_areas dict

            # Create builder node if new
            if builder_slug and builder_slug != "unknown-builder":
                existing = load_node("builder", builder_slug)
                if not existing:
                    builder_node = create_builder_node(builder, city)
                    save_node(builder_node)
                    logger.info("  Created builder node: %s", builder)

            # Create society node
            existing_society = load_node("society", society_slug)
            if not existing_society:
                society_node = create_society_node(prop_data, city)
                save_node(society_node)
                logger.info("  Created society node: %s", society_name)

            # Create property nodes (one per BHK config)
            seed_props = node_to_seed_property(prop_data, prop_data["base_id"], i, city)
            all_properties.extend(seed_props)

            # Create KG property node with facts from discovery
            prop_facts = [f.to_dict() for f in result.facts
                          if f.key in ("project_name", "builder_name", "area", "city",
                                       "configurations", "price_range_label", "price_min_lakhs",
                                       "price_max_lakhs", "price_per_sqft", "possession_status",
                                       "total_floors", "distance_from_query_km", "highlights",
                                       "rera_number")
                          and any(prop_data["project_name"] in str(f.value.get("data", ""))
                                  for _ in [None])  # Match facts to this property
                          ]
            # Use the seed properties to create KG nodes instead
            for sp in seed_props:
                now = datetime.now(timezone.utc).isoformat()
                source = {"source_type": "Google", "model": "gemini-2.5-flash",
                          "skill_id": "discover_properties"}
                kg_facts = [
                    {"key": "area", "value": {"type": "Text", "data": sp["area"]},
                     "confidence": 0.8, "source": source, "learned_at": now, "version": 1},
                    {"key": "city", "value": {"type": "Text", "data": sp["city"]},
                     "confidence": 0.8, "source": source, "learned_at": now, "version": 1},
                    {"key": "bhk", "value": {"type": "Numeric", "data": float(sp["bhk"])},
                     "confidence": 0.8, "source": source, "learned_at": now, "version": 1},
                    {"key": "price", "value": {"type": "Numeric", "data": float(sp["price"])},
                     "confidence": 0.6, "source": source, "learned_at": now, "version": 1},
                    {"key": "carpet_area_sqft", "value": {"type": "Numeric", "data": float(sp["carpet_area_sqft"])},
                     "confidence": 0.6, "source": source, "learned_at": now, "version": 1},
                    {"key": "builder_name", "value": {"type": "Text", "data": sp["builder_name"]},
                     "confidence": 0.8, "source": source, "learned_at": now, "version": 1},
                    {"key": "title", "value": {"type": "Text", "data": sp["title"]},
                     "confidence": 0.8, "source": source, "learned_at": now, "version": 1},
                ]
                kg_node = {
                    "id": f"property:{sp['id']}",
                    "node_type": "Property",
                    "name": sp["title"],
                    "facts": kg_facts,
                    "summary_embedding": None,
                    "created_at": now,
                    "updated_at": now,
                }
                save_node(kg_node)

            # Add edges (dedup)
            for edge in result.edges:
                edge_key = (edge["from"], edge["to"], edge["edge_type"])
                if edge_key not in existing_edge_set:
                    all_edges.append(edge)
                    existing_edge_set.add(edge_key)

            # Collect society for seed
            if society_slug not in all_societies:
                all_societies[society_slug] = node_to_seed_society(prop_data, city)

        # Rate limit between locations
        if not result.cached:
            time.sleep(2)

    # Build seed area profiles
    seed_areas = []
    for area_slug, area_name in all_areas.items():
        if isinstance(area_name, str):
            seed_areas.append(node_to_seed_area(area_name, city))

    # Write seed files
    SEED_DIR.mkdir(parents=True, exist_ok=True)

    # Properties
    props_path = SEED_DIR / "properties.json"
    props_path.write_text(json.dumps(all_properties, indent=2))
    logger.info("Wrote %d properties to %s", len(all_properties), props_path)

    # Societies
    societies_path = SEED_DIR / "societies.json"
    societies_list = list(all_societies.values())
    societies_path.write_text(json.dumps(societies_list, indent=2))
    logger.info("Wrote %d societies to %s", len(societies_list), societies_path)

    # Area profiles
    areas_path = SEED_DIR / "area_profiles.json"
    areas_path.write_text(json.dumps(seed_areas, indent=2))
    logger.info("Wrote %d areas to %s", len(seed_areas), areas_path)

    # Save edges
    save_edges(all_edges)
    logger.info("Saved %d edges", len(all_edges))

    stats = {
        "locations_queried": len(locations),
        "properties_discovered": total_discovered,
        "properties_written": len(all_properties),
        "societies_written": len(societies_list),
        "areas_written": len(seed_areas),
        "edges_total": len(all_edges),
        "api_calls": total_api_calls,
        "estimated_cost_usd": total_cost,
    }

    logger.info("\n=== Discovery Complete ===")
    for k, v in stats.items():
        logger.info("  %s: %s", k, v)

    return stats


def clear_old_data():
    """Remove old seed data and knowledge graph nodes to start fresh."""
    import shutil

    # Clear old seed files
    for f in ["properties.json", "societies.json", "area_profiles.json"]:
        path = SEED_DIR / f
        if path.exists():
            # Backup first
            backup = path.with_suffix(".json.bak")
            path.rename(backup)
            logger.info("Backed up %s → %s", path.name, backup.name)

    # Clear old knowledge graph nodes
    if NODES_DIR.exists():
        backup_dir = NODES_DIR.parent / "nodes_backup"
        if backup_dir.exists():
            shutil.rmtree(backup_dir)
        NODES_DIR.rename(backup_dir)
        logger.info("Backed up knowledge nodes → nodes_backup/")

    # Clear old edges
    if EDGES_FILE.exists():
        EDGES_FILE.rename(EDGES_FILE.with_suffix(".json.bak"))
        logger.info("Backed up edges.json")

    # Recreate dirs
    NODES_DIR.mkdir(parents=True, exist_ok=True)
    for sub in ["property", "society", "area", "builder"]:
        (NODES_DIR / sub).mkdir(exist_ok=True)


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Discover real properties via Gemini Search")
    parser.add_argument("locations", nargs="*", help="Locations to search (e.g., 'Whitefield')")
    parser.add_argument("--all-areas", action="store_true", help="Search all default Bengaluru areas")
    parser.add_argument("--city", default="Bengaluru", help="City (default: Bengaluru)")
    parser.add_argument("--clear", action="store_true", help="Clear old seed data before discovery")
    parser.add_argument("--force", action="store_true", help="Skip cache (re-discover)")
    args = parser.parse_args()

    # Check API key
    if not os.environ.get("GOOGLE_AI_API_KEY"):
        # Try loading from .env
        env_path = PROJECT_ROOT / ".env"
        if env_path.exists():
            for line in env_path.read_text().splitlines():
                if line.startswith("GOOGLE_AI_API_KEY="):
                    os.environ["GOOGLE_AI_API_KEY"] = line.split("=", 1)[1].strip().strip('"')
                    break

    if not os.environ.get("GOOGLE_AI_API_KEY"):
        print("ERROR: GOOGLE_AI_API_KEY not set. Add it to .env or export it.")
        sys.exit(1)

    if args.clear:
        logger.info("Clearing old data...")
        clear_old_data()

    locations = args.locations if args.locations else (DEFAULT_LOCATIONS if args.all_areas else None)

    if not locations:
        print("Usage: python3 -m pipeline.discover 'Whitefield' 'Sarjapur Road'")
        print("   or: python3 -m pipeline.discover --all-areas")
        print("   or: python3 -m pipeline.discover --all-areas --clear")
        sys.exit(1)

    stats = discover_and_write(locations, args.city)

    print(f"\nDone! {stats['properties_written']} properties across {stats['areas_written']} areas.")
    print("Next steps:")
    print("  1. Run enrichment: python3 -m pipeline.enrich_all")
    print("  2. Restart backend:  cd backend && cargo run")
    print("  3. Test search: curl 'http://localhost:4000/api/search?q=3bhk+whitefield'")
