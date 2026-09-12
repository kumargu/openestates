# Prestige Waterford API fixture

This directory contains the promoted local backend responses for
`discovered-prestige-waterford-3bhk`, captured on 2026-09-12. It lets browser
tests exercise the Waterford property page without compiling or running the
Rust backend.

`manifest.json` maps every captured method and path to its response file. The
property page's normal browser journey needs these three routes:

- `GET /api/properties/discovered-prestige-waterford-3bhk`
- `GET /api/properties/discovered-prestige-waterford-3bhk/surfaces/around_this_home`
- `GET /api/properties/discovered-prestige-waterford-3bhk/surfaces/arrival_story`

The remaining files cover evidence, RERA, recommendations, combined surfaces,
and the batch surface contract. For the POST route, match
`surfaces-batch.request.json` and return `surfaces-batch.json`.

In Playwright, register API interception before navigating, fulfill each match
with status `200`, content type `application/json`, and the corresponding file
from `manifest.json`. Navigate to:

```text
/property/discovered-prestige-waterford-3bhk
```

These are the backend field values, pretty-printed for review. Google Street
View URLs in `property.json` and `evidence.json` originally contained a local
API key. Only that query-parameter value was replaced with
`REDACTED_FOR_FIXTURE`; no secret is included in this directory. Tests that
need Street View imagery should inject their own authorized browser-test key
or mock those image requests.

The captured `learned_at` provenance timestamps are snapshot values. The local
backend regenerates those timestamps on later requests, while the remaining
payload values replay identically.
