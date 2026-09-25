# Parquet Compaction Management

OpenEstates treats Parquet files as immutable lake artifacts. Seller inputs,
manual corrections, RERA refreshes, and source-specific discoveries should append
new fact assertion rows instead of editing old Parquet files in place.

## Fact Delta Shape

Use row-based assertions for sparse or mutable facts:

- `entity_id`
- `fact_key`
- `value_type`
- typed value columns such as `value_text`, `value_number`, `value_bool`
- `source_type`
- `source_id`
- `confidence`
- `learned_at`
- `schema_version`

This shape lets a seller add `oc_approved`, RERA add `clubhouse_area_sqm`, or a
crawler add `ev_parking_count` without adding mostly-null columns to every
project row.

## Compaction Trigger

Compaction is controlled by `app/config/dag/compaction_policies.json`.

Run compaction when any configured threshold trips:

- too many small files in a partition
- too many new delta rows
- too many delta bytes
- delta files are older than the maximum configured age
- before a serving bundle promotion that needs current facts

The DAG runner should evaluate these thresholds after any asset writes sparse
project facts, including seller submissions, manual corrections, RERA refreshes,
and focused price/rent crawls. If compaction succeeds and the policy says
`trigger_after_success`, the next serving-bundle build should read the compacted
gold/current dataset.

## Why It Helps Search

Search should read compact current facts, not the full assertion history.
Compaction improves:

- backend startup time by reducing files and row groups loaded into memory
- search latency by keeping serving facts narrow and current
- evidence rendering by resolving duplicate/conflicting claims offline
- future S3 behavior by reducing small-object listing and fetch overhead

Silver delta datasets keep the audit trail. Gold/current datasets keep the fast
serving shape.

## DAG Materializer

The normal society DAG writes:

```text
data/lake/gold/society_fact_snapshot/version=<timestamp>/facts/<policy file_name_template>
data/lake/gold/society_fact_snapshot/version=<timestamp>/fact_annotations/<policy file_name_template>
```

The `facts/` file is the compacted current fact Parquet. The part filename comes
from `app/config/dag/compaction_policies.json`, and defaults to
`part-00000.parquet` when no template is configured. `manifest.json` stays next
to the Parquet outputs so the society gold snapshot pins the exact compacted
inputs used by catalog assembly. There is no separate compactor command or
serving promotion step.

Compaction resolves equal-scope conflicts by configured source priority and
confidence, then uses stable content/source identity as the deterministic
tie-break. Observation timestamps remain provenance and never decide product
meaning.

Graph-shaped assets such as `approach_road_graph_facts` are intentionally not
folded into `society_fact_snapshot`. They carry road-segment entities and edges
as well as facts, so the society gold snapshot keeps them as direct
dependencies while using `society_fact_snapshot` for row-shaped project
claims.

## Runtime Rule

Request handlers must not read silver deltas or per-society gold. The Rust API
reads only the one serving bundle selected by `manifests/catalog/dev.json`.
