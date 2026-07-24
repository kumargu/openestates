# Environmental Intelligence DAG Plan

Status: draft plan for review before implementation.

## Objective

Build environmental intelligence as first-class OpenEstates DAG facts, starting
with water and terrain context around societies and areas. The product goal is
not to show raw government maps as-is. The goal is to turn authoritative and
resident-backed evidence into scoped, inspectable facts:

- what the society coordinate intersects
- what exists within a nearby radius
- what the wider area or administrative unit reports
- what residents actually experience
- what the evidence can and cannot prove

This keeps OpenEstates different from listing sites: every claim should have a
source, a scope, an age, and a confidence.

## Reviewed Direction

The overall shape makes sense, with one important constraint:

> Join source datasets offline during DAG materialization, not at request time.

The runtime API should never load KML, parse GIS files, call public portals, or
perform heavy spatial joins while a buyer is waiting. It should read promoted
serving-bundle facts that were already joined, scoped, and resolved by the DAG.

## Layering

Use three layers for each source family.

```text
raw source snapshot
  -> normalized Parquet features
  -> derived OpenEstates facts
  -> kg_society_view
  -> search_serving_bundle
  -> Rust API
```

### 1. Raw source snapshot

Store source-native files or API responses exactly as fetched.

Examples:

```text
data/lake/raw/source=opencity/dataset=groundwater_potential/run_id=.../source.kml
data/lake/raw/source=opencity/dataset=stormwater_drains/run_id=.../primary.kml
data/lake/raw/source=opencity/dataset=flood_locations/run_id=.../locations.kml
```

Raw snapshots preserve auditability and let us re-normalize when schemas improve.

### 2. Normalized feature Parquet

Parse raw datasets into typed feature tables. Do not force all environmental
features into one giant table; polygons, line strings, points, reports, and text
evidence have different shapes.

Examples:

```text
data/lake/silver/environment_groundwater_potential_zones/version=.../features.parquet
data/lake/silver/environment_stormwater_drain_segments/version=.../features.parquet
data/lake/silver/environment_flood_locations/version=.../features.parquet
data/lake/silver/environment_lake_wetland_boundaries/version=.../features.parquet
```

Each normalized row should preserve:

- stable feature id
- geometry or coordinate representation
- source fields
- source dataset id
- source URL
- source organization
- license
- publication or observed date when available
- downloaded/materialized timestamp

### 3. Derived OpenEstates facts

Join normalized features to OpenEstates entities offline and write fact rows.

Examples:

```text
data/lake/gold/society_environment_facts/version=.../facts.parquet
data/lake/gold/area_environment_facts/version=.../facts.parquet
```

These facts are what the API and UI should consume.

## First Vertical Slice

Start with groundwater potential because we already found a reachable KML source
and the join is clear.

```text
raw_environment_groundwater_potential
  -> normalized_groundwater_potential_zones
  -> society_groundwater_potential_facts
  -> kg_society_view
  -> search_serving_bundle
```

The first fact can be:

```text
environment.groundwater_potential_class
```

The source KML has polygon records with fields like:

```text
GW_CODE
GW_PROS
```

For a society latitude/longitude, the DAG should perform point-in-polygon and
emit a scoped fact:

```json
{
  "entity_id": "society:example",
  "fact_key": "environment.groundwater_potential_class",
  "value": "Moderate",
  "scope": "society_coordinate",
  "match_method": "point_in_polygon",
  "source_field": "GW_PROS",
  "confidence": 0.85
}
```

Nearby boundary context can be added after the basic join works:

```text
environment.groundwater_potential_nearby_mix
```

## Area Mapping

For areas, use two levels.

### Simple first pass

Use the area centroid to produce a coarse area fact.

This is acceptable for fast exploration, but it must be labeled as centroid
based and should not pretend to describe the full area.

### Better pass

Overlay the area boundary with source polygons and calculate class distribution.

Example:

```json
{
  "entity_id": "area:whitefield",
  "fact_key": "environment.groundwater_potential_mix",
  "value": {
    "Moderate": 0.62,
    "Good": 0.25,
    "Water Body Mask": 0.13
  },
  "scope": "area_boundary",
  "match_method": "polygon_overlay"
}
```

This should power Area Tracker and area-level proof views.

## Initial Asset Families

### Water stress and supply

- groundwater potential zones
- groundwater stress or extraction regions
- historical groundwater depth observations
- resident water evidence
- tanker dependence
- Cauvery/BWSSB water source evidence

### Resilience

- rainwater harvesting presence
- rainwater harvesting quality
- recharge pits or recharge infrastructure
- STP reuse
- society-level water storage/reuse evidence

Keep resilience separate from stress. A water-stressed area can still have a
society with strong rainwater harvesting, recharge, BWSSB supply, or low tanker
complaints.

### Drainage and flooding

- stormwater drain proximity
- rajakaluve proximity
- flood-prone points or polygons
- underpass or road flooding reports
- lake and wetland proximity

### Transit and civic updates

The same pattern can later handle metro construction updates, road upgrades,
major civic works, and area-level change signals:

```text
raw_metro_construction_updates
  -> normalized_transit_project_updates
  -> area_transit_change_facts
```

## Dataset Registry

Create a dataset registry as the single catalog, not a single universal Parquet
file for all environmental data.

Example:

```json
{
  "dataset_id": "bengaluru_groundwater_potential",
  "theme": "groundwater_potential",
  "source": "KSRSAC via OpenCity",
  "format": "KML",
  "cadence": "monthly",
  "license": "Public Domain",
  "raw_asset": "raw_environment_groundwater_potential",
  "normalized_asset": "normalized_groundwater_potential_zones"
}
```

The registry should track source organization, license, attribution
requirements, commercial-use concerns, coverage, expected freshness, and update
cadence.

## Cadence

Default cadences should reflect how quickly each signal changes.

| Theme | Suggested cadence | Reason |
|-------|-------------------|--------|
| Groundwater potential geology | monthly or quarterly check | Source changes slowly; access can be revalidated monthly. |
| Groundwater stress reports | monthly check | New reports are periodic and should be picked up. |
| Stormwater drains | monthly check | Infrastructure maps may update, but not daily. |
| Flood/waterlogging incidents | weekly or event-driven | Incidents and news can change during monsoon. |
| Resident themes | daily/weekly | Reviews and Reddit themes are dynamic. |
| Metro/civic construction | weekly | Updates can affect area desirability quickly. |

## Evidence Contract

Every derived fact should preserve:

- source dataset id
- source organization
- source URL
- source date or observed date
- fetched/materialized date
- license
- entity id
- fact key
- value
- scope, such as `society_coordinate`, `radius_250m`, `radius_1km`, `area_boundary`, or `admin_region`
- match method, such as `point_in_polygon`, `nearest_feature`, `buffer_intersection`, or `polygon_overlay`
- distance or coverage share where applicable
- confidence
- caveat text for internal/API use

This supports the UI pattern:

```text
Claim -> source -> radius/scope -> age -> what we can/cannot conclude
```

## Backend/API Shape

The request path should only assemble structured views from serving-bundle facts.

Candidate buyer endpoint:

```text
GET /api/societies/{society_id}/environment
```

Candidate drilldown endpoint:

```text
GET /api/societies/{society_id}/environment/evidence?fact_key=...
```

Candidate admin endpoint:

```text
GET /api/admin/environment-coverage
```

Do not create separate runtime APIs for each source dataset. The API surface
should be about buyer questions and evidence groups, not source plumbing.

## Non-Goals For The First Pass

- No universal water score.
- No request-time GIS parsing.
- No live calls to OpenCity, BWSSB, CGWB, Bhuvan, Reddit, Google, or other
  sources from buyer APIs.
- No raw source documents shown as confident conclusions without scoped facts.
- No single all-purpose environmental feature Parquet table.

## Review Checklist Before Implementation

- Does each UI-visible claim have a DAG fact behind it?
- Is the source join done offline?
- Is the scope explicit enough for a buyer to understand?
- Is area-level stress separate from society-level resilience?
- Are licensing and attribution captured before product use?
- Can the same pattern handle groundwater, drains, floods, lakes, metro updates,
  and resident themes without new request-path logic?
