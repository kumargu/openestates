# OpenEstates Matching Engine Prototype
## Day 3 – OpenFang Direction, Roles, and “High Action” Prototype Integration

Before starting today, read:
- CLAUDE.md
- LEARNING.md
- days/day01.md
- days/day02.md

This Day 3 is intentionally focused on **OpenFang**  https://github.com/RightNow-AI/openfang and the direction we are steering toward. We want “high action” early: an agent-like system that behaves like a broker would—building long-lived context from conversations, producing explainable matches, and proactively re-engaging users when new high-fit opportunities appear.

This is still a prototype running locally. We are not building a production backend yet. But we will design today in a way that won’t waste tokens later and will clearly map to our long-term architecture.

---

## 0) North Star and Long-Term Shape (why OpenFang exists in this project)

OpenEstates is a trust-first, context-driven resale transaction network. Our differentiator is **context-based search and matchmaking** over traditional filter-based search.

Traditional search: price + location + BHK filters.
OpenEstates: context graph (preferences + flexibility + readiness + risk tolerance + seller urgency + negotiation compatibility) → match ranking → explanation → nurturing.

The system must “learn” from conversations and user behaviors. We are not training our own foundation model. We are building a product-level learning loop:

Conversation → structured signals → context graph updates → matching score changes → outcomes → weight refinement.

OpenFang will be the agent runtime that powers these broker-like behaviors:
- Coach conversations with memory across sessions
- Signal extraction from messy language into structured updates
- Match explanations (why this match is good)
- Watchers / nurturers (notify when new high-fit matches appear)
- Memory compaction (keep profiles clean over time)

However, OpenFang must not become the source of truth for core business state. The authoritative state should live in our application’s storage (JSON/SQLite initially), so the system remains inspectable and framework-independent long-term.

---

## 1) Day 3 Goal

Integrate OpenFang as an **agent layer** in a minimal but real way:
- We will define the interfaces between our app and OpenFang (“contracts”).
- We will implement a small OpenFang “Hand” (or equivalent) that can:
  1) take a buyer profile + conversation snippet,
  2) output structured signal updates,
  3) update the buyer’s context graph in our store,
  4) generate a human explanation of what it learned (“why context changed”).
- We will keep matching logic minimal today. The goal is to validate the *learning loop shape* and OpenFang’s role.

At the end of Day 3, we should be able to run:
- Generate synthetic market (from Day 2)
- Pick a buyer via `/buyer <id>` in the chat interface
- Type messages as the buyer
- Run `/extract` to see context graph updated via OpenFang
- See a short narrative summary of what was learned

This proves we can “learn from conversations” in a structured way.

---

## 1.5) TUI Design — Chat-First Interface

Replace the menu-driven TUI with a chat-first single-screen layout.

```
┌─────────────────────────────────────────────────────────────────────┐
│  OpenEstates                         OpenFang: ● connected  10:23  │
├──────────────────────────────────────┬──────────────────────────────┤
│  Chat                                │  Buyer Context               │
│                                      │                              │
│  > /buyer buyer_0042                 │  buyer_0042                  │
│  Loaded: buyer_0042                  │  Budget: 80L – 1.2Cr         │
│  HSR / Bellandur, 3BHK, 6mo          │  Areas:  HSR, Bellandur      │
│                                      │  BHK:    [2, 3]              │
│  > I prefer something near metro,    │  ────────────────────────    │
│    but can stretch budget a bit if   │  Signals (2)                 │
│    the society is good.              │  metro_pref    high  0.8     │
│                                      │  doc_safety    high  0.9     │
│  > /extract                          │  ────────────────────────    │
│  Running OpenFang extraction...      │  Last diff                   │
│                                      │  + metro_pref  ↑ 0.6→0.8    │
│  Signals extracted (3):              │  + budget_flex ↑ new 0.5    │
│    metro_preference  high   0.8      │                              │
│    budget_flex       medium 0.5      │                              │
│                                      │                              │
│  What changed: Strengthened metro    │                              │
│  preference. Budget flexibility      │                              │
│  added — stretch if society is good. │                              │
│                                      │                              │
├──────────────────────────────────────┴──────────────────────────────┤
│  /help  /buyer <id>  /context  /extract  /clear  /quit              │
│  > _                                                                │
└─────────────────────────────────────────────────────────────────────┘
```

**Textual layout:**
- `Horizontal` split: `ChatPane` (`RichLog`, 60%) + `ContextPane` (`Static`, 40%)
- `Input` widget fixed at bottom — all input goes here
- `Header` shows OpenFang connection status (`● connected` / `○ offline`)
- `Footer` shows available commands

**Commands (minimum):** `/help` · `/buyer <id>` · `/context` · `/extract [N]` · `/clear` · `/quit`

**Buyer selection:** `/buyer <id>` loads from `synthetic_market.json`. If no ID given, picks a random buyer. Must generate market first.

**Chat behavior:** All non-command input is treated as a buyer conversation turn and appended to `conversation_turns`. The `/extract` command runs signal extraction on the last N turns (default 5).

**Session state held in app (not in OpenFang):**
- `active_buyer_id: str | None`
- `conversation_turns: List[str]` — rolling buffer
- `context_before: dict` — snapshot taken just before `/extract`

---

## 2) Architecture Decision for Today: Who owns what?

### OpenEstates app owns (source of truth)
- buyer/seller/property observable fields (synthetic_market.json)
- context graph state per user (graph_store)
- event log (what happened and when)
- evaluation harness later
- truth model later (synthetic_market_truth.json)

### OpenFang owns (agent runtime)
- multi-step reasoning
- tool calling
- memory within the agent runtime (optional)
- producing structured outputs (signals) from text

### Critical rules
- Anything OpenFang produces must be converted into structured updates and written into our store. Never store raw LLM output as durable state.
- **OpenFang must be stateless for Day 3.** All context is provided in the prompt on every call. Do not rely on OpenFang's built-in memory or agent sessions. This keeps extraction reproducible and our app the single source of truth.

---

## 3) OpenFang “Hands” We Eventually Want (direction steering)

We will not implement all of these today, but we should design toward them.

### Hand A: Coach + Signal Extractor (Buyer)
Runs when a buyer chats. Extracts:
- hard constraints
- soft preferences
- flexibility
- readiness signals
- emotional friction signals that map to deal constraints
Outputs structured updates to the context graph.

### Hand B: Coach + Signal Extractor (Seller)
Same idea for sellers:
- urgency
- visit tolerance
- possession flexibility
- negotiation style
- privacy preference

### Hand C: Match Explainer
Given a ranked match + score breakdown, produce a trustworthy explanation:
- what matched strongly
- what are trade-offs
- what to confirm next

### Hand D: Nurture/Watcher
Scheduled job:
- new listings arrive → re-score for stored buyer contexts
- send alert only if relevance crosses a threshold
- log “why we alerted” to prevent spam

### Hand E: Memory Compactor
Periodically:
- compress long conversation history into a stable profile summary
- keep “learned signals” clean and non-contradictory

Today we build the foundations to support Hand A.

---

## 4) Contracts (interfaces) between our app and OpenFang

We will implement two interfaces today in our own code, even if OpenFang calls are stubbed initially.

### Interface 1: SignalExtractor
Input:
- `buyer_id: str`
- `context_snapshot: dict` — full current context graph (provided in prompt, not fetched by OpenFang)
- `conversation_turns: List[str]` — recent turns (last N, default 5)

Output — strict JSON contract (OpenFang must return exactly this shape, nothing else):
```json
{
  "updates": [
    {
      "signal_key": "metro_preference",
      "signal_value": "high",
      "weight": 0.8,
      "confidence": 0.7,
      "action": "add|update|weaken|remove",
      "provenance": {"turn_indices": [0, 2]}
    }
  ]
}
```

Rules:
- Response must be a JSON object with a single top-level key `"updates"`
- `updates` is an array (may be empty `[]` if no signals found)
- No extra fields, no prose, no markdown — only the object above
- `weight` and `confidence` are floats in `[0.0, 1.0]`
- `action` is one of exactly: `"add"`, `"update"`, `"weaken"`, `"remove"`
- If OpenFang returns anything that doesn't parse to this schema, treat it as an error and fall back to stub

### Interface 2: ChangeNarrator
Input:
- before_context
- after_context
- extracted_updates
Output:
- short explanation:
  “We strengthened your metro preference because you mentioned it twice and accepted budget flexibility.”

These interfaces ensure we don’t couple the whole app to OpenFang details.

---

## 5) Data Model Upgrade: Context Graph Minimal Schema (prototype level)

We will store context in a structured JSON-like format:

buyer_context = {
  "signals": {
    "metro_preference": {"value": "high", "weight": 0.8, "confidence": 0.7, "updated_at": "...", "sources": [...]},
    "budget_max_soft": {"value": 16500000, "weight": 0.6, "confidence": 0.6, ...},
    ...
  }
}

We do not need a full graph DB today, but we must store:
- weight
- confidence
- timestamp
- provenance

We must implement:
- load_context(buyer_id)
- apply_updates(buyer_id, updates)
- save_context(buyer_id)

We should also log a small event:
- conversation_ingested event with buyer_id + extracted_updates summary

---

## 6) Implementation Tasks for Day 3

### Task 1: Add OpenFang integration scaffolding
Create:
- `agents/openfang_client.py` (or similar)
- `agents/signal_extractor.py` (implements the SignalExtractor interface)
- `agents/change_narrator.py` (implements the ChangeNarrator interface)

Even if OpenFang calls are not fully wired, create a clean boundary.

### Task 2: Define the structured output schema for extracted signals
Create:
- `agents/schemas.py` with a dataclass or typed dict definition for SignalUpdate.
Ensure the schema is stable and easy to parse.

### Task 3: Implement a first OpenFang-powered extractor
Implement the minimum OpenFang workflow that:
- takes context + conversation
- returns a JSON list of SignalUpdate objects
If OpenFang usage requires a config file or package structure, implement it minimally.

### Task 4: Update the context graph store to apply updates
Implement:
- “reinforce” if the same signal repeats
- “weaken” if an update says to weaken
Keep it very simple and deterministic for now.

### Task 5: TUI — Chat-First Interface
Replace the existing menu TUI (`app/tui.py`) with the chat-first layout defined in section 1.5.

The `/extract` command must:
- take the last N conversation turns from the session buffer (default 5)
- pass them to `SignalExtractor` along with the current buyer context snapshot
- apply returned `SignalUpdate` objects to the context graph
- print: extracted signals (JSON) + context diff + narrator summary
- update the `ContextPane` to reflect new signal state

---

## 7) Constraints (what NOT to do today)

Do not implement:
- full matching engine
- baseline search
- truth compatibility model
- full conversation simulator
- scheduled watcher jobs
- email notifications

We are only proving:
OpenFang can convert conversation into structured context updates.

---

## 8) Deliverables

By the end of Day 3, we should have:

- OpenFang integration scaffolding in `agents/`
- a stable signal update schema (structured JSON output)
- context graph updates applied deterministically
- a TUI command that demonstrates:
  buyer context before → conversation → extracted updates → context after
- a short “what changed and why” narrative

---

## 9) Manual Verification Checklist

- Run the TUI.
- Generate a synthetic market (Day 2).
- Pick a buyer.
- Run the coach extraction action.
- Verify that:
  - OpenFang returns structured JSON updates
  - context graph changes are visible
  - the system logs what was extracted
  - no hidden truth or cheating exists
  - output remains inspectable and deterministic (aside from LLM variability)

---

## 10) Why this Day 3 matters

This Day 3 locks our long-term direction:
OpenFang is the agent layer that makes OpenEstates “feel alive” and broker-like.

Without this, the matching engine will become a static filter tool.
With this, the system begins to learn human intent and sentiment in a deal-relevant way, improving matching quality and enabling nurturing behaviors later.

Tomorrow (Day 4) we can decide whether to:
- build the hidden truth compatibility model (for evaluation correctness), or
- build baseline search for comparison, or
- extend OpenFang into a minimal nurturer Hand.

We will choose based on what we learn from Day 3 integration.

## 11) Decisions Summary

### OpenFang Invocation Pattern

**For Day 3: use `/v1/chat/completions`** (OpenAI-compatible endpoint at `localhost:4200`)

Rationale:
- Stateless — one call per `/extract` invocation, no persistent agent session needed
- Our app owns all state (context graph, signal history) — OpenFang just provides reasoning
- Easy to stub when OpenFang is offline
- Simple to inspect: one request in, one structured JSON response out
- Workflows API is better suited for multi-step pipelines (Day 4+ matching + explanation chains)

Request shape:
```json
{
  “model”: “default”,
  “messages”: [
    {“role”: “system”, “content”: “<signal extraction system prompt>”},
    {“role”: “user”,   “content”: “<buyer profile JSON> + <conversation turns>”}
  ],
  “response_format”: {“type”: “json_object”}
}
```

Response parsed into `List[SignalUpdate]` before touching the context graph.

---

### Graceful Degradation

When OpenFang is not running:
- Catch `ConnectionRefusedError` / timeout on first call
- Fall back to `StubExtractor` — returns a fixed deterministic set of signals based on keyword matching
- Print `[OpenFang offline — using stub extractor]` in the chat pane
- Context graph updates still apply — prototype remains fully functional offline

This means Day 3 can be demoed and verified without OpenFang installed.

---

### ZeroClaw → OpenFang

All prior references to “ZeroClaw” in CLAUDE.md and LEARNING.md now refer to OpenFang. Updated.

---

### Learnings Folder

After completing Day 3, mirror key decisions into `days/learnings/day03.md`:
- Which OpenFang invocation pattern was used and why
- Any JSON schema issues encountered and how they were resolved
- Whether stub mode was needed and what it returned
- Any context graph design changes made during implementation
- Anything that should change in Day 4 based on what was learned today

This keeps the learnings trail consistent with Days 1–2.

---

### Open Questions for Day 4

1. Hidden compatibility model still unbuilt — pushed from Day 3. Needed before evaluation can be meaningful.
2. Should `/extract` auto-run after every N messages, or remain manual? Manual for now.
3. OpenFang LLM provider config — which model/provider is configured? Must be set before running live.