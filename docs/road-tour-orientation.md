# Road tour orientation

The road tour needs a stable reference to Home as the camera travels. A small,
north-up overview now shows the existing road geometry, Home, and tour position
in both Aerial and Street View. The existing Home marker also remains visible
in the aerial scene. The Home navigation tab remains the way back.

Interaction note: uses the familiar map overview pattern. The ThreeUI research
found no directly relevant road-camera interaction; no demo code or decorative
motion was borrowed. At rest the overview stays fixed. It adds no hover-only
content, focus targets, or touch gestures and does not intercept map input.
Existing keyboard and touch playback/navigation controls remain unchanged.
Reduced-motion behavior stays with the existing tour controller; the overview
has no independent animation or interpolated transitions.

The aerial dot is the tour's position along the road, not the drone camera's
physical coordinates. Street View uses the panorama's actual position and
heading, without snapping it to the road. The overview keeps a fixed extent;
panoramas outside it have no position marker. No entrance connector, routing
distance, or new geographic evidence is inferred. Road name remains in its
existing location; stale Street View step counts/status no longer appear in
Aerial. Updates are isolated to the overview, sampled four times per second.

UI critic / React review: no drawer, duplicate property identity, new navigation
controls, instructions, or live-announced movement. Existing scene and playback
ownership are unchanged. This is a local preview, pending visual review: the
browser runtime reports no available browser, so resting, moving, Street View,
and mobile screenshots could not be captured. Check these states on Brigade
Woods, including Pause, Replay, switching modes, and returning Home.

Validation: frontend lint, TypeScript/production build, all 300 existing frontend
tests, and `git diff --check` pass. No new tests were added. The preview URL
responds with HTTP 200; that does not verify the rendered map.
