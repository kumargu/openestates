# Interior tour correction

## Evidence correction

The earlier conversational review mixed the published version 1 with the unpublished version 2 / PR #130. The reviewer inspected a version-1 screenshot and source code; it did **not** watch an animation frame by frame. Claims of having done so were incorrect.

PR #130 already had explicit entrance geometry, all thirteen rooms, a single playback clock, arrival-dependent inspection, and route-error rejection. It did not have the old fixed 8.5-second timer. Those behaviors have been preserved.

## Confirmed defects and changes

| Current-PR issue                                                                                        | Correction                                                                                                          | Evidence                                                                          |
| ------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| Measurement direction and an unconditional quarter turn determined what a buyer saw                     | Separate visibility-based view director chooses entrance reveal, room length and supported doorway connection views | Depth and balcony-length regressions over the source fixture and rotated geometry |
| Looking ahead could orient the camera beyond a corner while it still translated on the previous segment | Follow the immediate segment; turn before walking; round only collision-safe corners                                | Full-apartment per-frame displacement, yaw and clearance regression               |
| Manual look during a paused inspection could snap back on resume                                        | Resume through settling and bounded angular motion                                                                  | Pause/look/resume regression                                                      |
| Faster playback shortened room viewing time                                                             | Walking pace and stationary viewing time are separate                                                               | Slow/fast pace hold-duration regression                                           |
| High walls hid rooms in the dollhouse; room selection entered walking instead of framing the room       | Default cutaway, highlighted room floor, aspect-aware room focus and Whole home action                              | Source/type/build checks; rendered appearance still needs browser review          |
| Measurement labels could drift off their tape as the camera moved                                       | Anchor on tape geometry and test scene occlusion; keep fixed plan tapes and readable caption values                 | Source/type checks; rendered placement still needs browser review                 |
| Touch direction buttons moved the camera a fixed distance instantly                                     | Hold-to-walk through the same collision-checked clock                                                               | Source/type checks; touch-device check remains                                    |

The full sample has 27 planned viewing moments across 13 spaces. The route is intentionally unhurried; controls allow pausing, changing pace, and moving to the next room. No apartment-specific camera coordinates were added.

## Review boundary

Deterministic geometry and playback checks are not a substitute for watching a rendered tour. The configured cloud browser previously rejected the internal preview with `ERR_BLOCKED_BY_CLIENT`. No alternate browser or network path was used to bypass that restriction. There is still no honest end-to-end visual sign-off or device screenshot for this revision. Keep the PR draft until that review passes.

The brochure trace also remains approximate. Ceiling heights, lighting and finishes are illustrative. This revision improves the tour mechanism; it does not certify the physical apartment or replace a survey.
