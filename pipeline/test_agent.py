"""
Quick smoke test for agent.py core logic (no browser, no AI calls needed).
Tests checkpoint detection, day detection, and context loading.

Usage:
  python3 pipeline/test_agent.py
"""

import json
import sys
from pathlib import Path

# Setup path
sys.path.insert(0, str(Path(__file__).resolve().parent))

from agent import (
    load_checkpoint,
    detect_next_day_number,
    load_gpt_context,
    load_previous_days_summary,
    load_feedback_history,
    smoke_test,
    print_status,
    DAYS_DIR,
    CHECKPOINTS_DIR,
)


def test_checkpoint_loading():
    """Test that checkpoints load correctly."""
    print("  [1] Checkpoint loading...")

    # Day 06 should be done
    cp6 = load_checkpoint(6)
    assert cp6.get("code_success") is True, f"Day 06 should be done, got: {cp6}"
    print(f"      Day 06: DONE ✓")

    # Day 07 should be done
    cp7 = load_checkpoint(7)
    assert cp7.get("code_success") is True, f"Day 07 should be done, got: {cp7}"
    print(f"      Day 07: DONE ✓")

    # Day 08 should have gpt_plan but not be implemented
    cp8 = load_checkpoint(8)
    assert "gpt_plan" in cp8, f"Day 08 should have gpt_plan, got keys: {list(cp8.keys())}"
    print(f"      Day 08: plan exists ✓")

    print("      PASS")


def test_day_detection():
    """Test that next day detection picks the right day."""
    print("  [2] Day detection...")

    next_day = detect_next_day_number()
    # Day 07 has a plan file but isn't implemented → should be next
    print(f"      Next day detected: {next_day}")

    # Day 08 has a plan but isn't implemented → should be next
    cp8 = load_checkpoint(8)
    day08_plan = DAYS_DIR / "day08.md"
    if day08_plan.exists() and not cp8.get("code_success"):
        assert next_day == 8, f"Expected day 8 (plan exists, not implemented), got {next_day}"
        print(f"      Correctly detected Day 08 (plan ready, needs implementation) ✓")
    else:
        print(f"      Next day is {next_day} ✓")

    print("      PASS")


def test_context_loading():
    """Test that context files load without errors."""
    print("  [3] Context loading...")

    gpt_ctx = load_gpt_context()
    assert len(gpt_ctx) > 100, f"GPT context too short: {len(gpt_ctx)} chars"
    print(f"      GPT context: {len(gpt_ctx)} chars ✓")

    prev_days = load_previous_days_summary()
    assert "day" in prev_days.lower(), f"Previous days should mention 'day'"
    print(f"      Previous days: {len(prev_days)} chars ✓")

    feedback = load_feedback_history()
    print(f"      Feedback history: {len(feedback)} chars ✓")

    print("      PASS")


def test_smoke_test_offline():
    """Test smoke test against a port that's probably not running."""
    print("  [4] Smoke test (offline)...")

    results = smoke_test(port=49999)  # unlikely to be running
    assert isinstance(results, dict), "Smoke test should return dict"
    # All should be errors since nothing is on port 49999
    for ep, r in results.items():
        assert r.get("status") == "error", f"{ep} should error on unused port"
    print(f"      All {len(results)} endpoints correctly errored on unused port ✓")

    print("      PASS")


def test_new_conversation_flow():
    """Test that the agent can work with existing conversation (no new chat needed).
    This just verifies the code paths exist — no actual browser call."""
    print("  [5] Code path check...")

    # Verify ChatGPT client can be imported
    from chatgpt_client import ChatGPTClient
    print(f"      ChatGPTClient importable ✓")

    # Verify plan function signature
    from agent import gpt_create_plan
    import inspect
    sig = inspect.signature(gpt_create_plan)
    params = list(sig.parameters.keys())
    assert "smoke_results" in params, f"gpt_create_plan should accept smoke_results, got {params}"
    print(f"      gpt_create_plan accepts smoke_results ✓")

    # Verify feedback function signature
    from agent import gpt_receive_feedback
    sig = inspect.signature(gpt_receive_feedback)
    params = list(sig.parameters.keys())
    assert "learnings" in params, f"gpt_receive_feedback should accept learnings, got {params}"
    print(f"      gpt_receive_feedback accepts learnings ✓")

    print("      PASS")


def main():
    print("\n  Agent v2 — Core Logic Tests")
    print("  " + "="*40 + "\n")

    tests = [
        test_checkpoint_loading,
        test_day_detection,
        test_context_loading,
        test_smoke_test_offline,
        test_new_conversation_flow,
    ]

    passed = 0
    failed = 0
    for test in tests:
        try:
            test()
            passed += 1
        except Exception as e:
            print(f"      FAIL: {e}")
            failed += 1

    print(f"\n  {'='*40}")
    print(f"  Results: {passed} passed, {failed} failed")
    if failed == 0:
        print("  All tests passed!")
    print(f"  {'='*40}\n")

    # Also show current status
    print("  Current day status:")
    print_status()

    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
