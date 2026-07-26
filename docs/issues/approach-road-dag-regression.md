# Issue: Approach-road DAG regression

**Status:** Open
**Created:** 2026-07-23
**Example:** `discovered-sumadhura-capitol-residences-3bhk`

## Problem

The property UI still knows how to render `approach_road`, but the fresh serving
bundle lost the deterministic DAG path that materializes road-segment facts. As a
result, approach-road evidence appears only when Google review snippets happen to
contain phrases such as `approach road`, `access road`, or `wide road`.

That makes the surface fragile. Sumadhura Capitol Residences has RERA, Google,
nearby, and listing facts in the bundle, but no `approach_road` section because it
has no review-snippet road match and no `served_by_road` edge.

## Contract

Approach road must be a DAG-backed graph surface:

- `society:* -> served_by_road -> road_segment:*`
- road-segment facts such as `access_road_quality`, `road_width`,
  `risk.approach_road_waterlogging`, and `media.approach_road_frames`
- optional society-level review bridge `approach_road_condition`

Review snippets are useful support evidence, but they must not be the only path.

## Fix Plan

1. Restore a deterministic `approach_road_graph_facts` asset.
2. Derive road-segment edges from upstream DAG facts such as RERA address/geo
   and Google place context; do not maintain broad per-society seed rows.
3. Fan the asset into `kg_society_view` before `search_serving_bundle`.
4. Add a contract test proving Sumadhura has an `approach_road` section after DAG
   materialization.
5. Rerun the DAG with `approach_road_graph_facts`, `kg_society_view`, and
   `search_serving_bundle` forced.

## Source Boundary

The asset writes Parquet outputs. It should not use validation JSON as durable
truth. Richer approach-road facts such as road width, traffic, waterlogging, and
Street View frames should come from road-specific Google Maps/Street View
enrichment upstream, then flow through the same Parquet fact path.

## Acceptance

- Sumadhura detail evidence includes `approach_road`.
- The serving bundle contains a `served_by_road` edge for Sumadhura.
- The UI shows approach-road evidence without relying on incidental review text.
- No raw data gaps or invented road-quality claims are shown to users.
