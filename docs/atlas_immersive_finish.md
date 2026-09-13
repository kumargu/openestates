# Atlas immersive refinement

## Buyer behavior

The property opens at the society portrait. `Tour home` starts the existing
guided flight explicitly. Home navigation returns with a short transition.
Nearby overview and With home use deeper oblique compositions while retaining
the category orientation. Look closer now frames the selected destination's
actual geometry and anchor, independently of its distance from home. With home
restores the complete relationship. This intentionally replaces the earlier
requirement to contain home during inspection, which prevented true close views.
No coordinates, routes, ratings, or distances were added or changed.

The manual close camera uses elevation at the destination. Relationship arcs
remain comparison cues and disappear during inspection. Quiet surroundings and
edge shading are lighter; these are overlay changes, not new Google lighting
or shadow controls.

## Interaction note

Checked for a relevant ThreeUI map pattern; none was established. No ThreeUI
implementation is claimed. This refines the existing overview/selection/isolation
pattern documented in `atlas_nearby_parity.md`.

- Rest: stable property portrait and optional tour; no redundant Show together.
- Selection: explicit With home / Look closer, preserved when reopening drawer.
- Hover/focus: existing hover treatment and visible focus rings; close and Escape
  return keyboard focus to the selected category.
- Touch: 44px category actions, shorter bottom sheet, bounded scrolling list.
- Reduced motion: home return snaps; existing nearby and tour policies remain.
- Road: a native progress bar follows real route distance without React updates
  every animation frame; speed and Street View handoff remain available.
- Camera exclusions observe the drawer, categories, and dock as they resize.

UI critic: removed the inactive overview action, reduced mobile obstruction,
preserved the page theme and facts, and retained optional display settings under
Map view. No new dependencies or fabricated travel-time claims.

## Verification and remaining visual gate

Frontend tests, lint, Atlas build, and diff whitespace checks pass. The compact
browser matrix now checks close framing, focus restoration, and drawer state;
the full tour test explicitly starts Tour home.

Live screenshots could not be captured in this session: the managed browser
blocked localhost with ERR_BLOCKED_BY_CLIENT and the protected Vercel preview
redirected to login. Browser tests were updated but not executed. Run
`npm run atlas:lab` and `npm run atlas:lab:full` with authorized preview/browser
access before accepting visual parity. This commit is a focused refinement,
not evidence that every ambitious visual goal has been visually verified.
