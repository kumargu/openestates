"""Python boundary for the offline Three.js control-pass renderer.

Builds a render job from a structural scene, invokes the deterministic
Node/Three.js renderer in headless Chromium, and returns the written controls.
No network, provider imagery, or image model is involved.
"""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path

from .scene_models import StructuralScene


RENDERER_DIR = Path(__file__).with_name("renderer")
RENDERER_ENTRY = RENDERER_DIR / "render.mjs"
CONTROL_PASSES = ("clay", "depth", "semantic", "contour")


class RendererUnavailableError(RuntimeError):
    """The Node/Three.js renderer toolchain is not installed locally."""


class RenderError(RuntimeError):
    """The renderer ran but did not produce the expected artifacts."""


@dataclass(frozen=True)
class RenderedScene:
    output_dir: Path
    control_paths: dict[str, Path]
    semantic_colors: dict[str, tuple[int, int, int]]

    @property
    def passes(self) -> tuple[tuple[str, Path], ...]:
        return tuple((name, self.control_paths[name]) for name in CONTROL_PASSES)


def file_sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def renderer_available() -> bool:
    """Return whether Node and the installed renderer package are present."""
    return (
        shutil.which("node") is not None
        and RENDERER_ENTRY.exists()
        and (RENDERER_DIR / "node_modules/three/build/three.module.js").exists()
        and (RENDERER_DIR / "node_modules/playwright").exists()
    )


def render_scene(
    scene: StructuralScene,
    output_dir: Path,
) -> RenderedScene:
    """Render clay, depth, semantic, and contour passes deterministically."""
    if not renderer_available():
        raise RendererUnavailableError(
            "install the renderer with: cd film/illustrated/renderer && "
            "npm install && npx playwright install chromium-headless-shell"
        )
    scene.validate()
    output_dir = output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    job = {
        "scene": scene.to_payload(),
        "output_dir": str(output_dir),
        "width": scene.camera.image_width,
        "height": scene.camera.image_height,
    }
    job_path = output_dir / "render_job.json"
    job_path.write_text(json.dumps(job, indent=2, sort_keys=True) + "\n")
    completed = subprocess.run(
        ["node", str(RENDERER_ENTRY), str(job_path)],
        capture_output=True,
        text=True,
        cwd=RENDERER_DIR,
    )
    if completed.returncode != 0:
        raise RenderError(completed.stderr.strip() or "renderer failed")
    result = json.loads(completed.stdout.strip().splitlines()[-1])

    control_paths: dict[str, Path] = {}
    for name, raw_path in result["written"].items():
        control_paths[name] = Path(raw_path)
    missing = [name for name in CONTROL_PASSES if name not in control_paths]
    if missing:
        raise RenderError(f"renderer missing control passes: {missing}")

    return RenderedScene(
        output_dir=output_dir,
        control_paths=control_paths,
        semantic_colors={
            object_id: (color["r"], color["g"], color["b"])
            for object_id, color in result["semantic"].items()
        },
    )
