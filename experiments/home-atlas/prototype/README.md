# Preserved Home Atlas prototype

This is the runnable design snapshot produced before production integration. `/` contains the Prestige Waterford neighborhood atlas and `/brigade/` contains the Brigade Orchards township journey.

The snapshot now includes the accepted calm property shell and continuous ECC Road aerial journey: an overview hold, descent, route-following camera, 0.5–2× speed control, cancellation, and an explicit Street View handoff. The Waterford adapter declares route direction; the geometry helper does not infer it from coordinates or a society name.

The imperative HTML/Google renderer adapters are preserved because they are the visual reference. They are not the intended React architecture. Reusable production logic lives one level up in `../src/`; `web/road-path.js`, `web/arrival-walk.js`, and `web/atlas-shell.js` demonstrate how a renderer host can wire those ideas without becoming a second data source.

OpenEstates should keep its existing Street View component. Port the aerial route/camera/speed primitives and use the handoff contract to open or return from that existing surface. The prototype panorama is reference behavior only.

The Sites deployment manifest, generated Worker, local environment, node modules, duplicate OSM snapshot, discarded lift study, and vendored Three.js files are excluded.

Commands:

```bash
npm ci
npm run check
npm run dev
```

The OSM collector writes directly to `web/brigade/inventory.json`, keeping one canonical prototype fixture.
