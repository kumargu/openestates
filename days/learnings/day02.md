# Day 02 — Design Clarifications and Decisions

Key architectural decisions made after Day 2 review. Read this before starting Day 3.

---

## Core Philosophy (always keep this in mind)

OpenEstates is testing **context-based search vs traditional filter-based search**.

Traditional portals rank by: price · location · BHK · amenities

OpenEstates ranks by: buyer flexibility · seller urgency · timeline alignment · negotiation compatibility · risk tolerance · document safety · social friction

The system must answer:
> "Which buyer and seller are most likely to successfully close a deal?"

Not:
> "Which listings match the filters?"

---

## Evaluation Metrics (locked — these drive everything)

Three layers, in increasing importance:

| Metric | What it measures |
|---|---|
| **Precision@K** | Are the top K recommended properties among the most compatible according to hidden truth? Use K=5 and K=10. |
| **NDCG** | Do highly compatible listings appear earlier in the ranking? Measures rank quality, not just set membership. |
| **Simulated Closure Rate** | Most important. Does the contextual engine produce higher deal closure rates than baseline filter search? |

**The closure funnel:**
```
recommendation → visit probability → offer probability → closure probability
```

Each stage is driven by hidden compatibility scores. The matching engine output is compared against this hidden truth — never allowed to read it.

---

## Data Architecture — Two-File Split

The generator must produce two separate files:

**`data/synthetic_market.json`** — observable only
- What the matching engine is allowed to see
- Visible buyer, seller, property attributes
- No hidden fields

**`data/synthetic_market_truth.json`** — ground truth only
- Used exclusively by the simulator and evaluation layer
- Contains: `true_budget_limit`, `true_price_floor`, `true_urgency`, `hidden_area_flexibility`, compatibility scores
- The matching engine must never load this file

This separation ensures the contextual engine is tested on its ability to *infer* signals, not read them.

---

## Context Graph Design

Not a heavy graph DB. Lightweight structured in-memory model, serialized to JSON.

Conceptual edges:
- `Buyer → prefers → metro_proximity`
- `Buyer → dislikes → east_facing`
- `Buyer → flexible_on → budget`
- `Seller → owns → property`
- `Property → located_in → area`

Every signal on a node or edge must carry:
- `confidence` — how strongly to weight it
- `weight` — influence on scoring
- `timestamp` — when it was recorded
- `provenance` — source (conversation, onboarding, rejection, etc.)

May evolve into a formal graph structure later. Start simple.

---

## ZeroClaw

Agent runtime framework, introduced later. Helper layer only, not source of truth.

May be used for: signal extraction · match explanation · coach simulation · watcher agents

Must not: own durable state · be the source of truth · produce outputs stored as raw text

Prototype must run without ZeroClaw.

---

## Simulation Loop

**Now:** TUI drives each stage manually
- Generate market → Inspect → Run baseline → Run contextual → Simulate deals → Evaluate

**Later:** Automated end-to-end pipeline
```
generate market → run conversations → extract signals → run matching → simulate visits/offers → evaluate
```

---

## Coaching — Buyers First

Start with buyer coaching only. Seller context is simpler (urgency, visit tolerance, price flexibility) and partially covered by visible attributes.

**Buyer signals to extract from conversation:**
- Location preferences + flexibility
- Budget and stretch tolerance
- Commute/metro sensitivity
- Legal risk sensitivity
- Negotiation comfort
- Renovation tolerance
- Timeline urgency

**Seller signals** (add later): urgency · visit tolerance · negotiation style · possession flexibility · price flexibility

---

## Fixes Applied to Day 2 Code

### 1. Two-file output
`MarketGenerator.generate()` now writes:
- `data/synthetic_market.json` (visible attributes only)
- `data/synthetic_market_truth.json` (hidden attributes, keyed by entity id)

### 2. Buyer 1BHK distribution fixed
`preferred_bhk` now samples from `[1, 2, 3, 4]` with weights:
- 1BHK: 10% · 2BHK: 45% · 3BHK: 35% · 4BHK: 10%

### 3. Seller-to-property mapping is now 1:1
Each seller is assigned exactly one unique property. No two sellers share a property in the synthetic market.

---

## Day 3 Preview

Build the **hidden compatibility model**.

This function computes the true compatibility score between a buyer and a property, using hidden attributes from both. It remains invisible to the matching engine but is used by the evaluation layer to measure performance.
