# Map overlay seeds

Offline geometry used by the property “Around this home” plate.

| File | Theme | Clip radius |
|------|-------|-------------|
| `bengaluru_metro_lines.geojson` | Namma Metro ways/relations | 15 km |
| `bengaluru_parks.geojson` | OSM parks / recreation grounds | 4 km |
| `bengaluru_lakes.geojson` | OSM lakes / reservoirs | 4 km |

Source: OpenStreetMap via Overpass. Loaded once at API startup from
`data/seed/map/` and clipped per property — never fetched on the request path.

Refresh later by re-querying Overpass and replacing these files, then promote
through a proper DAG asset when city coverage expands beyond Bengaluru.
