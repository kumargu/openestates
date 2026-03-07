# Day 03 — Learnings and Decisions

Key decisions and observations from Day 3 implementation. Read this before starting Day 4.

---

## What Was Built

Day 3 integrated OpenFang as the agent layer for signal extraction from buyer conversations.

### Modules implemented:
- `agents/schemas.py` — `SignalUpdate` dataclass, strict JSON contract, `parse_updates_response()`
- `agents/openfang_client.py` — thin REST client for OpenFang `/v1/chat/completions`
- `agents/signal_extractor.py` — `OpenFangExtractor` (live) + `StubExtractor` (offline keyword-based)
- `agents/change_narrator.py` — `OpenFangNarrator` (live) + `StubNarrator` (template-based)
- `graph/context_graph.py` — `ContextGraph` with `Signal` model, `apply_updates()`, `snapshot()`
- `graph/graph_store.py` — file-based persistence + event logging (`events.jsonl`)
- `app/tui.py` — chat-first TUI with `/buyer`, `/extract`, `/context`, `/generate`, `/clear`, `/quit`

---

## OpenFang Invocation Pattern

Used `/v1/chat/completions` (OpenAI-compatible endpoint at `localhost:4200`). Stateless, one-shot per `/extract` call. Our app owns all state.

System prompt enforces strict JSON-only response with the `SignalUpdate` schema. `response_format: {"type": "json_object"}` is set in the request payload.

---

## Graceful Degradation

When OpenFang is offline (the common case during development):
- `StubExtractor` uses keyword matching against a fixed dictionary (metro, budget, document, etc.)
- `StubNarrator` uses template strings
- The TUI prints `[OpenFang offline — using stub extractor]` on mount
- All functionality works identically — context graph updates, diffs, narration

This was critical: the prototype is fully functional without any external LLM running.

---

## Context Graph Design

Signals carry: `value`, `weight`, `confidence`, `updated_at`, `sources` (provenance list), `repetition_count`.

Update mechanics:
- **add**: creates new signal with provided weight/confidence
- **update** (reinforce): blends weight/confidence upward (`(old + new) / 2 + 0.05`), increments repetition_count
- **weaken**: reduces weight by 0.15, confidence by 0.10
- **remove**: deletes signal entirely

This is deliberately simple. The reinforcement formula may need tuning, but the structure supports it.

---

## Event Logging

`conversation_ingested` events are logged to `data/events.jsonl` (one JSON object per line) with: buyer_id, turns_count, updates_count, signal keys, timestamp.

This provides an audit trail without complicating the main flow.

---

## What Was NOT Built (intentionally)

- Hidden compatibility model (deferred — needed before evaluation is meaningful)
- Baseline search
- Full matching engine
- Conversation simulator (automated)
- Watcher/nurture agents
- Seller signal extraction

---

## Open Questions for Day 4

1. **Hidden compatibility model** is the biggest gap. Without it, we can't evaluate whether extracted signals improve match quality. Strong candidate for Day 4.
2. **Auto-extract**: Should `/extract` run automatically after N messages? Kept manual for now — gives user control during prototyping.
3. **Signal decay**: Signals don't age or decay yet. If a buyer's preferences change over multiple conversations, old signals stay at full strength. May need a decay mechanism.
4. **Reinforcement formula**: The `(old + new) / 2 + 0.05` blend is arbitrary. Works for demo but needs calibration against real extraction outputs.
5. **OpenFang LLM provider**: No model/provider was configured since stub mode was sufficient. Must configure before live extraction testing.
