# Day 8 API Contract Note

Stabilized API contract between Rust backend (port 4000) and React frontend (port 5173).

## Endpoints

| Route | Method | Type | Frontend consumer | Response shape |
|---|---|---|---|---|
| `/` | GET | Health | - | `{ service, status }` |
| `/api/health` | GET | Health | Dev tooling | `{ service, status }` |
| `/api/properties` | GET | List | ResultsPage | `PropertyCard[]` |
| `/api/properties/{id}` | GET | Detail | PropertyPage | `{ property, society?, area? }` |
| `/api/areas` | GET | List | HomePage (area cards) | `AreaListItem[]` |
| `/api/areas/{id}` | GET | Detail | Future area detail page | Full `AreaProfile` |
| `/api/shortlist` | GET | List | ShortlistPage | `{ shortlist: string[] }` |

## Response shapes

### PropertyCard (list item)

```json
{
  "id": "prop_w_001",
  "title": "...",
  "area": "Whitefield",
  "price": 12500000,
  "price_per_sqft": 8800,
  "bhk": 3,
  "sqft": 1420,
  "society_name": "Prestige Shantiniketan",
  "hero_image": "placeholder://prop_w_001/hero.jpg",
  "transparency_tags": ["below_area_median", "ready_to_move", "low_litigation_risk"]
}
```

Tags are capped at 3 in the list endpoint (most decision-useful first).

### PropertyDetail (joined)

Returns `{ property, society, area }` where society and area may be `null` if no match found. The detail endpoint returns all transparency tags (not capped).

### AreaListItem

```json
{
  "id": "whitefield",
  "name": "Whitefield",
  "median_price_per_sqft": 9200,
  "trend_direction": "up",
  "primary_signal": "metro_nearby"
}
```

Lightweight summary for homepage cards. `primary_signal` is the first externality tag from the full area profile.

### ShortlistResponse

```json
{
  "shortlist": ["prop_w_001", "prop_h_002"]
}
```

Currently a hardcoded stub. Frontend resolves IDs by cross-referencing with `/api/properties`.

## Day 7 ambiguities resolved

1. **`/api/health` missing** — Now exists as a dedicated route (was only on `/` before).
2. **`/api/areas` missing** — Now serves a lightweight list endpoint. Previously only `/api/areas/{id}` existed.
3. **Transparency tag overload** — Results cards now cap at 3 tags server-side.
4. **Cargo.toml edition** — Fixed from invalid "2024" to "2021".

## Error handling

- Invalid property ID returns `404` with `{ "error": "property_not_found" }`.
- Invalid area ID returns `404` with `{ "error": "area_not_found" }`.
- Frontend shows explicit loading/error/empty/not-found states for every data-fetching page.
