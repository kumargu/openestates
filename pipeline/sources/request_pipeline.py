"""Small bounded work queue for retryable source requests."""

from __future__ import annotations

import queue
import random
import threading
import time
from dataclasses import dataclass
from typing import Callable, Generic, Iterable, List, Optional, Tuple, TypeVar, cast


InputT = TypeVar("InputT")
OutputT = TypeVar("OutputT")


@dataclass(frozen=True)
class RetryPolicy:
    max_attempts: int = 3
    initial_backoff_seconds: float = 2.0
    backoff_multiplier: float = 2.0
    max_backoff_seconds: float = 30.0
    jitter_ratio: float = 0.1
    max_concurrency: int = 1
    queue_capacity: int = 4
    min_request_interval_seconds: float = 0.0

    def __post_init__(self) -> None:
        if self.max_attempts < 1:
            raise ValueError("max_attempts must be at least one")
        if self.initial_backoff_seconds < 0.0:
            raise ValueError("initial_backoff_seconds cannot be negative")
        if self.backoff_multiplier < 1.0:
            raise ValueError("backoff_multiplier must be at least one")
        if self.max_backoff_seconds < 0.0:
            raise ValueError("max_backoff_seconds cannot be negative")
        if not 0.0 <= self.jitter_ratio <= 1.0:
            raise ValueError("jitter_ratio must be between zero and one")
        if self.max_concurrency < 1:
            raise ValueError("max_concurrency must be at least one")
        if self.queue_capacity < 1:
            raise ValueError("queue_capacity must be at least one")
        if self.min_request_interval_seconds < 0.0:
            raise ValueError("min_request_interval_seconds cannot be negative")


@dataclass(frozen=True)
class RequestOutcome(Generic[OutputT]):
    value: Optional[OutputT] = None
    error: Optional[Exception] = None


class RequestPipeline:
    """Run retryable calls through one paced, bounded in-process queue."""

    def __init__(
        self,
        policy: RetryPolicy,
        is_retryable: Callable[[Exception], bool],
        retry_after_seconds: Callable[
            [Exception], Optional[float]
        ] = lambda _error: None,
        *,
        sleep: Callable[[float], None] = time.sleep,
        monotonic: Callable[[], float] = time.monotonic,
        random_value: Callable[[], float] = random.random,
    ) -> None:
        self.policy = policy
        self._is_retryable = is_retryable
        self._retry_after_seconds = retry_after_seconds
        self._sleep = sleep
        self._monotonic = monotonic
        self._random_value = random_value
        self._concurrency = threading.BoundedSemaphore(policy.max_concurrency)
        self._spacing_lock = threading.Lock()
        self._next_request_at = 0.0

    def call(self, operation: Callable[[], OutputT]) -> OutputT:
        for attempt in range(1, self.policy.max_attempts + 1):
            try:
                with self._concurrency:
                    self._wait_for_request_slot()
                    return operation()
            except Exception as error:
                if attempt >= self.policy.max_attempts or not self._is_retryable(error):
                    raise
                self._sleep(self._retry_delay(error, attempt))
        raise RuntimeError("request pipeline exhausted without a result")

    def map(
        self,
        items: Iterable[InputT],
        operation: Callable[[InputT], OutputT],
    ) -> List[RequestOutcome[OutputT]]:
        pending = list(items)
        if not pending:
            return []
        work: queue.Queue[object] = queue.Queue(maxsize=self.policy.queue_capacity)
        stop = object()
        outcomes: List[Optional[RequestOutcome[OutputT]]] = [None] * len(pending)

        def worker() -> None:
            while True:
                task = work.get()
                try:
                    if task is stop:
                        return
                    index, item = cast(Tuple[int, InputT], task)
                    try:
                        outcomes[index] = RequestOutcome(
                            value=self.call(lambda: operation(item))
                        )
                    except Exception as error:
                        outcomes[index] = RequestOutcome(error=error)
                finally:
                    work.task_done()

        worker_count = min(self.policy.max_concurrency, len(pending))
        workers = [
            threading.Thread(target=worker, daemon=True)
            for _ in range(worker_count)
        ]
        for thread in workers:
            thread.start()
        for indexed_item in enumerate(pending):
            work.put(indexed_item)
        for _ in workers:
            work.put(stop)
        work.join()
        for thread in workers:
            thread.join()
        return [
            outcome
            if outcome is not None
            else RequestOutcome(error=RuntimeError("request worker produced no outcome"))
            for outcome in outcomes
        ]

    def _wait_for_request_slot(self) -> None:
        interval = self.policy.min_request_interval_seconds
        if interval <= 0.0:
            return
        with self._spacing_lock:
            now = self._monotonic()
            wait_seconds = max(0.0, self._next_request_at - now)
            if wait_seconds:
                self._sleep(wait_seconds)
            started_at = self._monotonic()
            self._next_request_at = max(started_at, self._next_request_at) + interval

    def _retry_delay(self, error: Exception, attempt: int) -> float:
        exponential = min(
            self.policy.max_backoff_seconds,
            self.policy.initial_backoff_seconds
            * (self.policy.backoff_multiplier ** (attempt - 1)),
        )
        jittered = min(
            self.policy.max_backoff_seconds,
            exponential * (1.0 + self.policy.jitter_ratio * self._random_value()),
        )
        retry_after = self._retry_after_seconds(error)
        return max(jittered, retry_after or 0.0)
