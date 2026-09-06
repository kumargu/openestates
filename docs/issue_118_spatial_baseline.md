# Issue 118 spatial-search baseline

Recorded 2026-09-06 for `feat/issue-118-spatial-intent`.

## Inspected runtime artifacts

- Current asset materialization pointer: `data/lake/manifests/assets/search_serving_bundle/partition=global/current.json`
- Inspected bundle: `data/lake/serving/search_bundle/version=search-proximity-category-v7-2026-09-04`
- Development catalog release used for API contracts: `waterford-osm-arrival-2026-08-31-release`

Direct Parquet inspection of the current asset materialization found no promoted
locality topology: zero `area` entities, zero `in_area` edges, and zero
`adjacent_area` edges. It contains 68 `geo.geometry_geojson` facts, all of which
are `LineString` geometries. It is therefore not a spatial-locality candidate
bundle and must not be described or promoted as one.

The development catalog release remains useful for ordinary API regression
tests, but it is not evidence that the Issue 118 locality asset has been
promoted. Generated spatial candidates must be inspected for entities, facts,
edges, search metadata, and `diagnostics/spatial_topology_gaps.json` before any
promotion.

## Baseline quality and gaps

- Existing controlled conversational search scenarios pass with stable ordered
  result IDs and proof contracts.
- Recommendation live audit: 103 anchors, 251 recommendations, 53 ms p95.
- Recommendation coordinate coverage is 88.4%, below the existing 90% gate.
  This is a pre-existing `data_gap`; the gate is intentionally unchanged.
- Locality polygons/topology missing from the promoted search materialization
  are a `data_gap`, not a ranking or parser issue.
- The controlled suite exercises three-, eight-, and sixteen-branch cohorts for
  disconnected-scope retention, branch-local result isolation, and duplicate
  prevention. The sixteen-branch variant remains a stress cohort rather than
  the product default; no live buyer-quality conclusion is claimed until a
  version-pinned spatial candidate bundle exists. The active limit remains
  eight branches and twelve revisions.

No missing geometry, ambiguous overlap, containment, adjacency, or proximity
relationship may be inferred on the request path. Missing or ambiguous evidence
must remain an internal enrichment gap.

Revision correlation IDs are authenticated with a server-side HMAC key. Set
`OPENESTATES_REVISION_SIGNING_KEY` to the same secret on every API replica and
keep it stable across restarts; local development uses an ephemeral process key.
