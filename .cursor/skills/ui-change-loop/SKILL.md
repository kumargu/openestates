---
name: ui-change-loop
description: Iterates on OpenEstates buyer-facing UI with screenshot-led critique, one visual hypothesis per loop, desktop/mobile interaction checks, and keep-or-revert decisions. Use for property, discovery, compare, workspace, RERA, or plan UI changes.
---

# UI Change Loop

Use this loop for buyer-facing UI work. Keep it small; do not redesign adjacent surfaces.

## Before editing

1. Read `.claude/skills/coding-practices.md` and `.claude/skills/ui-critic.md`.
2. Open the live route and capture the current desktop and mobile states.
3. Name one concrete problem visible in the screenshot:
   - hierarchy;
   - duplicate fact or copy;
   - unnecessary box or chrome;
   - inconsistent control language;
   - overflow, overlap, or unreachable interaction.
4. State one visual hypothesis and the smallest coherent fix.

## One loop

1. Change one compositional variable only.
2. Rebuild the same route with real data.
3. Capture and inspect:
   - desktop at roughly `1440 × 1000`;
   - mobile at roughly `390 × 844`;
   - tablet when fixed navigation or horizontal layouts are involved.
4. Walk the UI critic top-to-bottom:
   - identity before controls;
   - one fact, one surface;
   - cards only for discrete interactions;
   - buyer copy, never pipeline copy;
   - no empty decorative space;
   - no duplicated current-home facts in persistent navigation.
5. Exercise the changed interactions: Save, Note, playback, sidebar, links, scrolling, keyboard focus, and handoff routes as applicable.
6. Check the DOM for horizontal page overflow, fixed-nav overlap, off-screen popovers, and hidden controls.
7. Run lint, build, and the closest focused tests.
8. Keep the change only if the screenshot is clearer and behavior remains intact. Otherwise revert that loop before trying another hypothesis.

## Independent critique

After two loops, or whenever the page still feels assembled from different products, ask one UI-review agent to inspect the latest screenshots and code. Request only:

- blockers;
- should-fix inconsistencies;
- duplicated visible facts;
- functionality that must move rather than disappear.

Fix verified blockers one at a time, then ask for a short re-review.

## Guardrails

- Prefer deleting, merging, or demoting chrome before restyling it.
- Do not solve weak evidence with a larger card; compact it or omit it.
- Do not introduce a new visual language for one chapter.
- Do not invent facts, statuses, or source confidence.
- Do not copy ThreeUI styling; use it only for bounded motion and prompt discipline.
- Preserve reduced motion, keyboard focus, and buyer workflows.
- Delete temporary screenshots before finishing.

## Done when

- Desktop, mobile, and relevant tablet screenshots form one visual system.
- No blocker or important review finding remains.
- The changed interactions work.
- Lint, build, and relevant tests pass.
- Report what changed, what was intentionally removed, and what remains uncommitted.
