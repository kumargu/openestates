# KG Entity Refs UI Contract

`kg_entity_refs` is the bridge between search/listing responses and the dynamic
Knowledge Graph UI. It should let the frontend render evidence-backed sections
without turning every future fact type into a hardcoded React field.

## Product Intent

OpenEstates should feel like a proof-first property intelligence product, not a
static listing site. A property page should adapt to what we know:

- If RERA facts exist, show the RERA file and legal timeline.
- If Google nearby facts exist, show nearby schools, hospitals, tech parks,
  retail, and parks.
- If community/review facts exist, show community pulse and review themes.
- If a builder node exists, show delivery history and related projects.
- If a signal is missing or weak, hide that card or show a compact data gap.

`kg_entity_refs` gives the UI stable graph handles for that behavior.

## Contract Shape

Property cards include:

```ts
kg_entity_refs: {
  property_entity_id: string;
  society_entity_id: string;
  area_entity_id: string;
  builder_entity_id?: string;
  source_entity_ids?: string[];
}
```

Property detail responses include both graph identity and the canonical dynamic
evidence read model:

```ts
entity_refs: KgEntityRefs;
evidence: PropertyEvidenceResponse;
```

The top-level `entity_refs` field on detail responses is the preferred detail
contract. It avoids mixing graph identity into the flat listing payload.
The top-level `evidence` field is the preferred property-page rendering
contract. It lets the UI render dynamic proof cards from one backend call.

## What Each ID Means

`property_entity_id`

Listing-specific node. Use this for facts tied to a concrete unit/card/listing:
configuration, price, seller source, carpet area, market activity, and listing
freshness.

`society_entity_id`

Society/project node. Use this for RERA file facts, Google reviews, nearby
places, amenities, community evidence, complaint history, and builder edges.

`area_entity_id`

Area/locality node. Use this for traffic, waterlogging, metro access, price
trend, school quality, hospitals, externalities, and locality-level warnings.

`builder_entity_id`

Builder node when known. Use this for delivery history, RERA aggregate stats,
revocation history, complaint pattern, and related projects.

`source_entity_ids`

Backend-curated prefetch list. It is sorted, deduped, and filtered to nodes that
exist in the current KG. UI code should use this first for lightweight prefetch.
If it is empty, fall back to the individual IDs.

## Dynamic UI Pattern

The UI should use a two-pass render:

1. Fast first paint:
   - Render the property card/detail from normal response fields.
   - Do not block first paint on extra graph calls.

2. Evidence expansion:
   - Fetch KG nodes from `source_entity_ids`.
   - Build available sections from returned facts and source panels.
   - Use confidence/source metadata to choose what to show.
   - Hide unsupported sections.

Example shape:

```ts
async function loadEvidenceBundle(refs: KgEntityRefs) {
  const ids = refs.source_entity_ids?.length
    ? refs.source_entity_ids
    : [
        refs.property_entity_id,
        refs.society_entity_id,
        refs.area_entity_id,
        refs.builder_entity_id,
      ].filter(Boolean);

  const nodes = await Promise.all(ids.map((id) => getKnowledgeNode(id)));
  return buildDynamicSections(nodes);
}
```

`buildDynamicSections` should be the only place that maps KG facts into UI
sections. Components should receive already-shaped sections, not inspect every
raw fact themselves.

## Recommended Section Builder

Create a small frontend module later:

```text
frontend/src/features/knowledge/
  kgApi.ts
  kgSections.ts
  kgTypes.ts
```

Suggested responsibilities:

- `kgApi.ts`: fetch node, neighbors, subgraph.
- `kgSections.ts`: convert facts/source panels into display sections.
- `kgTypes.ts`: frontend-only view models such as `DynamicEvidenceSection`.

Suggested view model:

```ts
type DynamicEvidenceSection = {
  id: string;
  kind: "rera" | "nearby" | "reviews" | "community" | "builder" | "area" | "risk" | "market";
  title: string;
  confidencePct: number;
  sourceTypes: string[];
  primaryFacts: SourceItem[];
  secondaryFacts: SourceItem[];
  missing?: string[];
};
```

This keeps rendering simple:

```tsx
{sections.map((section) => (
  <EvidenceSection key={section.id} section={section} />
))}
```

## Rendering Rules

Prefer these rules over static cards:

- Show a section only when it has at least one strong fact or a useful explicit
  gap.
- Prefer source-backed facts over inferred copy.
- Prefer RERA facts for legal/project truth.
- Prefer Google/nearby facts for map-backed nearby context.
- Prefer community/review summaries for resident sentiment, not for legal truth.
- Show confidence/source badges in compact form.
- Let users expand details instead of putting every fact in the first viewport.

Do not do this:

```tsx
<SchoolCard />
<MetroCard />
<LegalCard />
<AmenitiesCard />
```

Do this:

```tsx
const sections = buildDynamicSections(nodes, sourcePanels);
return <EvidenceRail sections={sections} />;
```

The second version lets the same UI handle a luxury apartment, a plotted
development, a builder profile, and a future buy-vs-rent benchmark without
rewriting the page each time.

## Source Panels vs KG Nodes

`source_panels` in the property detail response is already a backend projection
of important facts. Use it for first detail-page evidence rendering.

The preferred property-page endpoint is:

```http
GET /api/properties/{id}
```

It returns the flat property detail plus:

```ts
type PropertyDetailResponse = {
  property: Property;
  entity_refs: KgEntityRefs;
  evidence: PropertyEvidenceResponse;
  // other detail fields
};
```

The standalone dynamic evidence endpoint remains useful for prefetching evidence
for search, compare, and shortlist surfaces:

```http
GET /api/properties/{id}/evidence
```

It returns backend-shaped evidence sections:

```ts
type PropertyEvidenceResponse = {
  property_id: string;
  entity_refs: KgEntityRefs;
  serving_bundle_version?: string;
  sections: EvidenceSection[];
};

type EvidenceSection = {
  kind: string;
  title: string;
  summary: string;
  subtitle: string;
  priority: number;
  confidence_pct: number;
  source_types: string[];
  entity_ids: string[];
  items: SourceItem[];
  missing: string[];
};
```

Use the endpoint this way:

- Detail/property page should use `GET /api/properties/{id}` and render
  evidence rail/cards from `response.evidence.sections`.
- Search, compare, shortlist, and side panels can use
  `GET /api/properties/{id}/evidence` or the batch endpoint when they do not
  need the full property detail payload.
- Search/list/compare prefetch should use:

```http
POST /api/properties/evidence/batch
```

with:

```json
{"property_ids":["discovered-prestige-park-grove-3bhk"],"limit":10}
```

The UI should render `EvidenceSection` objects directly. It should not decide
whether a fact is RERA proof, Google context, resident sentiment, market trail,
or an area externality. The backend owns that grouping.

`kg_entity_refs` is for the next layer:

- drill into a KG node,
- fetch neighbors,
- compare the society with adjacent projects,
- inspect builder lineage,
- show all facts behind one claim,
- build future dynamic cards without waiting for detail API changes.

Good rule:

- Detail page initial source trail: use `source_panels`.
- Expandable graph drilldown or dynamic side panel: use `kg_entity_refs`.
- Compare/shortlist workflows: use both.

## Example: Dynamic Nearby Card

Backend can return a `Nearby` source panel with facts like:

- `nearby_schools`
- `nearby_metro_stations`
- `nearby_hospitals`
- `nearby_fitness`
- `nearby_eateries`
- `nearby_tech_parks`

Nearby facts are source-backed and may include per-value attribution in
`SourceItem.attributions`. Prefer those per-place links for bullet rows; use
`SourceItem.source_url` only as the primary/fallback link.

The UI should not have five permanent cards. It should render only populated
groups:

```ts
const nearby = panel.items
  .filter((item) => item.values?.length)
  .map((item) => ({
    title: item.label,
    places: item.values,
    sourceUrl: item.source_url,
    confidencePct: item.confidence_pct,
  }));
```

If a society has schools and hospitals but no park evidence, show schools and
hospitals only. Do not show an empty parks card.

## Example: Dynamic Risk Card

Area and society facts can both contribute to risk:

- area traffic reality,
- waterlogging detail,
- RERA complaints,
- builder delay history,
- review complaints.

The UI should group by user-understandable risk, not by internal fact key:

```text
Risk
  Traffic: area fact + review complaints
  Waterlogging: area fact
  Legal: RERA complaints + revocations
  Builder: delay/revocation aggregate
```

This allows a future scorer to add new risk facts without adding new static UI
slots.

## Caching Guidance

Use `kg_entity_refs` as cache keys:

- cache `GET /api/knowledge/nodes/{id}` per ID,
- invalidate when serving bundle version changes,
- prefetch `source_entity_ids` for top search results after first paint,
- prefetch detail graph data on card hover or visible shortlist items.

Do not cache by display name. Names can change; graph IDs are the API handles.

## Current Gaps To Fix Later

The current KG refs are dereferenceable, but the next UI/backend iteration
should improve:

- relation-aware neighbor responses so the UI can say "built by", "located in",
  or "similar to" without guessing,
- a batch node endpoint to fetch multiple `source_entity_ids` in one request,
- a graph evidence endpoint that returns already-ranked dynamic sections,
- serving-bundle version in API responses so frontend caches can invalidate
  cleanly,
- canonical alias metadata so UI can distinguish RERA canonical IDs from display
  alias IDs when needed.

Do not block on these before using `kg_entity_refs`. They are improvements, not
requirements for the first dynamic UI pass.
