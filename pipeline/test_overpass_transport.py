import threading
import time
import unittest
from io import BytesIO
from urllib.error import HTTPError, URLError

from pipeline.sources.overpass_transport import OverpassTransport


class FakeClock:
    def __init__(self):
        self.now = 0.0
        self.sleeps = []

    def monotonic(self):
        return self.now

    def sleep(self, seconds):
        self.sleeps.append(seconds)
        self.now += seconds


def policy(**overrides):
    return {
        "max_attempts": 4,
        "initial_backoff_seconds": 2.0,
        "backoff_multiplier": 2.0,
        "max_backoff_seconds": 30.0,
        "jitter_ratio": 0.0,
        "max_retry_after_seconds": 120.0,
        "retry_status_codes": [429, 500, 502, 503, 504],
        "max_concurrency": 1,
        "queue_capacity": 4,
        "min_request_interval_seconds": 0.0,
        **overrides,
    }


def transport(fetch, clock, **overrides):
    return OverpassTransport(
        fetch,
        policy(**overrides),
        sleep=clock.sleep,
        monotonic=clock.monotonic,
        random_value=lambda: 0.0,
    )


class OverpassTransportTests(unittest.TestCase):
    def test_429_honors_retry_after(self):
        clock = FakeClock()
        attempts = []

        def fetch(_url, _query):
            attempts.append(True)
            if len(attempts) == 1:
                raise HTTPError(
                    "https://overpass.example/api",
                    429,
                    "Too Many Requests",
                    {"Retry-After": "7"},
                    BytesIO(),
                )
            return {"elements": []}

        result = transport(fetch, clock).request("url", "query")

        self.assertEqual(result, {"elements": []})
        self.assertEqual(len(attempts), 2)
        self.assertEqual(clock.sleeps, [7.0])

    def test_transient_failures_use_exponential_backoff(self):
        clock = FakeClock()
        failures = [
            URLError("temporary DNS failure"),
            TimeoutError("read timed out"),
            ConnectionRefusedError("connection refused"),
        ]

        def fetch(_url, _query):
            if failures:
                raise failures.pop(0)
            return {"elements": [1]}

        result = transport(fetch, clock).request("url", "query")

        self.assertEqual(result, {"elements": [1]})
        self.assertEqual(clock.sleeps, [2.0, 4.0, 8.0])

    def test_retry_exhaustion_is_bounded_and_deterministic(self):
        clock = FakeClock()
        attempts = []

        def fetch(_url, _query):
            attempts.append(True)
            raise TimeoutError("read timed out")

        with self.assertRaisesRegex(TimeoutError, "read timed out"):
            transport(fetch, clock, max_attempts=3).request("url", "query")

        self.assertEqual(len(attempts), 3)
        self.assertEqual(clock.sleeps, [2.0, 4.0])

    def test_non_retryable_4xx_fails_immediately(self):
        clock = FakeClock()
        attempts = []

        def fetch(_url, _query):
            attempts.append(True)
            raise HTTPError("url", 400, "Bad Request", {}, BytesIO())

        with self.assertRaises(HTTPError):
            transport(fetch, clock).request("url", "query")

        self.assertEqual(len(attempts), 1)
        self.assertEqual(clock.sleeps, [])

    def test_queue_never_exceeds_configured_concurrency(self):
        clock = FakeClock()
        lock = threading.Lock()
        release = threading.Event()
        active = 0
        maximum_active = 0

        def fetch(_url, query):
            nonlocal active, maximum_active
            with lock:
                active += 1
                maximum_active = max(maximum_active, active)
                if active == 2:
                    release.set()
            release.wait(timeout=1.0)
            time.sleep(0.005)
            with lock:
                active -= 1
            return {"query": query, "elements": []}

        outcomes = transport(
            fetch,
            clock,
            max_concurrency=2,
            queue_capacity=2,
        ).map_requests("url", ["a", "b", "c", "d"])

        self.assertEqual(maximum_active, 2)
        self.assertTrue(all(outcome.error is None for outcome in outcomes))
        self.assertEqual(
            [outcome.value["query"] for outcome in outcomes],
            ["a", "b", "c", "d"],
        )

    def test_minimum_interval_spaces_request_starts(self):
        clock = FakeClock()
        starts = []

        def fetch(_url, query):
            starts.append((query, clock.monotonic()))
            return {"query": query, "elements": []}

        outcomes = transport(
            fetch,
            clock,
            min_request_interval_seconds=1.5,
        ).map_requests("url", ["a", "b", "c"])

        self.assertTrue(all(outcome.error is None for outcome in outcomes))
        self.assertEqual(starts, [("a", 0.0), ("b", 1.5), ("c", 3.0)])


if __name__ == "__main__":
    unittest.main()
