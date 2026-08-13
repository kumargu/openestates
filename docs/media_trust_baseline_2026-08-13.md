# Media trust baseline — 2026-08-13

Issue: #71 (buyer-journey epic #70)

## Chain audit

Before this change, `external_images_weekly` collected dimensions, candidate kind,
scores, source URLs, hashes when bytes were staged, rejection reasons, and slot
eligibility. `image_media_facts` then retained the audit JSON in `image_gallery`
but separately projected `hero_image` and `images` as URL-only facts. The serving
loader discarded `image_gallery`, the API exposed URL strings, and React assigned
`Exterior`, `Building`, `Amenities`, `Neighbourhood`, and `Gallery` by array index.

The control plane is `app/config/dag/crawl_policies/media_source_policy.json`;
the asset edges are `external_images_weekly -> image_media_facts ->
current_project_facts -> search_serving_bundle` in `asset_registry.json`.

## Recorded baseline

- Frontend manifest: `catalog-118-rera-validation-14-2026-08-12` (manifest v1).
- Latest raw media snapshot inspected:
  `dt=2026-08-12/run_id=f3ec4886-56d4-4f77-8648-c708455f9d0c`.
- Raw rows: 402. The refresh audited eight candidates for one society; seven were
  approved for the old gallery and one rejected. The other 394 rows were retained
  from earlier materializations, so the old run report did not re-count them.
- That refresh recorded seven selected gallery URLs and one selected hero. All
  three attempted external source pages failed; the selected media came from the
  local ingest path.
- The live product audit found cross-project visible text, render/photo ambiguity,
  text-heavy artwork, and eight property rows without an image. These are trust
  failures, not a reason to fill empty slots.

The old report did not include a per-asset identity result, media kind, hash
coverage, duplicate groups, or final buyer eligibility. It therefore could not
prove that the seven selected URLs were safe for buyers.

## Fixture expectations

| Fixture | Expected promotion |
|---|---|
| Local file or external page without explicit identity proof | Quarantine; never infer scope from its folder or URL |
| Explicit source entity differs from canonical entity | Quarantine; no hero/gallery slot |
| Render with an allowed classification method | Typed `render`; quiet buyer label |
| Site photograph with an allowed classification method | Typed `site_photo` |
| Filename/slot without a verified media label | `unknown`; buyer-ineligible |
| Same content hash in one canonical entity | One buyer asset with hero/gallery eligibility merged |
| Hero-only media | Hero eligible independently; no fabricated gallery eligibility |
| Marketing/text-heavy/phone artwork | Retained in audit, never ordinary gallery media |
| Floor plan or map | Retained under its type, never ordinary photography |
| Missing valid SHA-256 | Retained in audit, buyer-ineligible |
| Missing media or failed fetch | Compact no-photo surface |
| Legacy array or wrong canonical envelope | Fail closed to no trusted media |

## Post-change release expectation

The typed contract is version 2. Existing version-1 bundles intentionally expose
zero trusted media after this code ships. Re-run `external_images_weekly`, promote
`image_media_facts`, and build a new serving bundle before release. The new media
QA report records project identity, hash, source, dimensions, type, validation,
duplicate groups, buyer eligibility, and rejection reason for every candidate.
