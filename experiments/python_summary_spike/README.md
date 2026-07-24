# Python Summary Spike

Offline/request-path-safe experiment for testing LLM polish over an
OpenEstates evidence brief. Production generation now lives in the
`generated_context_summaries` DAG source collector; this script is only for
manual provider experiments.

## DAG Run

Rerunning the DAG is sufficient when a real summary provider is configured:

```bash
OPENESTATES_SUMMARY_BASE_URL=http://127.0.0.1:11434/v1 \
OPENESTATES_SUMMARY_MODEL=llama3.2:3b \
PYTHONPATH=. cargo run --manifest-path backend/Cargo.toml --bin openestates-run-assets -- \
  --partition dt=2026-07-21 \
  --source-command python3.11 \
  --source-arg pipeline/collect_asset_sources.py
```

The collector builds evidence briefs from the current validation/DAG input
files, calls the configured provider, validates quality metadata, and emits the
`SkillFactsInput` consumed by the Rust asset executor. If no provider is
configured, the source collector returns a source failure instead of creating an
empty or skipped summary asset.

## Local llama.cpp

```bash
OPENESTATES_SUMMARY_GGUF=/path/to/model.gguf \
python3.11 experiments/python_summary_spike/run_summary_spike.py \
  --provider llama-cpp \
  --evidence experiments/python_summary_spike/waterford_evidence.json \
  --max-tokens 180
```

Install `llama-cpp-python` outside the backend first. The script never
downloads a model; it only reads the path you provide.

Manual batch output in the Rust `SkillFactsInput` shape:

```bash
OPENESTATES_SUMMARY_GGUF=/path/to/model.gguf \
python3.11 experiments/python_summary_spike/run_summary_spike.py \
  --provider llama-cpp \
  --evidence-dir experiments/python_summary_spike \
  --output-format skill-facts \
  --snapshot-date 2026-07-20
```

That output is useful for manual inspection. The DAG source collector no longer
depends on an externally supplied summary JSON file.

## OpenAI-Compatible Local/Remote Endpoint

Works with servers exposing `/v1/chat/completions`, including many local LLM
servers.

```bash
OPENESTATES_SUMMARY_BASE_URL=http://127.0.0.1:11434/v1 \
OPENESTATES_SUMMARY_MODEL=llama3.2:3b \
python3.11 experiments/python_summary_spike/run_summary_spike.py \
  --provider openai-compatible \
  --evidence experiments/python_summary_spike/waterford_evidence.json
```

Set `OPENESTATES_SUMMARY_API_KEY` only when the endpoint requires it.

## What To Look At

The output includes:

- load and generation latency
- generated summary
- expected evidence terms found or missed
- forbidden terms that leaked
- whether output length stayed inside the target range

The Rust request path only serves already-promoted text from the serving bundle.
