"""
OpenEstates Sprint Agent — Autonomous day-by-day feature builder.

Two-layer agent architecture using `claude` CLI (zero Python dependencies):
  PM Agent:      Envisions features, researches market, writes day specs
  Builder Agent: Implements each day's spec with full codebase access
  Verifier:      Checks acceptance criteria, runs build checks
  Orchestrator:  Python loop that sequences everything

Usage:
  python3 -u pipeline/sprint_agent.py                           # auto-detect, run 1 day
  python3 -u pipeline/sprint_agent.py --days 5                  # 5-day sprint
  python3 -u pipeline/sprint_agent.py --start-day 31 --days 10  # days 31-40
  python3 -u pipeline/sprint_agent.py --plan-only               # PM plans, no coding
  python3 -u pipeline/sprint_agent.py --build-only              # skip planning, build existing
  python3 -u pipeline/sprint_agent.py --status                  # show day status
  python3 -u pipeline/sprint_agent.py --vision vision.md        # custom vision file
  python3 -u pipeline/sprint_agent.py --dry-run                 # preview what would run

Requirements:
  - `claude` CLI installed (npm install -g @anthropic-ai/claude-code)
  - Claude Max/Pro subscription OR ANTHROPIC_API_KEY in .env

Notes:
  - Use `python3 -u` for unbuffered output when monitoring overnight
  - Checkpoints saved after every step — fully resumable with --resume
  - Compatible with existing pipeline/checkpoints/ from agent.py
  - Each agent invocation uses a fresh claude session
  - Builder uses --dangerously-skip-permissions for autonomous operation
"""

from __future__ import annotations

import argparse
import json
import logging
import os
import subprocess
import sys
import time
from datetime import date
from pathlib import Path
from typing import Optional

# ---------------------------------------------------------------------------
# Setup
# ---------------------------------------------------------------------------

PROJECT_ROOT = Path(__file__).resolve().parent.parent
DAYS_DIR = PROJECT_ROOT / "days"
CHECKPOINTS_DIR = PROJECT_ROOT / "pipeline" / "checkpoints"
FEEDBACK_DIR = PROJECT_ROOT / "pipeline" / "feedback"
LEARNINGS_DIR = PROJECT_ROOT / "pipeline" / "learnings"

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    datefmt="%H:%M:%S",
)
log = logging.getLogger("sprint")


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


def _claude_env() -> dict:
    """Env for claude CLI calls — remove CLAUDECODE to allow nesting."""
    env = os.environ.copy()
    env.pop("CLAUDECODE", None)
    return env


# ---------------------------------------------------------------------------
# Company pillars — system prompt for all agents
# ---------------------------------------------------------------------------

COMPANY_PILLARS = """\
You are building OpenEstates — a transparency-first property discovery platform.
Target market: India (Bengaluru first, then expanding).

PILLARS (non-negotiable in every decision):

1. TRANSPARENCY: Every ranking, score, recommendation must be explainable.
   No hidden magic. Users see WHY a property ranks where it does.
   Every fact carries provenance (SourcedFact pattern).

2. RICH UI: Premium, calm, Hinge-meets-Robinhood feel. Every component
   should reduce ambiguity and aid comparison. Property pages feel like
   Bloomberg terminals for homes — data-rich but not overwhelming.

3. SLEEK UI: Minimal, modern, no clutter. White space is a feature.
   Animations are subtle. Typography is clean. Colors are muted.

4. LATENCY SENSITIVITY: Users feel delays >200ms. Cache aggressively.
   Optimistic UI. Stream where possible. Backend responses <100ms for
   cached data. Live discovery can be slower but must show progress.

5. CONTEXT > FILTERS: Search by intent ("quiet family area near good schools"),
   not dropdowns. The search engine understands preferences, tradeoffs,
   and soft signals — not just price/BHK/area.

CODING PRACTICES:
- Rust+Axum backend (port 4000), React frontend (port 5173), Python pipeline
- Explicit models, clean file boundaries, no premature abstractions
- Every fact carries provenance (SourcedFact with display_template, scoring_hint)
- Skills own the domain, Rust owns the runtime
- Test before moving forward — cargo check, npm run build
- No over-engineering: minimum complexity for the current task\
"""

# ---------------------------------------------------------------------------
# Default vision
# ---------------------------------------------------------------------------

DEFAULT_VISION = """\
OpenEstates Vision — Phase by phase:

PHASE 1 (Current): Search + Discovery
- Natural language search that understands intent, not just keywords
- Results with match explanations showing WHY each property ranks
- Knowledge graph that learns from every search (the flywheel)
- Live discovery via Gemini when data is missing
- Progressive enrichment triggered by user interest

PHASE 2: Property Intelligence
- Property detail pages that feel like Bloomberg terminals for homes
- Society quality scores with Reddit-sourced evidence
- RERA verification status, builder track record
- Area intelligence: noise, traffic, waterlogging, metro access
- Conviction widgets that help users feel confident

PHASE 3: Social Proof Layer
- Reddit sentiment aggregation per society
- Community-reported issues and praises
- Builder reputation tracking across projects

PHASE 4: Decision Tools
- Shortlist and compare that reduces decision anxiety
- Side-by-side comparison with tradeoff analysis
- "What would you give up?" tradeoff exploration

PHASE 5: Market Intelligence
- Price trend tracking per area/society
- "Is this a good deal?" confidence scoring
- Supply-demand signals from listing velocity

COMPETITIVE GAPS TO EXPLOIT:
- 99acres/Housing.com: zero explanation of WHY results rank
- MagicBricks: no society-level intelligence
- NoBroker: good on brokerage, weak on area intelligence
- None of them: Reddit/community signal integration
- None of them: progressive enrichment that learns from searches\
"""


def load_vision(vision_path: Optional[str] = None) -> str:
    if vision_path:
        p = Path(vision_path)
        if p.exists():
            return p.read_text()
        log.warning("Vision file %s not found, using default", vision_path)
    return DEFAULT_VISION


# ---------------------------------------------------------------------------
# Checkpointing (compatible with existing agent.py)
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
    for d in range(1, 200):
        plan_exists = (DAYS_DIR / f"day{d:02d}.md").exists()
        done = is_day_done(d)
        if done:
            continue
        return d
    return 1


def print_status():
    print("\n  Day Status:")
    found_any = False
    for d in range(1, 200):
        cp = load_checkpoint(d)
        plan_exists = (DAYS_DIR / f"day{d:02d}.md").exists()
        if not plan_exists and not cp:
            if found_any:
                break
            continue
        found_any = True
        if cp.get("code_success"):
            label = "DONE"
        elif plan_exists:
            label = "PLAN READY (code pending)"
        elif cp.get("sprint_plan"):
            label = "planned (not saved to file)"
        else:
            label = "not started"
        print(f"    Day {d:02d}: {label}")
    print()


# ---------------------------------------------------------------------------
# Context loaders
# ---------------------------------------------------------------------------

def load_completed_days_summary() -> str:
    """Summaries of completed days for PM context (first 30 lines each)."""
    parts = []
    for d in range(1, 200):
        plan_file = DAYS_DIR / f"day{d:02d}.md"
        cp = load_checkpoint(d)
        if not plan_file.exists() and not cp:
            break
        status = "DONE" if cp.get("code_success") else "PENDING"
        if plan_file.exists():
            lines = plan_file.read_text().splitlines()[:30]
            parts.append(f"## Day {d:02d} [{status}]\n" + "\n".join(lines) + "\n...")
    return "\n\n---\n\n".join(parts) if parts else "No previous days yet."


def load_feedback_for_pm() -> str:
    """Accumulated feedback from previous days."""
    FEEDBACK_DIR.mkdir(parents=True, exist_ok=True)
    parts = []
    for f in sorted(FEEDBACK_DIR.glob("day*_feedback.json")):
        try:
            data = json.loads(f.read_text())
        except json.JSONDecodeError:
            continue
        day = data.get("day", "?")
        review = data.get("overall_impression", data.get("gpt_review", ""))
        improvements = data.get("improvements", [])
        if review or improvements:
            part = f"### Day {day} feedback\n{review}"
            if improvements:
                part += "\nCarry-over: " + ", ".join(str(i) for i in improvements)
            parts.append(part)
    return "\n\n".join(parts)


def load_search_log_insights() -> str:
    """Recent search queries for PM to understand user interest."""
    log_dir = PROJECT_ROOT / "data" / "knowledge" / "search_log"
    if not log_dir.exists():
        return ""
    log_files = sorted(log_dir.rglob("*.jsonl"), reverse=True)[:3]
    queries = []
    for lf in log_files:
        for line in lf.read_text().splitlines()[-20:]:
            try:
                q = json.loads(line).get("query", "")
                if q:
                    queries.append(q)
            except json.JSONDecodeError:
                pass
    if not queries:
        return ""
    return "### Recent user searches\n" + "\n".join(f"- {q}" for q in queries[:30])


# ---------------------------------------------------------------------------
# Claude CLI wrapper
# ---------------------------------------------------------------------------

def call_claude(
    prompt: str,
    system_prompt: str = "",
    allowed_tools: Optional[list] = None,
    model: str = "opus",
    timeout: int = 1800,  # 30 min default
    max_turns: Optional[int] = None,
    skip_permissions: bool = False,
    output_format: str = "text",
) -> tuple[bool, str]:
    """Call claude CLI with --print. Returns (success, output_text).

    Uses the installed `claude` CLI which handles auth via subscription
    or ANTHROPIC_API_KEY. Each call is a fresh session.
    """
    cmd = [
        "claude",
        "--print",
        "--model", model,
        "--output-format", output_format,
    ]

    if system_prompt:
        cmd.extend(["--system-prompt", system_prompt])

    if allowed_tools:
        cmd.extend(["--allowedTools", ",".join(allowed_tools)])

    if max_turns:
        cmd.extend(["--max-turns", str(max_turns)])

    if skip_permissions:
        cmd.append("--dangerously-skip-permissions")

    cmd.append(prompt)

    log.debug("claude cmd: %s", " ".join(cmd[:6]) + "...")

    try:
        result = subprocess.run(
            cmd,
            cwd=str(PROJECT_ROOT),
            capture_output=True,
            text=True,
            timeout=timeout,
            env=_claude_env(),
        )
        output = result.stdout.strip()
        if result.returncode != 0 and not output:
            output = result.stderr.strip()
        return result.returncode == 0, output
    except subprocess.TimeoutExpired:
        log.warning("Claude CLI timed out after %ds", timeout)
        return False, f"TIMEOUT after {timeout}s"
    except FileNotFoundError:
        log.error("'claude' CLI not found. Install: npm install -g @anthropic-ai/claude-code")
        return False, "claude CLI not found"


# ---------------------------------------------------------------------------
# PM Agent — plans and envisions features
# ---------------------------------------------------------------------------

def run_pm_agent(day_number: int, vision: str, num_days: int = 1) -> list[dict]:
    """PM agent: reads codebase, researches market, produces day specs.

    Returns list of {day, plan_markdown}.
    """
    completed = load_completed_days_summary()
    feedback = load_feedback_for_pm()
    search_insights = load_search_log_insights()

    prompt = f"""You are the Product Manager for OpenEstates. Plan the next {num_days} day(s)
of engineering work, starting at Day {day_number}.

## Product Vision
{vision}

## Completed Work (first 30 lines of each day plan)
{completed}

{"## Feedback from Previous Days" + chr(10) + feedback if feedback else ""}

{search_insights if search_insights else ""}

## Your Task

Plan Day {day_number}{f" through Day {day_number + num_days - 1}" if num_days > 1 else ""}.

For each day, produce a COMPLETE day spec markdown document with these sections:
## 1. Goal — one-line theme
## 2. Product Reason — why this is the right next thing to build.
   DO research before deciding:
   - Read the existing codebase to understand what exists
   - Consider what would differentiate OpenEstates from 99acres, Housing.com, NoBroker
   - Think about what Indian property buyers actually struggle with
   - Look at what features are half-built and should be completed first
## 3. Deliverables — specific, testable outcomes with file paths
## 4. Technical Guidance — concrete enough for an engineer to implement
   (API shapes, component names, data structures, which files to modify)
## 5. Constraints — what NOT to build, scope limits
## 6. Success Criteria — checkboxes, each objectively verifiable

RULES:
- Each day must be achievable in ONE coding session (~3 hours of agent time)
- Build incrementally on previous days — don't redo working code
- Reference specific files and modules
- Be concrete about API shapes, component names, data structures
- If the codebase has broken or incomplete work, fixing it is a valid deliverable

{"Separate multiple days with: ---DAY_SEPARATOR---" if num_days > 1 else ""}

Start each day with: # Day NN: [Theme]"""

    system = COMPANY_PILLARS + "\nYou are in PM/planning mode. Read the codebase, research, and plan — do NOT write code."

    log.info("PM Agent: planning Day %d...", day_number)
    ok, output = call_claude(
        prompt=prompt,
        system_prompt=system,
        allowed_tools=["Read", "Glob", "Grep", "WebSearch", "WebFetch"],
        model="opus",
        timeout=600,   # 10 min for planning
        max_turns=30,
        skip_permissions=True,
    )

    if not ok:
        log.error("PM Agent failed: %s", output[:300])
        return []

    # Parse specs
    specs = []
    if num_days == 1:
        specs.append({"day": day_number, "plan_markdown": output})
    else:
        parts = output.split("---DAY_SEPARATOR---")
        for i, part in enumerate(parts):
            part = part.strip()
            if part:
                specs.append({"day": day_number + i, "plan_markdown": part})

    log.info("PM Agent: produced %d day spec(s)", len(specs))
    return specs


# ---------------------------------------------------------------------------
# Builder Agent — implements one day
# ---------------------------------------------------------------------------

def run_builder_agent(day_number: int, plan_markdown: str) -> dict:
    """Builder agent: implements the day spec. Returns {success, output}."""

    # Write plan to a temp location so claude can reference it
    plan_ref = DAYS_DIR / f"day{day_number:02d}.md"

    prompt = f"""## Your Task: Implement Day {day_number}

Read the day plan at {plan_ref} and implement ALL deliverables.

Also read CLAUDE.md for project context and coding standards.

## Instructions
1. Read the day plan and CLAUDE.md first
2. Check what currently exists — read relevant files before changing them
3. Implement each deliverable one at a time
4. After each significant change, verify:
   - `cargo check` for Rust changes
   - `npm run build` for frontend changes
5. When ALL deliverables are done, verify ALL success criteria
6. Write a brief summary of what was built

RULES:
- DO NOT commit to git — the orchestrator handles that
- DO NOT deploy — the orchestrator handles Vercel
- DO NOT skip verification — every change must compile/build
- If something from a previous day is broken, fix it FIRST
- If a deliverable is unclear, make the best product-aligned decision
- Prefer editing existing files over creating new ones

## Day Plan
{plan_markdown}"""

    log.info("Builder Agent: implementing Day %d...", day_number)
    ok, output = call_claude(
        prompt=prompt,
        system_prompt=COMPANY_PILLARS,
        allowed_tools=[
            "Read", "Write", "Edit", "Bash", "Glob", "Grep",
            "Agent",   # can spawn subagents for parallel work
        ],
        model="opus",
        timeout=2400,    # 40 min for implementation
        max_turns=80,
        skip_permissions=True,
    )

    return {"success": ok, "output": output}


# ---------------------------------------------------------------------------
# Verifier Agent — checks acceptance criteria
# ---------------------------------------------------------------------------

def run_verifier_agent(day_number: int, plan_markdown: str) -> dict:
    """Checks if the day's acceptance criteria are met.

    Returns {passed, failures, summary}.
    """
    prompt = f"""## Verify Day {day_number} Implementation

Check EACH success criterion from the day plan below.

## Steps
1. Run `cargo check` in backend/ — must pass
2. Run `npm run build` in frontend/ — must pass
3. For each success criterion, verify it's actually done
   (check files exist, code is correct, endpoints would work)
4. If backend is running on port 4000, test API endpoints

## Day Plan
{plan_markdown}

## Output Format
Output ONLY a JSON object (no markdown, no commentary):
{{
  "passed": true/false,
  "cargo_check": "pass" or "fail",
  "npm_build": "pass" or "fail",
  "criteria_results": [
    {{"criterion": "...", "passed": true/false, "detail": "..."}}
  ],
  "failures": ["what failed"],
  "summary": "one paragraph"
}}"""

    log.info("Verifier Agent: checking Day %d...", day_number)
    ok, output = call_claude(
        prompt=prompt,
        allowed_tools=["Bash", "Read", "Glob", "Grep"],
        model="opus",
        timeout=300,     # 5 min for verification
        max_turns=20,
        skip_permissions=True,
    )

    # Parse JSON from output
    try:
        # Strip markdown code fences if present
        text = output
        if "```json" in text:
            text = text.split("```json")[1].split("```")[0].strip()
        elif "```" in text:
            text = text.split("```")[1].split("```")[0].strip()
        return json.loads(text)
    except (json.JSONDecodeError, IndexError):
        return {
            "passed": False,
            "failures": ["Could not parse verifier output"],
            "summary": output[:500],
        }


# ---------------------------------------------------------------------------
# Build checks (direct, no agent needed)
# ---------------------------------------------------------------------------

def run_build_checks() -> dict:
    """Run cargo check + npm build directly. Returns structured result."""
    results = {}

    for name, cmd, cwd in [
        ("cargo_check", ["cargo", "check"], PROJECT_ROOT / "backend"),
        ("npm_build", ["npm", "run", "build"], PROJECT_ROOT / "frontend"),
    ]:
        try:
            r = subprocess.run(
                cmd, cwd=str(cwd),
                capture_output=True, text=True, timeout=120,
            )
            results[name] = {
                "success": r.returncode == 0,
                "stderr_preview": r.stderr[:500] if r.returncode != 0 else "",
            }
        except (FileNotFoundError, subprocess.TimeoutExpired) as e:
            results[name] = {"success": False, "error": str(e)}

    results["all_pass"] = all(
        v.get("success", False) for v in results.values()
        if isinstance(v, dict) and "success" in v
    )
    return results


# ---------------------------------------------------------------------------
# Vercel deploy
# ---------------------------------------------------------------------------

def deploy_vercel() -> Optional[str]:
    frontend_dir = PROJECT_ROOT / "frontend"
    if not (frontend_dir / "package.json").exists():
        return None
    try:
        r = subprocess.run(
            ["npx", "vercel", "deploy", "--prod", "--yes"],
            cwd=str(frontend_dir),
            capture_output=True, text=True, timeout=180,
        )
        if r.returncode == 0:
            urls = [
                l.strip() for l in r.stdout.splitlines()
                if l.strip().startswith("https://") and "vercel.app" in l
            ]
            return urls[-1] if urls else None
    except (FileNotFoundError, subprocess.TimeoutExpired):
        pass
    return None


# ---------------------------------------------------------------------------
# Orchestrator — run one day
# ---------------------------------------------------------------------------

def run_one_day(
    day_number: int,
    vision: str,
    plan_only: bool = False,
    build_only: bool = False,
    deploy: bool = True,
    resume: bool = True,
) -> bool:
    """Run a single day: plan -> build -> verify -> checkpoint."""
    cp = load_checkpoint(day_number) if resume else {}

    log.info("=" * 60)
    log.info("  DAY %02d  —  %s", day_number, date.today().isoformat())
    log.info("=" * 60)

    plan_file = DAYS_DIR / f"day{day_number:02d}.md"

    # ── Step 1: Plan ──────────────────────────────────────────────
    if plan_file.exists():
        log.info("[1/5] Plan exists: %s", plan_file)
        plan_md = plan_file.read_text()
    elif build_only:
        log.error("No plan for Day %d and --build-only set", day_number)
        return False
    elif resume and cp.get("sprint_plan"):
        log.info("[1/5] Plan from checkpoint")
        plan_md = cp["sprint_plan"]
        DAYS_DIR.mkdir(exist_ok=True)
        plan_file.write_text(plan_md)
    else:
        log.info("[1/5] PM Agent: creating plan...")
        specs = run_pm_agent(day_number, vision, num_days=1)
        if not specs:
            log.error("PM Agent returned no specs")
            return False
        plan_md = specs[0]["plan_markdown"]
        save_checkpoint(day_number, {"sprint_plan": plan_md})
        DAYS_DIR.mkdir(exist_ok=True)
        plan_file.write_text(plan_md)
        log.info("Plan saved: %s (%d chars)", plan_file, len(plan_md))

    if plan_only:
        log.info("Plan saved. Stopping (--plan-only).")
        return True

    # ── Step 2: Build ─────────────────────────────────────────────
    if resume and cp.get("code_success") is not None:
        log.info("[2/5] Build already done (success=%s)", cp["code_success"])
        build_result = {"success": cp["code_success"], "output": cp.get("build_summary", "")}
    else:
        log.info("[2/5] Builder Agent: implementing...")
        build_result = run_builder_agent(day_number, plan_md)
        save_checkpoint(day_number, {
            "build_summary": build_result["output"][:3000],
        })
        log.info("Builder done. Success: %s", build_result["success"])

    # ── Step 3: Verify ────────────────────────────────────────────
    if resume and "verification" in cp:
        log.info("[3/5] Verification from checkpoint")
        verification = cp["verification"]
    else:
        log.info("[3/5] Verifier Agent: checking criteria...")
        verification = run_verifier_agent(day_number, plan_md)
        save_checkpoint(day_number, {"verification": verification})

    v_passed = verification.get("passed", False)
    v_failures = verification.get("failures", [])
    log.info("Verification: %s", "PASSED" if v_passed else f"FAILED: {v_failures}")

    # ── Step 4: Build checks ──────────────────────────────────────
    if resume and "build_checks" in cp:
        log.info("[4/5] Build checks from checkpoint")
        build_checks = cp["build_checks"]
    else:
        log.info("[4/5] Build checks (cargo check + npm build)...")
        build_checks = run_build_checks()
        save_checkpoint(day_number, {"build_checks": build_checks})

    log.info("Build checks: %s", "PASSED" if build_checks.get("all_pass") else "FAILED")
    if not build_checks.get("all_pass"):
        for k, v in build_checks.items():
            if isinstance(v, dict) and not v.get("success", True):
                log.warning("  %s: %s", k, v.get("stderr_preview", v.get("error", ""))[:200])

    # Overall success
    code_success = v_passed and build_checks.get("all_pass", False)
    save_checkpoint(day_number, {"code_success": code_success})

    # ── Step 5: Deploy ────────────────────────────────────────────
    if deploy and code_success:
        if resume and "vercel_url" in cp:
            log.info("[5/5] Vercel: %s (from checkpoint)", cp["vercel_url"])
        else:
            log.info("[5/5] Deploying to Vercel...")
            url = deploy_vercel()
            save_checkpoint(day_number, {
                "vercel_deployed": url is not None,
                "vercel_url": url,
            })
            if url:
                log.info("Deployed: %s", url)
            else:
                log.warning("Vercel deploy failed")
    elif not code_success:
        log.info("[5/5] Skipping deploy — verification/build failed")
    else:
        log.info("[5/5] Skipping deploy (disabled)")

    log.info("Day %02d: %s", day_number, "COMPLETE" if code_success else "FAILED")
    log.info("=" * 60)
    return code_success


# ---------------------------------------------------------------------------
# Sprint loop
# ---------------------------------------------------------------------------

def run_sprint(
    start_day: int,
    num_days: int,
    vision: str,
    plan_only: bool = False,
    build_only: bool = False,
    deploy: bool = True,
    max_retries: int = 1,
):
    """Run a multi-day sprint."""
    log.info("#" * 60)
    log.info("  SPRINT: Days %02d to %02d", start_day, start_day + num_days - 1)
    log.info("  %s", date.today().isoformat())
    log.info("#" * 60)

    # Batch-plan if multiple new days
    if not build_only and num_days > 1:
        needs_plan = [
            d for d in range(start_day, start_day + num_days)
            if not (DAYS_DIR / f"day{d:02d}.md").exists()
        ]
        if len(needs_plan) > 1:
            log.info("PM Agent: batch-planning %d days...", len(needs_plan))
            specs = run_pm_agent(needs_plan[0], vision, num_days=len(needs_plan))
            for spec in specs:
                d = spec["day"]
                pf = DAYS_DIR / f"day{d:02d}.md"
                DAYS_DIR.mkdir(exist_ok=True)
                pf.write_text(spec["plan_markdown"])
                save_checkpoint(d, {"sprint_plan": spec["plan_markdown"]})
                log.info("Saved plan: %s", pf)
            if plan_only:
                log.info("All plans saved. Stopping (--plan-only).")
                return

    completed = []
    failed = []

    for i in range(num_days):
        day = start_day + i

        if is_day_done(day):
            log.info("Day %02d already done — skipping.", day)
            completed.append(day)
            continue

        log.info("\n  Sprint day %d/%d (Day %02d)", i + 1, num_days, day)

        success = False
        for attempt in range(max_retries + 1):
            if attempt > 0:
                log.info("Retry %d/%d for Day %02d...", attempt, max_retries, day)
                # Clear build state for retry (keep plan)
                cp = load_checkpoint(day)
                for key in ["code_success", "verification", "build_checks",
                            "build_summary"]:
                    cp.pop(key, None)
                checkpoint_path(day).write_text(json.dumps(cp, indent=2))

            try:
                success = run_one_day(
                    day, vision,
                    plan_only=plan_only,
                    build_only=build_only,
                    deploy=deploy,
                    resume=(attempt == 0),  # only resume on first attempt
                )
                if success:
                    break
            except KeyboardInterrupt:
                log.info("\nSprint interrupted at Day %02d.", day)
                print_status()
                return
            except Exception as e:
                log.error("Day %02d crashed: %s", day, e, exc_info=True)
                save_checkpoint(day, {"crash": str(e), "code_success": False})

        if success:
            completed.append(day)
        else:
            failed.append(day)
            log.warning("Day %02d failed after %d attempts.", day, max_retries + 1)

    # Summary
    log.info("#" * 60)
    log.info("  SPRINT COMPLETE")
    log.info("  Completed: %s", completed)
    if failed:
        log.warning("  Failed: %s", failed)
    log.info("#" * 60)
    print_status()


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(
        description="OpenEstates Sprint Agent — autonomous day-by-day builder",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  python3 -u pipeline/sprint_agent.py                           # next day, full cycle
  python3 -u pipeline/sprint_agent.py --days 5                  # 5-day sprint
  python3 -u pipeline/sprint_agent.py --start-day 31 --days 10  # days 31-40
  python3 -u pipeline/sprint_agent.py --plan-only --days 3      # plan 3 days, no coding
  python3 -u pipeline/sprint_agent.py --build-only              # build existing plan
  python3 -u pipeline/sprint_agent.py --vision my_vision.md     # custom vision
  python3 -u pipeline/sprint_agent.py --status                  # show day status
  python3 -u pipeline/sprint_agent.py --dry-run --days 5        # preview
""",
    )
    parser.add_argument("--start-day", type=int, default=None,
                        help="Starting day number (default: auto-detect)")
    parser.add_argument("--days", type=int, default=1,
                        help="Number of days to run (default: 1)")
    parser.add_argument("--plan-only", action="store_true",
                        help="PM plans only — no coding")
    parser.add_argument("--build-only", action="store_true",
                        help="Build existing plans — no PM planning")
    parser.add_argument("--no-deploy", action="store_true",
                        help="Skip Vercel deployment")
    parser.add_argument("--vision", type=str, default=None,
                        help="Path to vision file (default: built-in)")
    parser.add_argument("--max-retries", type=int, default=1,
                        help="Max retries per failed day (default: 1)")
    parser.add_argument("--status", action="store_true",
                        help="Show day status and exit")
    parser.add_argument("--dry-run", action="store_true",
                        help="Preview without executing")
    parser.add_argument("--resume", action="store_true", default=True,
                        help="Resume from checkpoint (default: true)")
    parser.add_argument("--no-resume", action="store_true",
                        help="Start fresh, ignore checkpoints")
    args = parser.parse_args()

    if args.status:
        print_status()
        return

    start_day = args.start_day or detect_next_day()
    vision = load_vision(args.vision)

    if args.dry_run:
        print(f"\n  Dry run — would execute:")
        print(f"  Days: {start_day} to {start_day + args.days - 1}")
        print(f"  Mode: {'plan-only' if args.plan_only else 'build-only' if args.build_only else 'full'}")
        print(f"  Deploy: {not args.no_deploy}")
        print(f"  Vision: {'custom' if args.vision else 'default'} ({len(vision)} chars)")
        for d in range(start_day, start_day + args.days):
            s = "DONE" if is_day_done(d) else (
                "PLAN EXISTS" if (DAYS_DIR / f"day{d:02d}.md").exists() else "NEW"
            )
            print(f"    Day {d:02d}: {s}")
        print()
        return

    # Verify claude CLI exists
    try:
        subprocess.run(
            ["claude", "--version"],
            capture_output=True, timeout=10, env=_claude_env(),
        )
    except FileNotFoundError:
        print("ERROR: 'claude' CLI not found.")
        print("Install: npm install -g @anthropic-ai/claude-code")
        sys.exit(1)

    log.info("OpenEstates Sprint Agent")
    log.info("  Start day: %d, Days: %d", start_day, args.days)
    log.info("  Vision: %s", "custom" if args.vision else "default")
    print_status()

    run_sprint(
        start_day=start_day,
        num_days=args.days,
        vision=vision,
        plan_only=args.plan_only,
        build_only=args.build_only,
        deploy=not args.no_deploy,
        max_retries=args.max_retries,
    )

    log.info("Done.")


if __name__ == "__main__":
    main()
