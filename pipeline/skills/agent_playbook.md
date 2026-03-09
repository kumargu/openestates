# OpenEstates Agent Playbook

Living document — updated as the agent learns. Every rule here was earned, not assumed.

---

## Core Agent Architecture

```
ChatGPT (Firefox browser)  →  Product visionary — owns vision, creates day plans
Claude Sonnet (API)         →  Engineering reviewer — reviews plans, audits decisions
Claude Opus (CLI)           →  Coder — implements the finalized plan (new chat per day)
```

Flow per day:
1. ChatGPT creates plan (with product non-negotiables injected every time)
2. Claude Sonnet reviews for technical soundness + asks questions
3. ChatGPT answers (skipped only if Claude explicitly approves)
4. Claude Sonnet finalizes clean spec → saved to `days/dayNN.md`
5. Claude Sonnet audits product decisions → saved to `pipeline/learnings/`
6. [plan-only stops here]
7. Claude Opus implements via `claude` CLI (NEW conversation per day)
8. Smoke test (pure HTTP, zero AI tokens)
9. ChatGPT reviews failures only if smoke test finds issues

---

## Non-Negotiable Product Principles

These two principles must never be compromised. Audit every day plan against them.

**1. Transparency is the core product promise.**
- Every surface must explain *why* — search results, rankings, property pages, comparisons
- Users must never wonder "why am I seeing this?"
- Hidden reasoning = product failure

**2. The customer journey must be seamless.**
- From first search → shortlist → decision: every step feels like a natural next step
- No friction, no dead ends, no confusion about what to do next
- Friction is a product bug

---

## Decision Philosophy: Disagree and Commit

- Claude (engineering) CAN and SHOULD push back on product decisions that violate principles
- If ChatGPT proposes something that drifts from transparency or seamlessness — flag it
- But once a direction is committed (after disagreement is voiced and logged), execute fully
- Never silently go along. Never block progress by refusing to commit.
- The audit log in `pipeline/learnings/` is where concerns live — not in the implementation

Verdicts used in audit:
- `agree` — aligns with both principles
- `disagree-but-commit` — concern noted, moving forward
- `flag-for-human-review` — significant drift, needs human to decide before proceeding

---

## Checkpointing Rules

- Save checkpoint after EVERY step — agent must be resumable mid-day
- Never re-run completed steps unless `--resume` is explicitly passed
- Checkpoint path: `pipeline/checkpoints/dayNN.json`
- Keys: `gpt_plan`, `claude_review`, `gpt_answers`, `final_plan`, `product_audit`, `code_success`, `smoke_results`, `gpt_feedback`

---

## New Chat Per Day (Implementation)

Claude Opus coding step always starts a **new conversation**:
- Pass `--print --dangerously-skip-permissions --prompt` to `claude` CLI
- Never use `--continue` — each day is isolated
- No context leaks between days
- Each day's coding is a clean slate with only the day plan + CLAUDE.md as context

---

## Day Completion Detection

Before running any day:
1. `is_day_implemented(day)` → checks `code_success: true` in checkpoint → skip full run
2. `is_day_plan_done(day)` → checks `days/dayNN.md` exists → skip plan-only run
3. Use `--resume` to force re-run a completed day

---

## Product Alignment Reminder (injected every day to ChatGPT)

Every ChatGPT plan prompt includes a standing reminder about:
- Transparency as the core promise
- Seamless customer journey
- Explicitly flagging any product evolution or departure in `## 7. Product Decisions`

This prevents context drift in long ChatGPT conversations where the original product vision fades.

---

## Learnings Audit Log

Every finalized plan is audited by Claude Sonnet before coding starts.
Results saved to `pipeline/learnings/`:
- `dayNN_decisions.json` — structured audit per day
- `decisions_log.md` — running human-readable log of all decisions

Human reviews `decisions_log.md` to track product drift over time.

---

## Conversation Rotation

ChatGPT conversation rotates after `CHATGPT_MAX_MESSAGES = 80` messages.
New conversation ID saved to `.env` as `CHATGPT_CONVERSATION_ID`.
Each new day (when running multiple days in one session) checks the count first.

---

## Stack Boundary Rules (firm)

| Concern | Owner |
|---|---|
| Data crawling, enrichment, normalization | Python (`pipeline/`) |
| Backend API, ranking logic | Rust + Axum (`backend/`) |
| Frontend, UI, transparency surfaces | React (`frontend/`) |
| Seed data | JSON files (`data/`) |

Python and Rust communicate through structured JSON — never shared code.

---

## CLI Quick Reference

```bash
# Generate a reviewed day plan (no coding)
python3 pipeline/agent.py --plan-only --days 1

# Full loop: plan + review + audit + code + smoke test
python3 pipeline/agent.py --days 1

# Resume a failed run from checkpoint
python3 pipeline/agent.py --day 8 --resume

# EMERGENCY: skip Claude review (no API credits)
python3 pipeline/agent.py --day 8 --skip-review

# Plan multiple days ahead
python3 pipeline/agent.py --plan-only --days 3
```

---

## What Gets Logged Where

| Artifact | Location |
|---|---|
| Day plans | `days/dayNN.md` |
| Transcripts | `pipeline/transcripts/dayNN_transcript.json` |
| Checkpoints | `pipeline/checkpoints/dayNN.json` |
| Product audits | `pipeline/learnings/dayNN_decisions.json` |
| Running decisions log | `pipeline/learnings/decisions_log.md` |
| Smoke test feedback | `pipeline/feedback/dayNN_feedback.json` |
