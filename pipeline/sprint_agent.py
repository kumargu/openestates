"""
OpenEstates Sprint Utilities — Checkpoint management and status.

The orchestration happens via:
  1. `run-sprint.sh` — bash loop, one Claude conversation per day (fire-and-forget)
  2. `.claude/skills/run-sprint.md` — skill doc Claude reads for each day
  3. This file — checkpoint/status utilities Claude calls via Bash

Usage:
  python3 pipeline/sprint_agent.py --status                    # show sprint + day status
  python3 pipeline/sprint_agent.py --sprint-info               # show sprint boundaries
  python3 pipeline/sprint_agent.py --sprint-info 2             # show sprint 2 details
  python3 pipeline/sprint_agent.py --next-day                  # detect next day number
  python3 pipeline/sprint_agent.py --mark-done 31              # mark day as complete
  python3 pipeline/sprint_agent.py --mark-failed 31 --reason "build error in X"
  python3 pipeline/sprint_agent.py --mark-step 31 plan         # mark step done
  python3 pipeline/sprint_agent.py --resume-from 31            # which step to resume from
  python3 pipeline/sprint_agent.py --checkpoint 31             # show checkpoint data
  python3 pipeline/sprint_agent.py --checkpoint 31 --set k=v   # set checkpoint value
  python3 pipeline/sprint_agent.py --checkpoint 31 --get k     # get checkpoint value
  python3 pipeline/sprint_agent.py --load-context 31           # load context for a day

Sprint model: 5 sprints × 14 days = 70 days (day 31 to day 100).
Days 1-30 are pre-sprint (prototype phase, already done).

No API keys or SDK needed — this is just a checkpoint/status helper.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Setup
# ---------------------------------------------------------------------------

PROJECT_ROOT = Path(__file__).resolve().parent.parent
DAYS_DIR = PROJECT_ROOT / "days"
STANDUPS_DIR = PROJECT_ROOT / "days" / "standups"
CHECKPOINTS_DIR = PROJECT_ROOT / "pipeline" / "checkpoints"
FEEDBACK_DIR = PROJECT_ROOT / "pipeline" / "feedback"

# ---------------------------------------------------------------------------
# Sprint model: 5 sprints × 14 days covering days 31-100
# ---------------------------------------------------------------------------

SPRINTS = [
    {
        "id": 1,
        "name": "Buyer Experience & Foundation",
        "days": (31, 44),
        "theme": "Make discovery shareable, trustworthy, and mobile-ready.",
    },
    {
        "id": 2,
        "name": "Seller-Buyer Connection",
        "days": (45, 58),
        "theme": "Connect buyers and sellers. Every click matters.",
    },
    {
        "id": 3,
        "name": "Search Intelligence & Marketplace",
        "days": (59, 72),
        "theme": "Search that genuinely helps. The marketplace converts.",
    },
    {
        "id": 4,
        "name": "Performance & Data Expansion",
        "days": (73, 86),
        "theme": "Fast now, ready for 10x. More data, better data.",
    },
    {
        "id": 5,
        "name": "Launch-Ready Polish",
        "days": (87, 100),
        "theme": "Ship-quality product.",
    },
]


def get_sprint_for_day(day: int) -> dict | None:
    """Return the sprint a day belongs to, or None."""
    for s in SPRINTS:
        if s["days"][0] <= day <= s["days"][1]:
            return s
    return None


def get_sprint_by_id(sprint_id: int) -> dict | None:
    for s in SPRINTS:
        if s["id"] == sprint_id:
            return s
    return None


# ---------------------------------------------------------------------------
# Checkpointing
# ---------------------------------------------------------------------------

def checkpoint_path(day: int) -> Path:
    CHECKPOINTS_DIR.mkdir(parents=True, exist_ok=True)
    return CHECKPOINTS_DIR / f"day{day:02d}.json"


def load_checkpoint(day: int) -> dict:
    p = checkpoint_path(day)
    return json.loads(p.read_text()) if p.exists() else {}


def save_checkpoint(day: int, data: dict):
    existing = load_checkpoint(day)
    existing.update(data)
    checkpoint_path(day).write_text(json.dumps(existing, indent=2))


def is_day_done(day: int) -> bool:
    return load_checkpoint(day).get("code_success") is True


def detect_next_day() -> int:
    """Find the next day that isn't done, starting from day 31."""
    for d in range(31, 101):
        if not is_day_done(d):
            return d
    return 31


# ---------------------------------------------------------------------------
# Step-level checkpointing
# ---------------------------------------------------------------------------

# Ordered steps within a day. Each must complete before the next starts.
DAY_STEPS = ["plan", "build", "verify"]


def mark_step_done(day: int, step: str):
    """Mark a step as completed for a day."""
    cp = load_checkpoint(day)
    steps_done = cp.get("steps_done", [])
    if step not in steps_done:
        steps_done.append(step)
    save_checkpoint(day, {"steps_done": steps_done, "current_step": step})


def get_resume_step(day: int) -> str:
    """Return the step to resume from. If all done, returns 'done'."""
    if is_day_done(day):
        return "done"
    cp = load_checkpoint(day)
    steps_done = cp.get("steps_done", [])
    for step in DAY_STEPS:
        if step not in steps_done:
            return step
    return "done"


def mark_day_failed(day: int, reason: str):
    """Mark a day as failed with a reason that feeds into the next day's context."""
    save_checkpoint(day, {
        "code_success": False,
        "failure_reason": reason,
    })


# ---------------------------------------------------------------------------
# Feedback loop — builder questions, tradeoffs, verifier observations
# ---------------------------------------------------------------------------

TRADEOFFS_FILE = PROJECT_ROOT / "docs" / "tradeoffs.md"


def save_builder_feedback(day: int, feedback: dict):
    """Save structured feedback from builder to checkpoint.

    Expected keys:
      questions:     list of strings — things the builder was unsure about
      tradeoffs:     list of {decision, alternatives, reasoning} dicts
      concerns:      list of strings — risks or issues noticed
      data_gaps:     list of strings — missing data that affected the build
    """
    save_checkpoint(day, {"builder_feedback": feedback})


def save_verifier_observations(day: int, observations: dict):
    """Save verifier observations beyond pass/fail.

    Expected keys:
      passed:        bool
      failures:      list of strings
      warnings:      list of strings — things that technically pass but look wrong
      suggestions:   list of strings — improvements for next day
      data_quality:  list of strings — data issues noticed during verification
    """
    save_checkpoint(day, {"verification": observations})


def append_tradeoff(day: int, decision: str, alternatives: str, reasoning: str):
    """Append a tradeoff to the persistent tradeoffs log."""
    TRADEOFFS_FILE.parent.mkdir(parents=True, exist_ok=True)

    entry = f"\n### Day {day}: {decision}\n"
    entry += f"**Alternatives:** {alternatives}\n"
    entry += f"**Reasoning:** {reasoning}\n"

    if TRADEOFFS_FILE.exists():
        content = TRADEOFFS_FILE.read_text()
    else:
        content = "# Design Tradeoffs Log\n\nAccumulated decisions across sprint days.\n"

    content += entry
    TRADEOFFS_FILE.write_text(content)


def is_mid_sprint_review(day: int) -> bool:
    """Check if this day is the mid-sprint review point (day 7 of 14)."""
    sprint = get_sprint_for_day(day)
    if not sprint:
        return False
    start = sprint["days"][0]
    day_in_sprint = day - start + 1
    return day_in_sprint == 8  # after 7 days done, review before day 8


# ---------------------------------------------------------------------------
# Status
# ---------------------------------------------------------------------------

def print_status():
    print("\n  OpenEstates Sprint Status (Days 31-100)")
    print("  " + "=" * 50)

    for sprint in SPRINTS:
        start, end = sprint["days"]
        done_count = sum(1 for d in range(start, end + 1) if is_day_done(d))
        total = end - start + 1
        pct = int(done_count / total * 100)
        bar = "█" * (pct // 5) + "░" * (20 - pct // 5)

        print(f"\n  Sprint {sprint['id']}: {sprint['name']}")
        print(f"  Days {start}-{end} | {bar} {done_count}/{total} ({pct}%)")
        print(f"  Theme: {sprint['theme']}")

        # Show individual days
        for d in range(start, end + 1):
            cp = load_checkpoint(d)
            plan_exists = (DAYS_DIR / f"day{d:02d}.md").exists()
            standup_exists = (STANDUPS_DIR / f"day{d:02d}_standup.md").exists()

            if cp.get("code_success"):
                marker = "✓"
                detail = cp.get("day_summary", "")[:50]
                label = f"DONE  {detail}"
            elif cp.get("code_success") is False and cp.get("failure_reason"):
                marker = "✗"
                label = f"FAILED: {cp['failure_reason'][:40]}"
            elif cp.get("steps_done"):
                steps = cp["steps_done"]
                step_str = "+".join(steps)
                marker = "◐"
                label = f"PARTIAL [{step_str}] (resume: {get_resume_step(d)})"
            elif cp.get("build_summary"):
                marker = "◐"
                label = "BUILD DONE (verify pending)"
            elif plan_exists:
                marker = "◯"
                tag = " +standup" if standup_exists else ""
                label = f"PLAN READY{tag}"
            elif standup_exists:
                marker = "◯"
                label = "STANDUP DONE"
            elif cp:
                marker = "◯"
                label = "in progress"
            else:
                marker = "·"
                label = ""

            if marker != "·" or d == detect_next_day():
                if d == detect_next_day() and marker == "·":
                    marker = "→"
                    label = "NEXT"
                print(f"    {marker} Day {d:02d}: {label}")

    print()


def print_sprint_info(sprint_id: int | None = None):
    """Print sprint boundaries and progress."""
    if sprint_id:
        s = get_sprint_by_id(sprint_id)
        if not s:
            print(f"No sprint {sprint_id}. Valid: 1-5")
            return
        sprints = [s]
    else:
        sprints = SPRINTS

    for s in sprints:
        start, end = s["days"]
        done = sum(1 for d in range(start, end + 1) if is_day_done(d))
        total = end - start + 1
        print(f"Sprint {s['id']}: {s['name']}")
        print(f"  Days: {start}-{end} ({total} days)")
        print(f"  Theme: {s['theme']}")
        print(f"  Progress: {done}/{total} done")
        print()


# ---------------------------------------------------------------------------
# Context loaders
# ---------------------------------------------------------------------------

def load_context_for_day(day: int) -> str:
    """Load all available context for a day — used by Claude before standup/planning."""
    parts = []

    # Sprint context
    sprint = get_sprint_for_day(day)
    if sprint:
        start, end = sprint["days"]
        day_in_sprint = day - start + 1
        done = sum(1 for d in range(start, day) if is_day_done(d))
        parts.append(
            f"## Sprint {sprint['id']}: {sprint['name']}\n"
            f"Days {start}-{end} | Day {day_in_sprint} of {end - start + 1}\n"
            f"Theme: {sprint['theme']}\n"
            f"Completed so far: {done} days"
        )

    # Previous day's plan
    prev_plan = DAYS_DIR / f"day{day - 1:02d}.md"
    if prev_plan.exists():
        lines = prev_plan.read_text().splitlines()[:40]
        parts.append(f"## Day {day - 1} Plan (first 40 lines)\n" + "\n".join(lines))

    # Previous day's checkpoint
    prev_cp = load_checkpoint(day - 1)
    if prev_cp.get("build_summary"):
        parts.append(f"## Day {day - 1} Builder Output (truncated)\n{prev_cp['build_summary'][:2000]}")
    if prev_cp.get("verification"):
        v = prev_cp["verification"]
        parts.append(f"## Day {day - 1} Verification\n"
                     f"Passed: {v.get('passed')}\n"
                     f"Failures: {v.get('failures', [])}\n"
                     f"Summary: {v.get('summary', 'N/A')}")
    if prev_cp.get("day_summary"):
        parts.append(f"## Day {day - 1} Summary\n{prev_cp['day_summary']}")

    # Failure history — scan recent days for unresolved failures
    # This is the key feedback loop: if day N-1 or N-2 failed, the planner
    # MUST see why and address it before moving on to new work.
    failure_parts = []
    for d in range(max(31, day - 3), day):
        cp = load_checkpoint(d)
        if cp.get("code_success") is False and cp.get("failure_reason"):
            failure_parts.append(
                f"### Day {d} FAILED\n"
                f"Reason: {cp['failure_reason']}\n"
                f"Steps completed: {cp.get('steps_done', [])}"
            )
        if cp.get("verification") and not cp["verification"].get("passed"):
            v = cp["verification"]
            failures = v.get("failures", [])
            if failures:
                failure_parts.append(
                    f"### Day {d} VERIFICATION FAILURES\n"
                    f"Failures: {json.dumps(failures, indent=2)}\n"
                    f"Summary: {v.get('summary', 'N/A')}"
                )
    if failure_parts:
        parts.append(
            "## ⚠ UNRESOLVED FAILURES (must address before new work)\n\n"
            + "\n\n".join(failure_parts)
        )

    # Builder feedback from recent days — questions, tradeoffs, concerns
    # This is the KEY feedback loop: builder surfaces unknowns → planner addresses them
    feedback_from_builders = []
    for d in range(max(31, day - 3), day):
        cp = load_checkpoint(d)
        bf = cp.get("builder_feedback", {})
        if bf:
            lines = [f"### Day {d} Builder Feedback"]
            if bf.get("questions"):
                lines.append("**Open Questions:**")
                lines.extend(f"- {q}" for q in bf["questions"])
            if bf.get("tradeoffs"):
                lines.append("**Tradeoffs Made:**")
                for t in bf["tradeoffs"]:
                    lines.append(f"- **{t.get('decision', '?')}** — {t.get('reasoning', '?')}")
                    if t.get("alternatives"):
                        lines.append(f"  Alternatives considered: {t['alternatives']}")
            if bf.get("concerns"):
                lines.append("**Concerns:**")
                lines.extend(f"- {c}" for c in bf["concerns"])
            if bf.get("data_gaps"):
                lines.append("**Data Gaps:**")
                lines.extend(f"- {g}" for g in bf["data_gaps"])
            feedback_from_builders.append("\n".join(lines))

        # Also include verifier warnings/suggestions (beyond pass/fail)
        v = cp.get("verification", {})
        if v.get("warnings") or v.get("suggestions") or v.get("data_quality"):
            lines = [f"### Day {d} Verifier Observations"]
            if v.get("warnings"):
                lines.append("**Warnings (passed but look wrong):**")
                lines.extend(f"- {w}" for w in v["warnings"])
            if v.get("suggestions"):
                lines.append("**Suggestions for next day:**")
                lines.extend(f"- {s}" for s in v["suggestions"])
            if v.get("data_quality"):
                lines.append("**Data quality issues:**")
                lines.extend(f"- {d}" for d in v["data_quality"])
            feedback_from_builders.append("\n".join(lines))

    if feedback_from_builders:
        parts.append(
            "## 💬 FEEDBACK FROM PREVIOUS DAYS (planner must address)\n\n"
            "The builder and verifier raised these items. The planner should:\n"
            "- Answer open questions (decide and document in tradeoffs)\n"
            "- Acknowledge concerns (plan mitigations or accept risk)\n"
            "- Fill data gaps if they block upcoming work\n\n"
            + "\n\n".join(feedback_from_builders)
        )

    # Mid-sprint review trigger
    if is_mid_sprint_review(day):
        sprint = get_sprint_for_day(day)
        start = sprint["days"][0]
        done = sum(1 for d in range(start, day) if is_day_done(d))
        failed = sum(1 for d in range(start, day)
                     if load_checkpoint(d).get("code_success") is False)
        parts.append(
            f"## 🔄 MID-SPRINT REVIEW (Day 8 of 14)\n\n"
            f"This is the mid-point of Sprint {sprint['id']}: {sprint['name']}.\n"
            f"Progress: {done} done, {failed} failed out of 7 days.\n\n"
            f"**Before planning today, the planner MUST:**\n"
            f"1. Review what was actually built vs what was planned in `docs/vision.md`\n"
            f"2. Check `docs/tradeoffs.md` for accumulated decisions\n"
            f"3. Assess: are we on track? Should remaining days pivot?\n"
            f"4. Write a 5-line mid-sprint assessment into the day plan\n"
            f"5. Adjust remaining scope if needed — it's better to cut scope than ship broken\n"
        )

    # Recent standups (last 3)
    for d in range(max(31, day - 3), day):
        f = STANDUPS_DIR / f"day{d:02d}_standup.md"
        if f.exists():
            parts.append(f"## Day {d:02d} Standup\n{f.read_text()[:1500]}")

    # Current sprint completed days (not all 100 days — just this sprint)
    if sprint:
        start = sprint["days"][0]
        completed_parts = []
        for d in range(start, day):
            cp = load_checkpoint(d)
            plan_file = DAYS_DIR / f"day{d:02d}.md"
            if cp.get("code_success") and plan_file.exists():
                lines = plan_file.read_text().splitlines()[:10]
                summary = cp.get("day_summary", "")
                completed_parts.append(f"### Day {d:02d} [DONE] {summary}\n" + "\n".join(lines) + "\n...")
            elif plan_file.exists():
                completed_parts.append(f"### Day {d:02d} [PENDING]")
        if completed_parts:
            parts.append("## Sprint Progress (this sprint)\n" + "\n\n".join(completed_parts))

    # Feedback
    FEEDBACK_DIR.mkdir(parents=True, exist_ok=True)
    feedback_parts = []
    for f in sorted(FEEDBACK_DIR.glob("day*_feedback.json")):
        try:
            data = json.loads(f.read_text())
        except json.JSONDecodeError:
            continue
        day_num = data.get("day", "?")
        review = data.get("overall_impression", data.get("gpt_review", ""))
        if review:
            feedback_parts.append(f"Day {day_num}: {review[:200]}")
    if feedback_parts:
        parts.append("## Feedback\n" + "\n".join(feedback_parts[-5:]))  # last 5

    # Search log insights
    log_dir = PROJECT_ROOT / "data" / "knowledge" / "search_log"
    if log_dir.exists():
        log_files = sorted(log_dir.rglob("*.jsonl"), reverse=True)[:3]
        queries = []
        for lf in log_files:
            for line in lf.read_text().splitlines()[-10:]:
                try:
                    q = json.loads(line).get("query", "")
                    if q and q not in queries:
                        queries.append(q)
                except json.JSONDecodeError:
                    pass
        if queries:
            parts.append("## Recent Searches\n" + "\n".join(f"- {q}" for q in queries[:10]))

    # Today's existing plan
    today_plan = DAYS_DIR / f"day{day:02d}.md"
    if today_plan.exists():
        parts.append(f"## Today's Existing Plan (Day {day})\n{today_plan.read_text()[:3000]}")

    return "\n\n---\n\n".join(parts) if parts else "No context available."


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(
        description="OpenEstates Sprint Utilities — 5 sprints × 14 days (days 31-100)",
    )
    parser.add_argument("--status", action="store_true",
                        help="Show sprint + day status")
    parser.add_argument("--sprint-info", type=int, nargs="?", const=0, metavar="N",
                        help="Show sprint boundaries (all sprints, or sprint N)")
    parser.add_argument("--next-day", action="store_true",
                        help="Print next day number to work on")
    parser.add_argument("--mark-done", type=int, metavar="DAY",
                        help="Mark a day as successfully completed")
    parser.add_argument("--mark-failed", type=int, metavar="DAY",
                        help="Mark a day as failed")
    parser.add_argument("--reason", type=str, default="",
                        help="Failure reason (use with --mark-failed)")
    parser.add_argument("--mark-step", nargs=2, metavar=("DAY", "STEP"),
                        help="Mark a step as done (steps: plan, build, verify)")
    parser.add_argument("--resume-from", type=int, metavar="DAY",
                        help="Show which step to resume from for a day")
    parser.add_argument("--checkpoint", type=int, metavar="DAY",
                        help="Day number for checkpoint operations")
    parser.add_argument("--set", type=str, metavar="KEY=VALUE",
                        help="Set checkpoint key=value (use with --checkpoint)")
    parser.add_argument("--get", type=str, metavar="KEY",
                        help="Get checkpoint key (use with --checkpoint)")
    parser.add_argument("--load-context", type=int, metavar="DAY",
                        help="Load context for a day (sprint, yesterday, standups, progress)")
    parser.add_argument("--save-builder-feedback", type=int, metavar="DAY",
                        help="Save builder feedback JSON from stdin (pipe JSON)")
    parser.add_argument("--save-verifier-obs", type=int, metavar="DAY",
                        help="Save verifier observations JSON from stdin (pipe JSON)")
    parser.add_argument("--add-tradeoff", type=int, metavar="DAY",
                        help="Add a tradeoff entry (use with --decision, --alternatives, --reasoning)")
    parser.add_argument("--decision", type=str, default="")
    parser.add_argument("--alternatives", type=str, default="")
    parser.add_argument("--reasoning", type=str, default="")
    parser.add_argument("--is-mid-review", type=int, metavar="DAY",
                        help="Check if day is the mid-sprint review point")
    args = parser.parse_args()

    if args.status:
        print_status()
    elif args.sprint_info is not None:
        print_sprint_info(args.sprint_info if args.sprint_info > 0 else None)
    elif args.next_day:
        print(detect_next_day())
    elif args.mark_done is not None:
        save_checkpoint(args.mark_done, {"code_success": True})
        sprint = get_sprint_for_day(args.mark_done)
        tag = f" (Sprint {sprint['id']}: {sprint['name']})" if sprint else ""
        print(f"Day {args.mark_done} marked as done.{tag}")
    elif args.mark_failed is not None:
        mark_day_failed(args.mark_failed, args.reason or "no reason given")
        print(f"Day {args.mark_failed} marked as failed. Reason: {args.reason or 'no reason given'}")
    elif args.mark_step:
        day_num = int(args.mark_step[0])
        step = args.mark_step[1]
        if step not in DAY_STEPS:
            print(f"Invalid step '{step}'. Valid: {DAY_STEPS}")
            sys.exit(1)
        mark_step_done(day_num, step)
        print(f"Day {day_num}: step '{step}' marked done. Next: {get_resume_step(day_num)}")
    elif args.resume_from is not None:
        step = get_resume_step(args.resume_from)
        print(step)
    elif args.checkpoint is not None:
        if args.set:
            key, _, value = args.set.partition("=")
            try:
                value = json.loads(value)
            except json.JSONDecodeError:
                pass
            save_checkpoint(args.checkpoint, {key: value})
            print(f"Set day {args.checkpoint}: {key}={value}")
        elif args.get:
            cp = load_checkpoint(args.checkpoint)
            val = cp.get(args.get)
            if val is not None:
                print(json.dumps(val) if isinstance(val, (dict, list)) else str(val))
            else:
                print(f"Key '{args.get}' not found in day {args.checkpoint} checkpoint")
                sys.exit(1)
        else:
            cp = load_checkpoint(args.checkpoint)
            print(json.dumps(cp, indent=2))
    elif args.load_context is not None:
        print(load_context_for_day(args.load_context))
    elif args.save_builder_feedback is not None:
        data = json.loads(sys.stdin.read())
        save_builder_feedback(args.save_builder_feedback, data)
        n_items = sum(len(v) for v in data.values() if isinstance(v, list))
        print(f"Saved builder feedback for day {args.save_builder_feedback} ({n_items} items)")
    elif args.save_verifier_obs is not None:
        data = json.loads(sys.stdin.read())
        save_verifier_observations(args.save_verifier_obs, data)
        print(f"Saved verifier observations for day {args.save_verifier_obs}")
    elif args.add_tradeoff is not None:
        if not args.decision:
            print("--add-tradeoff requires --decision")
            sys.exit(1)
        append_tradeoff(args.add_tradeoff, args.decision, args.alternatives, args.reasoning)
        print(f"Tradeoff logged for day {args.add_tradeoff}: {args.decision}")
    elif args.is_mid_review is not None:
        print("yes" if is_mid_sprint_review(args.is_mid_review) else "no")
    else:
        print_status()


if __name__ == "__main__":
    main()
