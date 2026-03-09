"""
OpenEstates Day Agent (Simplified v2)

Loop (1 day at a time):
1. Find next day to implement from checkpoints
2. Ask ChatGPT for day plan (with prior feedback + smoke test results)
3. Implement with Claude Opus 4.6 via `claude` CLI
4. Run smoke tests
5. Document learnings and suggestions
6. Checkpoint after every step

Usage:
  python3 pipeline/agent.py                    # auto-detect next day, full cycle
  python3 pipeline/agent.py --day 8            # run specific day
  python3 pipeline/agent.py --day 8 --resume   # resume from checkpoint
  python3 pipeline/agent.py --plan-only        # plan only, skip implementation
  python3 pipeline/agent.py --status           # show day status

Notes:
  - Firefox must be CLOSED before running (profile is locked)
  - CHATGPT_CONVERSATION_ID in .env targets a specific conversation
  - Each day's Claude Opus run uses `claude` CLI (new conversation per day)
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

def load_gpt_context() -> str:
    """Load project context for ChatGPT — CLAUDE.md + blueprint."""
    parts = []

    claude_md = PROJECT_ROOT / "CLAUDE.md"
    if claude_md.exists():
        parts.append(f"# CLAUDE.md (Engineering Guidelines)\n{claude_md.read_text()}")

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
    """Load structured feedback from all previous days, including journey reviews."""
    FEEDBACK_DIR.mkdir(parents=True, exist_ok=True)
    feedback_files = sorted(FEEDBACK_DIR.glob("day*_feedback.json"))
    journey_files = sorted(FEEDBACK_DIR.glob("day*_journey_review.json"))

    if not feedback_files and not journey_files:
        return ""

    parts = []

    for f in feedback_files:
        data = json.loads(f.read_text())
        day = data.get("day", "?")
        review = data.get("gpt_review", data.get("overall_impression", ""))
        improvements = data.get("improvements", [])
        learnings = data.get("learnings", "")
        suggestions = data.get("suggestions", "")
        if review or improvements or learnings:
            part = f"### Day {day:02d} feedback\n{review}\n"
            if improvements:
                part += "\nCarry-over improvements:\n" + "\n".join(f"- {i}" for i in improvements)
            if learnings:
                part += f"\nLearnings:\n{learnings}\n"
            if suggestions:
                part += f"\nSuggestions for next day:\n{suggestions}\n"
            parts.append(part)

    for f in journey_files:
        data = json.loads(f.read_text())
        ux = data.get("overall_ux_impression", "")
        blockers = data.get("journey_blockers", [])
        improvements = data.get("improvements_for_next_day", [])
        journey_ok = data.get("customer_journey_works", None)
        day_str = f.stem.replace("_journey_review", "").replace("day", "")
        if ux or blockers or improvements:
            part = f"### Day {day_str} customer journey review (ChatGPT browsed live site)\n"
            part += f"Journey works: {journey_ok}\n"
            if ux:
                part += f"UX impression: {ux}\n"
            if blockers:
                part += "\nJourney blockers:\n" + "\n".join(f"- {b}" for b in blockers)
            if improvements:
                part += "\nUX improvements needed:\n" + "\n".join(f"- {i}" for i in improvements)
            parts.append(part)

    return "\n\n".join(parts)


def detect_next_day_number() -> int:
    """Find the next day that needs implementation.

    Days before the checkpoint system existed (1-5) are assumed done if their
    plan file exists and there's no checkpoint contradicting it.
    The first day with a checkpoint system is day 06.
    """
    FIRST_CHECKPOINTED_DAY = 6

    for day_num in range(1, 100):
        cp = load_checkpoint(day_num)
        plan_exists = (DAYS_DIR / f"day{day_num:02d}.md").exists()

        # Pre-checkpoint days: assume done if plan exists
        if day_num < FIRST_CHECKPOINTED_DAY:
            if not plan_exists:
                return day_num
            continue

        # Checkpointed days: use code_success flag
        if cp.get("code_success"):
            continue

        # Plan exists but not implemented → implement this one
        if plan_exists or cp.get("gpt_plan"):
            return day_num

        # Nothing exists → plan and implement this one
        return day_num

    return 1


def is_day_implemented(day_number: int) -> bool:
    cp = load_checkpoint(day_number)
    return cp.get("code_success") is True


def print_status():
    """Print status of all days."""
    FIRST_CHECKPOINTED_DAY = 6
    print("\n  Day status:")
    for day_num in range(1, 100):
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
        elif cp.get("gpt_plan"):
            label = "plan in checkpoint (not saved to file)"
        else:
            label = "not started"
        print(f"    Day {day_num:02d}: {label}")
    print()


# ---------------------------------------------------------------------------
# ChatGPT client
# ---------------------------------------------------------------------------

_chatgpt_client = None


def get_chatgpt_client():
    global _chatgpt_client
    if _chatgpt_client is None:
        sys.path.insert(0, str(PROJECT_ROOT / "pipeline"))
        from chatgpt_client import ChatGPTClient
        _chatgpt_client = ChatGPTClient()
        print(f"  ChatGPT conversation: {_chatgpt_client.conversation_id}")
    return _chatgpt_client


def call_chatgpt(prompt: str) -> str:
    """Send a message to ChatGPT (browser) and return the response."""
    return get_chatgpt_client().send_message(prompt)


# ---------------------------------------------------------------------------
# Planning — ChatGPT creates the day plan directly
# ---------------------------------------------------------------------------

GPT_VISIONARY_SYSTEM = """You are the product visionary for OpenEstates — a transparency-first property discovery platform.

Your role:
- Own the full product vision and day-by-day execution roadmap
- Create detailed, actionable day plans for your engineering partner (Claude) to implement
- Each plan should be scoped to a single focused session
- Build incrementally; reference what was accomplished previously
- Incorporate feedback and carry-over improvements from app reviews
- Be specific about deliverables, schemas, and acceptance criteria
- Review your own plan carefully before submitting — check for consistency, completeness, and achievability

IMPORTANT FORMATTING RULE:
Output your day plan as a properly formatted markdown document.
Use # for the title, ## for sections, ### for subsections.
Use bullet lists with - for lists.
Use ``` code blocks for all code samples and file structures.
Use **bold** for key emphasis.
The output will be saved directly as days/dayNN.md — it must be clean, readable markdown."""

PRODUCT_NON_NEGOTIABLES = """## NON-NEGOTIABLE PRODUCT PRINCIPLES

1. **Transparency is the core product promise.**
   Every surface must explain *why*. Users should never wonder "why am I seeing this?"

2. **The customer journey must be seamless.**
   From first search to shortlist to decision — no friction, no confusion, clear next steps.
"""


def gpt_create_plan(day_number: int, gpt_context: str,
                    previous_days: str, feedback: str,
                    smoke_results: Optional[dict] = None) -> str:
    """Ask ChatGPT to create and self-review a day plan."""
    prompt = f"""Here is the full project context:

{gpt_context}

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

{PRODUCT_NON_NEGOTIABLES}

---

Create the Day {day_number:02d} plan.

IMPORTANT:
- Review your own plan carefully before submitting
- Check that deliverables are specific and achievable in one session
- Ensure technical guidance is concrete enough for an engineer to implement without ambiguity
- If you're evolving the product direction, flag it explicitly in a ## Product Decisions section

Start the document with:
```
# days/day{day_number:02d}.md
```

Include sections: ## 1. Goal, ## 2. Product Reason, ## 3. Deliverables, ## 4. Technical Guidance, ## 5. Constraints, ## 6. Success Criteria

If any product decisions were made, add: ## 7. Product Decisions (what changed and why)

Remember: output must be properly formatted markdown."""

    return call_chatgpt(f"[ROLE CONTEXT]\n{GPT_VISIONARY_SYSTEM}\n\n[TASK]\n{prompt}")


def _plan_quality(text: str) -> str:
    """Check markdown quality of plan text. Returns 'good', 'partial', or 'bad'.

    'good'    — has headers, bullets look correct, no obvious formatting artifacts
    'partial' — has headers but has formatting issues (orphaned bullets, missing code fences)
    'bad'     — no markdown structure at all
    """
    has_headers = text.count("\n## ") >= 2 or text.count("\n# ") >= 1
    if not has_headers:
        return "bad"

    # Check for common formatting artifacts from bad HTML-to-markdown conversion:
    # 1. Orphaned bullet lines: "- \n" followed by content on next line
    orphaned_bullets = text.count("- \n")
    # 2. Bare language labels instead of code fences: "Plain text", "JSON", "TypeScript"
    #    appearing on their own line right before what should be a code block
    import re
    bare_labels = len(re.findall(r'\n(?:Plain text|JSON|TypeScript|Rust|Bash|Markdown)\n', text))
    # 3. Lines that are just "\n- \n" (empty bullets)
    empty_bullets = len(re.findall(r'\n- \n\n', text))

    artifact_count = orphaned_bullets + bare_labels + empty_bullets
    if artifact_count > 5:
        return "partial"

    return "good"


def format_plan_if_needed(day_number: int, plan: str) -> str:
    """If the plan has bad or partial markdown, use claude CLI to reformat it.

    This is a safety net — the ChatGPT client should return markdown via
    HTML-to-markdown conversion, but if it fails or produces artifacts, this
    catches it. Uses claude CLI (free via subscription, no API credits).
    """
    quality = _plan_quality(plan)
    if quality == "good":
        return plan

    print(f"      Plan markdown quality: {quality} — reformatting via claude CLI...")
    prompt = f"""The following is a day plan for OpenEstates Day {day_number:02d}.
It was extracted from ChatGPT but has formatting issues.

Clean it up into proper markdown:
- Start with: # days/day{day_number:02d}.md
- Use ## for sections, ### for subsections
- Use bullet lists with - for lists (each bullet on one line with its content)
- Use ``` code blocks for all code, file structures, and CLI examples
- Use **bold** for key emphasis
- Fix any orphaned bullet points (where "- " and the content are on separate lines)
- Fix any bare language labels (like "Plain text" or "JSON") that should be code fence languages
- Do NOT change the content, only fix formatting
- Output ONLY the reformatted markdown, no commentary

---

{plan}"""

    try:
        result = subprocess.run(
            ["claude", "--print", "--dangerously-skip-permissions", prompt],
            capture_output=True,
            text=True,
            timeout=120,
            env=_clean_env(),
        )
        if result.returncode == 0 and result.stdout.strip():
            formatted = result.stdout.strip()
            new_quality = _plan_quality(formatted)
            if new_quality != "bad":
                print(f"      Reformatted: {quality} → {new_quality} ({len(formatted)} chars)")
                return formatted
            print(f"      Reformat didn't improve quality — using original")
    except (FileNotFoundError, subprocess.TimeoutExpired) as e:
        print(f"      Could not reformat: {e}")

    return plan


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
# Implementation — Claude Opus 4.6 via CLI
# ---------------------------------------------------------------------------

def _clean_env() -> dict:
    """Return env for nested claude CLI calls.

    Removes:
    - CLAUDECODE: allows nested claude CLI invocations
    - ANTHROPIC_API_KEY: forces claude CLI to use subscription (Max/Pro)
      instead of the API (which may have no credits). The API key is only
      needed for the Anthropic SDK (used by chatgpt review steps), not for
      the claude CLI coding runs.
    """
    env = os.environ.copy()
    env.pop("CLAUDECODE", None)
    env.pop("ANTHROPIC_API_KEY", None)
    return env


def run_coding(day_number: int) -> bool:
    """Invoke Claude Opus 4.6 via the `claude` CLI to implement the day plan."""
    plan_path = DAYS_DIR / f"day{day_number:02d}.md"
    prompt = f"""Read the day plan at {plan_path} and implement it fully.

Also read CLAUDE.md and LEARNING.md for project context before starting.

Work through each deliverable. When done, write a brief summary of what was built.
If the plan includes starting a dev server, start it and leave it running.
Do NOT commit — the user will review and commit manually."""

    print(f"\n  Launching Claude Opus for Day {day_number:02d} implementation...")
    try:
        result = subprocess.run(
            ["claude", "--model", CLAUDE_OPUS, "--print", "--dangerously-skip-permissions",
             prompt],
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
# Vercel deployment
# ---------------------------------------------------------------------------

def deploy_vercel() -> Optional[str]:
    """Build and deploy frontend to Vercel production. Returns deployed URL or None."""
    frontend_dir = PROJECT_ROOT / "frontend"
    if not (frontend_dir / "package.json").exists():
        print("      Skipping — no frontend/package.json found")
        return None

    # Install deps + build
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

    # Deploy
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
            # Extract URL from output — prefer the aliased URL (publicly
            # accessible) over the deployment-specific URL (may be behind
            # Vercel SSO on team projects).
            urls = []
            for line in deploy.stdout.splitlines():
                line = line.strip()
                if line.startswith("https://") and "vercel.app" in line:
                    urls.append(line)
            # Vercel outputs: first = deployment URL, last = aliased URL.
            # The aliased URL is typically the publicly accessible one.
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


def capture_rendered_pages(vercel_url: str) -> list[dict]:
    """Use a headless browser to capture the rendered DOM text from each page.

    Delegates to the async review_capture module (Day 11 fix).
    The old sync_playwright implementation broke when called from an async context.
    """
    import asyncio
    from pathlib import Path

    try:
        from pipeline.review_capture import run_capture, CAPTURE_TARGETS
    except ImportError:
        # Fallback: try relative import
        try:
            from review_capture import run_capture, CAPTURE_TARGETS
        except ImportError:
            print("      review_capture module not found — falling back to stub")
            pages = [
                ("Landing page", vercel_url),
                ("Results page", f"{vercel_url}/results"),
                ("Shortlist page", f"{vercel_url}/shortlist"),
            ]
            return [{"name": name, "url": url, "rendered_text": "(capture unavailable)",
                     "error": "review_capture not importable"} for name, url in pages]

    output_dir = Path("pipeline/feedback/latest")
    try:
        captures = asyncio.run(run_capture(vercel_url, output_dir))
    except RuntimeError:
        # Already in an async loop — use nest_asyncio or run in thread
        import concurrent.futures
        with concurrent.futures.ThreadPoolExecutor() as pool:
            captures = pool.submit(
                asyncio.run, run_capture(vercel_url, output_dir)
            ).result()

    results = []
    for cap in captures:
        results.append({
            "name": cap.page,
            "url": cap.url,
            "title": "",
            "rendered_text": cap.rendered_text[:3000],
            "error": cap.error,
        })
        status = "OK" if cap.capture_status == "ok" else "FAIL"
        print(f"      [{status}] Captured: {cap.page} ({cap.text_length} chars)")

    return results


def gpt_customer_journey_review(day_number: int, vercel_url: str) -> dict:
    """Capture rendered page content via headless browser, then send to ChatGPT for review.

    ChatGPT can't render SPAs via its built-in browsing (it only sees raw HTML).
    So we use Playwright locally to capture the rendered DOM text, then send
    that content to ChatGPT for product review. This closes the feedback loop:
    plan → code → deploy → headless capture → ChatGPT review → next day.
    """
    print("      Capturing rendered pages with headless browser...")
    page_captures = capture_rendered_pages(vercel_url)

    # Format captures for ChatGPT
    capture_report = ""
    for cap in page_captures:
        capture_report += f"\n### {cap['name']} ({cap['url']})\n"
        if cap.get("error"):
            capture_report += f"Error: {cap['error']}\n"
        else:
            capture_report += f"Page title: {cap.get('title', 'unknown')}\n"
            capture_report += f"Rendered text content:\n```\n{cap['rendered_text']}\n```\n"

    system = """You are the product visionary for OpenEstates reviewing the deployed product.

You are receiving the ACTUAL RENDERED TEXT content from each page of the live site,
captured by a headless browser. This is what a real user would see.

Review the content and assess the customer journey.

Respond in JSON:
{
  "pages_reviewed": [
    {
      "name": "page name",
      "rendered": true/false,
      "what_user_sees": "description of visible content and layout",
      "issues": ["list of UX issues noticed from the text content"],
      "impression": "one-line product impression"
    }
  ],
  "customer_journey_works": true/false,
  "journey_blockers": ["any steps in the journey that are broken"],
  "overall_ux_impression": "2-3 sentence product review based on what was rendered",
  "improvements_for_next_day": ["specific UX improvements to carry forward"]
}"""

    prompt = f"""Day {day_number:02d} has been deployed to Vercel at {vercel_url}

A headless browser visited each page and captured the rendered text content.
Here is what each page actually shows to users:

{capture_report}

Review this as the product visionary:
1. Does the landing page content match the "premium, calm, transparent" brand?
2. Are error/empty states (expected since backend is localhost) intentional-looking?
3. Is the navigation text present and clear?
4. What specific UX improvements should the next day address?

NOTE: Data-fetching pages will show error states because the backend API is on
localhost. That is EXPECTED. Judge whether those error states feel intentional
and clean, not broken.

Respond in the JSON format specified."""

    response = call_chatgpt(f"[ROLE CONTEXT]\n{system}\n\n[TASK]\n{prompt}")

    try:
        result = json.loads(response)
    except json.JSONDecodeError:
        if "```json" in response:
            try:
                result = json.loads(response.split("```json")[1].split("```")[0].strip())
            except json.JSONDecodeError:
                result = None
        else:
            result = None

    if not result:
        result = {"overall_ux_impression": response, "improvements_for_next_day": []}

    # Attach raw captures for debugging
    result["raw_captures"] = page_captures
    return result


# ---------------------------------------------------------------------------
# Learnings & suggestions
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


def gpt_receive_feedback(day_number: int, plan: str, smoke_results: dict,
                         learnings: dict) -> dict:
    """Send implementation results back to ChatGPT for next day planning."""
    system = """You are the product visionary for OpenEstates reviewing the implementation results.

Respond in JSON:
{
  "overall_impression": "short summary of how implementation went",
  "matches_plan": true/false,
  "issues_found": ["list of specific issues"],
  "improvements": ["specific carry-over improvements for next day"],
  "suggestions_for_next_day": "what the next day should focus on and why",
  "priority_for_next_day": "single most important thing"
}"""

    prompt = f"""Day {day_number:02d} implementation is complete.

Plan summary: {plan[:800]}...

Smoke test results:
{json.dumps(smoke_results, indent=2)}

Learnings:
{json.dumps(learnings, indent=2)}

Review the results and provide structured feedback for the next day's planning."""

    response = call_chatgpt(f"[ROLE CONTEXT]\n{system}\n\n[TASK]\n{prompt}")

    try:
        return json.loads(response)
    except json.JSONDecodeError:
        if "```json" in response:
            try:
                return json.loads(response.split("```json")[1].split("```")[0].strip())
            except json.JSONDecodeError:
                pass
    return {"overall_impression": response, "improvements": [], "day": day_number}


def save_feedback(day_number: int, feedback: dict):
    FEEDBACK_DIR.mkdir(parents=True, exist_ok=True)
    feedback["day"] = day_number
    feedback["gpt_review"] = feedback.get("overall_impression", "")
    path = FEEDBACK_DIR / f"day{day_number:02d}_feedback.json"
    path.write_text(json.dumps(feedback, indent=2))
    print(f"  Feedback saved: {path}")


# ---------------------------------------------------------------------------
# Persistence
# ---------------------------------------------------------------------------

def save_plan(day_number: int, content: str) -> Path:
    DAYS_DIR.mkdir(exist_ok=True)
    path = DAYS_DIR / f"day{day_number:02d}.md"
    path.write_text(content)
    print(f"  Saved: {path}")
    return path


# ---------------------------------------------------------------------------
# Main loop — 1 day at a time
# ---------------------------------------------------------------------------

def run_day(day_number: int, plan_only: bool = False, port: int = 4000,
            resume: bool = False) -> bool:
    """Run a single day: plan → implement → test → learn → feedback."""

    cp = load_checkpoint(day_number) if resume else {}

    print(f"\n{'='*60}")
    print(f"  DAY {day_number:02d}  —  {date.today().isoformat()}")
    print(f"  Mode: {'plan-only' if plan_only else 'full (plan + code + test + feedback)'}")
    if resume and cp:
        print(f"  Resuming — completed steps: {list(cp.keys())}")
    print(f"{'='*60}\n")

    # Load context
    print("  Loading context...")
    gpt_context = load_gpt_context()
    previous_days = load_previous_days_summary()
    feedback = load_feedback_history()

    # Check for prior smoke results to send with plan request
    prev_day = day_number - 1
    prev_cp = load_checkpoint(prev_day) if prev_day > 0 else {}
    prev_smoke = prev_cp.get("smoke_results")

    # ── Step 1: Get plan from ChatGPT ──
    plan_file = DAYS_DIR / f"day{day_number:02d}.md"
    if plan_file.exists():
        print(f"\n  [1] Plan already exists at {plan_file}")
        plan = plan_file.read_text()
    elif resume and "gpt_plan" in cp:
        print(f"\n  [1] Resuming: plan from checkpoint")
        plan = cp["gpt_plan"]
        plan = format_plan_if_needed(day_number, plan)
        save_plan(day_number, plan)
    else:
        print(f"\n  [1] Asking ChatGPT for Day {day_number:02d} plan...")
        plan = gpt_create_plan(day_number, gpt_context, previous_days, feedback,
                               smoke_results=prev_smoke)
        save_checkpoint(day_number, {"gpt_plan": plan})
        plan = format_plan_if_needed(day_number, plan)
        save_plan(day_number, plan)
        print(f"      Plan received ({len(plan)} chars)")

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

    # ── Step 5: Send feedback to ChatGPT ──
    if resume and "gpt_feedback" in cp:
        print(f"\n  [5] Resuming: feedback from checkpoint")
        feedback_data = cp["gpt_feedback"]
    else:
        print(f"\n  [5] Sending results to ChatGPT for feedback...")
        feedback_data = gpt_receive_feedback(day_number, plan, smoke, learnings)
        save_feedback(day_number, feedback_data)
        save_checkpoint(day_number, {"gpt_feedback": feedback_data})

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

    # ── Step 7: ChatGPT customer journey review ──
    if vercel_url:
        if resume and "journey_review" in cp:
            print(f"\n  [7] Resuming: customer journey review from checkpoint")
            journey = cp["journey_review"]
        else:
            print(f"\n  [7] ChatGPT browsing deployed site for customer journey review...")
            journey = gpt_customer_journey_review(day_number, vercel_url)
            save_checkpoint(day_number, {"journey_review": journey})

            # Save journey review alongside other feedback
            journey_path = FEEDBACK_DIR / f"day{day_number:02d}_journey_review.json"
            FEEDBACK_DIR.mkdir(parents=True, exist_ok=True)
            journey_path.write_text(json.dumps(journey, indent=2))
            print(f"      Journey review saved: {journey_path}")

        ux_impression = journey.get("overall_ux_impression", "")
        if ux_impression:
            print(f"      UX impression: {ux_impression[:200]}")
        journey_blockers = journey.get("journey_blockers", [])
        if journey_blockers:
            print(f"      Journey blockers: {journey_blockers}")
        ux_improvements = journey.get("improvements_for_next_day", [])
        if ux_improvements:
            print("      UX improvements for next day:")
            for imp in ux_improvements:
                print(f"        - {imp}")
    else:
        print(f"\n  [7] Skipping customer journey review — no Vercel URL")

    status = "COMPLETE" if code_ok else "had issues — review manually"
    print(f"\n  Day {day_number:02d} {status}")
    print(f"{'='*60}\n")
    return code_ok


# ---------------------------------------------------------------------------
# Multi-day loop
# ---------------------------------------------------------------------------

OVERNIGHT_CONTEXT = """IMPORTANT SESSION CONTEXT:

This is an extended overnight work session. The engineering partner (Claude Opus) is
available for sustained, large-scope work across multiple days in sequence.

Guidelines for planning in this mode:
- Each day plan should still be focused and achievable in one session
- But you can be MORE AMBITIOUS with scope since implementation capacity is high
- If a feature is too large for one day, break it into day N-a and day N-b explicitly
- Prioritize work that compounds: foundation work early, polish work later
- The loop will run 5-6 days sequentially with feedback between each day
- Think of this as a sprint — by the end, the product should feel significantly more complete

Keep plans concrete and incremental. Each day builds on the last."""


def send_overnight_context():
    """Send the overnight session context to ChatGPT at the start of a multi-day run."""
    print("  Sending overnight session context to ChatGPT...")
    try:
        response = call_chatgpt(
            f"[SESSION CONTEXT]\n{OVERNIGHT_CONTEXT}\n\n"
            "Acknowledge briefly. We're about to start a multi-day sprint. "
            "I'll ask you for day plans one at a time, with implementation feedback between each."
        )
        print(f"  ChatGPT acknowledged: {response[:150]}...")
    except Exception as e:
        print(f"  WARNING: Could not send overnight context: {e}")
        print("  Continuing anyway — plans will still be generated per-day.")


def run_loop(start_day: int, num_days: int, plan_only: bool = False,
             port: int = 4000):
    """Run multiple days in sequence with ChatGPT feedback between each."""

    print(f"\n{'#'*60}")
    print(f"  OVERNIGHT SPRINT — Days {start_day:02d} to {start_day + num_days - 1:02d}")
    print(f"  {date.today().isoformat()}")
    print(f"  Mode: {'plan-only' if plan_only else 'full (plan + code + test + feedback)'}")
    print(f"{'#'*60}\n")

    # Send overnight context to ChatGPT
    send_overnight_context()

    completed = []
    failed = []

    for i in range(num_days):
        day = start_day + i

        # Check if already done
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
            # Save crash info to checkpoint
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
        description="OpenEstates Day Agent (simplified v2)",
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

Note: Firefox must be CLOSED before running.
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

    ff_profiles = Path.home() / "Library" / "Application Support" / "Firefox" / "Profiles"
    if not ff_profiles.exists():
        print("ERROR: Firefox profiles not found. Log into chatgpt.com in Firefox first.")
        sys.exit(1)

    start_day = args.day or detect_next_day_number()

    print(f"\n  OpenEstates Day Agent v2")
    print(f"  Next day: {start_day:02d}")
    print(f"  Opus {CLAUDE_OPUS} (implementation)")

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
