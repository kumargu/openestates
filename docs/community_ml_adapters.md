# Community ML Adapters

The community evidence summarizer is intentionally model-agnostic. Google
reviews, Reddit posts, and future resident sources are normalized into
`CommunityEvidenceRecord`s first; models can only improve ranking or wording on
top of that evidence.

## Current Local Embedding Layer

The production-safe default is `LocalEmbeddingCommunityThemeRanker`. It uses a
deterministic local hash embedding over review snippets and Reddit text, then
compares that evidence against the stable `community_theme_candidates()`
inventory. It can create structured `CommunityThemeHit`s, but only when actual
text or snippet tags are present. Rating/count metadata alone cannot create
themes.

This keeps review theme extraction local, reproducible, and cheap while the
external model dependencies remain unpinned.

## FastEmbed Layer

Use FastEmbed for dynamic card theme ranking. It should implement
`CommunityThemeRanker` and compare evidence snippets against the stable
`community_theme_candidates()` inventory. The output remains structured
`CommunityThemeHit`s, not free text.

Expected future enablement, once Cargo can update `backend/Cargo.lock`:

```toml
fastembed = { git = "https://github.com/Anush008/fastembed-rs.git", tag = "v5.17.3", optional = true, default-features = false, features = ["hf-hub-rustls-tls", "ort-download-binaries-rustls-tls"] }
community-fastembed = ["dep:fastembed"]
```

## rust-bert Layer

Use rust-bert for buyer-facing summary prose after facts and theme hits are
already selected. It should implement `CommunitySummaryWriter` and fall back to
`DeterministicCommunitySummaryWriter` when evidence text is missing or model
initialization fails.

Expected enablement:

```toml
rust-bert = { git = "https://github.com/guillaume-be/rust-bert.git", rev = "6db859ef097edfdda338004b4d60deebf6a3ab66", optional = true, default-features = false, features = ["remote", "rustls-tls"] }
community-rust-bert = ["dep:rust-bert"]
```

The default build must stay deterministic and must not download models.

## Cargo Resolution Note

This environment has previously failed on the sparse crates.io index while
GitHub fetches still worked. Prefer Git-sourced ML dependencies and Cargo's Git
index path when enabling these adapters:

```bash
CARGO_NET_GIT_FETCH_WITH_CLI=true \
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git \
cargo generate-lockfile --manifest-path backend/Cargo.toml
```

If the Git index fetch resets with `RPC failed`, retry on a stable network and
consider forcing HTTP/1.1 for the Git subprocess:

```bash
GIT_CONFIG_COUNT=1 \
GIT_CONFIG_KEY_0=http.version \
GIT_CONFIG_VALUE_0=HTTP/1.1 \
CARGO_NET_GIT_FETCH_WITH_CLI=true \
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git \
cargo generate-lockfile --manifest-path backend/Cargo.toml
```

Do not add `fastembed`, `rust-bert`, or their feature flags to `Cargo.toml`
unless `backend/Cargo.lock` is updated and the default offline backend build
still passes. Enable FastEmbed first; rust-bert is heavier because it pulls the
`tch`/libtorch stack. The current implementation intentionally avoids empty
placeholder features.
