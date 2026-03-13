# Skill: Run Sprint

## When to use
When the user asks to run a sprint, work on a day, or do autonomous multi-day work.

## Sprint Model

Sprint definitions are parsed from `docs/vision.md` at runtime — **do not hardcode sprint names or day ranges anywhere**. To see current sprints:

```bash
python3 pipeline/sprint_agent.py --sprint-info
```

Days 1-30 were the prototype phase. Full vision: `docs/vision.md`. Project context: `CLAUDE.md`.

## Architecture: One Conversation Per Day

**Multi-day sprints use `run-sprint.sh`** — a bash loop that launches one Claude conversation per day. Each day is isolated. Checkpoints carry state between days.

```bash
# Run a full sprint
./run-sprint.sh 1

# Run specific day range
./run-sprint.sh 31 44

# Run single day (same as Claude invoking this skill directly)
./run-sprint.sh 36 36
```

**Why one conversation per day:**
- No context bloat across days
- If a conversation dies, the next day picks up cleanly via checkpoints
- Step-level resume: if build finished but verify didn't, resume from verify
- Fire-and-forget: start the bash script and walk away

## Utilities
```bash
python3 pipeline/sprint_agent.py --status           # sprint + day progress
python3 pipeline/sprint_agent.py --sprint-info       # sprint boundaries
python3 pipeline/sprint_agent.py --next-day          # next day to work on
python3 pipeline/sprint_agent.py --resume-from 31    # which step to resume from
python3 pipeline/sprint_agent.py --mark-step 31 plan # mark step done
python3 pipeline/sprint_agent.py --mark-done 31      # mark day complete
python3 pipeline/sprint_agent.py --mark-failed 31 --reason "X broke"
python3 pipeline/sprint_agent.py --load-context 31   # full context for a day
python3 pipeline/sprint_agent.py --checkpoint 31     # view checkpoint data
python3 pipeline/sprint_agent.py --is-mid-review 38  # check if mid-sprint review day
```

## Feedback Loop Utilities
```bash
# Builder saves structured feedback (pipe JSON to stdin)
echo '{"questions":["Should X do Y?"],"tradeoffs":[{"decision":"Used Z","alternatives":"Could use W","reasoning":"Z is simpler"}],"concerns":["Data is sparse"],"data_gaps":["No price history for area X"]}' | python3 pipeline/sprint_agent.py --save-builder-feedback 31

# Verifier saves observations beyond pass/fail
echo '{"passed":true,"failures":[],"warnings":["Widget renders but looks wrong at 320px"],"suggestions":["Add integration test for claims endpoint"],"data_quality":["50% of properties have null society_quality_score"]}' | python3 pipeline/sprint_agent.py --save-verifier-obs 31

# Log a tradeoff decision
python3 pipeline/sprint_agent.py --add-tradeoff 31 --decision "Claims stored as flat JSON" --alternatives "SQLite, Postgres" --reasoning "No auth yet, flat files match existing data/ pattern, easy to migrate later"
```

## The Day Loop (what Claude does in each conversation)

When you receive a "work on day N" instruction, execute these steps in order.

### Step 0: Load Context + Determine Resume Point
```bash
python3 pipeline/sprint_agent.py --load-context N
python3 pipeline/sprint_agent.py --resume-from N
```
The context includes:
- Sprint position, yesterday's results
- **Builder feedback from recent days** — open questions, tradeoffs, concerns, data gaps
- **Verifier observations** — warnings, suggestions, data quality issues
- **Unresolved failures** (must address these first)
- **Mid-sprint review trigger** (on day 8 of 14)
- Sprint progress, standups, search logs

The resume point tells you which step to start from. If a previous run completed "plan" but not "build", you skip planning and go straight to build.

### Step 1: Plan (single agent — replaces old standup + planner)

Skip if: resume point is past "plan", OR plan file already exists and day is not first in sprint.

Spawn ONE agent that does both review + planning:
- **name**: `planner-dayNN`
- **subagent_type**: `Plan`
- **model**: `opus`
- **Prompt**:
  - Read `docs/vision.md` for sprint goals
  - Review yesterday's CODE (git diff, not just plans) — grade it briefly
  - If there are unresolved failures in context, plan to fix them FIRST
  - **If there is builder feedback (questions/tradeoffs/concerns), address each item:**
    - Answer open questions with a decision
    - Log significant tradeoffs: `python3 pipeline/sprint_agent.py --add-tradeoff N --decision "..." --alternatives "..." --reasoning "..."`
    - Acknowledge concerns — plan mitigations or explicitly accept the risk
    - Plan to fill data gaps if they block upcoming work
  - **If this is a mid-sprint review day**, include a 5-line assessment: on track? should remaining days pivot? scope cuts needed?
  - Plan day N: Goal, Product Reason, Deliverables (with file paths), Technical Guidance (reference `.claude/skills/`), Constraints, Success Criteria
  - Be specific about files to create/modify
  - Include the sprint context from Step 0

Save output to `days/dayNN.md`.
```bash
python3 pipeline/sprint_agent.py --mark-step N plan
```

### Step 2: Build

Skip if: resume point is past "build".

Spawn Agent:
- **name**: `builder-dayNN`
- **model**: `opus`
- **mode**: `auto`
- **Prompt**:
  - Read and implement the day plan at `days/dayNN.md`
  - Read `CLAUDE.md` and any referenced skill files first
  - Run `cargo check` and `npm run build` after each significant change
  - If something breaks, fix it before moving on — don't stack unverified changes
  - Don't commit to git
  - **IMPORTANT: At the end of your work, output a structured feedback section:**
    - **Questions**: Things you were unsure about and decided on your own
    - **Tradeoffs**: Decisions where you chose between alternatives (what, why, what else you considered)
    - **Concerns**: Risks or issues you noticed but didn't fix (out of scope, needs PM input)
    - **Data gaps**: Missing or sparse data that affected your implementation

After builder returns, parse its feedback and save:
```bash
python3 pipeline/sprint_agent.py --mark-step N build
python3 pipeline/sprint_agent.py --checkpoint N --set "build_summary=<one-line summary>"

# Save builder feedback (extract from builder's output)
echo '<builder feedback JSON>' | python3 pipeline/sprint_agent.py --save-builder-feedback N

# Log any significant tradeoffs
python3 pipeline/sprint_agent.py --add-tradeoff N --decision "..." --alternatives "..." --reasoning "..."
```

### Step 3: Verify

Skip if: resume point is "done".

Run build checks directly:
```bash
cd backend && cargo check 2>&1 | tail -20
cd frontend && npm run build 2>&1 | tail -20
```

Then spawn a lightweight verifier:
- **name**: `verifier-dayNN`
- **model**: `sonnet`
- **Prompt**:
  - Verify day N against success criteria in `days/dayNN.md`. Check each criterion.
  - **Beyond pass/fail, also report:**
    - **Warnings**: Things that technically pass but look wrong or fragile
    - **Suggestions**: Improvements the next day's planner should consider
    - **Data quality**: Issues with seed data, missing fields, null values that affect the feature
  - Output JSON: `{"passed": bool, "failures": [...], "warnings": [...], "suggestions": [...], "data_quality": [...], "summary": "..."}`

After verifier returns, save observations:
```bash
echo '<verifier JSON>' | python3 pipeline/sprint_agent.py --save-verifier-obs N
```

### Step 4: Mark Complete or Failed
```bash
# If verification passed:
python3 pipeline/sprint_agent.py --mark-step N verify
python3 pipeline/sprint_agent.py --mark-done N
python3 pipeline/sprint_agent.py --checkpoint N --set "day_summary=Day N: <what shipped>"

# If verification failed:
python3 pipeline/sprint_agent.py --mark-failed N --reason "<what failed and why>"
```

**On failure:** The failure reason is automatically included in the next day's context under "UNRESOLVED FAILURES". The next day's planner will see it and must address it before doing new work.

**On builder feedback:** Questions, tradeoffs, and concerns are automatically included in the next day's context under "FEEDBACK FROM PREVIOUS DAYS". The planner must address each item.

## Feedback Loop Summary

```
Day N Builder → surfaces questions, tradeoffs, concerns, data gaps
Day N Verifier → surfaces warnings, suggestions, data quality issues
         ↓ (saved to checkpoint)
Day N+1 Context Loader → includes all feedback under "FEEDBACK FROM PREVIOUS DAYS"
Day N+1 Planner → MUST address each item before planning new work
Day N+1 Planner → logs tradeoff decisions to docs/tradeoffs.md
         ↓
Day N+1 Builder → sees planner's decisions, builds accordingly
         ... cycle continues ...
```

**Mid-sprint review (day 8 of 14):** The context loader triggers a mandatory review. The planner must assess progress, check `docs/tradeoffs.md`, and decide whether to adjust remaining scope.

## Multi-day Sprint (via bash script)

For "run sprint 1" or "run days 31-44", use the bash runner:
```bash
./run-sprint.sh 1          # sprint 1 (days 31-44)
./run-sprint.sh 31 44      # explicit range
```

The bash script:
1. Loops through each day sequentially
2. Launches one `claude` CLI conversation per day
3. Each conversation reads this skill and executes the day loop
4. Checkpoints carry state between conversations (including feedback)
5. Skips days that are already done
6. Prints status after each day

## Modes
- **Full**: plan → build → verify (default, the bash script does this)
- **Plan only**: "just plan days 31-35" → only run Step 1 for each day
- **Build only**: "build day 31" → skip plan, build existing plan
- **Resume**: automatically resumes from the last incomplete step
