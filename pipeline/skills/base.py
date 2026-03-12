"""
BaseSkill — the skill abstraction for knowledge graph construction.

Every skill:
- Takes a typed input
- Produces SourcedFact entries with provenance
- Reports cost (LLM tokens, API calls, estimated USD)
- Is cacheable (same input + same version = skip)
- Is auditable (every fact traces to which skill produced it)
"""

import hashlib
import json
import logging
import time
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple, Type

logger = logging.getLogger(__name__)


@dataclass
class RetryPolicy:
    """Configuration for retry behavior on transient failures."""
    max_retries: int = 3
    base_delay: float = 1.0
    max_delay: float = 30.0
    backoff_factor: float = 2.0
    retryable_exceptions: Tuple[Type[BaseException], ...] = (
        ConnectionError,
        TimeoutError,
        OSError,  # covers URLError, socket errors, etc.
    )


class SkillExecutionError(Exception):
    """Wraps a skill failure after all retries are exhausted."""

    def __init__(self, skill_id: str, original_error: BaseException, attempts: int, elapsed: float):
        self.skill_id = skill_id
        self.original_error = original_error
        self.attempts = attempts
        self.elapsed = elapsed
        super().__init__(
            f"Skill '{skill_id}' failed after {attempts} attempt(s) "
            f"in {elapsed:.1f}s: {original_error}"
        )


@dataclass
class FactSource:
    """Where a fact came from — the provenance chain."""
    source_type: str  # "Reddit", "Google", "Rera", "Manual", "Llm", "Computed"
    url: Optional[str] = None
    model: Optional[str] = None
    skill_id: Optional[str] = None
    triggered_by: Optional[str] = None


@dataclass
class SourcedFact:
    """The atomic unit of knowledge. Mirrors the Rust SourcedFact type."""
    key: str
    value: dict  # {"type": "Text", "data": "..."} | {"type": "Numeric", "data": 0.5} | etc.
    confidence: float
    source: FactSource
    learned_at: str = ""  # ISO 8601
    version: int = 1
    display_template: Optional[str] = None  # e.g. "Maintenance is {value}"
    answers_preferences: Optional[List[str]] = None  # e.g. ["good society", "maintenance"]
    scoring_hint: Optional[dict] = None  # e.g. {"direction": "TextMatch", "weight": 2.0}

    def __post_init__(self):
        if not self.learned_at:
            self.learned_at = datetime.now(timezone.utc).isoformat()

    def to_dict(self) -> dict:
        d = {
            "key": self.key,
            "value": self.value,
            "confidence": self.confidence,
            "source": {
                "source_type": self.source.source_type,
                "url": self.source.url,
                "model": self.source.model,
                "skill_id": self.source.skill_id,
                "triggered_by": self.source.triggered_by,
            },
            "learned_at": self.learned_at,
            "version": self.version,
        }
        if self.display_template:
            d["display_template"] = self.display_template
        if self.answers_preferences:
            d["answers_preferences"] = self.answers_preferences
        if self.scoring_hint:
            d["scoring_hint"] = self.scoring_hint
        return d


@dataclass
class SkillCost:
    """Cost tracking for a skill execution."""
    llm_tokens: int = 0
    api_calls: int = 0
    estimated_usd: float = 0.0


@dataclass
class SkillResult:
    """What a skill execution produces."""
    facts: List[SourcedFact] = field(default_factory=list)
    edges: List[dict] = field(default_factory=list)  # New edges to add
    new_nodes: List[dict] = field(default_factory=list)  # New nodes to add
    confidence: float = 0.0
    cost: SkillCost = field(default_factory=SkillCost)
    cached: bool = False

    def to_dict(self) -> dict:
        return {
            "facts": [f.to_dict() for f in self.facts],
            "edges": self.edges,
            "new_nodes": self.new_nodes,
            "confidence": self.confidence,
            "cost": {
                "llm_tokens": self.cost.llm_tokens,
                "api_calls": self.cost.api_calls,
                "estimated_usd": self.cost.estimated_usd,
            },
            "cached": self.cached,
        }


class BaseSkill(ABC):
    """
    Abstract base for knowledge graph skills.

    Subclasses implement:
    - skill_id: unique identifier
    - description: human-readable description
    - execute(input_data) -> SkillResult
    - estimated_cost() -> SkillCost
    """

    skill_id: str = ""
    description: str = ""
    version: str = "1.0"
    output_keys: list = []  # Fact keys this skill produces (for cheap gap detection)
    retry_policy: RetryPolicy = RetryPolicy()  # Overridable by subclasses

    def __init__(self, cache_dir: Optional[Path] = None):
        self.cache_dir = cache_dir or Path("data/cache/skills")
        self.cache_dir.mkdir(parents=True, exist_ok=True)

    def _cache_key(self, input_data: dict) -> str:
        """Deterministic cache key from input + skill version."""
        raw = json.dumps(input_data, sort_keys=True) + f"|{self.skill_id}|{self.version}"
        return hashlib.sha256(raw.encode()).hexdigest()[:16]

    def _check_cache(self, input_data: dict) -> Optional[SkillResult]:
        """Check if we've already run this skill with these inputs."""
        key = self._cache_key(input_data)
        cache_file = self.cache_dir / f"{self.skill_id}_{key}.json"
        if cache_file.exists():
            try:
                data = json.loads(cache_file.read_text())
                result = self._deserialize_result(data)
                result.cached = True
                logger.debug("[%s] Cache hit for %s", self.skill_id, key)
                return result
            except Exception:
                pass
        return None

    def _write_cache(self, input_data: dict, result: SkillResult) -> None:
        """Cache a skill result."""
        key = self._cache_key(input_data)
        cache_file = self.cache_dir / f"{self.skill_id}_{key}.json"
        cache_file.write_text(json.dumps(result.to_dict(), indent=2, default=str))

    def _deserialize_result(self, data: dict) -> SkillResult:
        """Reconstruct a SkillResult from cached JSON."""
        facts = []
        for f in data.get("facts", []):
            src = f.get("source", {})
            facts.append(SourcedFact(
                key=f["key"],
                value=f["value"],
                confidence=f["confidence"],
                source=FactSource(
                    source_type=src.get("source_type", "Llm"),
                    url=src.get("url"),
                    model=src.get("model"),
                    skill_id=src.get("skill_id"),
                    triggered_by=src.get("triggered_by"),
                ),
                learned_at=f.get("learned_at", ""),
                version=f.get("version", 1),
                display_template=f.get("display_template"),
                answers_preferences=f.get("answers_preferences"),
                scoring_hint=f.get("scoring_hint"),
            ))
        return SkillResult(
            facts=facts,
            edges=data.get("edges", []),
            new_nodes=data.get("new_nodes", []),
            confidence=data.get("confidence", 0.0),
            cost=SkillCost(
                llm_tokens=data.get("cost", {}).get("llm_tokens", 0),
                api_calls=data.get("cost", {}).get("api_calls", 0),
                estimated_usd=data.get("cost", {}).get("estimated_usd", 0.0),
            ),
        )

    @abstractmethod
    def execute(self, input_data: dict) -> SkillResult:
        """Execute the skill. Override in subclasses."""
        ...

    @abstractmethod
    def estimated_cost(self) -> SkillCost:
        """Estimated cost for one execution."""
        ...

    def run(self, input_data: dict, force: bool = False, max_retries: Optional[int] = None) -> SkillResult:
        """
        Run the skill with caching and retry.

        Args:
            input_data: Skill-specific input.
            force: Skip cache check.
            max_retries: Override retry_policy.max_retries for this call.

        Returns:
            SkillResult with facts, edges, and cost.

        Raises:
            SkillExecutionError: If all retries are exhausted on a retryable error.
        """
        if not force:
            cached = self._check_cache(input_data)
            if cached:
                return cached

        policy = self.retry_policy
        attempts = max_retries if max_retries is not None else policy.max_retries
        attempts = max(attempts, 1)  # At least one attempt

        start = time.time()
        logger.info("[%s] Executing for input: %s", self.skill_id, input_data)

        last_error: Optional[BaseException] = None
        for attempt in range(1, attempts + 1):
            try:
                result = self.execute(input_data)
                result.cost.estimated_usd = max(result.cost.estimated_usd, 0.0)

                duration = time.time() - start
                if attempt > 1:
                    logger.info(
                        "[%s] Succeeded on attempt %d/%d in %.1fs",
                        self.skill_id, attempt, attempts, duration,
                    )
                logger.info(
                    "[%s] Done in %.1fs: %d facts, %d edges, $%.4f",
                    self.skill_id, duration, len(result.facts), len(result.edges),
                    result.cost.estimated_usd,
                )

                self._write_cache(input_data, result)
                return result

            except policy.retryable_exceptions as e:
                last_error = e
                elapsed = time.time() - start
                if attempt < attempts:
                    delay = min(
                        policy.base_delay * (policy.backoff_factor ** (attempt - 1)),
                        policy.max_delay,
                    )
                    logger.warning(
                        "[%s] Attempt %d/%d failed (%.1fs elapsed): %s — retrying in %.1fs",
                        self.skill_id, attempt, attempts, elapsed, e, delay,
                    )
                    time.sleep(delay)
                else:
                    logger.error(
                        "[%s] All %d attempts failed (%.1fs elapsed): %s",
                        self.skill_id, attempts, elapsed, e,
                    )
                    raise SkillExecutionError(
                        skill_id=self.skill_id,
                        original_error=e,
                        attempts=attempts,
                        elapsed=elapsed,
                    ) from e

            except Exception:
                # Non-retryable: raise immediately
                raise

        # Should not reach here, but safety net
        raise SkillExecutionError(
            skill_id=self.skill_id,
            original_error=last_error or RuntimeError("Unknown failure"),
            attempts=attempts,
            elapsed=time.time() - start,
        )
