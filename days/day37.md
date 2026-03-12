# Day 37: Pipeline Hardening — Retry, Checkpoint/Resume, Error Reporting

## Goal
Make skill execution pipeline resilient to transient failures and resumable across interrupted batch runs.

## Product Reason
Every future sprint depends on reliable enrichment. A single 429 or timeout currently kills a batch. Hardening now means every future enrichment sprint is faster, cheaper, and predictable.

## Deliverables

### 1. Retry with exponential backoff in BaseSkill.run() (`pipeline/skills/base.py`)
- `RetryPolicy` dataclass: max_retries=3, base_delay=1.0, max_delay=30.0, backoff_factor=2.0
- Wrap `execute()` in retry loop with logging
- `SkillExecutionError` wraps final failure with attempt count
- Skills can override `retry_policy` class attribute

### 2. Batch runner (`pipeline/skills/batch_runner.py`)
- New module: skill name + input list → sequential execution with checkpointing
- Checkpoint: `data/cache/skills/batch_{skill}_{hash}.json` with completed/failed/timestamps
- Resume: skip completed items, retry failed
- CLI: `python3 -m pipeline.skills.batch_runner learn_society --all-societies`

### 3. Structured error reporting
- After batch: write summary to `data/cache/skills/batch_report_{skill}_{timestamp}.json`
- Print human-readable summary to stdout

### 4. Extend run_skill.py CLI
- `--retries N` flag
- `--batch` mode with `--all-societies` / `--all-properties` convenience flags
- Backward compatible single-item invocation

## Constraints
- No new pip dependencies (stdlib only)
- Retry in BaseSkill, not per-skill
- Checkpoints under data/cache/skills/
- Don't break existing skill main blocks

## Success Criteria
1. BaseSkill.run() retries transient failures up to 3 times with backoff
2. Interrupted batch resumes from checkpoint
3. Batch produces structured JSON report
4. Existing run_skill.py single-item invocations unchanged
5. Zero new dependencies
