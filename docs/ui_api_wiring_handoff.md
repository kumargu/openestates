# UI API Wiring Handoff

This is the current backend-to-UI contract for the lake-backed OpenEstates
engine. Use this as the starting point for wiring property pages, search
results, compare surfaces, and dynamic evidence cards.

## What We Built

The backend is now a lake-backed, proof-first property engine instead of a
static listing API.

- **Lake/S3-shaped storage**: durable artifacts are modeled under `data/lake`
  with manifests and Parquet-backed serving assets. Local layout is meant to
  mirror the future S3 layout.
- **Asset DAG**: RERA, Google, project enrichment, source fan-in, KG view, and
  serving bundle materialization run through backend asset executors and run
  manifests.
- **Serving bundle**: the request path loads the promoted
  `search_serving_bundle` first. Legacy `data/knowledge` remains fallback and
  context, not the primary UI data path.
- **Search engine**: `/api/search` is local and deterministic. It combines local
  property recall, serving bundle recall, lightweight semantic recall, KG/serving
  facts, structured intent parsing, and proof-backed explanations.
- **Dynamic evidence graph**: property cards and details carry stable
  `kg_entity_refs`; property detail now also carries `evidence.sections`, which
  are backend-shaped cards the UI can render directly.
- **Google enrichment**: Google review links, ratings, review snippets, and
  focused nearby categories are available as sourced evidence.
- **Community summary layer**: Google/review snippets are converted into
  structured community facts. Reddit is still a future source, but the abstraction
  is shared.
- **Admin/runtime controls**: backend can reload serving bundles and trigger/read
  asset runs through admin endpoints.

Important product rule: the UI should render from available evidence and hide
weak/missing cards. Cards are dynamic. Do not build fixed school/metro/legal
sections that show empty placeholders.

## Primary UI Flow

For the property detail page, use one call:

```http
GET /api/properties/{propertyId}
```

Render the page from:

- `property`: flat listing facts for header, price, BHK, size, builder, etc.
- `entity_refs`: graph IDs for drill-down and future dynamic panels.
- `evidence.sections`: canonical dynamic evidence cards.
- `external_reviews`: compact Google rating/link/count for header-level display.
- `rera`, `builder_portfolio`, `themes`, `tradeoffs`, `market_activity`: richer
  already-shaped detail blocks where useful.

Do not make the property page call `/api/knowledge/*` for first paint. Use KG
endpoints only for drill-down interactions.

## Endpoints For UI

### Health

```http
GET /api/health
```

Use only for dev/server status.

### Property List

```http
GET /api/properties
```

Returns `PropertyCard[]`.

Use for browse/list surfaces. Important fields:

- `id`
- `title`
- `area`
- `price`, `price_per_sqft`, `bhk`, `sqft`, `carpet_area_sqft`
- `society_name`, `builder_name`
- `google_rating`, `google_review_count`, `google_reviews_url`
- `match_*` fields are not present here; they are search-only.
- `kg_entity_refs`

### Search

```http
GET /api/search?q=3bhk%20greenery%20whitefield
```

Returns:

```ts
type SearchResponse = {
  query: string;
  intent: SearchIntent;
  results: SearchResultCard[];
  area_context?: AreaDetail;
  total_results: number;
  knowledge_context?: KnowledgeContext;
};
```

`results[]` is a flattened property card plus:

- `match_score`
- `match_label`
- `match_reason`
- `match_explanation`
- `semantic_score`
- `confidence_score`

UI guidance:

- Use `match_reason` as the short result-card explanation.
- Use `match_explanation.preference_coverage` for expanded "why this matched".
- Use `knowledge_context.learning_gaps` as explicit "we need more data" copy,
  not as a failure.
- Do not infer proof from `semantic_score`; it is recall/order support, not a
  source-backed claim.

### Property Detail

```http
GET /api/properties/{propertyId}
```

This is the canonical property-page endpoint.

Backend shape:

```ts
type PropertyDetailResponse = {
  property: Property;
  entity_refs: KgEntityRefs;
  evidence: PropertyEvidenceResponse;
  society: Society | null;
  area: AreaDetail | null;
  themes: CompareThemes;
  tradeoffs: TradeoffsResponse;
  market_activity: MarketActivityResponse;
  similar_properties: PropertyCard[];
  rera?: ReraInfo;
  builder_portfolio?: BuilderPortfolio;
  source_panels?: SourcePanel[];
  external_reviews?: ExternalReviews;
};
```

First frontend wiring change: add `evidence?: PropertyEvidenceResponse` to
`frontend/src/lib/types.ts` under `PropertyDetailResponse`. The backend already
returns it.

### Dynamic Evidence

For detail pages, prefer `response.evidence.sections` from
`GET /api/properties/{id}`.

For search/compare/shortlist prefetch, use:

```http
GET /api/properties/{propertyId}/evidence
POST /api/properties/evidence/batch
```

Batch request:

```json
{
  "property_ids": ["discovered-prestige-lavender-fields-3bhk"],
  "limit": 10
}
```

Evidence response:

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

type SourceItem = {
  entity_id: string;
  key?: string;
  label: string;
  value: string;
  values?: string[];
  source_type: string;
  source_url?: string;
  attributions?: SourceAttribution[];
  confidence_pct: number;
  learned_at: string;
};
```

Known section kinds:

- `rera`: official RERA truth, legal/delivery facts.
- `market`: builder/project market trail, price, status, official/map URLs.
- `nearby`: Google-backed nearby schools, metro, hospitals, gyms, eateries,
  tech parks/offices.
- `reviews`: Google rating, review count, review link, review highlights.
- `community`: computed resident/community themes from review evidence.
- `area`: area traffic, waterlogging, metro, schools, externality context.

Render order should follow `priority`.

### Areas

```http
GET /api/areas
GET /api/areas/{id}
```

Use for locality pages, filters, or area context cards. Property detail already
embeds `area`, so do not call this again for first paint.

### Knowledge Graph Drill-Down

Use these for optional expand/debug/drill-down, not primary page rendering:

```http
GET /api/knowledge/nodes?type=society
GET /api/knowledge/nodes/{id}
GET /api/knowledge/nodes/{id}/neighbors
GET /api/knowledge/nodes/{id}/subgraph?depth=2
GET /api/knowledge/path?from=...&to=...
GET /api/knowledge/compare?a=...&b=...
GET /api/knowledge/coverage?type=society
GET /api/knowledge/nodes/{id}/similar?top_n=5
GET /api/knowledge/embeddings/stats
```

Use `kg_entity_refs.source_entity_ids` as the prefetch list. Do not build graph
IDs in the browser from names.

### Admin / Pipeline

These are not user-facing UI endpoints:

```http
GET  /api/admin/asset-runs/current
POST /api/admin/asset-runs
POST /api/admin/serving-bundle/reload
POST /api/admin/reload-knowledge
```

Use only for internal admin/dev tooling.

## How To Render Evidence Cards

Use a generic evidence renderer:

```tsx
const sections = detail.evidence?.sections ?? [];

return sections
  .filter((section) => section.items.length > 0 || section.missing.length > 0)
  .sort((a, b) => a.priority - b.priority)
  .map((section) => <EvidenceSection key={section.kind} section={section} />);
```

Inside each section:

- Show `summary` as the card headline.
- Show `subtitle` as compact context.
- Show top `items` as expandable rows or bullets.
- Use `values` for bullet lists when present.
- Use `source_url` or `attributions[].source_url` for "open source" links.
- Show `missing` only as small data-gap text, not as a scary error.
- Hide the entire section if it has no items and no useful missing data.

Do not hardcode cards like `<SchoolCard />`, `<MetroCard />`,
`<LegalCard />`. Build one polished evidence-card component that can render any
section kind.

## What To Avoid

- Do not call Gemini, Claude, Reddit, Google, or any network enrichment from the
  UI request path.
- Do not read `data/knowledge` or `data/lake` from frontend code.
- Do not depend on `source_panels` for new UI. It is a compatibility field.
- Do not treat `semantic_score` as proof.
- Do not display low-confidence data as a factual claim without source/context.
- Do not show static empty cards. Missing evidence should remove the card or
  show a compact "data gap" row.

## Current Gaps To Remember

- Reddit ingestion is not reliable yet; community cards currently lean on Google
  review snippets and computed themes.
- Official/MagicBricks image and price-comps enrichment is still future work.
- Some frontend types need to catch up with backend detail response:
  `PropertyDetailResponse` should include `evidence?: PropertyEvidenceResponse`.
- `source_panels` is still used in parts of the current frontend; migrate new
  detail UI to `evidence.sections`.

## Dev Commands

Run backend:

```bash
OPENESTATES_API_ADDR=127.0.0.1:4000 cargo run --manifest-path backend/Cargo.toml --bin openestates-api
```

Run smoke test:

```bash
./tests/smoke_test.sh 4000
```

Useful live checks:

```bash
curl -s http://127.0.0.1:4000/api/properties/discovered-prestige-lavender-fields-3bhk \
  | jq '.evidence.sections[] | {kind, summary, itemCount: (.items | length)}'

curl -s 'http://127.0.0.1:4000/api/search?q=3bhk%20greenery%20whitefield' \
  | jq '.results[:5] | map({id, title, match_score, match_reason})'
```

## Suggested Next UI Wiring Order

1. Add `evidence?: PropertyEvidenceResponse` to `PropertyDetailResponse`.
2. Build a generic `EvidenceSection` renderer for `detail.evidence.sections`.
3. Replace property-page static/source-panel evidence cards with the generic
   renderer.
4. Use batch evidence prefetch for search/compare only after the detail page is
   clean.
5. Add visual states for confidence, source links, missing evidence, and
   expandable item rows.
