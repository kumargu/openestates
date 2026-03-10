# Search Architecture Brainstorm

## Our Ideas + Claude Analysis

[BRAINSTORM SESSION — OpenEstates Search Architecture]

We're rethinking search for OpenEstates from the ground up. The goal is to build something genuinely different from traditional property search (filter → SQL → paginated results). Here are the raw ideas and engineering analysis. We need your product vision input.

---

## The Core Insight

We CANNOT and SHOULD NOT try to crawl and index everything upfront. Instead:
- Parse the user's natural language search context
- Extract the dimensions that matter (area vibe, budget sensitivity, commute, schools, lifestyle)
- For each dimension, check if we have cached knowledge (embeddings on disk)
- On cache miss: query Reddit API, Google Places API, Gemini API to fetch and synthesize
- Embed the results, store locally (disk now, S3 vector store later)
- Over time, the system builds incremental knowledge — first queries are slow, subsequent ones are fast

This means the system **learns the city** as users search. It doesn't need a massive upfront crawl.

---

## Idea 1: On-Demand Context Assembly

Traditional flow:
```
user query → parse filters → SQL WHERE → ranked results
```

Our flow:
```
user query → NL intent extraction → identify knowledge dimensions needed
  → for each dimension:
      cache hit? → use cached embeddings
      cache miss? → fetch from Reddit/Google/Gemini → embed → cache
  → assemble context vector from all dimensions
  → score/rank properties against assembled context
  → explain ranking (transparency!)
```

The key difference: we don't pre-index properties with fixed attributes. We build a **contextual scoring surface** on-demand from real-world signals.

Example: "3BHK near Whitefield with good schools and calm society"
- Dimension: area_vibe(Whitefield) → fetch Reddit threads about Whitefield living
- Dimension: schools(Whitefield) → fetch Google reviews of schools nearby
- Dimension: society_reputation → fetch Reddit/Google reviews of specific societies
- Dimension: budget_context → check recent price trends from cached market data
- All dimensions get embedded → context vector
- Properties scored against this context vector
- Ranking explanation: "Ranked #1 because Prestige Lakeside scores high on school proximity (4.5★ avg within 2km) and residents report calm environment (Reddit sentiment: 87% positive)"

---

## Idea 2: SKILLS-like Prefix Tree Traversal (instead of traditional search)

Claude SKILLS work like prefix trees — you type a path and it narrows down to the right skill. What if our search worked the same way?

Instead of keyword search or filter search, the user's intent creates a **traversal path through an embedding space**:

```
Intent: "family home in East Bangalore"
  → City: Bangalore [root node]
    → Region: East Bangalore [narrows]
      → Lifestyle: Family [narrows further]
        → Priorities: Schools, Safety, Green spaces [leaf dimensions]
```

Each node in the traversal has associated embeddings (cached knowledge). The path through the tree IS the search. No SQL. No Elasticsearch. Just embedding similarity at each level.

This means:
- Search is naturally explainable (the path shows why)
- New dimensions can be added without schema changes
- The tree grows organically as users explore
- Similar to how recommendation systems work, but transparent

---

## Idea 3: Reddit/Reviews as Proprietary Data Moat

Reddit threads like "best societies in Whitefield for families" are GOLD for property search. No one else embeds and serves this as structured context.

Architecture:
1. Reddit API → fetch threads matching area/topic patterns
2. Clean and chunk the content
3. Generate embeddings (OpenAI/local model)
4. Store on disk as `.npy` files with metadata JSON (phase 1)
5. Later: migrate to S3 with vector index (Pinecone/pgvector/Qdrant)
6. At query time: similarity search against cached Reddit knowledge
7. Use Gemini/Claude to synthesize relevant threads into area insights

This creates a moat: the more users search, the more area knowledge we accumulate.

---

## Idea 4: Incremental State Building

First-time penalty is acceptable if the system is honest about it:
- "Building knowledge about Whitefield... fetching area insights (30s)"
- Next user who searches Whitefield gets instant results
- System can also pre-warm popular areas in background

State grows in layers:
1. **Seed state**: our existing curated data (20 properties, area profiles)
2. **User-triggered state**: Reddit/Google fetches triggered by searches
3. **Background state**: overnight enrichment of popular areas
4. **Community state**: (future) user-contributed signals

---

## Questions for You (Product Visionary)

1. Does this on-demand architecture align with the transparency promise? The system can literally show "here's the Reddit thread that influenced this ranking."

2. How should we handle the cold-start UX? First queries will be slow (10-30s to fetch and embed). Is a loading state with "building area knowledge..." acceptable? Or should we pre-warm seed areas?

3. For the prefix tree traversal — should this be visible to the user (like breadcrumbs showing how search narrowed) or invisible (just powering the ranking)?

4. What's the MVP version of this? My suggestion:
   - Phase 1: Reddit API integration + disk embeddings + basic similarity search
   - Phase 2: Google Places integration + area synthesis
   - Phase 3: Prefix tree traversal + incremental caching
   - Phase 4: S3 vector store + background enrichment

5. How does this change the property detail page? If we have Reddit-sourced area insights, the detail page becomes much richer — "What residents say about this area" backed by real Reddit threads.

6. Does the SKILLS-like traversal concept resonate as a search UX? Or is it too abstract and we should keep a simpler NL-in, results-out flow?

---

Please think deeply about this. Challenge what doesn't work. Refine what does. Suggest what we're missing. This is a pivotal architecture decision for the product.


---

## ChatGPT (Product Visionary) Response

This is the right direction, with one important correction:

Do not make the product wait for raw knowledge assembly before it can be useful.
OpenEstates should feel instant first, then deepen intelligently.

Your architecture is strong if you frame it as:

fast product shell + on-demand enrichment + transparent evidence layer

not

query-time research engine that blocks search

That distinction matters a lot.

## What clearly works

### 1. This aligns strongly with the transparency promise

Yes — more than traditional search.

A normal portal hides its reasoning. Your approach can show:

- what user intent was understood

- which dimensions mattered

- what external evidence influenced ranking

- when the system is confident vs still learning

- which claims came from seed data vs Reddit vs Google reviews

That is very OpenEstates.

The strongest product form of this is not just “here’s the Reddit thread.” It is:

- Why this ranked

- What evidence supports it

- How fresh that evidence is

- How much confidence we have

That becomes a real trust surface.

### 2. On-demand city learning is a real moat

I agree with the core idea that you should not crawl everything first.

For OpenEstates, a full-city indexed approach is expensive, stale, generic, and not very differentiated. Your proposed model is better because it learns in proportion to real user demand.

That means:

- users teach the system what matters

- the city knowledge graph grows around actual search intent

- you accumulate hard-to-replicate neighborhood intelligence over time

- you avoid wasting effort on low-value parts of the city

That is the right startup posture.

### 3. Reddit and review synthesis can materially differentiate ranking

Yes, but only if you treat them as context evidence, not truth.

Reddit is excellent for:

- recurring concerns

- area vibe

- “who this area is for”

- hidden negatives

- emotional language

- pattern discovery

Google reviews are excellent for:

- schools

- hospitals

- parks

- society sentiment proxies

- place-level recurring complaints

This combination is much more valuable than generic listing filters.

### 4. Incremental cache-building is productively honest

Yes, but only if the first experience is still usable.

I like the idea that the system “learns the city as users search.” That is a compelling product story and a practical engineering strategy.

But the product cannot feel blocked or uncertain during that learning.

## What I would challenge

## 1. Do not replace all search with pure on-demand traversal

This is the biggest thing I would push back on.

You still need a stable property retrieval layer.

Not necessarily SQL-first in the old sense, but definitely:

- a property corpus

- structured baseline fields

- a candidate generation layer

- a ranking layer

The system should not try to discover both:

- what properties exist

- and how to contextualize them

entirely at query time.

That would be too slow, too fragile, and too hard to control.

The better architecture is:

Plain textuser query
→ intent extraction
→ candidate retrieval from structured property corpus
→ contextual enrichment layer (cached or fetched)
→ ranking + explanation
→ evidence-backed result presentation
So yes to contextual scoring on demand.
No to eliminating candidate retrieval discipline.

OpenEstates should be different from SQL-only search, but not allergic to structure.

## 2. The prefix-tree idea is powerful as an internal reasoning model, not a primary user-facing metaphor

The SKILLS-style traversal concept is strong, but I would not expose it too literally.

Users do not want to feel like they are navigating a semantic tree. They want to feel like:

- “this platform understood what I mean”

- “it narrowed intelligently”

- “I can see what it optimized for”

So the traversal should mostly live underneath.

The user-facing expression should be simpler:

- interpreted search chips

- search path summary

- ranking reasons

- evidence blocks

For example:

Plain textYou asked for:
Family-friendly • East Bangalore • Good schools • Calm society

We prioritized:
School access • Lower-noise surroundings • Strong society sentiment • Reasonable value
That is better than exposing a formal tree.

## 3. Cold start cannot be a 30-second blocking ritual for core search

A fully blocking “building knowledge…” experience will be acceptable only in very narrow cases.

For most users, 10–30 seconds is too long if nothing useful appears.

The right UX is:

### good cold-start UX

- return results fast from seed + structured property data

- show that enrichment is in progress

- progressively improve explanations and area intelligence

- update widgets as evidence arrives

Example:

Plain textShowing initial results from our current Bangalore dataset.
We’re enriching Whitefield with live area knowledge now.
Then later:

Plain textArea insights updated from community and place signals.
That is much better than making the entire experience wait.

## My recommended architecture shape

## A. Split search into three layers

### Layer 1: Candidate retrieval

Fast, deterministic, structured.

Input:

- area

- budget

- bhk

- possession

- basic hard constraints

Output:

- 20–100 plausible candidates

This can still be backed by normal structured filtering.

### Layer 2: Context assembly

This is where your real moat lives.

Input:

- parsed query intent

- missing dimensions

- cached knowledge state

Behavior:

- load cached embeddings and summaries where available

- fetch missing area/society/place knowledge selectively

- synthesize into reusable dimension-level evidence objects

Output:

- contextual evidence pack

### Layer 3: Ranking + explanation

Use the contextual evidence pack to score and explain.

Output:

- ranked properties

- reasons

- evidence references

- confidence levels

This gives you both speed and differentiation.

## B. Treat the knowledge system as a reusable “city intelligence layer”

Do not think of this only as search-time enrichment.

Think of it as building reusable knowledge objects such as:

- area vibe profile

- family suitability profile

- school access profile

- traffic pain profile

- calmness/noise profile

- society reputation profile

- greenery/open-space profile

- flooding/waterlogging caution profile

Those objects can power:

- search ranking

- property pages

- area pages

- shortlist comparison

- onboarding

- recommendations later

That is much more valuable than one-off query synthesis.

## C. Cache the right thing, not just embeddings

This is important.

Do not only cache vectors.

Cache a full reusable knowledge bundle:

JSON{
  "entity_type": "area",
  "entity_id": "whitefield",
  "dimension": "family_suitability",
  "summary": "Strong school ecosystem and established societies, but commute stress remains a real tradeoff.",
  "signals": [
    "good school density",
    "mixed commute experience",
    "premium society options",
    "traffic caution"
  ],
  "evidence": [
    {
      "source": "reddit",
      "title": "...",
      "snippet": "...",
      "url": "...",
      "freshness": "2026-03"
    }
  ],
  "embedding_path": "...",
  "confidence": 0.79,
  "updated_at": "..."
}
Embeddings are only one part.
The real product value is in the reusable summarized object plus evidence.

## Answers to your questions

## 1. Does this align with the transparency promise?

Yes, strongly — if you expose evidence carefully.

Best practice:

- do not dump raw thread clutter into ranking

- do show “what influenced this”

- do cite evidence categories

- do expose freshness

- do distinguish hard data from synthesized interpretation

Good product wording:

- “Residents frequently mention peak-hour traffic near Varthur Road.”

- “School access scores well based on nearby highly rated schools.”

- “Calm-society signal is moderate confidence — still learning from limited evidence.”

That is excellent transparency.

## 2. How should we handle cold-start UX?

My answer:

Never make the first result experience depend entirely on enrichment.

Use a 3-stage UX:

### Stage 1 — instant

Show seed-data-backed results immediately.

### Stage 2 — enriching

Show non-blocking progress:

- “Gathering area intelligence for Whitefield…”

- “Checking school and commute signals…”

### Stage 3 — upgraded

Refresh explanations/widgets when context arrives.

Also pre-warm a few areas because you already know what matters:

- Whitefield

- Sarjapur Road

- Bellandur

- HSR Layout

- North Bangalore / Hebbal

Those should never feel cold.

## 3. Should prefix traversal be visible?

Mostly invisible.

Expose it lightly as:

- interpreted intent chips

- maybe one “search path” summary

- maybe advanced “why these results” accordion

Do not make the main UI feel like a semantic directory browser.

## 4. What’s the MVP version?

Your phases are close, but I would reorder slightly.

### Phase 1 — Seeded candidate retrieval + Reddit-backed area intelligence

- keep current structured results

- add Reddit fetch + cache for area-level dimensions

- add reusable knowledge objects on disk

- use them first in results explanations and area panels

### Phase 2 — Society/place enrichment

- Google Places for schools, hospitals, parks, landmarks

- society-level and nearby-amenity context

- stronger property-page and shortlist evidence

### Phase 3 — Incremental context cache orchestration

- dimension cache checks

- background refresh

- confidence/freshness model

- visible “learning the city” system behavior

### Phase 4 — More advanced traversal and vector infrastructure

- prefix-like internal traversal

- reusable node graph / taxonomy

- S3 + vector store

- smarter prewarm and background enrichment

Why this order works:

- Phase 1 already creates visible product differentiation

- Phase 2 makes pages feel much richer

- Phase 3 improves system behavior and speed

- Phase 4 is infrastructure sophistication after product proof

## 5. How does this change the property detail page?

A lot — in a very good way.

The property page becomes not just a listing page but an evidence page.

Add sections like:

- What residents say about this area

- Common family concerns nearby

- School access around this property

- Traffic and commute reality

- What people like about this micro-market

- How confident this area profile is

This is much more defensible than generic portal copy.

The key is evidence discipline:

- synthesized summary first

- sources behind it

- freshness visible

- confidence visible

## 6. Does the SKILLS-like traversal resonate as UX?

As architecture, yes. As explicit UX, only lightly.

The strongest user-facing version is not “tree traversal.”

It is:

- “We understood your search in these dimensions”

- “We ranked using these priorities”

- “We found this evidence”

- “You can refine what matters”

That is the product language.

## What I think is missing

## 1. Confidence scoring must be first-class

If the system is learning on demand, confidence matters a lot.

Each dimension should carry:

- evidence count

- freshness

- source diversity

- confidence score

Without this, the system can sound too certain too early.

## 2. Freshness policy

Area and society knowledge ages differently.

You need explicit refresh logic:

- traffic complaints: refresh more often

- school reputation: slower refresh

- area vibe: moderate refresh

- flooding risk: seasonal refresh

- launch/construction activity: faster refresh

This will matter a lot later.

## 3. Entity resolution and canonicalization

You will quickly hit messy identity problems:

- Whitefield vs Varthur vs ECC Road

- society nicknames vs official names

- school aliases

- overlapping micro-markets

You need a canonical entity layer early, even if lightweight.

## 4. Evidence governance

Not every Reddit post deserves equal weight.

You need rules for:

- source quality

- deduping repeated complaints

- avoiding single-thread overfitting

- separating anecdote from recurring pattern

This is crucial for trust.

## 5. Product distinction between “facts” and “signals”

OpenEstates should explicitly separate:

### facts

- price

- sqft

- metro distance

- school distance

- days on market

### signals

- family-friendliness

- calmness

- society vibe

- traffic pain

- greenery feel

This distinction will make the transparency model much stronger.

## My recommendation in one sentence

Yes to on-demand contextual knowledge assembly, but only as an enrichment and ranking layer on top of fast structured candidate retrieval — not as a full replacement for disciplined property retrieval.

## The product framing I would use

OpenEstates search should feel like:

“We don’t just search listings. We assemble the context around your life, your tradeoffs, and the city — then show you homes with the evidence behind the ranking.”

That is a real product story.

## What I would do next, concretely

If you want the cleanest next move, I would define Day 16 around:

- a canonical list of search dimensions

- cache object schema for area/society intelligence

- clear separation between candidate retrieval and context enrichment

- Reddit-backed area intelligence MVP for 2–3 areas

- evidence + confidence model visible in results or property page

That would turn this from a good idea into a buildable architecture.
