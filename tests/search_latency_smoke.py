#!/usr/bin/env python3
import json
import os
import statistics
import sys
import time
import urllib.parse
import urllib.request


BASE_URL = os.environ.get("OPENESTATES_API_BASE", "http://127.0.0.1:4000").rstrip("/")
REPEATS = int(os.environ.get("SEARCH_LATENCY_REPEATS", "6"))

QUERIES = [
    ("waterford", 175),
    ("wateford", 175),
    ("3BHK Whitefield under 2Cr", 200),
    ("family friendly 3BHK", 250),
    ("metro", 350),
    ("near metro low traffic", 350),
]


def fetch_search(query):
    url = "{}/api/search?q={}".format(BASE_URL, urllib.parse.quote(query))
    started = time.perf_counter()
    with urllib.request.urlopen(url, timeout=15) as response:
        body = response.read()
        status = response.status
    elapsed_ms = (time.perf_counter() - started) * 1000
    payload = json.loads(body.decode("utf-8"))
    return {
        "status": status,
        "elapsed_ms": elapsed_ms,
        "bytes": len(body),
        "results": len(payload.get("results") or []),
    }


def main():
    failures = []
    for query, threshold_ms in QUERIES:
        samples = [fetch_search(query) for _ in range(REPEATS)]
        warm_samples = samples[1:] if len(samples) > 1 else samples
        warm_times = [sample["elapsed_ms"] for sample in warm_samples]
        median_ms = statistics.median(warm_times)
        max_ms = max(warm_times)
        latest = samples[-1]
        ok = latest["status"] == 200 and median_ms <= threshold_ms
        marker = "PASS" if ok else "FAIL"
        print(
            "{} search latency query={!r} median_ms={:.1f} max_ms={:.1f} "
            "threshold_ms={} results={} bytes={}".format(
                marker,
                query,
                median_ms,
                max_ms,
                threshold_ms,
                latest["results"],
                latest["bytes"],
            )
        )
        if not ok:
            failures.append(
                "{} median {:.1f}ms exceeded {}ms".format(
                    query, median_ms, threshold_ms
                )
            )

    if failures:
        print("\nLatency smoke failed:")
        for failure in failures:
            print("- {}".format(failure))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
