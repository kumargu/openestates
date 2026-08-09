# Serving bundle release

The serving bundle is the runtime commit point. A release is usable only when
its Parquet/Tantivy artifacts, pinned lineage, and every local media reference
are available together.

## One-command promotion

Promote an existing candidate:

```bash
cd backend
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo run \
  --bin openestates-promote-materialization -- \
  --asset search_serving_bundle \
  --materialization <materialization-uuid>
```

The command performs the full preflight, writes
`frontend/media-manifest.json`, promotes the pinned lineage, and changes the
serving pointer last. Any failed gate exits non-zero before the pointer change.
The manifest is a frontend deployment certificate: its `assets` inventory must
remain empty because validated `/media/*` objects are served from the lake and
are deliberately not packaged by Vite.

Audit the current release without changing state:

```bash
cd backend
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo run \
  --bin openestates-promote-materialization -- \
  --asset search_serving_bundle --current --check-only
```

Normal production DAG runs use the same pre-promotion validator. Catalog
release validate/promote/rollback commands also run it.

## Release gates

- materialization succeeded and pinned lineage is coherent
- manifest version matches the materialization version
- all artifacts remain under the immutable bundle version prefix
- every artifact size and SHA-256 matches the manifest
- required Parquet, schema, trust-policy, and Tantivy artifacts exist
- manifest row counts match decoded tables
- every local URL nested anywhere in serving facts resolves
- gallery media bytes match their recorded `content_sha256`
- the frontend deployment manifest is generated only from a passing candidate
- every frontend production build rejects frontend-packaged media
- image URLs use immutable content hashes and are streamed from the configured lake

## Media storage

Project images live at:

```text
media/images/sha256/{first-two-hash-characters}/{sha256}.{canonical-extension}
```

The same logical key is used under local `data/lake` and the configured S3
prefix. Original imports are retained once and indexed by
`media/inventory/*.json`; serving facts point to bounded delivery copies.
`/media/*` responses stream from `LakeStore` and content-addressed images use a
one-year immutable browser/CDN cache. `/societies/*` is retired and fails both
release promotion and frontend builds.

The media materializer verifies magic bytes, canonicalizes extensions, rejects
declared-hash mismatches, deduplicates identical content, and carries forward
existing lake-backed observations across weekly snapshots. Collector downloads
are temporary `data/cache/media_ingest` inputs only. A cache reset cannot remove
an already-promoted gallery.

The first migration archived 723 society originals and 4 launch images. The
active delivery set is 383 unique bounded browser-safe JPEG/WebP images (45.9 MiB, 363 KiB
maximum), down from 152.2 MiB in the frontend bundle.
