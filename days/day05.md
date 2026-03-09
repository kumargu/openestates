# days/day05.md

# OpenEstates v2
## Day 5 – Product Surfaces, Transparency Widgets, and Data Strategy Reset

Before starting today, read:
- `CLAUDE.md`
- `LEARNING.md`
- `DECISIONS_INDEX.md`
- the latest OpenEstates v2 concept/design document

Today is a **replanning and product-shaping day**, not a feature-building sprint.

We are intentionally slowing down before building more code because two things have become clear:

1. OpenEstates v2 is now a **transparency-first web product**, not a terminal-first AI prototype.
2. “Transparency” and “data strategy” are still too abstract to build well unless we define them concretely.

So Day 5 is about **making the product buildable**.

We will not try to implement the whole app today.  
We will define the exact surfaces, widgets, and data shape the app needs, clean up old assumptions, and only then set up the initial web foundation if time remains.

---

## 1. Why Day 5 is being replanned

Earlier days were useful for learning:
- signal extraction
- context graph ideas
- agent boundaries
- TUI experimentation

But the product direction has changed.

OpenEstates v2 is now centered on:
- web-first experience
- transparent property discovery
- context-based search
- premium listing / asset pages
- shortlist and comparison
- market legibility

That means the most important thing today is not “more code.”

The most important thing is to answer:

- What are the actual product surfaces?
- What are the exact transparency widgets?
- What data fields do they require?
- What kind of seed dataset is needed to make the product feel real?
- Which parts of the old prototype are still worth keeping?

This day should reduce ambiguity and future rework.

---

## 2. Day 5 Goal

By the end of Day 5, we should have:

1. a concrete definition of the **4 core product pages**
2. a concrete definition of the first **transparency widgets**
3. a practical **data strategy** for v2
4. a list of what old prototype work should be preserved, archived, or removed
5. optionally, a very small web-app skeleton if the above is completed cleanly

The main output of today is **clarity**, not velocity.

---

## 3. Critical instruction to Claude

Claude, today you are explicitly asked to do three things:

### 3.1 Challenge old work if needed
If any earlier plan, file, or subsystem is no longer aligned with the v2 product direction, say so clearly.

You are allowed to recommend:
- deleting dead work
- archiving TUI-related code
- simplifying folder structure
- rewriting old assumptions

Do not preserve old code just because it exists.

### 3.2 Make “transparency” concrete
Do not leave transparency as philosophy.
Convert it into:
- actual widgets
- actual page sections
- actual data fields
- actual user-visible explanations

### 3.3 Make data strategy practical
Do not leave data strategy as “we’ll figure it out later.”
Define:
- what data is needed now
- what can be synthetic
- what should be curated
- what can be extracted later
- how we avoid overcomplicated crawling early

---

## 4. Main deliverable for Day 5

Create a new product blueprint document, for example:

- `docs/openestates_v2_surfaces_and_data.md`

This document should become the implementation reference for the next several days.

It should contain at least the following sections:

### A. Product surfaces
Define the 4 core pages:
- homepage / search
- results page
- property detail page
- shortlist / compare page

For each page, describe:
- the user’s emotional/job-to-be-done
- what the page must show
- what makes the page feel transparent
- what should not be on the page yet

### B. Transparency widgets
Define the first 4–6 concrete transparency widgets.

Each widget should include:
- widget name
- what it shows
- why it matters
- what data fields it needs
- where it appears (results page, property detail page, shortlist page)

Examples of transparency widgets to evaluate:
- “Why this property for you”
- “Price vs area median”
- “Area signals”
- “Society / livability summary”
- “Market activity / demand signal”
- “Tradeoffs to know”

These are examples, not a final required list.

### C. Listing schema / property schema
Define the data model required to render the first web UI properly.

This should include:
- base listing fields
- pricing context fields
- area context fields
- society / livability fields
- user-fit explanation fields
- image/media fields
- optional future fields

The schema should be practical and product-facing, not purely technical.

### D. Data strategy
Define the first realistic data approach for v2.

This must answer:
- what can be mocked
- what should be manually curated
- what should be semi-curated
- whether we should use small controlled extraction from listing websites
- how to think about dm8.in as a reference/data-source candidate
- what not to attempt yet

The expected conclusion should likely be:
- use a small high-quality seed dataset first
- do not build broad crawling yet
- do not rely on fully synthetic data alone
- use smart extraction and AI enrichment later in a controlled way

### E. Codebase cleanup recommendation
Write a short section stating:
- what from the old TUI / agent prototype is still useful
- what should be archived
- what should be removed
- what should be preserved as internal engine logic

This section is important because we do not want legacy prototype assumptions to pollute v2.

---

## 5. Concrete outputs required today

By the end of Day 5, produce the following:

### 5.1 A v2 surfaces and data document
This is the main output.

### 5.2 A short cleanup note
Example file:
- `docs/day05_cleanup_note.md`

This should summarize:
- which parts of the old codebase no longer match the new product
- what should happen to them
- whether any files were actually moved/removed today

### 5.3 Optional minimal web foundation
Only if the blueprint work is completed cleanly and there is time:
- create `frontend/` and `backend/` folders
- initialize a minimal React app
- initialize a minimal Axum app
- create placeholder routes/pages only

Do not let bootstrapping frontend/backend distract from the blueprint work.

The product clarity work comes first.

---

## 6. What should NOT be built today

Do not build today:
- full React UI implementation
- full Axum API implementation
- ranking engine
- real context-based search
- OpenFang flows
- real shortlist persistence
- validation workflows
- bid system
- crawling pipeline
- review summarization pipeline

Today is about **defining the shape of the product and the data it needs**.

---

## 7. Guidance on transparency widgets

Claude, when defining widgets, avoid abstract or vague phrases.

A good widget definition should answer:

- What exactly does the user see?
- What decision does it help them make?
- What raw or derived data powers it?
- How does it reduce ambiguity?

For example, do not write:
- “show price transparency”

Instead write something like:
- “Price vs Area Median widget shows asking price, computed price/sqft, area median price/sqft, and whether the listing is above or below the median band”

That is concrete enough to build.

---

## 8. Guidance on data strategy

Claude, when defining the Day 5 data strategy, assume the following posture:

- We want a **credible localhost product feel**, not huge scale
- Pure synthetic data is useful for engine testing but not enough for product feel
- We should likely create a **small, high-quality, curated or semi-curated seed dataset**
- We may later use controlled extraction from a few source types
- dm8.in should be treated as a **reference listing page / possible selective source candidate**
- We should avoid broad or naive crawling at this stage
- AI/OpenFang may later help with normalization, summarization, and enrichment, not as a primary uncontrolled crawler

The data plan should be grounded and practical.

---

## 9. Suggested structure of the main Day 5 document

The new `openestates_v2_surfaces_and_data.md` document should roughly include:

1. Why we are resetting before coding more
2. The 4 core product pages
3. Transparency widgets
4. Listing / property schema
5. Area / society / market signal schema
6. Data acquisition and seed dataset strategy
7. What is mocked vs curated vs extracted later
8. What old prototype work to retain or archive
9. Recommended next implementation sequence

This will give us a real implementation map.

---

## 10. Optional code work if clarity is achieved

If the surfaces/data document is strong and there is time left, then you may:

- initialize `frontend/` with a minimal React app
- initialize `backend/` with a minimal Axum app
- add placeholder routes matching the 4 product pages:
  - `/`
  - `/results`
  - `/property/:id`
  - `/shortlist`

These can be placeholders only.

But only do this after the product blueprint is written well.

---

## 11. Manual verification checklist

By the end of Day 5, verify:

- the new v2 surfaces/data document exists
- the transparency widgets are concrete enough that a designer/engineer could build them
- the data strategy is realistic and not hand-wavy
- the cleanup note clearly states what happens to the old TUI prototype
- if code bootstrapping happened, it reflects the new web-first direction

---

## 12. Expected success definition

At the end of Day 5, we should feel much clearer about:
- what OpenEstates v2 actually looks like
- what “transparency” means in the UI
- what data the product needs
- what we should build next
- and what from the old prototype should be left behind

If this day is successful, then Day 6 onward can become much more focused and efficient.

That is the real value of today.