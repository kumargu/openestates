# Preserved Home Atlas prototype

This is the runnable design snapshot produced before production integration. `/` contains the Prestige Waterford neighborhood atlas and `/brigade/` contains the Brigade Orchards township journey.

It corresponds to source checkpoint `b39ecc1` (`Drive Brigade shell from atlas presentation policy`).

The snapshot deliberately keeps its imperative HTML/Google renderer adapters because they are the visual reference. They are not the intended React architecture. Reusable production logic lives one level up in `../core/`.

The Sites deployment manifest, generated Worker, local environment, node modules, duplicate OSM snapshot, discarded lift study, and vendored Three.js files are excluded.

Commands:

```bash
npm ci
npm run check
npm run dev
```

The OSM collector writes directly to `web/brigade/inventory.json`, keeping one canonical prototype fixture.
