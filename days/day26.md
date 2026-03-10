# Day 26: Data Pipeline Reset — RERA as Canonical Index + r/bangalore Area Intelligence

## The Insight

We've been building sources in isolation: seed data, live discovery, Reddit, images, RERA verification. Each one has its own shape, its own storage path, its own relationship to the property index. The result is a loosely assembled data model where entity identity is fragile and enrichment is ad-hoc.

Today we fix the foundation. Two structural moves:

1. **RERA becomes the canonical property index** — the skeleton that everything else hangs on
2. **r/bangalore becomes the area intelligence layer** — structured area knowledge extracted from the most active Bangalore subreddit

The flow after today:

```
RERA (canonical index — what exists, who built it, is it legal)
  ↓ feeds
Property Index (our structured model — identity, slug, linking)
  ↓ enriched by
Reddit r/bangalore, Google Reviews, Images, Area Knowledge (soft signals)
  ↓ scored by
Claude Skills (judgment layer)
  ↓ served by
Rust Backend → Frontend
```

---

## 1. The Problem With the Current Data Model

### 1.1 Entity Identity is Fragile

Right now, a property gets its identity from how it was discovered:
- Seed data: manually assigned IDs like `prop-w-001`
- Live discovery: auto-generated IDs like `discovered-sobha-insignia-3bhk`
- RERA: has ACK numbers like `ACK/KA/RERA/1251/446/PR/090822/006221`
- Reddit: references society names like "Sobha Insignia" in free text
- Knowledge graph: uses slugs like `society:sobha-insignia`

There's no **entity resolution** layer. If RERA says "Sobha Insignia" and Reddit says "Sobha insignia review" and our seed data says "3 BHK in Sobha Insignia", these should all resolve to the same entity. Today they might not.

### 1.2 Area Knowledge is Thin

Area nodes have 13 facts (Whitefield example: metro_status, traffic, waterlogging, etc.) — all from a single `learn_area` Gemini call. But r/bangalore has **thousands** of threads with real resident experiences about areas. This is the richest source of area intelligence we have access to, and we're barely using it.

### 1.3 No Source Orchestration

There's no concept of "what sources has this entity been enriched from?" and "what's stale?" Each skill runs independently. We need a light orchestration layer that tracks source freshness per entity.

---

## 2. What We're Building

### Phase A: Entity Resolution & Matching Layer (1 hour)

A simple but durable entity matching system:

**`pipeline/entity_resolver.py`** — the single place where names become slugs

```python
class EntityResolver:
    """Maps external names to canonical entity IDs.

    Maintains a lookup table:
      ("Sobha Insignia", "Whitefield") → "society:sobha-insignia"
      ("SOBHA LIMITED", None) → "builder:sobha-limited"
      ("Whitefield", None) → "area:whitefield"

    Resolution strategy:
      1. Exact match (normalized: lowercase, strip suffixes like "Phase 1")
      2. Fuzzy match (Levenshtein distance < 3, same area)
      3. Alias lookup (manual overrides for known mismatches)
      4. Create new if no match (for genuinely new entities)
    """

    def resolve_society(self, name: str, area: str = None) -> str: ...
    def resolve_area(self, name: str) -> str: ...
    def resolve_builder(self, name: str) -> str: ...

    def register_alias(self, alias: str, canonical_id: str): ...
    def register_rera_mapping(self, rera_ack: str, entity_id: str): ...
```

Key design:
- **Loads existing KG nodes at init** — builds the lookup table from what we already have
- **Persists aliases** to `data/knowledge/aliases.json` — manual overrides survive restarts
- **RERA ACK → entity ID mapping** stored separately in `data/knowledge/rera_index.json`
- Used by ALL skills — RERA fetcher, Reddit enricher, image fetcher all resolve through this

### Phase B: RERA Scraper Skill (2 hours)

**`pipeline/skills/fetch_rera.py`** — replaces the existing `verify_rera.py` (which uses Claude as a crutch)

This is the reference implementation for how a data source plugs into the pipeline.

#### B.1 Listing Scraper

Scrape the full Karnataka RERA project listing page (9,469 projects):

```python
def scrape_rera_listing() -> List[ReraListingEntry]:
    """
    GET https://rera.karnataka.gov.in/viewAllProjects?language=en
    Parse the .push() JS arrays to extract:
      - ack_number (ACK/KA/RERA/...)
      - registration_number (PRM/KA/RERA/...)
      - project_name
      - promoter_name

    Returns all 9,469 entries. Cached to disk for 7 days.
    """
```

#### B.2 Search + Detail Scraper

For a specific project, fetch the full detail page:

```python
def scrape_rera_project(project_name: str) -> Optional[ReraProjectDetail]:
    """
    1. POST to /projectViewDetails with project name
    2. Parse search results to get numeric ID
    3. POST to /projectDetails with numeric ID (needs session cookie)
    4. Parse the multi-tab HTML response

    Returns structured data:
      - registration: ack, reg_number, status, approved_on, completion_date
      - project: name, type, address, lat/lng, total_units, towers, floors
      - units: list of {floor, unit_no, bhk, carpet_area, parking}
      - cost: land_cost, construction_cost, total_cost, itemized_approvals
      - escrow: bank_name, account_no, ifsc, branch
      - land: survey_numbers, ownership_chain, conversion, khatha, litigation_status
      - builder_track_record: list of RERA projects across states
      - complaints: list of {complaint_no, date, subject, status}
      - timeline: original_completion, covid_extension, section6_extension, further_extension
      - schedule: phase-wise construction dates
      - documents: list of {name, pdf_url} (NOCs, certificates, financial docs)
    """
```

#### B.3 Fact Producer

Convert RERA data into SourcedFacts:

```python
def rera_to_facts(detail: ReraProjectDetail) -> List[SourcedFact]:
    """
    Produces self-describing facts:
      - rera_registered (Bool, confidence=1.0)
      - rera_number (Text)
      - rera_status (Text: "Approved" | "Expired" | "Rejected")
      - rera_completion_date (Text, ISO date)
      - rera_delay_months (Numeric — original vs current completion)
      - rera_total_units (Numeric)
      - rera_total_project_cost (Numeric, INR)
      - rera_land_cost (Numeric)
      - rera_construction_cost (Numeric)
      - rera_complaints_count (Numeric)
      - rera_complaints_resolved_pct (Numeric)
      - rera_builder_projects_count (Numeric — across states)
      - rera_builder_revocations (Numeric)
      - rera_land_litigation (Bool)
      - rera_escrow_bank (Text)
      - rera_lat_lng (Text)
      - rera_carpet_area_sqm (Numeric — per unit type)
      - rera_unit_inventory (Tags — list of unit types)

    All facts have:
      - source_type: "Rera"
      - url: RERA portal link
      - confidence: 1.0 (government source)
      - display_template: buyer-friendly text
      - answers_preferences: mapped to user search terms
    """
```

#### B.4 Entity Resolution Integration

When RERA data is fetched:
1. Resolve project name → society slug via `EntityResolver`
2. Resolve promoter name → builder slug
3. Resolve district/taluk → area slug
4. If new entity → create KG node, register in resolver
5. Attach facts to resolved node

### Phase C: r/bangalore Area Intelligence (1.5 hours)

**Enhance `pipeline/skills/learn_area.py`** — use r/bangalore as the primary area knowledge source

#### C.1 Area-Focused Reddit Fetcher

```python
def fetch_area_threads(area_name: str, subreddit: str = "bangalore") -> List[dict]:
    """
    Search r/bangalore for area-specific threads:
      - "{area_name} living"
      - "{area_name} review"
      - "{area_name} pros cons"
      - "{area_name} rent buy"
      - "{area_name} traffic"
      - "{area_name} safety"

    Combine and deduplicate. Fetch comment threads for top 5 posts.
    Returns raw thread data with comments.
    """
```

#### C.2 Claude Area Synthesis

Feed Reddit threads to Claude for structured area intelligence:

```python
def synthesize_area_knowledge(area_name: str, threads: List[dict]) -> List[SourcedFact]:
    """
    Claude reads Reddit threads and produces self-describing facts:

    Infrastructure:
      - metro_connectivity (Text + confidence)
      - road_quality (Text)
      - water_supply_reliability (Text)
      - power_supply_reliability (Text)
      - internet_connectivity (Text)

    Livability:
      - safety_perception (Text)
      - noise_level (Text)
      - air_quality (Text)
      - green_cover (Text)
      - walkability (Text)

    Demographics:
      - typical_residents (Text — families, young professionals, etc.)
      - community_vibe (Text)
      - pet_friendliness (Text)

    Practical:
      - grocery_shopping (Text — options, quality)
      - healthcare_access (Text)
      - school_quality (Text — specific school names if mentioned)
      - restaurant_scene (Text)
      - commute_reality (Text — actual commute times from residents)

    Concerns:
      - recurring_complaints (Tags — top 5 complaints from residents)
      - deal_breakers (Tags — things that make people leave)
      - hidden_gems (Tags — things residents love that aren't obvious)

    Each fact has Reddit URLs as source, answers_preferences mapped.
    """
```

#### C.3 Freshness Tracking

```python
# data/knowledge/source_freshness.json
{
    "area:whitefield": {
        "reddit": {"last_fetched": "2026-03-10T...", "thread_count": 47, "ttl_days": 14},
        "learn_area": {"last_fetched": "2026-03-09T...", "ttl_days": 30},
        "rera_listing": null  // not applicable for areas
    },
    "society:prestige-lakeside-habitat": {
        "reddit": {"last_fetched": "2026-03-08T...", "thread_count": 12, "ttl_days": 14},
        "rera": {"last_fetched": null, "ttl_days": 30},  // not yet fetched
        "images": {"last_fetched": "2026-03-07T...", "count": 5, "ttl_days": 90}
    }
}
```

This is the orchestration layer — any skill can check if its source is stale before re-fetching.

### Phase D: Pipeline Orchestrator (30 min)

**`pipeline/orchestrate.py`** — the entry point for "make this entity smarter"

```python
def enrich_entity(entity_id: str, force: bool = False):
    """
    Given an entity ID (e.g., "society:prestige-lakeside-habitat"):
    1. Check source_freshness.json — what's stale?
    2. Run stale skills in dependency order:
       - RERA first (canonical data, highest confidence)
       - Reddit next (enrichment, moderate confidence)
       - Claude synthesis last (judgment, depends on raw data)
    3. Update freshness timestamps
    4. Push facts to Rust backend via graph_client

    For areas:
    1. Reddit area threads (raw data)
    2. Claude area synthesis (structured intelligence)
    3. Update freshness

    For societies:
    1. RERA project detail (if not yet fetched)
    2. Reddit society threads
    3. Claude society synthesis (learn_society)
    4. Score society (depends on facts from above)
    5. Update freshness
    """
```

This replaces ad-hoc skill runs. One command: `python3 -m pipeline.orchestrate --entity society:prestige-lakeside-habitat`

Or batch: `python3 -m pipeline.orchestrate --type society --stale-only`

---

## 3. Scope

| Phase | What | Time |
|-------|------|------|
| **A** | Entity resolver + matching layer | 1 hour |
| **B** | RERA scraper skill (listing + detail + facts) | 2 hours |
| **C** | r/bangalore area intelligence (fetch + synthesize) | 1.5 hours |
| **D** | Pipeline orchestrator + freshness tracking | 30 min |

---

## 4. Files

### New
- `pipeline/entity_resolver.py` — canonical entity matching
- `pipeline/skills/fetch_rera.py` — RERA Karnataka scraper (replaces verify_rera.py)
- `pipeline/orchestrate.py` — pipeline orchestrator
- `data/knowledge/aliases.json` — manual entity aliases
- `data/knowledge/rera_index.json` — RERA ACK → entity ID mapping
- `data/knowledge/source_freshness.json` — per-entity source staleness

### Modified
- `pipeline/skills/learn_area.py` — enhanced with Reddit area threads + Claude synthesis
- `pipeline/skills/search_reddit.py` — add area-focused search queries + comment fetching

### Deleted
- `pipeline/skills/verify_rera.py` — replaced by fetch_rera.py (direct scraping > Claude hallucination)

---

## 5. What NOT to Build Today

- Frontend changes (that's Day 27)
- Rust backend changes (the graph already accepts facts via API)
- Full RERA listing import (9,469 projects) — we build the scraper, test on 5-10 projects
- r/bangalore full area sweep — test on 2-3 areas (Whitefield, Sarjapur, Hebbal)
- Database migration — files are fine for now
- Cron/scheduler — manual runs first, automate later

---

## 6. Success Criteria

- [ ] EntityResolver can match "SOBHA LIMITED" → "builder:sobha-limited" and "Sobha Insignia" → "society:sobha-insignia"
- [ ] RERA scraper extracts full project detail for Sobha Insignia: units, costs, complaints, timeline
- [ ] RERA facts stored in KG with confidence=1.0, source_type="Rera", proper display_templates
- [ ] `verify_rera.py` deleted — no more Claude-guessed RERA data
- [ ] r/bangalore threads fetched for Whitefield area with comment text
- [ ] Claude synthesizes area threads into 15+ structured facts per area
- [ ] Area facts have proper answers_preferences (e.g., waterlogging fact answers "flooding", "rain", "monsoon")
- [ ] source_freshness.json tracks when each entity was last enriched by each source
- [ ] `python3 -m pipeline.orchestrate --entity society:prestige-lakeside-habitat` runs end-to-end
- [ ] Entity resolver persists aliases and RERA mappings to disk

---

## 7. The Principle

The data pipeline is the moat. Not the frontend. Not the backend. Not the AI.

Every source we add makes the system smarter. Every search that triggers enrichment makes the next search better. But this only works if the pipeline is **tight**: clean entity identity, structured fact storage, source provenance, freshness tracking.

Today we stop treating data sources as islands and start treating them as a **coordinated intelligence network** with RERA as the backbone.
