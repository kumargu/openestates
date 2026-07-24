# OpenEstates page-fit mocks

## Purpose

This document records where the Levels.fyi-inspired visualization ideas fit in
the existing OpenEstates product. It preserves the intent of
`openestates-page-fit-mocks.canvas.tsx` without making the canvas a dependency.

All values shown in the mocks are illustrative product data, not current market
claims. Production versions must use DAG-backed serving facts or deterministic
computations over those facts.

## Surface 1: Results insight

**Routes:** `/` and `/results`  
**Status:** Addition to a current page  
**Placement:** After the interpreted query and before the ranked property
universe  
**Estimated lift:** Medium

### Buyer question

Which credible market paths satisfy my intent, and why do they differ?

### Mock

For a query such as `quiet 3BHK near good schools under ₹2.5 Cr`, show a compact
market read before individual homes:

- total matching homes and number of credible area paths
- one distinction per area, such as best proof, more space, or lower commute
- comparable median or price band
- matching-home count and evidence strength
- a short sentence explaining the tradeoff across areas

Ranked property cards remain below this insight and continue to show concise
reasons with receipts.

### Data contract

- parsed local search intent
- ranked local results
- Area Tracker aggregates cut to the same BHK, budget, and possession filters
- evidence coverage and freshness

Do not ship this from seed averages. Add it after percentile aggregates are
available in the serving bundle.

## Surface 2: Property market position

**Route:** `/property/:id`  
**Status:** Addition to a current page  
**Placement:** After the property hero and before the evidence stack  
**Estimated lift:** Low

### Buyer question

Where does this home's price sit among truly comparable homes?

### Mock

Show one horizontal market-position band:

- p10-p90 comparable range
- p25-p75 typical range
- median marker
- `This home` marker
- concise read such as `Priced 3.4% below median`
- sample size, refresh date, and peer-set caption

Below it, explain only the largest price drivers, for example floor, readiness,
or carpet area. Each driver names its source or derivation.

### Data contract

The detail contract already has `area_price_range` and `market_activity`.
Production quality additionally needs:

- explicit comparable-set definition
- percentile values rather than only low/high
- sample size and observation window
- deterministic price-driver derivations
- source lineage and confidence

This is the recommended first implementation because it adds decision context
without creating a new navigation surface.

## Surface 3: Monthly reality

**Route:** `/property/:id/plan`  
**Status:** Addition to a current page  
**Placement:** A third Plan view beside Net worth and Payoff  
**Estimated lift:** Medium

### Buyer question

What does this home actually cost my household each month?

### Mock

Use a shared monthly-cost stack:

- EMI
- maintenance
- annual ownership costs divided monthly
- commute-time burden, clearly marked as derived
- effective monthly load and share of take-home income

Keep user assumptions in a compact rail: income, down payment, office, loan
terms, and optional time value. A short OpenEstates read explains the dominant
tradeoff and can compare it with one shortlisted alternative.

### Data contract

- existing Plan model
- property maintenance and ownership facts
- user-supplied financing assumptions
- commute destination and deterministic travel estimate
- explicit formula and assumption lineage for derived burden

Commute cost must remain optional. Do not present a subjective time valuation
as a market fact.

## Surface 4: Area asset page

**Route:** `/areas/:id`  
**Status:** New page  
**Entry points:** Area Tracker bubbles and search area context  
**Estimated lift:** Medium

### Buyer question

What kind of property market and daily life does this area offer?

### Mock

Treat an area like an asset page:

- concise area thesis
- median price, tracked societies, and evidence strength
- price distribution or trend with a fair-cut caption
- daily-life receipts for metro, traffic, water, schools, and other available
  facts
- a small set of DAG-generated society paths such as best proof, family fit, or
  value

The page should explain market shape and tradeoffs, not become a generic
locality guide.

### Data contract

`GET /api/areas/:id` and the Area Tracker endpoint already exist. Converge them
onto the same serving-bundle facts and add:

- comparable price distributions
- society coverage and freshness
- structured externality evidence
- DAG-backed society collections

This is the recommended second major surface because it gives Area Tracker a
real destination.

## Surface 5: Society asset page

**Route:** `/societies/:slug`  
**Status:** New page  
**Entry points:** Society names on results and property pages  
**Estimated lift:** Medium

### Buyer question

What does this society consistently do well, what is the tradeoff, and how
strong is the proof?

### Mock

Show:

- society thesis and best-fit buyer intent
- delivery state and compact proof labels
- decision dimensions such as maintenance, family fit, connectivity, and price
  proof
- resident themes with mention counts and source drill-down
- society price band compared with its area
- currently available homes

Avoid a generic score dashboard. Dimensions should appear only when supported,
and each signal should have one primary location with evidence available on
drill-down.

### Data contract

`GET /api/societies/:slug` exists, but the response should first converge onto
the DAG evidence model:

- society-scoped sourced facts
- derived buyer-surface signals from config
- resident-theme provenance and freshness
- comparable prices and listing availability
- stable property and source entity references

## Shared interaction and visual rules

- Use one decision headline and one primary visualization per section.
- Keep controls close to the peer-set definition they change.
- Show sample size, observation window, freshness, and scope on every aggregate.
- Use shared axes for comparison rather than repeating isolated metric cards.
- Reserve color for meaning: selection, positive evidence, caution, or risk.
- Let aggregates open into receipts; do not duplicate receipt detail inline.
- Hide unsupported sections instead of showing synthetic defaults.
- Preserve search state and provide a clear next action from every surface.
- Keep OpenEstates calm and editorial; the adaptation must not look like a
  compensation dashboard reskinned for property.

## Suggested delivery sequence

1. Add **Property market position** using the current detail contract, then
   strengthen the percentile and lineage fields.
2. Build the **Area asset page** and link Area Tracker and search context to it.
3. Add **Results insight** after serving-bundle percentile cuts are available.
4. Add **Monthly reality** when commute assumptions and formulas are explicit.
5. Add the **Society asset page** after its endpoint is fully DAG-backed.

## Acceptance test for any mock becoming product

A surface is ready only when:

- its headline can be traced to facts
- its peer set is explicit and reproducible
- sample size and freshness are visible
- thin evidence reduces or removes the claim
- derived values expose assumptions
- the user can open the underlying receipts
- the visualization changes a buyer decision rather than merely adding polish
