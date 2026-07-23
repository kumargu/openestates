# Whitefield Sequential DAG Seed List

Date: 2026-07-23

Purpose: first dense Whitefield search bundle trial. This file is a run plan, not product truth. Properties only become runtime-searchable after the asset DAG resolves them through RERA/canonical society nodes, source collectors, promoted `kg_society_view`, and the serving bundle.

## Scope Rules

- Run sequentially, one society at a time, so source failures and fact gaps are easy to attribute.
- Use `dt=2026-07-23` for the trial partition.
- Prefer canonical RERA society selectors when available.
- Keep locality classification provisional until source facts confirm it.
- Keep request-time search local: no runtime fallback to seed/list files.
- Preserve Google nearby-place metro evidence, but do not use the removed OSM metro proximity path.

## Core Whitefield / Hoodi / EPIP

These are the first pass for the dense bundle.

| Order | Society | Locality tag | Notes |
| --- | --- | --- | --- |
| 1 | Godrej United | whitefield_core | Keep high priority; known Whitefield/Hoodi candidate. |
| 2 | Godrej Air | whitefield_core | Keep high priority; known Whitefield/Hoodi candidate. |
| 3 | Godrej Splendour | whitefield_core | Already used as the first fresh DAG trial society. |
| 4 | Prestige Shantiniketan | whitefield_core | User-mentioned anchor; spelling variants matter. |
| 5 | Prestige Waterford | whitefield_core | Existing useful test bed; verify current locality facts. |
| 6 | Brigade Lakefront | whitefield_core | Strong Whitefield/EPIP candidate. |
| 7 | Assetz Marq 3.0 | whitefield_core | Verify phase naming and RERA mapping. |
| 8 | Vaswani Exquisite | whitefield_core | Verify exact society naming. |
| 9 | Prestige Lakeside Habitat | whitefield_adjacent | Treat as Varthur/Whitefield-access until facts confirm. |
| 10 | Total Environment Pursuit of a Radical Rhapsody | whitefield_adjacent | Long-name matching needs alias care. |
| 11 | Sumadhura Capitol Residences | whitefield_core | Verify exact RERA/project naming. |
| 12 | Sobha Windsor | whitefield_access_belt | Verify Whitefield-access vs location label. |
| 13 | Provident Capella | whitefield_access_belt | Verify Whitefield-access vs location label. |
| 14 | Vaswani Starlight | whitefield_core | Verify exact project naming. |
| 15 | Alembic Cloud Forest | whitefield_core | Verify exact project naming. |

## Whitefield Access Belt

Include these in the trial, but tag them as adjacent until RERA, Google, and listing facts settle the locality.

| Order | Society | Locality tag | Notes |
| --- | --- | --- | --- |
| 16 | Brigade Cornerstone Utopia | whitefield_access_belt | Varthur/Gunjur belt candidate. |
| 17 | SBR One Residence | whitefield_access_belt | Verify exact location and RERA identity. |
| 18 | Candeur Signature | whitefield_access_belt | Verify exact location and RERA identity. |
| 19 | Candeur Landmark | whitefield_access_belt | Verify exact location and RERA identity. |
| 20 | Godrej Woodscapes | whitefield_access_belt | Budigere Cross/Whitefield-access candidate. |

## Sequential DAG Command Shape

Current local readiness: the promoted canonical society snapshot is `2026-07-23-godrej-splendour` and currently resolves `Godrej Splendour` as `society:rera-e33c7fdcc0d15688`. The rest of this list should first go through canonical selector discovery/bootstrap before using `--source-entity`.

After the canonical society snapshot contains each selector, run one society at a time:

```bash
cd /home/gulshan.kumar/openestates/backend
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo run --bin openestates-run-assets -- \
  --partition dt=2026-07-23 \
  --source-command python3 \
  --source-arg -m \
  --source-arg pipeline.collect_asset_sources \
  --source-entity <canonical-society-selector>
```

For dry-run readiness:

```bash
cd /home/gulshan.kumar/openestates/backend
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git cargo run --bin openestates-run-assets -- \
  --partition dt=2026-07-23 \
  --dry-run
```

## Acceptance Checks

- DAG plan has no generated prose summary assets.
- DAG plan has no Prestige inventory or OSM metro proximity assets.
- Each run produces or preserves durable facts only: RERA, Google reviews, Google nearby places, listings/pricing, images, builder RERA aggregates, and home-state signals.
- `kg_society_view` fans in only current support partitions plus required RERA/canonical facts.
- Serving bundle promotes cleanly after each successful run.
- Search can recall the society by name, locality, and common BHK query after promotion.
