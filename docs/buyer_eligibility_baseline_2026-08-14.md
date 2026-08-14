# Buyer eligibility baseline — 2026-08-14

Issue: #72 (part of #70)

## Chain audit

- Branch base: `fa70777 Build trusted typed property media (#71)`.
- Promoted search asset pointer: `catalog-118-simple-plan-previews-evidence-final-2026-08-13`.
- Runtime `dev` catalog release: `catalog-118-simple-plan-previews-production-2026-08-13`.
- The runtime release has 776 entities and 121 property candidates. The search
  asset profile has 15,549 facts, 15,122 search-metadata rows, and 4,115 edges.
- No #72 behavior existed before this change. Buyer visibility was the
  `Property::is_listable` price-or-`Price unavailable` shortcut.

## Candidate classification

The profile reads the promoted Parquet through `ServingBundleLoader` and
`properties_from_serving_bundle`; it does not inspect seed JSON.

| Condition | Candidates |
|---|---:|
| Total | 121 |
| Missing title | 0 |
| Missing area | 28 |
| Missing price | 35 |
| Missing BHK/configuration | 0 |
| Missing/unknown lifecycle | 1 |
| Raw `APPROVED` projected as possession | 120 |
| No #71-trusted hero media | 121 |

The promoted bundle carries 102 legacy `image_gallery` facts, but none use the
version-2 typed media envelope introduced by #71. Media must therefore remain
an observed eligibility reason until the catalog is rematerialized; making it
a hard requirement in this release would remove every home.

## Current fallback map

- Society `rera_status` becomes `Property.possession_status`.
- Property `possession_status` falls back to society `rera_status`.
- Missing representative price and area become numeric/string zero values.
- Missing society configuration becomes a synthetic 3 BHK.
- Missing city becomes `Bengaluru`; a missing area becomes `area-`.
- A price-missing property is listable when it carries the generated
  `Price unavailable` tag.
- List, detail, recommendations, search, and sibling expansion each rely on
  the listability shortcut independently.

## Ordered-result baseline

Captured from the local API using the promoted `dev` catalog release before
eligibility filtering. The full order is reproducible with:

```bash
curl -fsS --get --data-urlencode 'q=<query>' \
  http://127.0.0.1:4000/api/search | jq '[.results[].id]'
```

| Query | Result count | First ten IDs |
|---|---:|---|
| `good value with proof` | 1 | `discovered-woods-3bhk` |
| `near metro low traffic` | 108 | `prestige-waterford` 1/3/4 BHK; `salarpuria-sattva-misty-charm` 1/2/3 BHK; `vaswani-starlight` 3/4 BHK; `sobha-forest-edge` 3 BHK; `sumadhura-capitol-residences` 3 BHK |
| `family friendly 3BHK` | 50 | `mantri-tranquil`, `prestige-bagamane-temple-bells`, `sumadhura-capitol-residences`, `prestige-waterford`, `century-central`, `renaissance-reserva`, `godrej-air`, `prestige-fairfield`, `salarpuria-sattva-misty-charm`, `sobha-forest-edge` |
| `premium explainable homes` | 10 | `snn-raj-greenbay` 2/3/4 BHK; `mirabelle` 3/4 BHK; `arvind-bel-air` 2/3/5 BHK; `sumadhura-solea` 3/4 BHK |
| `area tracker picks` | 16 | `32-richmond` 2/3/4 BHK; `birla-tisya` 2/3/4 BHK; `brigade-7-gardens` 2/3/4 BHK; `brigade-laguna` 2 BHK |

Search miss classification: `architecture_gap`. The facts are present in
Parquet, but visibility is decided after hydration by an unversioned runtime
shortcut and regulatory status is projected into the wrong typed concept.

Hardcoding audit baseline: 394 warning-only findings and zero blocked
search-config alias findings.

## Final verification

The final policy was verified with a freshly compiled API on isolated port
`4022` and independently with `openestates-profile-serving-bundle`, both reading
the same promoted production release. They agree on:

- 121 internal candidates;
- 88 eligible candidates on each buyer surface;
- 26 missing-area, 33 missing-price, and 5 missing-configuration candidates;
- 121 missing lifecycle and trusted-media observations (not hard gates for this release);
- 120 regulatory `APPROVED` values, zero lifecycle `APPROVED` values, and zero
  possession values derived from `APPROVED`.

The transient 86 count exposed an alias-hydration ordering bug: injected society
aliases and canonical rows could produce the same runtime property ID, and an
unordered fact-index iteration sometimes retained the less complete candidate.
Runtime hydration now considers only real serving entities and deterministically
prefers the config-eligible, lower-gap candidate. Three clean profiler processes,
the isolated API, `/api/properties`, and `/api/admin/data-health` all reproduce
88. The complete ordered eligible ID artifact is
`docs/buyer_eligibility_after_2026-08-14.json`.
Two clean API restarts returned the same 88 IDs in the same order; the ordered
ID SHA-256 was
`6279a920967b0f9c4e606a47a379f44a2ebf53635723f1f5c050c4f5f35f7ab4`
on both runs.

For every retained result, relative order remained stable. The audited query
counts after gating are 0 (`good value with proof`), 86 (`near metro low
traffic`), 35 (`family friendly 3BHK`), 7 (`premium explainable homes`), and 9
(`area tracker picks`). The prior single good-value result became ineligible;
all other removals are policy exclusions rather than ranking changes.

Direct requests for an incomplete known property return `409
property_not_ready` from detail, evidence, RERA, recommendations, and surface
routes. Unknown IDs return `404 property_not_found`. Evidence batch responses
separate `not_ready_property_ids` from `missing_property_ids`; surface batches
return the same `409` contract when any requested property is not ready.
