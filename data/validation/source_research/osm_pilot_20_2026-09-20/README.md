# OSM 20-society pilot

This pilot refreshed OSM observations for 19 societies in catalog revision 9 and evaluated Brigade Orchards as a catalog candidate. It queried the `maps.mail.ru` Overpass instance sequentially with a 30-second request-start interval. Large boundaries were split into bounded polygon tiles.

These are validation results, not active serving data. The active catalog remains revision 9 with 71 societies. Existing immutable OSM artifacts were not deleted; a promoted refresh should supersede their owned contributions after the structure observations are wired into the catalog assembly path.

## Coverage

| Measure | Result |
|---|---:|
| Societies | 20 |
| Current catalog members | 19 |
| Boundary area | 341.6 acres |
| OSM gate/entrance associations | 11 |
| Named approach roads | 2 |
| Structure tiles completed | 35 / 35 |
| Societies with complete structure coverage | 20 / 20 |
| Building footprints | 468 |
| Footprints with mapped levels | 209 |

`Building footprints` are OSM building geometries inside the society boundary. They are not tower counts; RERA remains authoritative for project phases and towers. Mapped levels remain rendering-only OSM observations and are not search or buyer-facing tower facts. An `inferred` entrance is an explicit OSM gate or entrance node associated by distance with both the society boundary and a public road. It is not a synthetic point.

## Society results

| Society | Catalog | OSM boundary | Acres | Entrance | Approach road | Structures | Tiles | Footprints | With levels | Max levels |
|---|:---:|---|---:|---|---|---|---:|---:|---:|---:|
| Amrutha Platinum Towers | yes | way/1144036211 | 2.6 | — | — | complete | 1/1 | 2 | 2 | 10 |
| Brigade Lakefront - Crimson | yes | way/229628369 | 20.0 | inferred | — | complete | 2/2 | 6 | 5 | 15 |
| Brigade Woods | yes | way/765656003 | 6.4 | inferred | — | complete | 1/1 | 3 | 3 | 5 |
| Desai Grandeur | yes | way/1239793107 | 1.7 | inferred | — | complete after resume | 1/1 | 2 | 0 | — |
| DESAI RADIANT | yes | way/1239793104 | 1.3 | — | — | complete | 1/1 | 1 | 0 | — |
| DISHA COURTYARD | yes | way/773145599 | 4.4 | inferred | Public road | complete | 1/1 | 2 | 0 | — |
| Godrej Air | yes | way/792488587 | 6.2 | — | — | complete | 1/1 | 10 | 9 | 17 |
| Godrej Splendour | yes | way/1132098866 | 18.1 | — | — | complete | 2/2 | 2 | 0 | — |
| Godrej United | yes | way/140235891 | 7.4 | — | — | complete | 1/1 | 2 | 2 | 17 |
| Gopalan Aqua | yes | way/773145595 | 4.7 | inferred | — | complete | 1/1 | 2 | 2 | 15 |
| Habitat Eden Heights | yes | way/353236629 | 4.5 | inferred | Muniswamy Shetty Layout Road | complete | 1/1 | 4 | 4 | 19 |
| MAHAVEER PROMENADE | yes | way/1132098870 | 1.1 | inferred | — | complete | 1/1 | 1 | 1 | 4 |
| MAHAVEER TRANQUIL | yes | way/185975845 | 2.6 | inferred | — | complete | 1/1 | 2 | 0 | — |
| NARYA 5 ELEMENTS | yes | way/1304106633 | 1.6 | inferred | — | complete empty | 1/1 | 0 | 0 | — |
| Parimala Skyview | yes | way/1134272289 | 0.9 | — | — | complete | 1/1 | 4 | 3 | 14 |
| Pavani Divine | yes | way/886184260 | 2.7 | — | — | complete | 1/1 | 3 | 0 | — |
| PRESTIGE BOULEVARD | yes | way/1080943278 | 2.7 | inferred | — | complete | 1/1 | 3 | 2 | 5 |
| PRESTIGE FONTAINE BLEAU | yes | way/1080943279 | 1.0 | inferred | — | complete | 1/1 | 3 | 2 | 11 |
| Prestige Lakeside Habitat | yes | way/266310354 | 103.6 | — | — | complete | 8/8 | 298 | 78 | 30 |
| Brigade Orchards | no | way/843854047 | 148.1 | — | — | complete | 7/7 | 118 | 96 | 7 |

The successful empty result for NARYA 5 ELEMENTS is reusable for this boundary and query. Desai Grandeur's first attempt ended in an HTTP 504; resuming the same checkpoint reused the other 19 society results and successfully collected its missing tile. The resolved failure remains in `failures.json` for diagnosis.
