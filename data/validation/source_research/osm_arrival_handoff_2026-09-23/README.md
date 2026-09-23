# OSM arrival handoff check

This check follows the 20-society comparison in
`../osm_pilot_20_2026-09-20/README.md`, where 17 societies had explicit OSM
gates but only four produced an entrance-connected approach corridor.

For Brigade Woods, a bounded 200 m Overpass query around OSM gate
`node/11121619834` returned public residential way `123168401`, named
`G. R. Tech Park Road`. The gate is at `12.984046, 77.7429562`. Its nearest
point on the exact OSM road geometry is `12.984084189987005,
77.74296963937813`, a separation of 4.47 m.
The complete bounded response is retained in `overpass_gate_sample.json`.

The earlier collector required a road-to-gate separation of at most 1 m, so it
kept the gate but discarded the road. Google Street View could render a
panorama at the gate while the aerial renderer received no route.

The generic rule is to retain a legal public-road path when the same explicit
gate has already passed the configured 30 m road-association check. The path
ends at the nearest point on the sourced road geometry. It does not add a line
from the road to the gate, infer a driveway, or treat Google panorama state as
durable geometry. Both renderers can therefore follow the same OSM road and
orient toward the same OSM gate while preserving the visible topology gap.

No ThreeUI catalog component provides a directly relevant sourced-road camera
handoff. The interaction stays within the existing OpenEstates road-flight and
Street View playback controls, including pause, resume, touch cancellation,
and reduced motion.
