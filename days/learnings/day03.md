# Day 03 Contract (What Day 4 Assumes)

This is the minimal set of learnings/decisions from Day 3 that remain relevant for Day 4 onward. Anything not listed here should be treated as implementation detail and can change.

## 1) Working Chat-First TUI Exists
The TUI is chat-first and supports at minimum:
- `/generate` to generate/load synthetic market
- `/buyer <id>` to select a buyer (or random if blank)
- `/context` to inspect the buyer context graph
- `/extract` to run signal extraction and apply updates
- `/clear`, `/quit`

Day 4 should extend the same TUI with new commands (e.g., `/truth`, `/research`).

## 2) Signal Extraction Contract Is Strict and Structured
We have a strict structured output contract for extraction results, validated before applying updates. The system must not mutate context if extraction output is invalid.

Signal updates contain:
- `signal_key`, `signal_value`
- `weight`, `confidence`
- `action` = add/update/weaken/remove
- provenance (`turn_indices`) at minimum

OpenFang can be used if available; otherwise the system must degrade to stub mode.

## 3) Context Graph Storage Is Owned by the App
The app owns durable user context. Context signals store:
- value, weight, confidence, repetition_count
- updated_at timestamp
- sources/provenance

Updates apply deterministically (reinforce/weaken/remove) even if extraction is LLM-based.

## 4) Graceful Degradation Works
When OpenFang is offline:
- Stub extractor + narrator are used
- The rest of the flow still works (context updates, diffs, logging)

Day 4 must preserve this property (truth model + reddit research must also fail gracefully).

## 5) Event Logging Exists
Conversation ingestion events are written to `data/events.jsonl` as one JSON object per line. This provides auditability without coupling.

Day 4 may add new event types (truth inspections, research runs) but should keep logging simple and append-only.

## 6) What Day 3 Did NOT Build
There is no ground truth model, baseline search, matching engine evaluation, watcher/nurture agents, or seller coaching yet. Day 4 should not assume they exist.

---

## Day 4 Direction Reminder
OpenEstates is testing context-based search and matching over traditional filter-based search. The system must remain inspectable and must avoid leaking hidden truth to the matching engine.