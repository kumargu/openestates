# Road tour orientation

The side-map experiment was removed after buyer feedback. The road pass now
keeps Home near the centre of the main aerial view. It follows the same
continuous mapped road for up to 200 metres either side of the explicit
entrance's road projection. Without a mapped entrance it centres that viewing
window near Home, with no invented gate or entrance pause. The complete sourced
road geometry remains available; this only shortens the viewing itinerary.

The camera first shows context, then descends. Its target blends toward Home
while its range and tilt follow the road position at a fixed flight elevation.
The default speed is 6 m/s (1×), easing to 2.4 m/s near the gate. It pauses there
for 2.6 seconds, continues where mapped geometry exists, and settles at the end.
Replay restarts the pass; manual interaction pauses playback. The existing
controller owns pause/resume, including the entrance dwell and descent.

Flight elevation uses the resolved Home terrain elevation as its reference;
this is not a sampled terrain-following path. A single higher flight elevation
is selected for distant Home anchors to avoid looking toward the horizon.
Heading damping softens turns. Google camera state remains presentation only.

Aerial and Street View use the same clipped road. Street View starts near the
current road-tour position, provided the first resolved panorama is within
50 metres; unavailable coverage does not restart the tour at a distant point.
Returning to Aerial projects the panorama onto the road within that same limit,
otherwise it retains the previous tour position. Aerial looks toward Home
rather than using the Street View heading to reposition the camera elsewhere.
Street View Replay starts from the beginning. No road connection is inferred.

Interaction note: no new chrome or copy. Home and the mapped entrance remain
visible as markers; the existing Home tab returns to the society view. Rest,
hover, focus and touch use the existing controls. Reduced motion settles the
camera without autoplay. No decorative loops or extra panels were borrowed;
earlier ThreeUI research found no directly relevant road-camera pattern.
UI critic: this concentrates orientation in the scene and avoids a competing
side map, duplicate headings, and new instructional text.

Validation uses the existing frontend contracts, lint, build and diff check.
The speed expectations in the existing Waterford contract were updated for the
approved slower pass; no new tests were added. Captured Brigade Woods geometry
clips from about 1,498 m to 200 m, ending at the gate. The Waterford fixture clips
from about 1,224 m to 400 m. Desktop and mobile camera projections keep Home in
view along both paths; numeric results are in
`/tmp/home-road-camera-validation.json`. These checks do not establish visual
quality: no browser is connected, so screenshots and a rendered-motion review
are still pending. Try Brigade Woods first, then Waterford, checking mode
switches, entrance pause/resume, Replay, manual dragging and reduced motion.
