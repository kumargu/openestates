# Society geometry source research — 2026-09-20

This is a research artifact, not a promoted DAG input. It compares 29 societies
from the active Bengaluru roster and one historical-roster society before any
production crawler rule is changed.

## Sources and method

- OSM candidates came from Nominatim name searches on 2026-09-20. The search
  retained up to five candidates, then fetched the selected element's raw tags
  and version from the OSM API. Phase-neutral fallback queries were used only
  when the registered phase name returned no named candidate.
- RERA names, registrations, promoters, addresses, and coordinates came from
  immutable local Karnataka RERA raw runs. A coordinate is present only when a
  collected `rera_lat_lng` fact exists. RERA addresses were not geocoded.
- The active serving bundle was
  `catalog-71-ffb4dc50-117e-453c-b26f-41822430324e`. It contains no
  `rera_lat_lng` facts because the RERA materializer deliberately removes that
  untrusted raw field. Historical raw observations remain available for this
  comparison.
- Existing roster coordinates were used only to rank or sanity-check OSM
  candidates. They are not presented as RERA or OSM coordinates.

The compact side-by-side table is in `source_comparison.csv`. The raw OSM and
RERA responses were inspected from a local lake archive and are intentionally
excluded from Git. Their filenames and content hashes remain in
`../raw_capture_manifest.sha256` so the exact research inputs stay identifiable
without adding source payloads to this review.

## Observed pattern

| Observation | Count |
| --- | ---: |
| Societies sampled | 30 |
| Named OSM polygon found | 30 |
| `landuse=residential` | 24 |
| `landuse=construction` | 4 |
| Apartment building polygon without `landuse` | 2 |
| Exact normalized OSM/RERA name | 23 |
| Name with a source qualifier such as `(u/c)` or `Apartments` | 4 |
| Whole-project OSM name for a RERA phase | 2 |
| Whole-project OSM name for a RERA subproject | 1 |
| Direct RERA coordinate observed in historical raw data | 15 |
| RERA coordinate absent | 15 |
| RERA and OSM coordinates within 250 m | 14 |
| Major RERA/OSM disagreement | 1 |

A strict `landuse=residential` requirement would discard six valid named
polygons in this sample: Godrej Splendour, Sumadhura Capitol Residences,
Vaswani Starlight, Pursuit of a Radical Rhapsody, Mahaveer Promenade, and
Ruchira Lilium. The first four are tagged as construction; the last two are
mapped directly as apartment buildings. Exact normalized name equality would
also discard seven of the thirty matches.

The residential-only filter was introduced in commit `32befce2` to fetch the
accepted Waterford boundary for the arrival experience. It encoded the tag on
that successful example before the source pattern had been sampled. It is not
supported as a general society-identity rule by this dataset.

RERA coordinates are useful but cannot be trusted without corroboration.
Fourteen of the fifteen observed coordinates are within 250 m of the OSM
polygon. Prestige Waterford's collected RERA coordinate is about 91.3 km from
its OSM polygon. A future materializer should preserve the raw observation and
derive a separately validated location from agreement among independent
sources rather than either trusting every RERA coordinate or deleting the
field before validation.

## Project and phase identity

The source records show that one buyer-facing society does not always map to
one record in every source:

- `Pursuit of a Radical Rhapsody Phase 2` has RERA registration
  `PRM/KA/RERA/1251/446/PR/190102/002271`, while OSM way `345899243` names the
  whole project. The raw RERA corpus also contains Phase III
  (`PRM/KA/RERA/1251/446/PR/200817/003551`), Tower 5
  (`PRM/KA/RERA/1251/446/PR/281222/005565`), and Tower 8
  (`PRM/KA/RERA/1251/446/PR/220922/005261`) registrations.
- `SUMADHURA SOHAM PHASE-I` maps to whole-project OSM way `1072344841`, named
  `Sumadhura Soham`.
- `Brigade Lakefront - Crimson` maps to whole-project OSM way `229628369`,
  named `Brigade Lakefront`.
- The sampled RERA corpus has one `Godrej Air` registration and OSM has one
  `Godrej Air` society polygon plus a separate `Godrej Air Lawns` feature.
  `Godrej Air Next` was not found as a separate RERA project or named OSM
  society in this pass. Its relationship should be checked against Google's
  source handle in the next source-comparison pass, not inferred from its
  display name.

These cases support a source-record cluster with stable source handles and
explicit `whole project`, `phase`, `tower`, and `feature` relationships. They
do not support flattening records by exact name or duplicating a society for
every source label.

## Research conclusion

The current OSM filter is narrower than the observed data. OSM feature type is
evidence about lifecycle and geometry ownership, not a sufficient identity
gate. The next implementation should score candidates from source identity,
name tokens, independent coordinates, containment, and phase relationships,
then retain ambiguity when those signals do not agree. No production crawler,
DAG, serving bundle, or active pointer was changed in this pass.
