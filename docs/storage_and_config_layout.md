# Storage & Config Layout

> **Status:** canonical reference (Jul 2026)  
> **Principle:** `app/config/` = behavior schemas in Git. `data/lake/` = enriched facts in Parquet. Same keys local and S3.

---

## 1. Two trees, one contract

```text
app/config/          Git — what the system *can* do
  dag/                 leaves, ontology, assets, enrichment, UI
  bootstrap/           import policies (no instances)
  lake/                parquet layout + S3 sync rules
  runtime/             env defaults

data/lake/             Parquet — what the system *knows*
  raw/ → silver/ → gold/ → serving/
  manifests/           promotion pointers (small JSON)
```

| Question | Answer |
|----------|--------|
| Where is "waterlogging" defined? | `app/config/dag/concern_taxonomy.json` + `fact_registry.json` |
| Where is "ECC Road floods in monsoon" stored? | `data/lake/.../facts/part-00000.parquet` on `road:*` entity |
| Where is Prestige Waterford → ECC Road edge? | `data/lake/gold/.../edges/part-00000.parquet` |
| What does search read at request time? | `data/lake/serving/search_bundle/version=*/` (via `current.json` pointer) |

**Never** put society names, road names, or fact values in `app/config/`.

---

## 2. `app/config/` complete map

Read `app/config/manifest.json` first.

### `app/config/dag/` — control plane

| File | Role |
|------|------|
| `manifest.json` | DAG package index, proof labels, agent routing |
| `ontology.json` | **Node types** + **edge types** (schema only) |
| `concern_taxonomy.json` | **Leaf definitions** (78 buckets) — fact_key, scopes, terms |
| `fact_registry.json` | Leaf **search semantics** — answers_preferences, scoring_hint |
| `resolution_policies.json` | Source tiers, confidence caps, conflict resolution |
| `asset_registry.json` | Crawl/enrich/materialize **asset DAG** |
| `enrichment_targets.json` | Leaf/surface-scoped **re-enrichment plans** |
| `ui_surfaces.json` | Buyer UI surface → leaf set mapping |
| `crawl_policies/*.json` | Per-source skip/cadence/isolated worker |
| `source_adapters/` | Per-source adapter contracts (pending) |

### `app/config/bootstrap/` — import policy

| File | Role |
|------|------|
| `policy.json` | Importer output → lake, confidence caps, source priority |
| `edge_inference.json` | How `served_by_road`, `in_area`, etc. are inferred |

### `app/config/lake/` — data layout

| File | Role |
|------|------|
| `layout.json` | Zones, key patterns, parquet tables per zone |
| `sync.json` | S3/local sync, env vars, never-sync paths |

### `app/config/runtime/`

| File | Role |
|------|------|
| `defaults.json` | Env var names, local path defaults, request-path rules |

---

## 3. `data/lake/` zones

All paths are **relative to lake root** (`OPENESTATES_LAKE_URL`). Implemented in `backend/src/assets/paths.rs`.

### Raw — immutable crawl snapshots

```text
raw/source={source}/{partition}/run_id={run_id}/{file}.parquet
```

### Silver — normalized facts per skill

```text
silver/{asset_id}/source={source}/dt={dt}/run_id={run_id}/manifest.json
silver/facts/entity_type={type}/fact_key={key}/source={source}/...
```

### Gold — KG view (merged graph)

```text
gold/kg_society_view/version={version}/
  entities/part-00000.parquet      # nodes
  edges/part-00000.parquet       # typed edges
  facts/part-00000.parquet       # leaf instances
  fact_annotations/part-00000.parquet
```

### Serving — runtime bundle (only API input)

```text
serving/search_bundle/version={version}/
  manifest.json
  schema.json
  entities/part-00000.parquet
  facts/part-00000.parquet
  search_metadata/part-00000.parquet
  tantivy_index/
```

### Manifests — promotion pointers (JSON only)

```text
manifests/assets/search_serving_bundle/partition=global/current.json
  → { "version": "...", "materialization_id": "..." }
```

---

## 4. Nodes, edges, leaves — where each lives

| Concept | Config (schema) | Data (instances) |
|---------|-----------------|------------------|
| **Node type** | `ontology.json` → `society`, `road_segment` | — |
| **Node instance** | — | `gold/.../entities.parquet` → `society:prestige-waterford` |
| **Edge type** | `ontology.json` → `served_by_road` | — |
| **Edge instance** | — | `gold/.../edges.parquet` → society → road |
| **Leaf definition** | `concern_taxonomy.json` + `fact_registry.json` | — |
| **Leaf instance** | — | `gold/.../facts.parquet` → `risk.approach_road_waterlogging` on `road:*` |

---

## 5. S3 sync

### Runtime (preferred)

```bash
export OPENESTATES_LAKE_URL=s3://openestates-prod/lake
export AWS_REGION=ap-south-1
# backend uses object_store — same LakeKey paths, no code changes
```

### Bulk migration / dev seed

```bash
# Push local lake to S3
aws s3 sync data/lake/ s3://openestates-prod/lake/

# Pull production lake to local
aws s3 sync s3://openestates-prod/lake/ data/lake/
```

### Never sync

- `data/cache/` — rebuildable
- `app/config/` — Git only

### Promotion flow across sync

1. DAG run materializes `serving/search_bundle/version=new/`
2. Updates `manifests/.../current.json` pointer
3. Sync pushes new version folder + updated pointer
4. API restart (or hot reload) reads new bundle

---

## 6. Legacy `data/` paths (migrate away)

| Path | Fate |
|------|------|
| `data/search/fact_schema_registry.json` | Merged into `app/config/dag/fact_registry.json` |
| `data/product/livability_theme_registry.json` | Merged into `concern_taxonomy.json` |
| `data/product/approach_road_visuals.json` | Gate coords → lake facts; policy in bootstrap |
| `data/knowledge/` | Delete when fully in gold KG |

---

## 7. Regenerate merged DAG config

```bash
python3.10 pipeline/tools/build_dag_json.py
```

---

## 8. Mental model

```text
Config  = document DB schema  (app/config/)
Lake    = document DB data    (data/lake/ Parquet)
S3      = same lake, remote disk
Search  = reads promoted serving snapshot only
```

Adding a buyer concern: edit `concern_taxonomy.json` → rebuild `fact_registry.json` → re-enrich → new serving version → sync.
