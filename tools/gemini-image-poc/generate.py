#!/usr/bin/env python3
"""Generate auditable Radical Rhapsody comparison images with Gemini.

The API key is read by the Google GenAI SDK from GEMINI_API_KEY. It is never
accepted as a command-line argument or written to a manifest.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import html
import json
import mimetypes
import os
from pathlib import Path
import shutil
import sys
from typing import Any


ROOT = Path(__file__).resolve().parent
DEFAULT_PROMPT = ROOT / "prompts" / "radical-rhapsody.txt"
DEFAULT_MODELS = ("gemini-3-pro-image", "gemini-3.1-flash-image")
DEFAULT_RESPONSE_FORMAT = {
    "type": "image",
    "aspect_ratio": "16:9",
    "image_size": "2K",
}


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def detect_mime(path: Path) -> str:
    guessed, _ = mimetypes.guess_type(path.name)
    if guessed not in {"image/png", "image/jpeg", "image/webp"}:
        raise ValueError(f"Unsupported reference image type: {path}")
    return guessed


def build_request(model: str, prompt: str, reference: Path | None) -> dict[str, Any]:
    if reference is None:
        input_value: Any = prompt
    else:
        image_bytes = reference.read_bytes()
        input_value = [
            {"type": "text", "text": prompt},
            {
                "type": "image",
                "data": base64.b64encode(image_bytes).decode("ascii"),
                "mime_type": detect_mime(reference),
            },
        ]

    request: dict[str, Any] = {
        "model": model,
        "input": input_value,
        "response_format": dict(DEFAULT_RESPONSE_FORMAT),
    }
    if model == "gemini-3.1-flash-image":
        request["generation_config"] = {"thinking_level": "high"}
    return request


def public_request(request: dict[str, Any], reference: Path | None) -> dict[str, Any]:
    """Return an auditable request description without embedding image bytes."""
    visible = {key: value for key, value in request.items() if key != "input"}
    if reference is None:
        visible["input"] = {"type": "text", "text": request["input"]}
    else:
        visible["input"] = [
            request["input"][0],
            {
                "type": "image",
                "path": str(reference),
                "mime_type": detect_mime(reference),
                "sha256": sha256_file(reference),
                "bytes": reference.stat().st_size,
            },
        ]
    return visible


def safe_error(exc: Exception) -> str:
    message = f"{exc.__class__.__name__}: {exc}"
    secret = os.environ.get("GEMINI_API_KEY")
    return message.replace(secret, "[REDACTED]") if secret else message


def extension_for(mime_type: str | None) -> str:
    return {
        "image/jpeg": ".jpg",
        "image/webp": ".webp",
    }.get(mime_type or "", ".png")


def response_metadata(interaction: Any) -> dict[str, Any]:
    usage = getattr(interaction, "usage", None)
    if usage is not None and hasattr(usage, "model_dump"):
        usage = usage.model_dump(mode="json")
    elif usage is not None and not isinstance(usage, (dict, str, int, float, bool, list)):
        usage = str(usage)
    return {
        "interaction_id": getattr(interaction, "id", None),
        "status": getattr(interaction, "status", None),
        "response_model": getattr(interaction, "model", None),
        "usage": usage,
    }


def write_compare_html(
    output_dir: Path,
    benchmark: Path | None,
    outputs: list[tuple[str, Path]],
) -> Path:
    cards: list[str] = []
    if benchmark is not None:
        benchmark_copy = output_dir / ("benchmark" + benchmark.suffix.lower())
        if benchmark.resolve() != benchmark_copy.resolve():
            shutil.copy2(benchmark, benchmark_copy)
        cards.append(
            '<article><h2>Original benchmark</h2>'
            f'<img src="{html.escape(benchmark_copy.name)}" alt="Original benchmark"></article>'
        )

    for model, path in outputs:
        cards.append(
            f'<article><h2>{html.escape(model)}</h2>'
            f'<img src="{html.escape(path.name)}" alt="{html.escape(model)} output"></article>'
        )

    page = """<!doctype html>
<html lang="en">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Radical Rhapsody image comparison</title>
<style>
  :root { color-scheme: dark; font-family: Inter, ui-sans-serif, system-ui, sans-serif; }
  body { margin: 0; padding: 28px; background: #111512; color: #edf1ec; }
  header { max-width: 980px; margin: 0 auto 24px; }
  h1 { margin: 0 0 8px; font-size: 24px; font-weight: 600; }
  p { margin: 0; color: #aeb7ae; }
  main { display: grid; gap: 22px; }
  article { background: #1a201c; border: 1px solid #303a32; border-radius: 16px; overflow: hidden; }
  h2 { margin: 0; padding: 13px 16px; font-size: 14px; font-weight: 550; }
  img { display: block; width: 100%; height: auto; background: #0b0d0c; }
</style>
<header><h1>Radical Rhapsody clarified-aerial benchmark</h1>
<p>Compare composition, tower separation, urban context, labels and product UI—not factual geometry.</p></header>
<main>""" + "\n".join(cards) + "</main></html>"

    destination = output_dir / "compare.html"
    destination.write_text(page, encoding="utf-8")
    return destination


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prompt-file", type=Path, default=DEFAULT_PROMPT)
    parser.add_argument("--reference", type=Path)
    parser.add_argument("--benchmark", type=Path)
    parser.add_argument("--models", nargs="+", default=list(DEFAULT_MODELS))
    parser.add_argument("--out-dir", type=Path, default=ROOT / "output" / "radical-rhapsody")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--overwrite", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    prompt_path = args.prompt_file.resolve()
    if not prompt_path.is_file():
        raise SystemExit(f"Prompt file not found: {prompt_path}")

    reference = args.reference.resolve() if args.reference else None
    benchmark = args.benchmark.resolve() if args.benchmark else reference
    for candidate, label in ((reference, "Reference"), (benchmark, "Benchmark")):
        if candidate is not None and not candidate.is_file():
            raise SystemExit(f"{label} image not found: {candidate}")

    prompt = prompt_path.read_text(encoding="utf-8").strip()
    requests = [build_request(model, prompt, reference) for model in args.models]
    plan = {
        "prompt_file": str(prompt_path),
        "prompt_sha256": sha256_file(prompt_path),
        "requests": [public_request(request, reference) for request in requests],
    }

    if args.dry_run:
        print(json.dumps(plan, indent=2, ensure_ascii=False))
        return 0

    if not os.environ.get("GEMINI_API_KEY"):
        raise SystemExit(
            "GEMINI_API_KEY is not set. Revoke the exposed key, create a replacement, "
            "and set it only in the local environment."
        )

    try:
        from google import genai
    except ImportError as exc:
        raise SystemExit(
            "google-genai is not installed. Run: python -m pip install -r requirements.txt"
        ) from exc

    args.out_dir.mkdir(parents=True, exist_ok=True)
    (args.out_dir / "run-plan.json").write_text(
        json.dumps(plan, indent=2, ensure_ascii=False), encoding="utf-8"
    )

    client = genai.Client()
    completed: list[tuple[str, Path]] = []
    failures: list[dict[str, str]] = []

    for request in requests:
        model = request["model"]
        destination_base = args.out_dir / model
        existing = list(args.out_dir.glob(model + ".*"))
        if existing and not args.overwrite:
            failures.append({"model": model, "error": "output exists; use --overwrite"})
            continue
        try:
            interaction = client.interactions.create(**request)
            generated = interaction.output_image
            if generated is None or not generated.data:
                raise RuntimeError("Gemini returned no output image")
            image_bytes = base64.b64decode(generated.data)
            extension = extension_for(getattr(generated, "mime_type", None))
            destination = destination_base.with_suffix(extension)
            destination.write_bytes(image_bytes)

            manifest = {
                **public_request(request, reference),
                **response_metadata(interaction),
                "output": {
                    "path": str(destination),
                    "bytes": len(image_bytes),
                    "sha256": sha256_bytes(image_bytes),
                    "mime_type": getattr(generated, "mime_type", None),
                },
            }
            destination_base.with_suffix(".json").write_text(
                json.dumps(manifest, indent=2, ensure_ascii=False), encoding="utf-8"
            )
            completed.append((model, destination))
        except Exception as exc:  # API errors differ across SDK versions.
            failures.append({"model": model, "error": safe_error(exc)})

    comparison = write_compare_html(args.out_dir, benchmark, completed)
    summary = {
        "completed": [{"model": model, "path": str(path)} for model, path in completed],
        "failed": failures,
        "comparison": str(comparison),
    }
    (args.out_dir / "summary.json").write_text(
        json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8"
    )
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())

