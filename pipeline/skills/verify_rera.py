"""
verify_rera — verify RERA registration for a property/society via Karnataka RERA.

Input: {
    "project_name": "Prestige Lakeside Habitat",
    "builder_name": "Prestige Group",  # optional, for cross-verification
    "rera_number": "PRM/KA/RERA/...",  # optional, if already known
    "area": "Whitefield",
    "city": "Bengaluru"
}
Output: SkillResult with RERA verification facts (confidence 1.0 for government-sourced data).

Strategy:
1. Try direct HTTP search on Karnataka RERA website
2. Fallback: use Claude for grounded RERA verification
3. Structure results into self-describing SourcedFacts
"""

import json
import logging
import os
import urllib.request
import urllib.parse
from typing import Optional

from pipeline.skills.base import BaseSkill, SkillResult, SkillCost, SourcedFact, FactSource

logger = logging.getLogger(__name__)

RERA_SEARCH_URL = "https://rera.karnataka.gov.in/projectViewDetails"

CLAUDE_RERA_PROMPT = """You are a real estate compliance researcher. Verify the RERA registration for this project.

**Project:** {project_name}
**Builder:** {builder_name}
**Area:** {area}, {city}
**Known RERA Number:** {rera_number}

Search your knowledge for Karnataka RERA registration details of this project.

Return a JSON object with these fields (use null if information is not available):

{{
  "registration_found": true | false,
  "rera_number": "PRM/KA/RERA/1251/..." or null,
  "status": "Registered" | "Not Found" | "Expired",
  "promoter_name": "builder/promoter name from RERA records" or null,
  "approval_date": "YYYY-MM-DD" or null,
  "completion_date": "YYYY-MM-DD" or null,
  "project_type": "Apartment" | "Villa" | "Plotted" | null,
  "total_units": number or null,
  "certificate_available": true | false,
  "source_notes": "brief note on how confident you are about this data"
}}

IMPORTANT:
- Only include information you are reasonably confident about
- If you are unsure, set registration_found to false and status to "Not Found"
- Be honest about uncertainty in source_notes
- RERA numbers for Karnataka follow patterns like PRM/KA/RERA/1251/... or ACK/KA/RERA/..."""


def _try_direct_rera_search(project_name: str, rera_number: Optional[str] = None) -> Optional[dict]:
    """
    Attempt direct HTTP search on Karnataka RERA website.

    The Karnataka RERA portal uses server-rendered pages that are hard to scrape
    without a browser. This attempts a basic search and returns None if it fails.
    """
    try:
        # Try searching by project name
        search_term = rera_number or project_name
        params = urllib.parse.urlencode({"projectName": search_term})
        url = f"https://rera.karnataka.gov.in/projectViewDetails?{params}"

        req = urllib.request.Request(
            url,
            headers={
                "User-Agent": "Mozilla/5.0 (compatible; OpenEstates/1.0)",
                "Accept": "text/html,application/json",
            },
        )

        with urllib.request.urlopen(req, timeout=15) as resp:
            body = resp.read().decode("utf-8", errors="replace")

        # The RERA portal returns HTML. Check if we got meaningful data.
        # Look for known RERA page patterns.
        if "RERA Registration" in body or "Project Details" in body:
            # Found something — but parsing the full HTML reliably needs
            # more work. For now, confirm the portal is reachable.
            logger.info("RERA portal responded with project data page")

            # Try to extract basic info from HTML if RERA number is visible
            result = {"portal_reachable": True}

            # Look for registration number pattern
            import re
            rera_pattern = re.compile(r"(PRM|ACK)/KA/RERA/\d+/\d+/\d+/\d+")
            match = rera_pattern.search(body)
            if match:
                result["rera_number"] = match.group(0)
                result["registration_found"] = True

            # Look for status
            if "Registered" in body:
                result["status"] = "Registered"
            elif "Expired" in body:
                result["status"] = "Expired"

            return result

        logger.info("RERA portal did not return expected content")
        return None

    except Exception as e:
        logger.warning("Direct RERA search failed: %s", e)
        return None


def _call_claude_rera(
    project_name: str,
    builder_name: str,
    area: str,
    city: str,
    rera_number: str,
    model: str = "claude-sonnet-4-20250514",
) -> Optional[dict]:
    """Use Claude as fallback for RERA verification."""
    api_key = os.environ.get("ANTHROPIC_API_KEY")
    if not api_key:
        logger.error("ANTHROPIC_API_KEY not set")
        return None

    prompt = CLAUDE_RERA_PROMPT.format(
        project_name=project_name,
        builder_name=builder_name or "Unknown",
        area=area,
        city=city,
        rera_number=rera_number or "Not provided",
    )

    payload = json.dumps({
        "model": model,
        "max_tokens": 1024,
        "messages": [{"role": "user", "content": prompt}],
    }).encode()

    req = urllib.request.Request(
        "https://api.anthropic.com/v1/messages",
        data=payload,
        headers={
            "Content-Type": "application/json",
            "x-api-key": api_key,
            "anthropic-version": "2023-06-01",
        },
    )

    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            data = json.loads(resp.read().decode())
    except Exception as e:
        logger.error("Claude API call failed: %s", e)
        return None

    # Extract text content
    text = ""
    for block in data.get("content", []):
        if block.get("type") == "text":
            text = block["text"]
            break

    # Parse JSON from response (handle markdown code blocks)
    text = text.strip()
    if text.startswith("```"):
        lines = text.split("\n")
        text = "\n".join(lines[1:-1] if lines[-1].strip() == "```" else lines[1:])

    try:
        result = json.loads(text)
        usage = data.get("usage", {})
        result["_tokens"] = usage.get("input_tokens", 0) + usage.get("output_tokens", 0)
        return result
    except json.JSONDecodeError:
        logger.error("Failed to parse Claude RERA response as JSON: %s", text[:200])
        return None


def _compute_verification_score(
    registration_found: bool,
    promoter_name: Optional[str],
    builder_name: Optional[str],
    certificate_available: bool,
    status: str,
    completion_date: Optional[str],
) -> int:
    """Compute 0-100 verification score based on RERA data quality."""
    score = 0

    if registration_found:
        score += 40

    # Promoter name matches builder (fuzzy)
    if promoter_name and builder_name:
        p = promoter_name.lower()
        b = builder_name.lower()
        # Check if builder name appears in promoter name or vice versa
        if b in p or p in b or any(word in p for word in b.split() if len(word) > 3):
            score += 20

    if certificate_available:
        score += 20

    if status == "Registered":
        score += 10

    if completion_date:
        score += 10

    return score


class VerifyReraSkill(BaseSkill):
    skill_id = "verify_rera"
    description = "Verify RERA registration for a property/society via Karnataka RERA"
    version = "1.0"

    def execute(self, input_data: dict) -> SkillResult:
        project_name = input_data.get("project_name", "")
        builder_name = input_data.get("builder_name", "")
        rera_number = input_data.get("rera_number", "")
        area = input_data.get("area", "")
        city = input_data.get("city", "Bengaluru")

        if not project_name:
            logger.error("verify_rera requires project_name")
            return SkillResult(confidence=0.0)

        # Track cost
        total_tokens = 0
        api_calls = 0

        # Step 1: Try direct RERA portal search
        direct_result = _try_direct_rera_search(project_name, rera_number or None)
        api_calls += 1

        rera_data = None
        source_type = "Rera"
        source_url = RERA_SEARCH_URL

        if direct_result and direct_result.get("registration_found"):
            # Direct scrape succeeded with meaningful data
            logger.info("Direct RERA search found registration for %s", project_name)
            rera_data = {
                "registration_found": True,
                "rera_number": direct_result.get("rera_number"),
                "status": direct_result.get("status", "Registered"),
                "promoter_name": None,
                "approval_date": None,
                "completion_date": None,
                "certificate_available": False,
            }
        else:
            # Step 2: Fallback to Claude
            logger.info("Direct RERA search inconclusive, falling back to Claude for %s", project_name)
            claude_result = _call_claude_rera(
                project_name, builder_name, area, city, rera_number
            )
            api_calls += 1

            if claude_result:
                total_tokens = claude_result.pop("_tokens", 0)
                rera_data = claude_result
                # Claude-sourced data gets Rera source_type but with model attribution
                source_type = "Rera"  # Still RERA domain, but model is recorded
                source_url = None

        if not rera_data:
            # Both methods failed — produce minimal "Not Found" result
            rera_data = {
                "registration_found": False,
                "rera_number": None,
                "status": "Not Found",
                "promoter_name": None,
                "approval_date": None,
                "completion_date": None,
                "certificate_available": False,
            }

        # Compute verification score
        registration_found = bool(rera_data.get("registration_found"))
        status = rera_data.get("status", "Not Found")
        promoter_name = rera_data.get("promoter_name")
        certificate_available = bool(rera_data.get("certificate_available"))
        completion_date = rera_data.get("completion_date")

        verification_score = _compute_verification_score(
            registration_found=registration_found,
            promoter_name=promoter_name,
            builder_name=builder_name,
            certificate_available=certificate_available,
            status=status,
            completion_date=completion_date,
        )

        # Determine confidence: 1.0 for direct government source, 0.7 for Claude-mediated
        confidence = 1.0 if (direct_result and direct_result.get("registration_found")) else 0.7

        # Build source
        source = FactSource(
            source_type=source_type,
            url=source_url,
            model="claude-sonnet-4-20250514" if total_tokens > 0 else None,
            skill_id=self.skill_id,
            triggered_by=input_data.get("triggered_by"),
        )

        # Step 3: Convert to self-describing SourcedFacts
        facts = []

        # --- rera_verified (Bool) ---
        facts.append(SourcedFact(
            key="rera_verified",
            value={"type": "Bool", "data": registration_found},
            confidence=confidence,
            source=source,
            display_template="RERA Verified: {value}",
            answers_preferences=["rera verified", "legally verified", "safe investment", "verified project"],
            scoring_hint={"direction": "TextMatch", "weight": 3.0},
        ))

        # --- rera_registration_number (Text) ---
        rera_num = rera_data.get("rera_number")
        if rera_num:
            facts.append(SourcedFact(
                key="rera_registration_number",
                value={"type": "Text", "data": rera_num},
                confidence=confidence,
                source=source,
                display_template="RERA No: {value}",
            ))

        # --- rera_status (Text) ---
        facts.append(SourcedFact(
            key="rera_status",
            value={"type": "Text", "data": status},
            confidence=confidence,
            source=source,
            display_template="RERA Status: {value}",
            answers_preferences=["rera verified", "safe investment"],
            scoring_hint={"direction": "TextMatch", "weight": 2.0},
        ))

        # --- rera_promoter_name (Text) ---
        if promoter_name:
            facts.append(SourcedFact(
                key="rera_promoter_name",
                value={"type": "Text", "data": promoter_name},
                confidence=confidence,
                source=source,
                display_template="RERA Promoter: {value}",
            ))

        # --- rera_approval_date (Text) ---
        approval_date = rera_data.get("approval_date")
        if approval_date:
            facts.append(SourcedFact(
                key="rera_approval_date",
                value={"type": "Text", "data": approval_date},
                confidence=confidence,
                source=source,
                display_template="RERA Approved: {value}",
            ))

        # --- rera_completion_date (Text) ---
        if completion_date:
            facts.append(SourcedFact(
                key="rera_completion_date",
                value={"type": "Text", "data": completion_date},
                confidence=confidence,
                source=source,
                display_template="Expected Completion: {value}",
            ))

        # --- rera_verification_score (Numeric) ---
        facts.append(SourcedFact(
            key="rera_verification_score",
            value={"type": "Numeric", "data": verification_score},
            confidence=confidence,
            source=source,
            display_template="Verification Score: {value}/100",
            answers_preferences=["rera verified", "legally verified", "safe investment"],
            scoring_hint={"direction": "HigherIsBetter", "weight": 3.0, "thresholds": [80.0, 50.0]},
        ))

        return SkillResult(
            facts=facts,
            confidence=confidence,
            cost=SkillCost(
                llm_tokens=total_tokens,
                api_calls=api_calls,
                estimated_usd=total_tokens * 0.000003,  # rough Sonnet pricing
            ),
        )

    def estimated_cost(self) -> SkillCost:
        return SkillCost(llm_tokens=1500, api_calls=2, estimated_usd=0.01)


if __name__ == "__main__":
    """Quick test: python3 -m pipeline.skills.verify_rera"""
    import sys
    logging.basicConfig(level=logging.INFO)

    skill = VerifyReraSkill()
    result = skill.run({
        "project_name": "Prestige Lakeside Habitat",
        "builder_name": "Prestige Group",
        "area": "Whitefield",
        "city": "Bengaluru",
        "triggered_by": "manual_test",
    })

    print(f"\n=== RERA Verification Results: {len(result.facts)} facts, confidence={result.confidence:.2f} ===")
    print(f"Cost: {result.cost.llm_tokens} tokens, {result.cost.api_calls} API calls, ${result.cost.estimated_usd:.4f}")
    print(f"Cached: {result.cached}")
    print()
    for fact in result.facts:
        template = fact.display_template or "{value}"
        display = template.replace("{value}", str(fact.value.get("data", "")))
        print(f"  [{fact.key}] {display}")
        print(f"    value={fact.value}, conf={fact.confidence}, src={fact.source.source_type}")
        if fact.answers_preferences:
            print(f"    answers: {fact.answers_preferences}")
        if fact.scoring_hint:
            print(f"    scoring: {fact.scoring_hint}")
        print()
