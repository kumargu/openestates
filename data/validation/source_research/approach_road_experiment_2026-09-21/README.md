# Approach-road source experiment

This experiment compares saved Google place pins, validated OSM society polygons, explicit OSM gate nodes, and public OSM roads for the 20-society geometry pilot. It makes no product claim and publishes no serving facts.

## Findings

| Check | Result |
|---|---:|
| Societies with saved Google observations | 19 / 20 |
| Distinct Google place-coordinate variants | 27 |
| Google pins inside the OSM polygon | 23 / 27 |
| Google pins within 20 m of the polygon boundary | 13 / 27 |
| Google pins within 50 m of the polygon boundary | 22 / 27 |
| Polygon/gate/external-road pairs | 17 / 20 |
| Pairs with Google pin within 30 m of the OSM gate | 2 / 13 comparisons |
| Gate/road pairs forming an exact connected driving corridor | 4 / 20 |

A normal Google Place pin resolves the society but is not a reliable entrance. Of the saved observations that could be compared with an explicit OSM gate, six were more than 100 metres from that gate. Google Routes may terminate navigation at a vehicle entrance, but the bounded Routes API probe returned `403 PERMISSION_DENIED` because the API is disabled for the configured project. It was not enabled as part of this experiment.

The existing OSM algorithm chooses one frontage road before associating a gate. Pairing each explicit gate with any external public road within 30 metres of the polygon raised gate/road association coverage to 17 societies. Only four pairs produced a corridor whose OSM road geometry actually reaches the gate. The other 13 must not receive an invented connector.

## Product boundary

- Keep polygon geometry and building footprints for map rendering.
- Keep OSM building levels inside the rendering snapshot only; do not bind them to search or buyer-facing tower claims.
- Keep explicit OSM gates as optional entrance evidence.
- Emit an approach-road corridor only when stored road geometry connects to the explicit gate.
- Do not expose raw nearby-road or OSM-way counts.
- Do not treat a Google Place pin as an entrance.
- Revisit Google route termination only after Routes API access is intentionally enabled and its responses are captured as immutable source evidence.

`google_pin_gate_comparison.csv` preserves the Google place IDs, raw names, coordinates, polygon relationship, and gate distances. `osm_gate_road_pairing.csv` preserves the OSM boundary, gate, road IDs, distances, and corridor result.
