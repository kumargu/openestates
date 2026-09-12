# Atlas browser lab

This lab gives a visual agent two deterministic property-page targets while
keeping Google Photorealistic 3D and WebGL real.

- Camera lab: `/property/fixture-prestige-waterford-3bhk`
  - Geometry-rich fixture for every society, nearby, road, and mobile state.
- Backend replay: `/property/discovered-prestige-waterford-3bhk`
  - Exact redacted backend responses captured from the promoted local service.

On a Vercel preview, append `?fixture=atlas` to either URL. This opt-in keeps
ordinary preview visits connected to the configured backend.

## First-time setup

Put an authorized browser key in the gitignored `frontend/.env.local` file:

```dotenv
VITE_GOOGLE_MAPS_API_KEY=your_key_here
```

Install the project-pinned browser once:

```bash
cd frontend
PLAYWRIGHT_DOWNLOAD_CONNECTION_TIMEOUT=120000 npx playwright install chromium
```

## Use the lab

For interactive work:

```bash
npm run atlas:lab:serve
```

For the compact desktop/mobile capture matrix:

```bash
npm run atlas:lab
```

For the complete timed camera and road journey:

```bash
npm run atlas:lab:full
```

Each automated run writes screenshots, video, console diagnostics, a fixture
hash manifest, and an `index.html` contact sheet under
`frontend/test-results/atlas-parity/<run-id>/`.
