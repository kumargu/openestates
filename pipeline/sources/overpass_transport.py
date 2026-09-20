"""Configured HTTP transport for Overpass collectors."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any, Callable, Dict, Iterable, List, Optional
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import Request, urlopen

from pipeline.sources.request_pipeline import (
    RequestOutcome,
    RequestPipeline,
    RetryPolicy,
)


POLICY_PATH = (
    Path(__file__).resolve().parents[2]
    / "app"
    / "config"
    / "dag"
    / "source_adapters"
    / "overpass_transport.json"
)


def load_overpass_transport_policy(path: Path = POLICY_PATH) -> Dict[str, Any]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    transport = payload.get("transport")
    if not isinstance(transport, dict):
        raise ValueError("Overpass adapter requires a transport policy")
    return transport


class OverpassTransport:
    """Apply shared pacing and retry rules to Overpass requests."""

    def __init__(
        self,
        fetch: Optional[Callable[[str, str], Dict[str, Any]]] = None,
        policy: Optional[Dict[str, Any]] = None,
        **pipeline_dependencies: Any,
    ) -> None:
        config = dict(load_overpass_transport_policy())
        config.update(policy or {})
        retry_status_codes = {
            int(code) for code in config.get("retry_status_codes", [])
        }
        self.request_timeout_seconds = float(
            config.get("request_timeout_seconds", 90.0)
        )
        self.max_retry_after_seconds = float(
            config.get("max_retry_after_seconds", 120.0)
        )
        self._fetch = fetch or (
            lambda url, query: fetch_overpass_json_once(
                url, query, self.request_timeout_seconds
            )
        )
        self._pipeline = RequestPipeline(
            RetryPolicy(
                max_attempts=int(config.get("max_attempts", 3)),
                initial_backoff_seconds=float(
                    config.get("initial_backoff_seconds", 2.0)
                ),
                backoff_multiplier=float(config.get("backoff_multiplier", 2.0)),
                max_backoff_seconds=float(config.get("max_backoff_seconds", 30.0)),
                jitter_ratio=float(config.get("jitter_ratio", 0.1)),
                max_concurrency=int(config.get("max_concurrency", 1)),
                queue_capacity=int(config.get("queue_capacity", 4)),
                min_request_interval_seconds=float(
                    config.get("min_request_interval_seconds", 0.0)
                ),
            ),
            lambda error: overpass_error_is_retryable(error, retry_status_codes),
            lambda error: retry_after_seconds(
                error, self.max_retry_after_seconds
            ),
            **pipeline_dependencies,
        )

    def request(self, url: str, query: str) -> Dict[str, Any]:
        return self._pipeline.call(lambda: self._fetch(url, query))

    def map_requests(
        self, url: str, queries: Iterable[str]
    ) -> List[RequestOutcome[Dict[str, Any]]]:
        return self._pipeline.map(queries, lambda query: self._fetch(url, query))


def overpass_error_is_retryable(
    error: Exception, retry_status_codes: set[int]
) -> bool:
    if isinstance(error, HTTPError):
        return error.code in retry_status_codes
    if isinstance(error, (TimeoutError, ConnectionError, URLError)):
        return True
    return isinstance(error, OSError)


def fetch_overpass_json_once(
    url: str, query: str, timeout_seconds: float = 90.0
) -> Dict[str, Any]:
    request = Request(
        url,
        data=urlencode({"data": query}).encode("utf-8"),
        headers={
            "Content-Type": "application/x-www-form-urlencoded; charset=utf-8",
            "User-Agent": "OpenEstates DAG source collector",
        },
        method="POST",
    )
    with urlopen(request, timeout=timeout_seconds) as response:
        return json.loads(response.read().decode("utf-8"))


def retry_after_seconds(
    error: Exception, maximum_seconds: float
) -> Optional[float]:
    if not isinstance(error, HTTPError):
        return None
    header = error.headers.get("Retry-After") if error.headers else None
    try:
        value = float(header) if header else None
    except (TypeError, ValueError):
        return None
    if value is None or value < 0.0:
        return None
    return min(value, maximum_seconds)
