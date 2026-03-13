#!/usr/bin/env bash
#
# run-sprint.sh — Fire-and-forget sprint runner.
# One Claude conversation per day. Checkpoints carry state between days.
#
# Usage:
#   ./run-sprint.sh 1          # Run sprint 1 (days 31-44)
#   ./run-sprint.sh 2          # Run sprint 2 (days 45-58)
#   ./run-sprint.sh 31 44      # Run specific day range
#   ./run-sprint.sh 36 36      # Run single day
#   ./run-sprint.sh 36 44 --plan-only   # Only plan, don't build
#
set -euo pipefail

# Allow launching claude CLI even if we're inside a Claude Code session
unset CLAUDECODE 2>/dev/null || true

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SPRINT_AGENT="python3 ${SCRIPT_DIR}/pipeline/sprint_agent.py"
LOG_DIR="${SCRIPT_DIR}/pipeline/checkpoints"

# ---------------------------------------------------------------------------
# Rate limit / retry config
# ---------------------------------------------------------------------------

MAX_RETRIES=10                # max retries per day before giving up
DEFAULT_WAIT=3600             # 1 hour — fallback if we can't parse reset time
MAX_WAIT=7200                 # 2 hours — cap on wait time
# Extended pattern: includes "hit your limit" which is Claude CLI's rate limit message
RATE_LIMIT_PATTERNS="rate.limit\|too many requests\|429\|quota\|limit exceeded\|capacity\|overloaded\|hit your limit"

# ---------------------------------------------------------------------------
# Parse arguments
# ---------------------------------------------------------------------------

PLAN_ONLY=false
BUILD_ONLY=false

# Check for flags
POSITIONAL=()
for arg in "$@"; do
    case $arg in
        --plan-only) PLAN_ONLY=true ;;
        --build-only) BUILD_ONLY=true ;;
        *) POSITIONAL+=("$arg") ;;
    esac
done
set -- ${POSITIONAL[@]+"${POSITIONAL[@]}"}

if [ $# -eq 0 ]; then
    echo "Usage: ./run-sprint.sh <sprint_number|start_day> [end_day] [--plan-only|--build-only]"
    echo ""
    $SPRINT_AGENT --status
    exit 0
fi

# If single arg looks like a sprint number (small), query sprint_agent for day range
SPRINT_DAYS=$($SPRINT_AGENT --sprint-info "$1" 2>/dev/null | grep "Days:" | head -1 | sed 's/.*Days:[[:space:]]*\([0-9]*\)-\([0-9]*\).*/\1 \2/')
if [ $# -eq 1 ] && [ -n "$SPRINT_DAYS" ]; then
    SPRINT_ID=$1
    START_DAY=$(echo "$SPRINT_DAYS" | awk '{print $1}')
    END_DAY=$(echo "$SPRINT_DAYS" | awk '{print $2}')
elif [ $# -eq 1 ]; then
    START_DAY=$1
    END_DAY=$1
else
    START_DAY=$1
    END_DAY=$2
fi

echo "============================================="
echo "  OpenEstates Sprint Runner"
echo "  Days ${START_DAY} → ${END_DAY}"
if $PLAN_ONLY; then echo "  Mode: PLAN ONLY"; fi
if $BUILD_ONLY; then echo "  Mode: BUILD ONLY"; fi
echo "============================================="
echo ""

# ---------------------------------------------------------------------------
# Build the Claude prompt based on mode
# ---------------------------------------------------------------------------

build_prompt() {
    local day=$1
    local mode=""

    if $PLAN_ONLY; then
        mode="Plan only — do Step 1 (plan) only, skip build and verify."
    elif $BUILD_ONLY; then
        mode="Build only — skip planning, build the existing plan at days/day${day}.md."
    else
        mode="Full day — plan, build, and verify. Resume from wherever the checkpoint says."
    fi

    cat <<PROMPT
You are working on OpenEstates day ${day}.

Read .claude/skills/run-sprint.md for the full procedure, then execute the day loop for day ${day}.

Mode: ${mode}

Start by loading context:
  python3 pipeline/sprint_agent.py --load-context ${day}
  python3 pipeline/sprint_agent.py --resume-from ${day}

Then follow the steps in the skill doc. Work autonomously — don't ask for confirmation, just execute.
PROMPT
}

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

is_rate_limited() {
    local log_file=$1
    # Check last 50 lines of log for rate limit signals
    if tail -50 "$log_file" 2>/dev/null | grep -qi "$RATE_LIMIT_PATTERNS"; then
        return 0  # yes, rate limited
    fi
    return 1  # no
}

compute_wait_seconds() {
    # Parse "resets HH:MMam/pm (timezone)" from log and compute seconds until then.
    # Returns the number of seconds to wait, or $DEFAULT_WAIT if parsing fails.
    local log_file=$1

    # Extract reset time like "12:30pm" from "resets 12:30pm (Asia/Calcutta)"
    local reset_str
    reset_str=$(tail -10 "$log_file" 2>/dev/null | grep -oi "resets [0-9]\{1,2\}:[0-9]\{2\}[ap]m" | head -1 | sed 's/resets //i')

    if [ -z "$reset_str" ]; then
        echo "$DEFAULT_WAIT"
        return
    fi

    # Parse hour, minute, ampm
    local hour minute ampm
    hour=$(echo "$reset_str" | sed 's/\([0-9]*\):\([0-9]*\)\([ap]m\)/\1/')
    minute=$(echo "$reset_str" | sed 's/\([0-9]*\):\([0-9]*\)\([ap]m\)/\2/')
    ampm=$(echo "$reset_str" | sed 's/\([0-9]*\):\([0-9]*\)\([ap]m\)/\3/')

    # Convert to 24h
    if [ "$ampm" = "pm" ] && [ "$hour" -ne 12 ]; then
        hour=$((hour + 12))
    elif [ "$ampm" = "am" ] && [ "$hour" -eq 12 ]; then
        hour=0
    fi

    # Get current time in seconds since midnight
    local now_h now_m now_secs target_secs wait_secs
    now_h=$(date "+%H" | sed 's/^0//')
    now_m=$(date "+%M" | sed 's/^0//')
    now_secs=$(( now_h * 3600 + now_m * 60 ))
    target_secs=$(( hour * 3600 + minute * 60 ))

    wait_secs=$(( target_secs - now_secs ))

    # If target is in the past (already reset), try again in 5 minutes
    if [ "$wait_secs" -le 0 ]; then
        wait_secs=300
    fi

    # Add 2 minute buffer after reset time
    wait_secs=$((wait_secs + 120))

    # Cap at MAX_WAIT
    if [ "$wait_secs" -gt "$MAX_WAIT" ]; then
        wait_secs=$MAX_WAIT
    fi

    echo "$wait_secs"
}

human_duration() {
    local seconds=$1
    if [ "$seconds" -ge 3600 ]; then
        echo "$((seconds / 3600))h $((seconds % 3600 / 60))m"
    elif [ "$seconds" -ge 60 ]; then
        echo "$((seconds / 60))m $((seconds % 60))s"
    else
        echo "${seconds}s"
    fi
}

wait_with_countdown() {
    local total=$1
    local reason=$2
    local end_time=$((SECONDS + total))
    local wake_time=$(date -v+${total}S "+%H:%M" 2>/dev/null || date -d "+${total} seconds" "+%H:%M" 2>/dev/null || echo "?")

    echo "  ⏳ ${reason}"
    echo "  Waiting $(human_duration $total) (until ~${wake_time})..."

    while [ $SECONDS -lt $end_time ]; do
        local remaining=$((end_time - SECONDS))
        printf "\r  ⏳ Resuming in $(human_duration $remaining)...   "
        sleep 30
    done
    printf "\r  ⏳ Wait complete. Resuming...                 \n"
}

# ---------------------------------------------------------------------------
# Main loop — one conversation per day, with retry on rate limit
# ---------------------------------------------------------------------------

DAYS_DONE=0
DAYS_FAILED=0
DAYS_SKIPPED=0

for DAY in $(seq "$START_DAY" "$END_DAY"); do
    echo "--- Day ${DAY} ---"

    # Check if already done
    RESUME_STEP=$($SPRINT_AGENT --resume-from "$DAY")
    if [ "$RESUME_STEP" = "done" ]; then
        echo "  Already done. Skipping."
        DAYS_SKIPPED=$((DAYS_SKIPPED + 1))
        continue
    fi

    ATTEMPT=0
    WAIT_TIME=$DEFAULT_WAIT
    DAY_COMPLETE=false
    LOG_FILE="${LOG_DIR}/day${DAY}_log.txt"

    while [ $ATTEMPT -lt $MAX_RETRIES ]; do
        ATTEMPT=$((ATTEMPT + 1))
        RESUME_STEP=$($SPRINT_AGENT --resume-from "$DAY")

        if [ "$RESUME_STEP" = "done" ]; then
            DAY_COMPLETE=true
            break
        fi

        if [ $ATTEMPT -gt 1 ]; then
            echo "  Retry #$((ATTEMPT - 1)) — resuming from step: ${RESUME_STEP}"
        else
            echo "  Starting from step: ${RESUME_STEP}"
        fi

        PROMPT=$(build_prompt "$DAY")

        echo "  Launching Claude conversation... (attempt ${ATTEMPT}/${MAX_RETRIES})"
        EXIT_CODE=0
        claude --print --dangerously-skip-permissions -p "$PROMPT" >> "$LOG_FILE" 2>&1 || EXIT_CODE=$?

        # Check if day completed despite exit code
        RESULT=$($SPRINT_AGENT --resume-from "$DAY")
        if [ "$RESULT" = "done" ]; then
            DAY_COMPLETE=true
            break
        fi

        # Check for rate limit FIRST — Claude CLI may exit 0 on rate limit!
        # The "hit your limit" message comes with exit code 0, so we must
        # check the log content regardless of exit code.
        if is_rate_limited "$LOG_FILE"; then
            WAIT_TIME=$(compute_wait_seconds "$LOG_FILE")
            echo "  ⚠ Rate limited (exit code: ${EXIT_CODE})"

            if [ $ATTEMPT -lt $MAX_RETRIES ]; then
                wait_with_countdown "$WAIT_TIME" "Rate limited. Will retry day ${DAY} from step: ${RESULT}"
            fi
        elif [ $EXIT_CODE -ne 0 ]; then
            # Non-rate-limit error — don't retry, move on
            echo "  ✗ Claude exited with error (code: ${EXIT_CODE}), not a rate limit."
            break
        else
            # Exit code 0 but day not done — Claude finished but didn't complete all steps.
            # This can happen if context ran out. Retry once to pick up from checkpoint.
            if [ $ATTEMPT -ge 3 ]; then
                echo "  ✗ Day incomplete after ${ATTEMPT} attempts. Moving on."
                break
            fi
            echo "  ◐ Conversation ended but day not complete. Retrying from: ${RESULT}"
        fi
    done

    if $DAY_COMPLETE; then
        echo "  ✓ Day ${DAY} complete."
        DAYS_DONE=$((DAYS_DONE + 1))
        # Reset wait time for next day
        WAIT_TIME=$DEFAULT_WAIT
    else
        echo "  ✗ Day ${DAY} incomplete (stopped at: $($SPRINT_AGENT --resume-from "$DAY"))"
        DAYS_FAILED=$((DAYS_FAILED + 1))
    fi

    echo ""
done

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

echo "============================================="
echo "  Sprint Run Complete"
echo "  Done: ${DAYS_DONE} | Failed: ${DAYS_FAILED} | Skipped: ${DAYS_SKIPPED}"
echo "============================================="
echo ""
$SPRINT_AGENT --status
