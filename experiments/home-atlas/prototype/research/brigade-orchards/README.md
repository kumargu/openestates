# Brigade Orchards spatial research

This folder is a data checkpoint, not a Brigade UI implementation. It records the current OpenStreetMap geometry needed to test whether the Waterford atlas foundation can support a large township walkthrough later.

## What the OSM boundary contains

- Boundary: [`way/843854047`](https://www.openstreetmap.org/way/843854047), 121 coordinates
- Bounding-box midpoint: `13.2300955, 77.7210859`
- Extent: about `1,741 m` north–south by `841 m` east–west
- Polygon area: about `598,059 m²` / `147.8 acres`
- Polygon perimeter: about `7,790 m`
- Inventory: 202 clipped features, including 61 road ways, 117 building footprints, 10 residential precincts, and 13 amenities

The OSM polygon and the developer's marketed township acreage are different measurements. Treat this polygon as community-mapped context, not a legal parcel boundary.

## What is already useful for a tour

The map has a strong north–south structure. A named spinal road provides a natural tour backbone, with named internal roads branching into precincts. OSM also maps large sub-areas such as Aspen, Cedar, Deodar, Ivory, Juniper, Kino, Pavilion Villas, Neem Grove, and Parkside Retirement Homes. This is enough for a staged aerial township journey without depending on tower names.

Individual villa/building footprints are dense and repetitive. They should form visual texture at close zoom; they should not all become labels.

## Fit with the Waterford foundation

| Foundation concept | Brigade fit | Later extension |
|---|---|---|
| Point, boundary, and segment geometry | Fits unchanged | None |
| OSM evidence and source links | Fits unchanged | Preserve timestamps and ODbL attribution |
| Feature and group cameras | Fits for precinct focus | Add a scale-aware township overview and corridor framing |
| Standalone road segments | Imports cleanly | Build a connected internal road graph for a continuous tour |
| Flat nearby categories | Too flat for this site | Add `site → precinct → feature` hierarchy |
| Current labels | Too dense at 117 footprints | Add semantic zoom, clustering, and collision priorities |
| Waterford scene recipes | Reusable camera primitives | Generate scenes from a tour itinerary instead of fixed feature IDs |

The important result: Brigade does **not** require a new renderer. It needs a richer spatial document and tour planner above the renderer—nested precincts, a connected route, staged cameras, and density-aware labels.

An aerial 3D township tour is supported by this dataset. A true eye-level continuous walk is a separate evidence problem: it depends on usable Street View coverage or authorized internal panoramas, which this research does not claim.

## Files

- `../../web/brigade/inventory.json` — canonical prototype snapshot with normalized geometry, tags, provenance, and summary
- `osm-preview.svg` — research-only view of the extracted geometry
- `collect.mjs` — reproducible OSM/Overpass collector with local polygon clipping
- `render-preview.mjs` — deterministic SVG renderer for visual inspection

Refresh with:

```bash
node research/brigade-orchards/collect.mjs
node research/brigade-orchards/render-preview.mjs
```
