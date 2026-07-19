# DAG Convergence Design

> **Status:** design draft — code convergence follows this doc  
> **Execution plan:** see [`dag_execution_plan.md`](./dag_execution_plan.md)  
> **Goal:** one source of truth for search, tiles, property page, and livability brief  
> **Principle:** DAG models and orchestrates; KG holds identity and relationships; serving bundle is the only runtime input

---

## 1. Why this doc exists

OpenEstates already has the right primitives:

- `SourcedFact` with provenance, confidence, `answers_preferences`, `scoring_hint`
- Asset DAG with dependencies, partitions, refresh cadence, trust tiers
- Serving bundle (`entities`, `facts`, `search_metadata`)
- KG with typed nodes and edges

What is **not** yet consistent:

- Asset definitions are hardcoded in Rust (`assets/registry.rs`)
- Legacy seed fields are hydrated with fake defaults (`noise_score = 0.5`) instead of sourced facts
- Serving bundle materializes mostly `society:*` and `builder:*`, not `property:*` or `area:*`
- Search/UI sometimes read different truth paths
- Crawl skip logic is ad hoc (`OPENESTATES_SKIP_REDDIT`, `force_refresh_assets`)

This design makes the **DAG fully config-driven** while keeping the **KG split** for entity identity and graph traversal.

---

## 2. Three layers (do not collapse them)

```text
┌─────────────────────────────────────────────────────────────┐
│  DAG (config + orchestration)                               │
│  - asset graph                                              │
│  - crawl/materialize schedule                               │
│  - source adapters                                          │
│  - skip/stale/freshness policy                              │
│  - bootstrap imports                                        │
└──────────────────────────┬──────────────────────────────────┘
                           │ produces materialized assets
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  KG (identity + relationships)                              │
│  - entity nodes: property, society, area, builder, place... │
│  - edges: in_society, in_area, built_by, served_by_road   │
│  - facts attach to entities; KG does not own a second truth │
└──────────────────────────┬──────────────────────────────────┘
                           │ promoted projection
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  Serving bundle (runtime only)                              │
│  - entities, facts, search_metadata, tantivy index          │
│  - Rust search, tiles, property page, livability brief    │
└─────────────────────────────────────────────────────────────┘
```

**Rule:** request-path code reads **only** the serving bundle. DAG config is never parsed at request time.

---

## 3. Config-driven DAG

All durable DAG behavior should live in versioned config under `data/dag/`.

Proposed layout:

```text
data/dag/
  asset_registry.json          # asset graph: deps, stage, refresh, cost, trust
  ontology.json                # entity types + allowed relations
  fact_registry.json           # canonical fact keys, display, search, resolution
  source_adapters/
    legacy_seed.json           # bootstrap current seed/listing data
    rera.json
    google_places.json
    reddit.json
    external_listings.json
    registered_transactions.json   # future — no DAG shape change
  crawl_policies/
    global.json                  # budgets, default cadence, disabled sources
    society_enrichment.json      # per-entity-type crawl selectors
  resolution_policies.json       # how conflicting facts are resolved
  bootstrap/
    entity_seeds.json            # canonical society/property seeds
    area_aliases.json
```

Rust/Python loaders validate config at DAG startup. Adding a new source or fact type should be a **config + skill** change, not a registry rewrite.

---

## 4. Asset registry config (replace hardcoded Rust)

Move `AssetDefinition` fields from `registry.rs` into `data/dag/asset_registry.json`.

Example:

```json
{
  "version": 1,
  "assets": [
    {
      "id": "google_places_weekly",
      "stage": "raw",
      "description": "Weekly Google place snapshot for societies with resolved place IDs.",
      "dependencies": ["canonical_society_nodes"],
      "optional_dependencies": [],
      "refresh": { "cadence": "weekly", "ttl_days": 7 },
      "cost_tier": "moderate",
      "trust_tier": "support",
      "partition_policy": { "kind": "composite", "coordinates": [{ "static": { "source": "google" } }] },
      "crawl_policy_ref": "google_places_weekly",
      "produces": {
        "entity_types": ["society", "place"],
        "fact_keys": ["google_place_id", "google_rating", "google_review_count"]
      }
    },
    {
      "id": "registered_transaction_facts",
      "stage": "silver",
      "description": "Registered sale/lease transaction observations.",
      "dependencies": ["canonical_property_nodes"],
      "optional_dependencies": ["rera_legal_facts"],
      "refresh": { "cadence": "monthly", "ttl_days": 45 },
      "cost_tier": "expensive",
      "trust_tier": "authoritative",
      "crawl_policy_ref": "registered_transactions",
      "produces": {
        "entity_types": ["property", "society"],
        "fact_keys": ["market.transaction.price_per_sqft", "market.transaction.date"]
      }
    }
  ]
}
```

**Wild combo support:** assets can declare:

- `optional_dependencies` — downstream assets still run if upstream is skipped
- `dependency_fan_in` — how partitions merge across upstreams
- `produces` — what facts/entities this asset claims to materialize (used for coverage + skip logic)
- `crawl_policy_ref` — link to crawl policy block

---

## 5. Crawl policy config (frequency, skip, budget)

Crawlers and the DAG executor consult **crawl policies** before doing work.

### 5.1 Policy fields

```json
{
  "policy_id": "google_places_weekly",
  "enabled": true,
  "cadence": "weekly",
  "ttl_days": 7,
  "cost_tier": "moderate",
  "max_entities_per_run": 500,
  "entity_selector": {
    "entity_type": "society",
    "where": {
      "has_fact": "google_place_id",
      "missing_fact": null,
      "root_source_in": ["rera", "discovered"],
      "priority_tier_in": ["A", "B"],
      "search_demand_score_gte": 0.1
    }
  },
  "skip_rules": [
    { "if": "upstream_disabled", "source": "reddit", "action": "skip_asset" },
    { "if": "materialization_fresh", "ttl_days": 7, "action": "skip_partition" },
    { "if": "entity_fresh", "fact_key": "google_rating", "max_age_days": 14, "action": "skip_entity" },
    { "if": "env_flag", "name": "OPENESTATES_SKIP_REDDIT", "value": "1", "action": "skip_asset" },
    { "if": "cost_budget_exceeded", "action": "defer_remaining" }
  ],
  "retry": { "max_attempts": 3, "initial_delay_ms": 100, "max_delay_ms": 2000 },
  "isolated_worker": null
}
```

Reddit isolated fetcher example:

```json
{
  "policy_id": "reddit_threads_daily",
  "enabled": false,
  "cadence": "daily",
  "isolated_worker": {
    "kind": "container",
    "reason": "egress_ip_block",
    "artifact_sink": "data/lake/assets/reddit_threads_daily/"
  },
  "skip_rules": [
    { "if": "env_flag", "name": "OPENESTATES_SKIP_REDDIT", "value": "1", "action": "skip_asset" }
  ]
}
```

### 5.2 Skip actions

| Action | Meaning |
|--------|---------|
| `skip_asset` | Do not run this asset at all this plan |
| `skip_partition` | Skip a partition because upstream output is still fresh |
| `skip_entity` | Skip one entity within an asset run |
| `defer_remaining` | Stop after budget; queue rest for next run |
| `emit_empty_inputs` | Keep DAG shape; write empty artifact (current Reddit skip mode) |

### 5.3 Who reads this

- **DAG planner** — builds run plan, marks skipped assets/partitions with reason
- **Python collectors** (`collect_asset_sources.py`) — receive resolved entity list + skip hints
- **Rust executor** — respects `force_assets`, watermarks, materialization freshness
- **Enrichment queue** — promotes high-demand missing facts into next run's `entity_selector`

This is how crawlers "skip some crawling from DAG" without hardcoded env checks scattered everywhere.

---

## 6. Ontology config (KG shape, config-seeded)

`data/dag/ontology.json` defines the stable world model.

```json
{
  "version": 1,
  "entity_types": [
    { "id": "property", "id_prefix": "property:", "description": "Listing/unit-level entity" },
    { "id": "society", "id_prefix": "society:", "description": "Project/gated community" },
    { "id": "area", "id_prefix": "area:", "description": "Micro-market / locality" },
    { "id": "builder", "id_prefix": "builder:", "description": "Developer/promoter" },
    { "id": "road_segment", "id_prefix": "road:", "description": "Approach/access road" },
    { "id": "place", "id_prefix": "place:", "description": "External place identity (Google etc.)" },
    { "id": "transaction", "id_prefix": "txn:", "description": "Optional future entity for sale records" }
  ],
  "relations": [
    { "from": "property", "edge": "in_society", "to": "society" },
    { "from": "society", "edge": "in_area", "to": "area" },
    { "from": "society", "edge": "built_by", "to": "builder" },
    { "from": "society", "edge": "served_by_road", "to": "road_segment" },
    { "from": "society", "edge": "maps_to_place", "to": "place" },
    { "from": "transaction", "edge": "for_property", "to": "property" }
  ]
}
```

Adding `transaction` later = add entity type + relation in config. No Rust `NodeType` enum change required once loader is generic.

### Trie-like fact namespaces

Facts use dotted namespaces for stable addressing (not a separate storage engine):

```text
legal.rera_status
legal.rera_registered
market.price_per_sqft
market.transaction.price_per_sqft
market.transaction.date
operating.maintenance_quality
risk.waterlogging
risk.noise_level
access.metro_distance_mins
access.nearby_schools
community.positive_themes
community.concern_themes
lifecycle.home_state
lifecycle.home_age_bucket
```

Config, search registry, and livability theme registry all reference these canonical keys.

---

## 7. Fact registry config (search + UI + resolution)

Unify `fact_schema_registry.json`, livability themes, and future transaction facts into `data/dag/fact_registry.json` (or split files included by version manifest).

Per fact:

```json
{
  "fact_key": "risk.waterlogging",
  "value_types": ["text", "tags", "numeric"],
  "entity_types": ["society", "area", "road_segment"],
  "display_template": "Waterlogging signal: {value}",
  "answers_preferences": ["waterlogging risk", "flooding", "rajakaluve"],
  "scoring_hint": {
    "direction": "LowerIsBetter",
    "weight": 2.0,
    "thresholds": [0.2, 0.4]
  },
  "resolution": {
    "strategy": "prefer_highest_confidence",
    "source_priority": ["google", "reddit", "area_plan", "legacy_seed"],
    "never_default": true
  },
  "ui_surfaces": {
    "search": true,
    "tile_chip": true,
    "livability_brief_lens": "risk",
    "community_pulse": false
  }
}
```

**Critical rule:** `never_default: true` means missing stays missing. No `0.5` placeholders in `data_loader.rs`.

---

## 8. Source adapter config (bootstrap + future crawlers)

Each source adapter maps raw input → `SourcedFact` rows.

### 8.1 Legacy bootstrap (current data)

```json
{
  "source_id": "legacy_seed",
  "trust_tier": "legacy",
  "default_confidence": 0.25,
  "source_type": "LegacySeed",
  "maps": [
    {
      "input_path": "data/seed/properties/*.json",
      "entity_type": "property",
      "entity_id_field": "id",
      "fields": [
        { "from": "price_per_sqft", "fact_key": "market.price_per_sqft", "value_type": "numeric" },
        { "from": "floor", "fact_key": "listing.floor", "value_type": "numeric" },
        { "from": "facing", "fact_key": "listing.facing", "value_type": "text" }
      ]
    },
    {
      "input_path": "data/seed/societies/*.json",
      "entity_type": "society",
      "entity_id_field": "id",
      "fields": [
        { "from": "noise_score", "fact_key": "risk.noise_level", "value_type": "numeric", "internal_only": true },
        { "from": "waterlogging_risk_score", "fact_key": "risk.waterlogging", "value_type": "numeric", "internal_only": true },
        { "from": "society_quality_score", "fact_key": "operating.society_quality", "value_type": "numeric", "internal_only": true }
      ]
    }
  ]
}
```

`internal_only: true` = available for audit/backfill and low-confidence ranking fallback, but **not rendered** until superseded by source-backed facts.

### 8.2 Future transaction source (no DAG reshape)

```json
{
  "source_id": "registered_transactions",
  "trust_tier": "authoritative",
  "default_confidence": 0.9,
  "source_type": "RegisteredTransaction",
  "maps": [
    {
      "input_path": "data/lake/raw/transactions/{dt}/*.parquet",
      "entity_type": "property",
      "entity_id_field": "property_id",
      "fields": [
        { "from": "sale_price_per_sqft", "fact_key": "market.transaction.price_per_sqft", "value_type": "numeric" },
        { "from": "registration_date", "fact_key": "market.transaction.date", "value_type": "text" }
      ]
    }
  ]
}
```

Same asset DAG. New adapter file. New asset entry in `asset_registry.json`.

---

## 9. Resolution policy (correctness over completeness)

When multiple sources emit the same `fact_key` for one entity:

1. Filter by `resolution.source_priority`
2. Prefer higher `confidence`
3. Prefer newer `learned_at` within same source tier
4. Record `supersedes` lineage internally
5. Never synthesize a value

Resolution output is what gets written to serving bundle `facts` + `search_metadata`.

---

## 10. Serving projection contract

The serving bundle materializer reads resolved facts and emits:

| Table | Purpose |
|-------|---------|
| `entities` | All searchable entities: property, society, area, builder |
| `facts` | Resolved fact rows per entity |
| `search_metadata` | `answers_preferences`, `scoring_hint`, `display_template` |
| `entity_edges` | Optional denormalized edges for recall |
| `tantivy_index` | Text recall |

### Minimum entity coverage targets

| Entity | Required for |
|--------|----------------|
| `property:*` | listing price, BHK, floor, facing, photos, unit-level search |
| `society:*` | reviews, operating quality, RERA, amenities |
| `area:*` | market trend, infrastructure, externality context |
| `builder:*` | delivery record, revocations, project count |

Current gap: bundle has ~9.7k societies, ~6.5k builders, **0 properties**. Convergence must add `property:*` and `area:*` materialization.

### Property struct becomes a view

`backend/src/data_loader.rs` should hydrate `Property` from serving facts only:

```text
property.price_per_sqft  <- fact market.price_per_sqft (or missing)
property.noise_score     <- REMOVED; use risk.noise_level fact via brief/search
```

No `.unwrap_or(0.5)`.

---

## 11. Search, tiles, UI read the same resolved facts

| Surface | Reads |
|---------|-------|
| Search ranking | `search_metadata` + `facts` via self-describing `answers_preferences` |
| Result tiles | Short chips from resolved facts + match reasons |
| Livability brief | Theme registry + society/area/road facts |
| Community pulse | Review facts + community theme facts |
| Property action rail | Market activity + brief risk themes (not seed scores) |

Delete `legacy_preference_score` only after:

1. Bootstrap import provides parity for listing/lifecycle facts
2. Coverage report shows acceptable % for key preferences
3. `eval_search.py` shows no regression

---

## 12. DAG run flow (config-driven)

```text
1. Load data/dag/* config
2. Planner builds topological asset order from asset_registry.json
3. For each asset:
   a. Resolve crawl_policy
   b. Evaluate skip_rules (env, freshness, budget, upstream disabled)
   c. Build entity_selector → list of entity_ids to process
   d. Python collector / Rust materializer runs on that list only
   e. Write materialization record + watermarks
4. Resolver merges facts across sources per resolution_policies.json
5. Promote search_serving_bundle
6. Emit coverage + data-health artifacts
```

### Artifacts per run

- `manifests/assets/{asset_id}/.../materializations/{id}.json`
- `data/validation/coverage_report.json` — % entities with fact_key X by source
- `data/validation/enrichment_gaps.json` — queued missing facts from search demand

---

## 13. Convergence phases

### Phase 0 — Design lock (this doc)
- [ ] Review and approve config shapes
- [ ] Agree canonical fact namespaces
- [ ] Agree `never_default` policy

### Phase 1 — Config extraction (no behavior change)
- [ ] Export current `registry.rs` assets to `data/dag/asset_registry.json`
- [ ] Loader validates JSON against existing DAG tests
- [ ] Move Reddit skip to crawl policy config

### Phase 2 — Bootstrap import
- [ ] `legacy_seed` source adapter imports current seed/listing data as low-confidence facts
- [ ] Stop fake defaults in `data_loader.rs`
- [ ] Coverage report artifact

### Phase 3 — Entity expansion
- [ ] Materialize `property:*` and `area:*` in serving bundle
- [ ] Link properties → societies → areas in KG edges
- [ ] Update tiles/search to use property + society facts

### Phase 4 — UI truth consolidation
- [ ] Remove seed-derived risk list from property page
- [ ] De-duplicate theme chips (brief vs community pulse)
- [ ] Drive decision verdict from brief + market + RERA only

### Phase 5 — Search cleanup
- [ ] Delete `legacy_preference_score` after eval parity
- [ ] Enrichment queue feeds crawl `entity_selector`

### Phase 6 — Fresh crawler
- [ ] New sources plug in via `source_adapters/*.json` + skills
- [ ] Re-ingest replaces bootstrap facts by resolution policy

---

## 14. Example: how a crawl skips work

Query: "Should `google_places_weekly` run for `society:prestige-lakeside-habitat`?"

```text
1. Asset enabled? yes
2. Env skip? no
3. Partition materialization fresh (< 7 days)? yes → skip_asset unless force_refresh
4. For entity:
   - google_rating learned_at = 3 days ago, ttl = 14 → skip_entity
5. Result: asset skipped entirely OR entity omitted from collector input list
```

Crawler receives:

```json
{
  "asset_id": "google_places_weekly",
  "entities": [],
  "skip_reason": "all_entities_fresh"
}
```

No Google API calls. DAG stays consistent.

---

## 15. Non-goals

- LLM calls on `/api/search` request path
- Request-time DAG config parsing
- Multiple concurrent promoted serving bundles
- Perfect data before shipping UI — but **no fake data**

---

## 16. Open questions

1. **Single `fact_registry.json` vs split files?** Start split by domain (`market.json`, `risk.json`, `legal.json`) with a manifest.
2. **Transaction entity vs property facts?** Prefer property-attached facts first; add `transaction:*` entity only if we need many-to-one history.
3. **Area node seeding:** from RERA locality + manual alias table, or geospatial clustering?
4. **Isolated Reddit worker:** Fargate/EC2 container writing to `data/lake` — already planned; wire via `isolated_worker` in crawl policy.

---

## 17. Summary

| Layer | Role | Config-driven? |
|-------|------|----------------|
| DAG | Orchestrate crawls, materialize assets, bootstrap, skip policy | **Yes — target state** |
| KG | Entity identity, relationships, fact attachment | Ontology from config |
| Serving bundle | Runtime truth for search/UI | Projected from resolved facts |
| Skills | Source-specific extraction | Declared in source adapter config |
| Rust search/UI | Read serving bundle only | No hardcoded preference match arms |

The DAG becomes powerful because it is not just "a pipeline graph" — it is the **control plane** for what to crawl, when to skip, what facts exist, how they resolve, and what the product is allowed to show.
