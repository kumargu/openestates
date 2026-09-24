# Named-building township tour research

This research extends the 20-society OSM pilot with a closer look at large sites. It preserves raw OSM IDs, names, refs, coordinates, exact polygons, observed levels/heights, tile coverage, and failures. The production rule is generic: group footprints by a normalized sourced name and visit the combined group centroid. A missing name never becomes a label.

The broader sample and per-society coverage remain in [`../osm_pilot_20_2026-09-20/README.md`](../osm_pilot_20_2026-09-20/README.md). That pilot completed 35/35 structure tiles, including 298 Lakeside footprints and 118 Brigade Orchards footprints.

## Large-society findings

| Society | Boundary | Acres | Footprints in this capture | Distinct sourced labels | Labels |
|---|---|---:|---:|---:|---|
| Prestige Lakeside Habitat | `way/266310354` | 103.6 | 161 from 7 completed tiles; 1 tile timed out | 11 | Kiara, Jasper, Irwin, Giselle, Humbert, Figaro, Duncan, Elinor, Andrina, Basil, Calliope |
| Brigade Orchards | `way/843854047` | 148.1 | 118 from 7/7 completed tiles | 6 | Crysanthemum Villas, Tulip Villas, Carnation Villas, Signature Club Resort, Cedarian, Deodar |

The later Lakeside capture is intentionally marked incomplete because `way-266310354:r001:c000` returned HTTP 504. It is useful for name-pattern research but must not replace the complete pilot snapshot or become evidence of absence. The collector now quarters a dense failed tile within bounded depth and keeps the original failure in coverage diagnostics.

`Kino` does not appear in the captured OSM building records for Brigade Orchards. The product must not display it as OSM evidence. RERA remains authoritative for legal tower and phase identity; OSM levels and heights are rendering observations only and are excluded from search.

## Interaction note

The interaction borrows the spatial-tour idea from 3D Tiles viewers without copying a demo camera. Rest opens on the complete society boundary. Playback descends once, follows a short path between grouped sourced centroids, and shows one small label at the active stop. Pointer/touch input cancels automatic movement, existing pause/resume controls retain the current segment, and reduced-motion keeps the stable complete-boundary pose. Sites with fewer than two distinct sourced names retain the close orbit. Hover adds no decorative state, and tower facts are not duplicated in page chrome.
