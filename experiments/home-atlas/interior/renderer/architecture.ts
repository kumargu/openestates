import * as THREE from "three";
import type { HomePlan, Vec2 } from "../core/types.ts";
import { distance } from "../core/geometry.ts";
import { INTERIOR_THEME as theme } from "./theme.ts";

/** Scene construction only: receives metre geometry, never parses a brochure or plans a tour. */
export function buildArchitecture(plan: HomePlan) {
  const group = new THREE.Group(),
    ceilings = new THREE.Group();
  const materials: THREE.Material[] = [],
    textures: THREE.Texture[] = [];
  const material = (color: string, roughness = 0.8) => {
    const m = new THREE.MeshStandardMaterial({ color, roughness });
    materials.push(m);
    return m;
  };
  const plaster = material(theme.plaster),
    ceiling = material(theme.ceiling),
    frame = material(theme.frame, 0.4),
    trim = material(theme.skirting);
  function box(
    parent: THREE.Object3D,
    p: [number, number, number],
    size: [number, number, number],
    mat: THREE.Material,
  ) {
    const mesh = new THREE.Mesh(new THREE.BoxGeometry(...size), mat);
    mesh.position.set(...p);
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    parent.add(mesh);
    return mesh;
  }
  // Deterministic fine-grain tile, generated locally; no external texture or random render drift.
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 256;
  const ctx = canvas.getContext("2d")!;
  ctx.fillStyle = "#f0ece4";
  ctx.fillRect(0, 0, 256, 256);
  for (let i = 0; i < 5000; i++) {
    const x = (i * 73) % 256,
      y = (i * 131 + Math.floor(i / 256) * 17) % 256;
    ctx.fillStyle = i % 2 ? "#e9e5dd" : "#f5f2ed";
    ctx.fillRect(x, y, 1, 1);
  }
  ctx.strokeStyle = "#cfcac1";
  ctx.lineWidth = 1;
  ctx.strokeRect(0.5, 0.5, 255, 255);
  const texture = new THREE.CanvasTexture(canvas);
  texture.wrapS = texture.wrapT = THREE.RepeatWrapping;
  texture.colorSpace = THREE.SRGBColorSpace;
  textures.push(texture);
  for (const room of plan.rooms) {
    const shape = new THREE.Shape();
    room.polygon.forEach(([x, z], i) =>
      i ? shape.lineTo(x, -z) : shape.moveTo(x, -z),
    );
    shape.closePath();
    const geometry = new THREE.ShapeGeometry(shape),
      uv = geometry.attributes.uv,
      pos = geometry.attributes.position;
    for (let i = 0; i < uv.count; i++)
      uv.setXY(i, pos.getX(i) / 0.65, pos.getY(i) / 0.65);
    const mat = material(
      room.kind === "balcony"
        ? theme.balconyFloor
        : room.kind === "bathroom" || room.kind === "utility"
          ? theme.wetFloor
          : theme.floor,
      0.66,
    );
    mat.map = texture;
    const floor = new THREE.Mesh(geometry, mat);
    floor.rotation.x = -Math.PI / 2;
    floor.receiveShadow = true;
    floor.userData.roomId = room.id;
    group.add(floor);
    if (room.kind !== "balcony") {
      const roof = new THREE.Mesh(geometry.clone(), ceiling);
      roof.rotation.x = Math.PI / 2;
      roof.scale.y = -1;
      roof.position.y = plan.architecture.ceilingM;
      ceilings.add(roof);
    }
  }
  for (const wall of plan.walls) {
    const length = distance(wall.a, wall.b),
      height = wall.heightM ?? plan.architecture.ceilingM;
    const section = new THREE.Group();
    section.position.set(wall.a[0], 0, wall.a[1]);
    section.rotation.y = -Math.atan2(
      wall.b[1] - wall.a[1],
      wall.b[0] - wall.a[0],
    );
    group.add(section);
    const panel = (
      a: number,
      b: number,
      lo: number,
      hi: number,
      mat: THREE.Material = plaster,
      thickness = plan.architecture.wallM,
    ) => {
      if (b - a < 0.001 || hi - lo < 0.001) return;
      box(
        section,
        [((a + b) * length) / 2, (lo + hi) / 2, 0],
        [(b - a) * length, hi - lo, thickness],
        mat,
      );
    };
    const opening = wall.opening;
    if (opening) {
      const { start: a, end: b, kind } = opening,
        lo = kind === "window" ? (opening.sillM ?? 0.95) : 0,
        hi = opening.headM ?? 2.3;
      panel(0, a, 0, height);
      panel(b, 1, 0, height);
      panel(a, b, 0, lo);
      // Full-width internal openings represent open-plan thresholds and have no lintel.
      const openPlan = a === 0 && b === 1 && opening.connects?.[1] !== null;
      if (!openPlan) {
        panel(a, b, hi, height);
        panel(a, a + 0.028 / length, lo, hi, frame, 0.19);
        panel(b - 0.028 / length, b, lo, hi, frame, 0.19);
        panel(a, b, hi - 0.03, hi, frame, 0.19);
        if (kind === "window") {
          const glass = new THREE.MeshPhysicalMaterial({
            color: "#d8eded",
            transparent: true,
            opacity: 0.12,
            roughness: 0.1,
            depthWrite: false,
            side: THREE.DoubleSide,
          });
          materials.push(glass);
          panel(a, b, lo + 0.03, hi - 0.03, glass, 0.014);
          panel(a, b, lo, lo + 0.04, frame, 0.22);
          panel(
            (a + b) / 2 - 0.012 / length,
            (a + b) / 2 + 0.012 / length,
            lo,
            hi,
            frame,
            0.08,
          );
        }
      }
      for (const [s, t] of [
        [0, a],
        [b, 1],
      ])
        panel(s, t, 0, 0.08, trim, plan.architecture.wallM + 0.015);
    } else {
      panel(0, 1, 0, height);
      panel(0, 1, 0, 0.08, trim, plan.architecture.wallM + 0.015);
    }
  }
  group.add(ceilings);
  return {
    group,
    ceilings,
    dispose: () => {
      const geometries = new Set<THREE.BufferGeometry>();
      group.traverse((o) => {
        if (o instanceof THREE.Mesh) geometries.add(o.geometry);
      });
      geometries.forEach((g) => g.dispose());
      materials.forEach((m) => m.dispose());
      textures.forEach((t) => t.dispose());
    },
  };
}

export const vector = (p: Vec2, y = 0) => new THREE.Vector3(p[0], y, p[1]);
