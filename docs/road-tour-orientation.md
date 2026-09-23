# Road tour orientation

The road tour again uses the earlier Waterford aerial motion as its reference
(commit `1a6b1a28`). The camera follows the continuous mapped road, looking along
its direction with a steady 67-degree tilt and a 270 m desktop / 350 m mobile
range. Heading damping softens bends. The complete selected road is traversed;
there is no arbitrary 200 m crop, forced Home-centred target, or altitude increase
based on the route's distance from Home. Those superseded paths and settings
have been removed.

The reference speed is restored: 12 m/s at 1×, starting at 2×. Near a mapped
entrance the current slowdown and 2.6-second pause remain. Context, descent,
manual pause/resume, reduced motion, and the finite ending use the existing
playback controller. Home and entrance markers remain, and the Home tab returns
to the society. Home may leave the viewport as the camera follows the road.
The side map stays removed.

Street View and Aerial use the same road direction and geometry. Street View
starts near the current tour position if panorama coverage is within 50 m;
returning to Aerial uses the road projection within the same limit, otherwise
retaining the prior tour position. Replay starts from the beginning. These are
viewing itineraries, not invented driving connections or durable source facts.
Terrain elevation remains referenced to Home, not sampled along the road.

Interaction note: reuse the product's own earlier Waterford road-following
pattern. Earlier ThreeUI research found no directly relevant road-camera
pattern. No new controls, hover states, focus targets, touch gestures, copy or
panels are introduced. Reduced motion still settles without autoplay. The
UI-critic pass removes the forced Home lock that competed with road movement;
markers supply orientation without additional chrome.

The captured Waterford road is about 1,224 m long; the rejected experiment
cropped it to 400 m and slowed travel from 24 m/s to 6 m/s. This is the concrete
reference comparison, not a claim of a visual review. Existing frontend lint,
build, tests and diff checks are the verification gates; the existing speed
assertions return to their reference expectations. No new tests were added.
The browser connection is unavailable, so screenshots and rendered-motion
review remain pending. Start with Waterford, then compare Brigade Woods.
