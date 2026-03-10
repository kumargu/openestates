"""
Entity Resolver — maps external names to canonical entity IDs in the knowledge graph.

Resolution strategy:
  1. Exact match (normalized: lowercase, strip suffixes, collapse whitespace)
  2. Fuzzy match (all significant words appear in a known name)
  3. Alias lookup (manual overrides from aliases.json)
  4. Create new if no match (for genuinely new entities)

Also includes FreshnessTracker for source staleness tracking.

Usage:
    python3 -m pipeline.entity_resolver
"""

import json
import logging
import re
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, List, Optional

logger = logging.getLogger(__name__)


class EntityResolver:
    """Maps external names to canonical entity IDs in the knowledge graph."""

    def __init__(self, knowledge_dir: str = "data/knowledge"):
        self.knowledge_dir = Path(knowledge_dir)
        self.nodes_dir = self.knowledge_dir / "nodes"
        self.aliases_path = self.knowledge_dir / "aliases.json"
        self.rera_index_path = self.knowledge_dir / "rera_index.json"

        # Lookup tables: normalized_name -> canonical_id
        # Also store: canonical_id -> display_name for reverse lookup
        self._name_to_id: Dict[str, str] = {}
        self._id_to_name: Dict[str, str] = {}
        self._slug_to_id: Dict[str, str] = {}  # slug (no type prefix) -> canonical_id

        # Aliases: alias_normalized -> canonical_id
        self._aliases: Dict[str, str] = {}

        # RERA ACK -> entity ID
        self._rera_index: Dict[str, str] = {}

        self._load_existing_nodes()
        self._load_aliases()
        self._load_rera_index()

    def _load_existing_nodes(self):
        """Scan knowledge graph nodes to build initial lookup table."""
        if not self.nodes_dir.exists():
            logger.warning("Nodes directory not found: %s", self.nodes_dir)
            return

        count = 0
        for type_dir in self.nodes_dir.iterdir():
            if not type_dir.is_dir():
                continue
            node_type = type_dir.name  # "society", "area", "builder", "property"
            for node_file in type_dir.glob("*.json"):
                try:
                    data = json.loads(node_file.read_text())
                    node_id = data.get("id", "")
                    name = data.get("name", "")
                    if not node_id or not name:
                        continue

                    slug = node_file.stem  # filename without .json
                    normalized = self._normalize(name)

                    self._name_to_id[normalized] = node_id
                    self._id_to_name[node_id] = name
                    self._slug_to_id[f"{node_type}:{slug}"] = node_id
                    count += 1
                except (json.JSONDecodeError, OSError) as e:
                    logger.warning("Failed to read node file %s: %s", node_file, e)

        logger.info("Loaded %d nodes into resolver lookup", count)

    def _load_aliases(self):
        """Load manual alias overrides."""
        if self.aliases_path.exists():
            try:
                raw = json.loads(self.aliases_path.read_text())
                for alias, canonical_id in raw.items():
                    self._aliases[self._normalize(alias)] = canonical_id
                logger.info("Loaded %d aliases", len(self._aliases))
            except (json.JSONDecodeError, OSError) as e:
                logger.warning("Failed to load aliases: %s", e)

    def _load_rera_index(self):
        """Load RERA ACK -> entity ID mappings."""
        if self.rera_index_path.exists():
            try:
                self._rera_index = json.loads(self.rera_index_path.read_text())
                logger.info("Loaded %d RERA mappings", len(self._rera_index))
            except (json.JSONDecodeError, OSError) as e:
                logger.warning("Failed to load RERA index: %s", e)

    def _normalize(self, name: str) -> str:
        """Lowercase, strip HTML entities, remove common prefixes/suffixes,
        collapse whitespace, strip punctuation."""
        s = name.strip()
        # Decode HTML entities
        s = s.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")
        s = s.replace("&#39;", "'").replace("&quot;", '"')
        # Lowercase
        s = s.lower()
        # Remove common prefixes
        for prefix in ["m/s.", "m/s ", "messrs.", "messrs "]:
            if s.startswith(prefix):
                s = s[len(prefix):]
        # Remove common suffixes
        for suffix in [
            "private limited", "pvt. ltd.", "pvt ltd", "pvt. limited",
            "limited", "ltd.", "ltd", "llp", "inc.", "inc",
            "phase 1", "phase 2", "phase 3", "phase i", "phase ii", "phase iii",
            "apartments", "apartment", "residences", "residence",
        ]:
            if s.endswith(suffix):
                s = s[: -len(suffix)]
        # Strip punctuation (keep alphanumeric, spaces, and &)
        s = re.sub(r"[^\w\s&]", " ", s)
        # Collapse whitespace
        s = re.sub(r"\s+", " ", s).strip()
        return s

    def _slugify(self, name: str) -> str:
        """Convert 'Prestige Lakeside Habitat' -> 'prestige-lakeside-habitat'."""
        s = self._normalize(name)
        # Replace spaces with hyphens
        s = re.sub(r"\s+", "-", s)
        # Remove any remaining non-alphanumeric chars except hyphens
        s = re.sub(r"[^a-z0-9-]", "", s)
        # Collapse multiple hyphens
        s = re.sub(r"-+", "-", s).strip("-")
        return s

    def _fuzzy_match(self, normalized_query: str, entity_type: str) -> Optional[str]:
        """Check if all significant words from the query appear in any known name.

        Only matches entities of the given type. Words shorter than 4 chars are
        ignored to avoid noise from articles, prepositions, etc.
        """
        query_words = [w for w in normalized_query.split() if len(w) >= 4]
        if not query_words:
            return None

        prefix = f"{entity_type}:"
        best_match = None
        best_score = 0

        for normalized_name, node_id in self._name_to_id.items():
            if not node_id.startswith(prefix):
                continue

            name_words = set(normalized_name.split())
            # Count how many query words appear in the name
            matches = sum(1 for w in query_words if w in name_words)

            if matches == len(query_words) and matches > best_score:
                best_score = matches
                best_match = node_id

        # Also check: all significant name words appear in query (for shorter canonical names)
        if not best_match:
            for normalized_name, node_id in self._name_to_id.items():
                if not node_id.startswith(prefix):
                    continue
                name_words = [w for w in normalized_name.split() if len(w) >= 4]
                if not name_words:
                    continue
                matches = sum(1 for w in name_words if w in set(normalized_query.split()))
                if matches == len(name_words) and matches > best_score:
                    best_score = matches
                    best_match = node_id

        return best_match

    def _resolve(self, name: str, entity_type: str, area: str = None) -> str:
        """Core resolution: exact -> fuzzy -> alias -> create new."""
        normalized = self._normalize(name)

        # 1. Exact normalized match
        if normalized in self._name_to_id:
            candidate = self._name_to_id[normalized]
            if candidate.startswith(f"{entity_type}:"):
                return candidate

        # 2. Alias lookup
        if normalized in self._aliases:
            candidate = self._aliases[normalized]
            if candidate.startswith(f"{entity_type}:"):
                return candidate

        # 3. Slug-based lookup (handles case where slug matches directly)
        slug = self._slugify(name)
        slug_key = f"{entity_type}:{slug}"
        if slug_key in self._slug_to_id:
            return self._slug_to_id[slug_key]

        # 4. Fuzzy match
        fuzzy = self._fuzzy_match(normalized, entity_type)
        if fuzzy:
            return fuzzy

        # 5. Create new entity ID
        new_id = f"{entity_type}:{slug}"
        self._name_to_id[normalized] = new_id
        self._id_to_name[new_id] = name.strip()
        self._slug_to_id[slug_key] = new_id
        logger.info("Created new entity: %s (from '%s')", new_id, name)
        return new_id

    def resolve_society(self, name: str, area: str = None) -> str:
        """Returns canonical ID like 'society:prestige-lakeside-habitat'."""
        return self._resolve(name, "society", area)

    def resolve_area(self, name: str) -> str:
        """Returns canonical ID like 'area:whitefield'."""
        return self._resolve(name, "area")

    def resolve_builder(self, name: str) -> str:
        """Returns canonical ID like 'builder:sobha-limited'."""
        return self._resolve(name, "builder")

    def register_alias(self, alias: str, canonical_id: str):
        """Manual override. Persists to aliases.json."""
        normalized = self._normalize(alias)
        self._aliases[normalized] = canonical_id
        logger.info("Registered alias: '%s' -> %s", alias, canonical_id)

    def register_rera_mapping(self, rera_ack: str, entity_id: str):
        """Map RERA ACK number to entity. Persists to rera_index.json."""
        self._rera_index[rera_ack] = entity_id
        logger.info("Registered RERA mapping: %s -> %s", rera_ack, entity_id)

    def lookup_by_rera(self, rera_ack: str) -> Optional[str]:
        """Reverse lookup: RERA ACK -> entity ID."""
        return self._rera_index.get(rera_ack)

    def get_all_entities(self, entity_type: str) -> List[str]:
        """Return all known entity IDs of a given type."""
        prefix = f"{entity_type}:"
        return sorted(set(
            eid for eid in self._id_to_name.keys() if eid.startswith(prefix)
        ))

    def get_name(self, entity_id: str) -> Optional[str]:
        """Get display name for an entity ID."""
        return self._id_to_name.get(entity_id)

    def save(self):
        """Persist aliases and RERA index to disk."""
        # Write aliases
        raw_aliases = {}
        for norm, cid in self._aliases.items():
            # Store the normalized form as key
            raw_aliases[norm] = cid
        _atomic_write(self.aliases_path, raw_aliases)
        logger.info("Saved %d aliases to %s", len(raw_aliases), self.aliases_path)

        # Write RERA index
        _atomic_write(self.rera_index_path, self._rera_index)
        logger.info("Saved %d RERA mappings to %s", len(self._rera_index), self.rera_index_path)


class FreshnessTracker:
    """Tracks when each entity was last enriched by each source.

    Data format:
    {
        "society:prestige-lakeside-habitat": {
            "rera": {"last_fetched": "2026-03-10T...", "ttl_days": 30},
            "reddit": {"last_fetched": "2026-03-08T...", "ttl_days": 14}
        }
    }
    """

    def __init__(self, path: str = "data/knowledge/source_freshness.json"):
        self.path = Path(path)
        self.data: Dict[str, Dict[str, dict]] = {}
        if self.path.exists():
            try:
                self.data = json.loads(self.path.read_text())
            except (json.JSONDecodeError, OSError):
                self.data = {}

    def is_stale(self, entity_id: str, source: str, ttl_days: int = 14) -> bool:
        """Check if source data for entity is older than TTL."""
        entry = self.data.get(entity_id, {}).get(source)
        if not entry:
            return True  # Never fetched = stale

        last_fetched = entry.get("last_fetched")
        if not last_fetched:
            return True

        try:
            fetched_dt = datetime.fromisoformat(last_fetched)
            now = datetime.now(timezone.utc)
            age_days = (now - fetched_dt).total_seconds() / 86400
            return age_days > ttl_days
        except (ValueError, TypeError):
            return True

    def mark_fresh(self, entity_id: str, source: str, metadata: dict = None):
        """Record that source was just fetched for entity."""
        if entity_id not in self.data:
            self.data[entity_id] = {}

        entry = {
            "last_fetched": datetime.now(timezone.utc).isoformat(),
        }
        if metadata:
            entry.update(metadata)

        self.data[entity_id][source] = entry

    def get_stale_entities(self, entity_type: str, source: str, ttl_days: int = 14) -> List[str]:
        """Find all entities of a type that need refresh from a source.

        Checks both entities with stale data and entities with no record at all.
        For entities with no record, we need an external list — so this only
        returns entities already tracked in freshness data.
        """
        prefix = f"{entity_type}:"
        stale = []
        for entity_id in self.data:
            if entity_id.startswith(prefix):
                if self.is_stale(entity_id, source, ttl_days):
                    stale.append(entity_id)
        return sorted(stale)

    def get_never_enriched(self, all_entity_ids: List[str], source: str) -> List[str]:
        """Find entities that have never been enriched by a source."""
        return sorted([
            eid for eid in all_entity_ids
            if eid not in self.data or source not in self.data.get(eid, {})
        ])

    def save(self):
        """Persist to disk."""
        _atomic_write(self.path, self.data)
        logger.info("Saved freshness data for %d entities to %s", len(self.data), self.path)


def _atomic_write(path: Path, data: dict):
    """Write JSON atomically via tmp+rename."""
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp_path = path.with_suffix(".tmp")
    tmp_path.write_text(json.dumps(data, indent=2, sort_keys=True, default=str))
    tmp_path.rename(path)


# ---------------------------------------------------------------------------
# Self-test
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")

    print("=" * 60)
    print("EntityResolver Self-Test")
    print("=" * 60)

    resolver = EntityResolver()

    # Show loaded entities
    for etype in ["society", "area", "builder"]:
        entities = resolver.get_all_entities(etype)
        print(f"\n  {etype}: {len(entities)} entities loaded")

    # Test exact resolution
    print("\n--- Exact Resolution Tests ---")
    tests = [
        ("resolve_builder", "Sobha Limited", "builder:sobha-limited"),
        ("resolve_society", "Prestige Lakeside Habitat", "society:prestige-lakeside-habitat"),
        ("resolve_area", "Whitefield", "area:whitefield"),
        ("resolve_area", "Sarjapur Road", "area:sarjapur-road"),
        ("resolve_builder", "Brigade Group", "builder:brigade-group"),
    ]

    for method_name, input_name, expected in tests:
        method = getattr(resolver, method_name)
        result = method(input_name)
        status = "PASS" if result == expected else "FAIL"
        print(f"  [{status}] {method_name}('{input_name}') -> {result}")
        if result != expected:
            print(f"         Expected: {expected}")

    # Test case insensitivity
    print("\n--- Case Insensitivity Tests ---")
    case_tests = [
        ("resolve_builder", "SOBHA LIMITED", "builder:sobha-limited"),
        ("resolve_society", "PRESTIGE LAKESIDE HABITAT", "society:prestige-lakeside-habitat"),
        ("resolve_area", "WHITEFIELD", "area:whitefield"),
    ]

    for method_name, input_name, expected in case_tests:
        method = getattr(resolver, method_name)
        result = method(input_name)
        status = "PASS" if result == expected else "FAIL"
        print(f"  [{status}] {method_name}('{input_name}') -> {result}")
        if result != expected:
            print(f"         Expected: {expected}")

    # Test fuzzy matching
    print("\n--- Fuzzy Matching Tests ---")
    fuzzy_tests = [
        ("resolve_society", "PRESTIGE LAKESIDE", "society:prestige-lakeside-habitat"),
        ("resolve_builder", "M/s. SOBHA LIMITED", "builder:sobha-limited"),
    ]

    for method_name, input_name, expected in fuzzy_tests:
        method = getattr(resolver, method_name)
        result = method(input_name)
        status = "PASS" if result == expected else "FAIL"
        print(f"  [{status}] {method_name}('{input_name}') -> {result}")
        if result != expected:
            print(f"         Expected: {expected}")

    # Test HTML entity handling
    print("\n--- HTML Entity Tests ---")
    result = resolver._normalize("Sobha &amp; Sons Pvt. Ltd.")
    expected_norm = "sobha & sons"
    status = "PASS" if result == expected_norm else "FAIL"
    print(f"  [{status}] normalize('Sobha &amp; Sons Pvt. Ltd.') -> '{result}'")
    if result != expected_norm:
        print(f"         Expected: '{expected_norm}'")

    # Test alias registration and persistence
    print("\n--- Alias Registration Test ---")
    resolver.register_alias("Sobha Insignia Whitefield", "society:sobha-insignia")
    result = resolver.resolve_society("Sobha Insignia Whitefield")
    status = "PASS" if result == "society:sobha-insignia" else "FAIL"
    print(f"  [{status}] After alias registration: resolve('Sobha Insignia Whitefield') -> {result}")

    # Test RERA mapping
    print("\n--- RERA Mapping Test ---")
    resolver.register_rera_mapping("ACK/KA/RERA/1251/446/PR/090822/006221", "society:sobha-insignia")
    result = resolver.lookup_by_rera("ACK/KA/RERA/1251/446/PR/090822/006221")
    status = "PASS" if result == "society:sobha-insignia" else "FAIL"
    print(f"  [{status}] RERA lookup -> {result}")

    # Save and verify persistence
    resolver.save()
    print("\n--- Persistence Test ---")
    resolver2 = EntityResolver()
    result = resolver2.resolve_society("Sobha Insignia Whitefield")
    status = "PASS" if result == "society:sobha-insignia" else "FAIL"
    print(f"  [{status}] After reload: resolve('Sobha Insignia Whitefield') -> {result}")

    rera_result = resolver2.lookup_by_rera("ACK/KA/RERA/1251/446/PR/090822/006221")
    status = "PASS" if rera_result == "society:sobha-insignia" else "FAIL"
    print(f"  [{status}] After reload: RERA lookup -> {rera_result}")

    # Test FreshnessTracker
    print("\n--- FreshnessTracker Test ---")
    tracker = FreshnessTracker()
    print(f"  is_stale('society:test', 'rera'): {tracker.is_stale('society:test', 'rera')}")
    tracker.mark_fresh("society:test", "rera", {"thread_count": 5})
    print(f"  After mark_fresh: is_stale('society:test', 'rera'): {tracker.is_stale('society:test', 'rera')}")
    print(f"  is_stale with ttl_days=0: {tracker.is_stale('society:test', 'rera', ttl_days=0)}")
    tracker.save()

    print("\n" + "=" * 60)
    print("Self-test complete.")
    print("=" * 60)
