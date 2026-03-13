"""
OpenEstates Day Agent (v3 — Claude-only)

Claude Opus drives the entire loop: planning, implementation, testing, and feedback.
No external dependencies (no ChatGPT, no Firefox, no Playwright).

Loop (1 day at a time):
1. Find next day to implement from checkpoints
2. Claude generates the day plan (with vision, prior feedback, smoke test results)
3. Claude implements the plan via `claude` CLI
4. Run smoke tests (pure HTTP)
5. Claude reviews results and generates feedback for next day
6. Deploy to Vercel (optional)
7. Checkpoint after every step

Usage:
  python3 pipeline/agent.py                    # auto-detect next day, full cycle
  python3 pipeline/agent.py --day 8            # run specific day
  python3 pipeline/agent.py --day 8 --resume   # resume from checkpoint
  python3 pipeline/agent.py --plan-only        # plan only, skip implementation
  python3 pipeline/agent.py --loop 6           # run 6 days in sequence (overnight mode)
  python3 pipeline/agent.py --loop 6 --plan-only  # plan 6 days without coding
  python3 pipeline/agent.py --status           # show all day statuses
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request
from datetime import date
from pathlib import Path
from typing import Optional

PROJECT_ROOT = Path(__file__).resolve().parent.parent


def _load_dotenv():
    env_file = PROJECT_ROOT / ".env"
    if not env_file.exists():
        return
    for line in env_file.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, value = line.partition("=")
        key, value = key.strip(), value.strip()
        if key and not os.environ.get(key):
            os.environ[key] = value


_load_dotenv()

DAYS_DIR = PROJECT_ROOT / "days"
DOCS_DIR = PROJECT_ROOT / "docs"
FEEDBACK_DIR = PROJECT_ROOT / "pipeline" / "feedback"
CHECKPOINTS_DIR = PROJECT_ROOT / "pipeline" / "checkpoints"
LEARNINGS_DIR = PROJECT_ROOT / "pipeline" / "learnings"

CLAUDE_OPUS = "claude-opus-4-6"


# ---------------------------------------------------------------------------
# Checkpointing
# ---------------------------------------------------------------------------

def checkpoint_path(day_number: int) -> Path:
    CHECKPOINTS_DIR.mkdir(parents=True, exist_ok=True)
    return CHECKPOINTS_DIR / f"day{day_number:02d}.json"


def load_checkpoint(day_number: int) -> dict:
    path = checkpoint_path(day_number)
    if path.exists():
        return json.loads(path.read_text())
    return {}


def save_checkpoint(day_number: int, data: dict):
    path = checkpoint_path(day_number)
    existing = load_checkpoint(day_number)
    existing.update(data)
    path.write_text(json.dumps(existing, indent=2))


def checkpoint_has(day_number: int, step: str) -> bool:
    return step in load_checkpoint(day_number)


# ---------------------------------------------------------------------------
# Context loaders
# ---------------------------------------------------------------------------

def load_project_context() -> str:
    """Load project context — CLAUDE.md + vision + blueprint."""
    parts = []

    claude_md = PROJECT_ROOT / "CLAUDE.md"
    if claude_md.exists():
        parts.append(f"# CLAUDE.md (Engineering Guidelines)\n{claude_md.read_text()}")

    vision = DOCS_DIR / "vision.md"
    if vision.exists():
        parts.append(f"# Product Vision & Sprint Plan\n{vision.read_text()}")

    blueprint = DOCS_DIR / "openestates_v2_surfaces_and_data.md"
    if blueprint.exists():
        parts.append(f"# Product Blueprint\n{blueprint.read_text()}")

    return "\n\n---\n\n".join(parts)


def load_previous_days_summary() -> str:
    """Load all existing day plans as context."""
    day_files = sorted(DAYS_DIR.glob("day*.md"))
    if not day_files:
        return "No previous day plans exist yet."
    parts = [f"## {f.name}\n{f.read_text()}" for f in day_files]
    return "\n\n---\n\n".join(parts)


def load_feedback_history() -> str:
    """Load structured feedback from all previous days."""
    FEEDBACK_DIR.mkdir(parents=True, exist_ok=True)
    feedback_files = sorted(FEEDBACK_DIR.glob("day*_feedback.json"))

    if not feedback_files:
        return ""

    parts = []
    for f in feedback_files:
        data = json.loads(f.read_text())
        day = data.get("day", "?")
        review = data.get("review", data.get("overall_impression", ""))
        improvements = data.get("improvements", [])
        suggestions = data.get("suggestions", "")
        if review or improvements:
            part = f"### Day {day:02d} feedback\n{review}\n"
            if improvements:
                part += "\nCarry-over improvements:\n" + "\n".join(f"- {i}" for i in improvements)
            if suggestions:
                part += f"\nSuggestions for next day:\n{suggestions}\n"
            parts.append(part)

    return "\n\n".join(parts)


def detect_next_day_number() -> int:
    """Find the next day that needs implementation."""
    FIRST_CHECKPOINTED_DAY = 6

    for day_num in range(1, 200):
        cp = load_checkpoint(day_num)
        plan_exists = (DAYS_DIR / f"day{day_num:02d}.md").exists()

        if day_num < FIRST_CHECKPOINTED_DAY:
            if not plan_exists:
                return day_num
            continue

        if cp.get("code_success"):
            continue

        if plan_exists or cp.get("plan"):
            return day_num

        return day_num

    return 1


def is_day_implemented(day_number: int) -> bool:
    cp = load_checkpoint(day_number)
    return cp.get("code_success") is True


def print_status():
    """Print status of all days."""
    FIRST_CHECKPOINTED_DAY = 6
    print("\n  Day status:")
    for day_num in range(1, 200):
        cp = load_checkpoint(day_num)
        plan_exists = (DAYS_DIR / f"day{day_num:02d}.md").exists()

        if not plan_exists and not cp:
            break

        if day_num < FIRST_CHECKPOINTED_DAY and plan_exists:
            label = "DONE (pre-checkpoint era)"
        elif cp.get("code_success"):
            label = "DONE (plan + code)"
        elif plan_exists:
            label = "PLAN READY (code pending)"
        elif cp.get("plan"):
            label = "plan in checkpoint (not saved to file)"
        else:
            label = "not started"
        print(f"    Day {day_num:02d}: {label}")
    print()


# ---------------------------------------------------------------------------
# Claude CLI helper
# ---------------------------------------------------------------------------

def _clean_env() -> dict:
    """Return env for nested claude CLI calls.

    Removes CLAUDECODE (allows nesting) and ANTHROPIC_API_KEY (forces
    subscription mode instead of API credits).
    """
    env = os.environ.copy()
    env.pop("CLAUDECODE", None)
    env.pop("ANTHROPIC_API_KEY", None)
    return env


def call_claude(prompt: str, timeout: int = 300) -> Optional[str]:
    """Call Claude via the `claude` CLI in --print mode. Returns response text."""
    try:
        result = subprocess.run(
            ["claude", "--model", CLAUDE_OPUS, "--print",
             "--dangerously-skip-permissions", prompt],
            capture_output=True,
            text=True,
            cwd=str(PROJECT_ROOT),
            timeout=timeout,
            env=_clean_env(),
        )
        if result.returncode == 0 and result.stdout.strip():
            return result.stdout.strip()
        if result.stderr:
            print(f"      Claude stderr: {result.stderr[:200]}")
        return None
    except FileNotFoundError:
        print("  ERROR: 'claude' CLI not found.")
        return None
    except subprocess.TimeoutExpired:
        print(f"  WARNING: Claude timed out after {timeout}s.")
        return None


# ---------------------------------------------------------------------------
# Planning — Claude generates the day plan
# ---------------------------------------------------------------------------

PLANNER_PROMPT = """You are the product visionary AND engineering planner for OpenEstates — a transparency-first property discovery platform.

Your role:
- Own the full product vision and day-by-day execution roadmap
- Create detailed, actionable day plans that you (Claude) will implement next
- Each plan should be scoped to a single focused session (~2-4 hours of coding)
- Build incrementally; reference what was accomplished previously
- Incorporate feedback and carry-over improvements from previous days
- Be specific about deliverables, file paths, schemas, and acceptance criteria
- Follow the sprint plan from docs/vision.md strictly

Output format — clean markdown:
- # for title, ## for sections, ### for subsections
- Bullet lists with - for lists
- ``` code blocks for code samples and file structures
- **bold** for key emphasis

Include these sections:
## Goal
## Product Reason
## Sprint Context
## Deliverables (with file paths, schemas, specific code guidance)
## Files to Create / Files to Modify
## Constraints (cargo check, npm build, no new deps, etc.)
## Success Criteria (numbered, testable)

If feedback from previous days needs addressing, add: ## Feedback Responses"""


def create_plan(day_number: int, project_context: str,
                previous_days: str, feedback: str,
                smoke_results: Optional[dict] = None) -> Optional[str]:
    """Ask Claude to create a day plan."""
    prompt = f"""{PLANNER_PROMPT}

---

Here is the full project context:

{project_context}

---

Previous day plans:

{previous_days}"""

    if feedback:
        prompt += f"""

---

## Accumulated feedback from previous days (IMPORTANT — address these):

{feedback}"""

    if smoke_results:
        prompt += f"""

---

## Latest smoke test results (from previous day's implementation):

{json.dumps(smoke_results, indent=2)}

Use these to understand what's working and what needs fixing."""

    prompt += f"""

---

Create the Day {day_number:02d} plan now.

Start the document with:
# Day {day_number:02d}: <descriptive title>

The plan will be saved as days/day{day_number:02d}.md and then implemented."""

    return call_claude(prompt, timeout=300)


# ---------------------------------------------------------------------------
# Implementation — Claude Opus via CLI (interactive mode)
# ---------------------------------------------------------------------------

def run_coding(day_number: int) -> bool:
    """Invoke Claude Opus via the `claude` CLI to implement the day plan."""
    plan_path = DAYS_DIR / f"day{day_number:02d}.md"
    prompt = f"""Read the day plan at {plan_path} and implement it fully.

Also read CLAUDE.md and docs/vision.md for project context before starting.

Work through each deliverable. When done, write a brief summary of what was built.
If the plan includes starting a dev server, start it and leave it running.
Do NOT commit — the user will review and commit manually."""

    print(f"\n  Launching Claude Opus for Day {day_number:02d} implementation...")
    try:
        result = subprocess.run(
            ["claude", "--model", CLAUDE_OPUS, "--print",
             "--dangerously-skip-permissions", prompt],
            cwd=str(PROJECT_ROOT),
            timeout=900,
            env=_clean_env(),
        )
        return result.returncode == 0
    except FileNotFoundError:
        print("  ERROR: 'claude' CLI not found. Plan saved — implement manually.")
        return False
    except subprocess.TimeoutExpired:
        print("  WARNING: Claude Opus timed out after 15 minutes.")
        return False


# ---------------------------------------------------------------------------
# Smoke testing — pure HTTP, zero AI tokens
# ---------------------------------------------------------------------------

def smoke_test(port: int = 4000) -> dict:
    """Hit localhost endpoints. Returns structured results."""
    endpoints = ["/", "/api/health", "/api/properties", "/api/areas"]
    results = {}
    for ep in endpoints:
        url = f"http://localhost:{port}{ep}"
        try:
            req = urllib.request.Request(url, headers={"Accept": "application/json, text/html"})
            with urllib.request.urlopen(req, timeout=5) as resp:
                body = resp.read().decode("utf-8", errors="replace")
                results[ep] = {
                    "status": resp.status,
                    "content_type": resp.headers.get("Content-Type", ""),
                    "body_preview": body[:500],
                }
        except urllib.error.URLError as e:
            results[ep] = {"status": "error", "error": str(e)}
        except Exception as e:
            results[ep] = {"status": "error", "error": str(e)}
    return results


# ---------------------------------------------------------------------------
# Feedback — Claude reviews results
# ---------------------------------------------------------------------------

def generate_feedback(day_number: int, plan: str, smoke_results: dict,
                      learnings: dict) -> dict:
    """Claude reviews implementation results and generates feedback."""
    prompt = f"""You are reviewing the implementation results for OpenEstates Day {day_number:02d}.

Plan summary:
{plan[:1500]}

Smoke test results:
{json.dumps(smoke_results, indent=2)}

Learnings:
{json.dumps(learnings, indent=2)}

Review the results and respond in JSON (no markdown fences, just raw JSON):
{{
  "overall_impression": "short summary of how implementation went",
  "matches_plan": true/false,
  "issues_found": ["list of specific issues"],
  "improvements": ["specific carry-over improvements for next day"],
  "suggestions": "what the next day should focus on and why",
  "priority_for_next_day": "single most important thing"
}}"""

    response = call_claude(prompt, timeout=120)
    if not response:
        return {"overall_impression": "Review unavailable", "improvements": [], "day": day_number}

    # Parse JSON from response
    try:
        return json.loads(response)
    except json.JSONDecodeError:
        # Try extracting from code block
        if "```json" in response:
            try:
                return json.loads(response.split("```json")[1].split("```")[0].strip())
            except json.JSONDecodeError:
                pass
        elif "```" in response:
            try:
                return json.loads(response.split("```")[1].split("```")[0].strip())
            except (json.JSONDecodeError, IndexError):
                pass
    return {"overall_impression": response[:500], "improvements": [], "day": day_number}


# ---------------------------------------------------------------------------
# Learnings & persistence
# ---------------------------------------------------------------------------

def document_learnings(day_number: int, smoke_results: dict) -> dict:
    """Create a structured learnings document after implementation."""
    LEARNINGS_DIR.mkdir(parents=True, exist_ok=True)

    failures = {ep: r for ep, r in smoke_results.items() if r.get("status") == "error"}
    successes = {ep: r for ep, r in smoke_results.items() if r.get("status") != "error"}

    learnings = {
        "day": day_number,
        "date": date.today().isoformat(),
        "endpoints_ok": list(successes.keys()),
        "endpoints_failed": list(failures.keys()),
        "failure_details": {ep: r.get("error", "") for ep, r in failures.items()},
    }

    path = LEARNINGS_DIR / f"day{day_number:02d}_learnings.json"
    path.write_text(json.dumps(learnings, indent=2))
    print(f"  Learnings saved: {path}")

    return learnings


def save_feedback(day_number: int, feedback: dict):
    FEEDBACK_DIR.mkdir(parents=True, exist_ok=True)
    feedback["day"] = day_number
    feedback["review"] = feedback.get("overall_impression", "")
    path = FEEDBACK_DIR / f"day{day_number:02d}_feedback.json"
    path.write_text(json.dumps(feedback, indent=2))
    print(f"  Feedback saved: {path}")


def save_plan(day_number: int, content: str) -> Path:
    DAYS_DIR.mkdir(exist_ok=True)
    path = DAYS_DIR / f"day{day_number:02d}.md"
    path.write_text(content)
    print(f"  Saved: {path}")
    return path


# ---------------------------------------------------------------------------
# Vercel deployment
# ---------------------------------------------------------------------------

def deploy_vercel() -> Optional[str]:
    """Build and deploy frontend to Vercel production. Returns deployed URL or None."""
    frontend_dir = PROJECT_ROOT / "frontend"
    if not (frontend_dir / "package.json").exists():
        print("      Skipping — no frontend/package.json found")
        return None

    print("      Installing dependencies and building...")
    build = subprocess.run(
        ["npm", "run", "build"],
        cwd=str(frontend_dir),
        capture_output=True,
        text=True,
        timeout=120,
    )
    if build.returncode != 0:
        print(f"      Build failed: {build.stderr[:300]}")
        return None

    print("      Deploying to Vercel (production)...")
    try:
        deploy = subprocess.run(
            ["npx", "vercel", "deploy", "--prod", "--yes"],
            cwd=str(frontend_dir),
            capture_output=True,
            text=True,
            timeout=120,
        )
        if deploy.returncode == 0:
            urls = []
            for line in deploy.stdout.splitlines():
                line = line.strip()
                if line.startswith("https://") and "vercel.app" in line:
                    urls.append(line)
            url = urls[-1] if urls else None
            if url:
                print(f"      Deployed: {url}")
            else:
                print("      Deployed (URL not parsed from output)")
            return url
        else:
            print(f"      Deploy failed: {deploy.stderr[:300]}")
            return None
    except FileNotFoundError:
        print("      'vercel' CLI not found — run 'npx vercel login' first")
        return None
    except subprocess.TimeoutExpired:
        print("      Deploy timed out after 120s")
        return None


# ---------------------------------------------------------------------------
# Main loop — 1 day at a time
# ---------------------------------------------------------------------------

def run_day(day_number: int, plan_only: bool = False, port: int = 4000,
            resume: bool = False) -> bool:
    """Run a single day: plan → implement → test → feedback → deploy."""

    cp = load_checkpoint(day_number) if resume else {}

    print(f"\n{'='*60}")
    print(f"  DAY {day_number:02d}  —  {date.today().isoformat()}")
    print(f"  Mode: {'plan-only' if plan_only else 'full (plan + code + test + feedback)'}")
    print(f"  Engine: Claude Opus ({CLAUDE_OPUS})")
    if resume and cp:
        print(f"  Resuming — completed steps: {list(cp.keys())}")
    print(f"{'='*60}\n")

    # Load context
    print("  Loading context...")
    project_context = load_project_context()
    previous_days = load_previous_days_summary()
    feedback = load_feedback_history()

    # Check for prior smoke results
    prev_day = day_number - 1
    prev_cp = load_checkpoint(prev_day) if prev_day > 0 else {}
    prev_smoke = prev_cp.get("smoke_results")

    # ── Step 1: Generate plan with Claude ──
    plan_file = DAYS_DIR / f"day{day_number:02d}.md"
    if plan_file.exists():
        print(f"\n  [1] Plan already exists at {plan_file}")
        plan = plan_file.read_text()
    elif resume and "plan" in cp:
        print(f"\n  [1] Resuming: plan from checkpoint")
        plan = cp["plan"]
        save_plan(day_number, plan)
    else:
        print(f"\n  [1] Claude generating Day {day_number:02d} plan...")
        plan = create_plan(day_number, project_context, previous_days, feedback,
                           smoke_results=prev_smoke)
        if not plan:
            print("  ERROR: Plan generation failed.")
            return False
        save_checkpoint(day_number, {"plan": plan})
        save_plan(day_number, plan)
        print(f"      Plan generated ({len(plan)} chars)")

    if plan_only:
        print(f"\n  Plan saved. Stopping (--plan-only).")
        return True

    # ── Step 2: Implement with Claude Opus ──
    if resume and cp.get("code_success") is not None:
        print(f"\n  [2] Resuming: coding already done (success={cp['code_success']})")
        code_ok = cp["code_success"]
    else:
        print(f"\n  [2] Implementing Day {day_number:02d} with Claude Opus...")
        code_ok = run_coding(day_number)
        save_checkpoint(day_number, {"code_success": code_ok})

    # ── Step 3: Smoke test ──
    if resume and "smoke_results" in cp:
        print(f"\n  [3] Resuming: smoke test from checkpoint")
        smoke = cp["smoke_results"]
    else:
        print(f"\n  [3] Smoke testing localhost:{port}...")
        time.sleep(3)
        smoke = smoke_test(port=port)
        save_checkpoint(day_number, {"smoke_results": smoke})

    failures = {ep: r for ep, r in smoke.items() if r.get("status") == "error"}
    successes = {ep: r for ep, r in smoke.items() if r.get("status") != "error"}
    print(f"      OK: {list(successes.keys())}")
    if failures:
        print(f"      FAILED: {list(failures.keys())}")

    # ── Step 4: Document learnings ──
    if resume and "learnings" in cp:
        print(f"\n  [4] Resuming: learnings from checkpoint")
        learnings = cp["learnings"]
    else:
        print(f"\n  [4] Documenting learnings...")
        learnings = document_learnings(day_number, smoke)
        save_checkpoint(day_number, {"learnings": learnings})

    # ── Step 5: Claude reviews and generates feedback ──
    if resume and "feedback" in cp:
        print(f"\n  [5] Resuming: feedback from checkpoint")
        feedback_data = cp["feedback"]
    else:
        print(f"\n  [5] Claude reviewing results and generating feedback...")
        feedback_data = generate_feedback(day_number, plan, smoke, learnings)
        save_feedback(day_number, feedback_data)
        save_checkpoint(day_number, {"feedback": feedback_data})

    improvements = feedback_data.get("improvements", [])
    priority = feedback_data.get("priority_for_next_day", "")
    if improvements:
        print("  Carry-over improvements:")
        for imp in improvements:
            print(f"    - {imp}")
    if priority:
        print(f"  Next day priority: {priority}")

    # ── Step 6: Deploy frontend to Vercel ──
    vercel_url = None
    if resume and "vercel_url" in cp:
        print(f"\n  [6] Resuming: Vercel deploy from checkpoint")
        vercel_url = cp["vercel_url"]
    else:
        print(f"\n  [6] Deploying frontend to Vercel...")
        vercel_url = deploy_vercel()
        save_checkpoint(day_number, {
            "vercel_deployed": vercel_url is not None,
            "vercel_url": vercel_url,
        })

    if vercel_url:
        print(f"      Live at: {vercel_url}")

    status = "COMPLETE" if code_ok else "had issues — review manually"
    print(f"\n  Day {day_number:02d} {status}")
    print(f"{'='*60}\n")
    return code_ok


# ---------------------------------------------------------------------------
# Multi-day loop
# ---------------------------------------------------------------------------

def run_loop(start_day: int, num_days: int, plan_only: bool = False,
             port: int = 4000):
    """Run multiple days in sequence with feedback between each."""

    print(f"\n{'#'*60}")
    print(f"  SPRINT — Days {start_day:02d} to {start_day + num_days - 1:02d}")
    print(f"  {date.today().isoformat()}")
    print(f"  Engine: Claude Opus ({CLAUDE_OPUS})")
    print(f"  Mode: {'plan-only' if plan_only else 'full (plan + code + test + feedback)'}")
    print(f"{'#'*60}\n")

    completed = []
    failed = []

    for i in range(num_days):
        day = start_day + i

        if is_day_implemented(day):
            print(f"\n  Day {day:02d} already done — skipping.")
            completed.append(day)
            continue

        print(f"\n  ── Sprint day {i + 1}/{num_days} (Day {day:02d}) ──")

        try:
            ok = run_day(day, plan_only=plan_only, port=port)
            if ok:
                completed.append(day)
            else:
                failed.append(day)
                print(f"\n  Day {day:02d} had issues. Continuing to next day...")
        except KeyboardInterrupt:
            print(f"\n\n  Sprint interrupted at Day {day:02d}.")
            break
        except Exception as e:
            print(f"\n  Day {day:02d} crashed: {e}")
            failed.append(day)
            save_checkpoint(day, {"crash": str(e), "code_success": False})
            print("  Continuing to next day...")

    # Sprint summary
    print(f"\n{'#'*60}")
    print(f"  SPRINT COMPLETE")
    print(f"  Completed: {completed}")
    if failed:
        print(f"  Failed:    {failed}")
    print(f"{'#'*60}")
    print_status()


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main():
    import argparse

    parser = argparse.ArgumentParser(
        description="OpenEstates Day Agent (v3 — Claude-only)",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  python3 pipeline/agent.py                    # auto-detect next day, full cycle
  python3 pipeline/agent.py --day 8            # run specific day
  python3 pipeline/agent.py --day 8 --resume   # resume from checkpoint
  python3 pipeline/agent.py --plan-only        # plan only, skip implementation
  python3 pipeline/agent.py --loop 6           # run 6 days in sequence (overnight mode)
  python3 pipeline/agent.py --loop 6 --plan-only  # plan 6 days without coding
  python3 pipeline/agent.py --status           # show all day statuses
""")
    parser.add_argument("--day", type=int, default=None,
                        help="Specific day number (default: auto-detect next)")
    parser.add_argument("--plan-only", action="store_true",
                        help="Generate plan only — skip coding and testing")
    parser.add_argument("--port", type=int, default=4000,
                        help="Localhost port to smoke-test (default: 4000)")
    parser.add_argument("--resume", action="store_true",
                        help="Resume from last checkpoint")
    parser.add_argument("--loop", type=int, default=None, metavar="N",
                        help="Run N days in sequence (overnight sprint mode)")
    parser.add_argument("--status", action="store_true",
                        help="Show day status and exit")
    args = parser.parse_args()

    if args.status:
        print_status()
        return

    start_day = args.day or detect_next_day_number()

    print(f"\n  OpenEstates Day Agent v3 (Claude-only)")
    print(f"  Next day: {start_day:02d}")
    print(f"  Engine: Claude Opus ({CLAUDE_OPUS})")

    print_status()

    if args.loop:
        run_loop(start_day, num_days=args.loop, plan_only=args.plan_only,
                 port=args.port)
    else:
        run_day(start_day, plan_only=args.plan_only, port=args.port,
                resume=args.resume)

    print("\n  Done.")


if __name__ == "__main__":
    main()
