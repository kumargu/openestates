# Lake Storage

OpenEstates uses one logical object layout for local development and S3. Data
artifacts remain Parquet. Small JSON objects are limited to manifests, current
pointers, schemas, and policies.

`OPENESTATES_LAKE_URL` selects the physical store:

```text
unset                                      <project>/data/lake
file:///var/lib/openestates/lake           /var/lib/openestates/lake
s3://property-data/openestates/prod/lake   S3 bucket plus object prefix
```

An S3 production build enables the backend feature:

```bash
cargo build --release --features s3
```

The S3 client reads the standard `AWS_*` environment variables for region,
credentials, role or web-identity credentials, and optional endpoint settings.
Use `AWS_ENDPOINT_URL_S3` for an S3-specific custom endpoint, or `AWS_ENDPOINT`
for the generic endpoint supported by the object-store client.
`OPENESTATES_LAKE_URL` must not contain credentials, query parameters, or a
fragment.

Prefixes are a deployment concern, not part of durable metadata. For example,
the logical key:

```text
gold/view=kg_society/version=2026-07-14/part-00000.parquet
```

is stored at:

```text
<project>/data/lake/gold/view=kg_society/version=2026-07-14/part-00000.parquet
s3://property-data/openestates/prod/lake/gold/view=kg_society/version=2026-07-14/part-00000.parquet
```

Manifests contain the logical key only. Moving a snapshot between local storage
and S3 therefore requires no path rewrite. Current-pointer promotion is
conditional: S3 uses object ETags, while the local adapter serializes updates
per logical key because the upstream local object-store backend does not
support conditional overwrite.
