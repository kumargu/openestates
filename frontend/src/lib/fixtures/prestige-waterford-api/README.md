# Prestige Waterford API test bed

These are raw responses captured from the local Rust API on 2026-09-13. All
three snapshots came from promoted serving bundle
`catalog-71-ffb4dc50-117e-453c-b26f-41822430324e`.

The fixture covers every request made by the direct property page:

- `GET /api/properties/discovered-prestige-waterford-3bhk`
- `GET /api/properties/discovered-prestige-waterford-3bhk/surfaces/arrival_story`
- `GET /api/properties/discovered-prestige-waterford-3bhk/surfaces/around_this_home`

Run the real React property page without Rust:

```bash
cd frontend
npm ci
npm run dev:waterford
```

Then open
`http://localhost:5173/property/discovered-prestige-waterford-3bhk`.

The checked-in data contains no Google key or media bytes. Set the
browser-restricted `VITE_GOOGLE_MAPS_API_KEY` in an untracked `.env.local` to
exercise Google 3D. Property image URLs remain the original lake-backed
`/media` paths, so image review still needs a reachable media server; the Atlas
scene itself does not.

This is a UI regression fixture, not a second source of product truth. Refresh
all three files together when intentionally moving the test bed to a newer
serving bundle.
