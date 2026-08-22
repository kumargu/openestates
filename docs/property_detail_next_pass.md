# Property detail — next pass

**Status:** M0–M4 implemented on `agent/property-detail-next-pass`; M5 next.
**Updated:** 2026-08-23
**Companion:** canvas `property-detail-mocks.canvas.tsx`

Search already presents a journey instead of a dump. Detail should feel the same: one home, a walkable set of photos, then context — not a listings page that throws media and facts at the buyer.

## M0–M2 learnings

- Treat the public property-story reference as the visual contract: full-bleed sequence, compact dark navigation, stationary facts, and no workspace sidebar on the story route.
- The Story Lab must render the same projection and production component as `/property/:id`. Add a deck to the Lab only when its production component is genuinely auditable there.
- Projection owns deterministic ordering, sparse compaction, fact deduplication, and buyer-safe formatting. Presentation must not repair or reinterpret data.
- Unknown image provenance stays unlabelled. Never infer current-site or proposed-render state from a URL.
- Motion timing has one owner. Autoplay, cross-fade cleanup, CSS animation duration, and speed controls all use the same registry values.
- Failed media must disappear from both the hero sequence and gallery handoff; an enabled action that opens nothing is a product bug.
- Sparse media should compact to identity typography rather than reserve a cinematic empty slab.
- Mobile story controls need 44 px touch targets even when the visual control remains compact.
- Story and dossier modes share the projection and component tree. Dossier mode pauses decorative motion instead of creating a second detail page.
- Preserve existing property subroutes and workspace tools. Only the cinematic property detail and internal Lab bypass workspace sidebar chrome.

## M3–M4 learnings

- Preserve the map as a product surface. Story framing can change its position and surrounding rhythm, but not its filters, proof focus, notes, zoom, metrics, or MapLibre implementation.
- Arrival projection must select `approach_road` evidence explicitly. Flattening every evidence strip into a film silently turns unrelated media into a buyer claim.
- Street View remains linked evidence: render the runtime image, keep the source action, and never copy it into owned story assets.
- Below-fold cinema needs the same pause contract as the hero. Pausing only the timer while CSS motion continues still wastes work offscreen.
- Google review cards come only from `external_reviews`. Community-pulse prose is not a substitute for a canonical review card.
- An unresolved review match may stay as a compact source handoff; missing reviews omit the deck.
- The property-page RERA teaser uses detail-payload registration and document facts. It does not fetch, summarize, or duplicate the dedicated report.
- Short Compare is the current home plus two distinct peers, or it is omitted. The full Compare link is projected with the same IDs and current-home focus.
- If one comparison home lacks media, remove the media row from all three cards. A uniform compact comparison is better than large empty image slabs.

## What is already good

- Title → inline meta (`₹… · 3 BHK · sqft · Delivered · ★`) → media. Keep that rhythm.
- Airbnb-style mosaic on the page (lead + four tiles + Show all).
- Around This Home layers and icons. The map is a product surface, not decoration.
- RERA now has its own tab. That is the official-record drill-down.

## Gaps

### 1. Photo viewer is a placeholder

`PropertyPhotoMosaic` opens `CleanDialog` titled "All photos" with a static CSS grid. There is no selected index, no next/prev, no keyboard, no swipe. Scene labels in `sceneLabelForIndex` cycle Exterior / Building / Amenities / Neighbourhood / Gallery by list position. `PropertySceneCard` is unused.

A home is a sequence. The mosaic can stay. Entering photos must become a walker: one large frame, arrows, `4 / 18`, filmstrip, Esc, open at the clicked tile.

### 2. Mosaic + full map overload the page

The first screen is a 16:9 mosaic and then a tall map plate. Approach road and the RERA report row sit under that stack and get hidden. More detail content is coming. Two large media blocks leave no room.

### 3. Image quality is a backend problem

We scrape because there are almost no live listings yet. Volume is not the limiter. Curation is. This is a separate automated pass — do not block the viewer on it.

## Recommended composition (mock A)

Keep the mosaic. Make photos walkable. Keep the map on the first decision screen, but compact it. Approach road stays until we design its entry.

```text
Title / area                                              Save · Note
₹2.15 Cr · 3 BHK · 1,412 sqft · Delivered · ★ 4.3

[shorter mosaic]                         [Show all → walker]


AROUND THIS HOME                         Schools  Metro  Lakes  Warnings
[compact map plate — same icons, less height]


Signals · Google reviews · Nearby homes · Micro-market
(room for the next detail chapter)
```

**Leaves this page later, not now**

- Dummy rotating scene labels — silence until serving frames carry a kind.

**Stays**

- Approach road. No one else shows the road in. It is a differentiator, built like Airbnb amenities: a compact entry that should expand into the trail. It currently sits under a tall map, which is why it feels hidden. Do not delete it, and do not invent a new entry until the gallery and map are rebuilt. Then design the desktop entry (small affordance → expand into one or more approach roads). Uber-style icon-to-route may be enough on a phone; desktop web needs a clearer first read.
- RERA report row can wait the same beat. Official record already has a tab; do not restack that problem while photos and map are in motion.

**Close second (mock B):** photos own the first screen; the map becomes a later chapter. Use B if a compact map is still too tall once the next detail chapter lands.

**Do not ship (mock C):** photos left, map right. That is a generic listing dump.

## Image curation — follow-up, automated

Tiny team. No hand-picking. Same motivation as RERA plan promotion (`promote_rera_project_plans.py` + OCR: what does this frame show, then promote). Do not reuse that code in this pass.

Already true:

- `pipeline/skills/fetch_images.py` classifies exterior / interior / amenities / master_plan / floor_plan.
- `TARGET_IMAGES = 5` — too few for a walker.
- `backend/src/assets/media.rs` already keeps floor plans out of hero and gallery.

Later skill (not this UI mock):

- Scrape more candidates.
- Reject collages, watermarks, map screenshots, blur, tiny thumbs, floor plans in the hero.
- Promote a short set: 1 hero exterior, 1 building, 1 amenity, 1 neighbourhood, rest in gallery.
- Serve `media.gallery_frames` with `kind`, confidence, source. UI reads the kind generically.
- No human review in the loop.

Ship the walker on the URLs we already have. Curation changes which photo is first and what the chip says. It should not change whether you can move to the next photo.

## Implementation order

1. Replace the photo grid dialog with a walker. Open at the clicked index. Drop dummy scene labels.
2. Compact the map plate. Keep Expand map. Leave approach road and the RERA row as they are.
3. Redesign the approach-road entry so it is findable on desktop, then expands into the trail.
4. Backend curator as its own pass.
