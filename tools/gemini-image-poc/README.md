# Gemini clarified-aerial POC

This POC compares Google's highest-quality and fast native image models against the preferred OpenEstates Radical Rhapsody benchmark:

- `gemini-3-pro-image` — primary quality candidate;
- `gemini-3.1-flash-image` — faster comparison with high thinking enabled.

The generator records the complete non-secret request, prompt and reference hashes, output hashes, model response metadata and a local three-panel comparison page. It never accepts an API key on the command line and never writes the key to disk.

## Security first

The key pasted into chat must be treated as exposed. Revoke it and create a new Gemini auth key. Do not put a key in this repository, a prompt, a command-line argument, a GitHub issue, or a screenshot.

Set the replacement only in the local shell:

```bash
export GEMINI_API_KEY="REPLACEMENT_KEY"
```

## Set up

```bash
cd tools/gemini-image-poc
python3 -m venv .venv
source .venv/bin/activate
python -m pip install -r requirements.txt
```

Fetch the preferred visual benchmark (stored on the existing design branch):

```bash
curl -L \
  https://raw.githubusercontent.com/kumargu/openestates/b78d9df0d7e3b36daf48e6a0530783dfc529d221/docs/design/clarified-3d/radical-rhapsody-clarified.jpg \
  -o reference/radical-rhapsody-openai-benchmark.jpg
```

## Inspect the exact request without calling Google

```bash
python generate.py \
  --reference reference/radical-rhapsody-openai-benchmark.jpg \
  --dry-run
```

## Generate the comparison

```bash
python generate.py \
  --reference reference/radical-rhapsody-openai-benchmark.jpg \
  --benchmark reference/radical-rhapsody-openai-benchmark.jpg
```

Outputs are written under `output/radical-rhapsody/`:

- one image and JSON manifest per model;
- `run-plan.json` containing the requests with image bytes removed;
- `summary.json`;
- `compare.html` showing the original benchmark, Pro result and Flash result.

Generated imagery remains illustrative. It must not be presented as verified tower geometry or a factual site plan.

## Tests

The unit tests require only Python's standard library and do not call Google:

```bash
python -m unittest discover -s tests -v
```

## Current Google documentation

- https://ai.google.dev/gemini-api/docs/image-generation
- https://ai.google.dev/gemini-api/docs/get-started
- https://ai.google.dev/gemini-api/docs/api-key
