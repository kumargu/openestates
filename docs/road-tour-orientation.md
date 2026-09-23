# Road tour orientation

The side-map experiment was removed after buyer feedback: it did not help
orientation. Its component, position polling, and styles are deleted. The Home
marker remains visible in the aerial scene, and the existing Home tab remains
the way back. Street View status is only shown while Street View is requested.

Interaction note: this change removes the overview in all viewport sizes.
Existing rest, hover, keyboard focus, touch, and reduced-motion behavior remain
with the map and playback controls. No replacement drawer or interaction is
introduced. Earlier ThreeUI research found no directly relevant road-camera
pattern; none was borrowed. The UI-critic pass favors this removal of extra
chrome. Visual screenshots remain unavailable because no browser is connected.

Proposed next experiment, not implemented: frame Home while traversing up to
200 metres of the mapped public road on each side of its association with the
explicit entrance. These distances follow the road, not a compass direction
or an inferred path behind the buildings. Use only available continuous road
geometry; do not extend missing roads or invent an entrance connector. Keep
Home visible with smooth camera framing rather than forcing an exact screen
centre through every road bend.
