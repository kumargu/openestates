"""Exercise the current journey and snapshot-pinned property contracts over HTTP."""
import json
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid

BASE = f"http://127.0.0.1:{int(sys.argv[1]) if len(sys.argv) > 1 else 4000}"


def request(path, body=None, expected=200):
    data = None if body is None else json.dumps(body).encode()
    for attempt in range(5):
        req = urllib.request.Request(BASE + path, data=data, headers={"Content-Type": "application/json"})
        try:
            response = urllib.request.urlopen(req, timeout=30)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            status = response.status
            payload = response.read()
            if status == 429 and attempt < 4:
                time.sleep(min(float(response.headers.get("Retry-After", "1")), 10))
                continue
        assert status == expected, (path, status, payload[:400])
        return json.loads(payload) if payload else None
    raise AssertionError(f"rate limit prevented {path}")


def current(journey):
    assert journey["contractVersion"] == 1
    results = journey["active"]["results"]
    assert results["kind"] == "current"
    cards = [card for group in results["resultSets"] for card in group["results"]]
    # Each result may occur in distinct branches; the authoritative ID list deduplicates it.
    assert list(dict.fromkeys(card["id"] for card in cards)) == results["orderedResultIds"]
    return results, cards


assert request("/api/health")["status"] == "ok"
catalog = request("/api/properties")
assert catalog, "a promoted catalog must contain browseable societies"
# Browse once; all subsequent home hydration is bounded and snapshot-pinned.
query = catalog[0]["society_name"]
journey = request("/api/search?" + urllib.parse.urlencode({"q": query}))
results, cards = current(journey)
assert cards, "catalog society must remain discoverable"
snapshot = journey["runtimeVersion"]["snapshotIdentity"]
selected = results["orderedResultIds"][:2]
pinned = urllib.parse.urlencode({"snapshotIdentity": snapshot})
for property_id in selected:
    path = "/api/properties/" + urllib.parse.quote(property_id, safe="")
    detail = request(path + "?" + pinned)
    assert detail["snapshot_identity"] == snapshot
    assert detail["property"]["id"] == property_id
    context = request(path + "/context?" + pinned)
    assert context["snapshotIdentity"] == snapshot
    assert context == detail["context"]
    request(path + "?snapshotIdentity=retired-smoke-snapshot", expected=409)

batch = request("/api/properties/batch", {"propertyIds": selected, "snapshotIdentity": snapshot})
assert batch["snapshotIdentity"] == snapshot
assert [card["id"] for card in batch["items"]] == selected
receipt_count = 0
for card in cards[:5]:
    for reason in card["reasons"]:
        proof = request("/api/search/proofs/resolve", {"proofToken": reason["proofToken"], "propertyId": card["id"]})
        assert proof["resolutionStatus"] == "resolved"
        assert proof["snapshotIdentity"] == snapshot
        assert proof["sourceObservations"] or proof.get("catalogEvidence")
        receipt_count += 1
assert receipt_count, "named society must retain its identity receipt"
token = journey["active"]["revision"]["stateToken"]
resumed = request("/api/search/resume", {"parentToken": token, "knownResultIds": results["orderedResultIds"]})
assert resumed["active"]["results"]["orderedResultIds"] == results["orderedResultIds"]
revision = request("/api/search/revisions", {"parentToken": token, "parentResultIds": results["orderedResultIds"],
    "utterance": "3 BHK", "clientMutationId": "smoke-" + uuid.uuid4().hex})
assert revision["contractVersion"] == 1
assert revision["runtimeVersion"]["snapshotIdentity"] == snapshot
request("/api/properties/nonexistent-smoke-home", expected=404)
print(f"API smoke passed: {len(selected)} bounded homes, {receipt_count} resolved receipts, resume, revision and stale-snapshot rejection.")
