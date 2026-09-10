import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import type { PreparedHome, TourFrame } from "../core/types.ts";
import { HomeTour } from "../core/playback.ts";
import { buildArchitecture, vector } from "./architecture.ts";
import { INTERIOR_THEME as theme } from "./theme.ts";

export type ViewMode = "overview" | "walk" | "plan";
export interface ViewerState {
  frame: TourFrame;
  playing: boolean;
  mode: ViewMode;
}
/** The renderer owns pixels and input; HomeTour owns all movement and timing. */
export class HomeViewer {
  readonly tour: HomeTour;
  mode: ViewMode = "overview";
  dimensions = true;
  private renderer: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private camera = new THREE.PerspectiveCamera(58, 1, 0.035, 250);
  private architecture: ReturnType<typeof buildArchitecture>;
  private controls: OrbitControls;
  private measurements = new THREE.Group();
  private labels: HTMLSpanElement[] = [];
  private measurementRoom = "";
  private abort = new AbortController();
  private resize: ResizeObserver;
  private raf = 0;
  private keys = new Set<string>();
  private last = 0;
  private notifyAt = 0;
  constructor(
    private host: HTMLElement,
    home: PreparedHome,
    private changed: (state: ViewerState) => void,
  ) {
    this.tour = new HomeTour(home);
    this.tour.reducedMotion = matchMedia(
      "(prefers-reduced-motion: reduce)",
    ).matches;
    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: false });
    this.renderer.setPixelRatio(
      Math.min(devicePixelRatio, theme.maxPixelRatio),
    );
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1.1;
    host.appendChild(this.renderer.domElement);
    this.scene.background = new THREE.Color(theme.background);
    this.scene.add(new THREE.HemisphereLight("#fff9ee", "#a4b4b0", 2.2));
    const sun = new THREE.DirectionalLight("#fff4df", 3.2);
    const b = home.bounds;
    sun.position.set(b.center[0] - 8, 16, b.center[1] - 7);
    sun.target.position.copy(vector(b.center));
    sun.castShadow = true;
    Object.assign(sun.shadow.camera, {
      left: -b.span,
      right: b.span,
      top: b.span,
      bottom: -b.span,
      near: 0.5,
      far: 65,
    });
    sun.shadow.mapSize.set(2048, 2048);
    sun.shadow.normalBias = 0.025;
    this.scene.add(sun, sun.target);
    this.architecture = buildArchitecture(home.plan);
    this.scene.add(this.architecture.group, this.measurements);
    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = true;
    this.controls.minDistance = 3;
    this.controls.maxDistance = b.span * 3;
    this.controls.maxPolarAngle = Math.PI / 2.1;
    this.setMode("overview");
    const signal = this.abort.signal;
    let pointer: { id: number; x: number; y: number } | null = null;
    const canvas = this.renderer.domElement;
    canvas.addEventListener(
      "pointerdown",
      (e) => {
        if (this.mode !== "walk") return;
        host.focus();
        pointer = { id: e.pointerId, x: e.clientX, y: e.clientY };
        canvas.setPointerCapture(e.pointerId);
        this.tour.pause();
        this.emit();
      },
      { signal },
    );
    canvas.addEventListener(
      "pointermove",
      (e) => {
        if (!pointer || pointer.id !== e.pointerId) return;
        this.tour.look(
          -(e.clientX - pointer.x) * 0.004,
          -(e.clientY - pointer.y) * 0.003,
        );
        pointer.x = e.clientX;
        pointer.y = e.clientY;
      },
      { signal },
    );
    const release = () => {
      pointer = null;
    };
    canvas.addEventListener("pointerup", release, { signal });
    canvas.addEventListener("pointercancel", release, { signal });
    canvas.addEventListener("lostpointercapture", release, { signal });
    host.addEventListener(
      "keydown",
      (e) => {
        if (
          (e.target as HTMLElement).matches("input,select,button,textarea") ||
          this.mode !== "walk"
        )
          return;
        if (
          [
            "w",
            "a",
            "s",
            "d",
            "ArrowUp",
            "ArrowDown",
            "ArrowLeft",
            "ArrowRight",
          ].includes(e.key)
        ) {
          e.preventDefault();
          this.keys.add(e.key);
        }
        if (e.code === "Space") {
          e.preventDefault();
          this.toggle();
        }
      },
      { signal },
    );
    host.addEventListener("keyup", (e) => this.keys.delete(e.key), { signal });
    host.addEventListener("focusout", () => this.keys.clear(), { signal });
    window.addEventListener(
      "blur",
      () => {
        this.keys.clear();
        this.tour.pause();
        this.emit();
      },
      { signal },
    );
    document.addEventListener(
      "visibilitychange",
      () => {
        if (document.hidden) {
          this.tour.pause();
          this.keys.clear();
          this.emit();
        }
      },
      { signal },
    );
    this.resize = new ResizeObserver(() => {
      const w = host.clientWidth,
        h = host.clientHeight;
      if (!w || !h) return;
      this.camera.aspect = w / h;
      this.camera.fov = w < 600 ? theme.mobileFov : theme.eyeFov;
      this.camera.updateProjectionMatrix();
      this.renderer.setSize(w, h);
    });
    this.resize.observe(host);
    const animate = (now: number) => {
      this.raf = requestAnimationFrame(animate);
      const dt = this.last ? Math.min((now - this.last) / 1000, 0.05) : 0;
      this.last = now;
      if (this.mode === "walk") {
        const k = this.keys;
        this.tour.move(
          Number(k.has("w") || k.has("ArrowUp")) -
            Number(k.has("s") || k.has("ArrowDown")),
          Number(k.has("d") || k.has("ArrowRight")) -
            Number(k.has("a") || k.has("ArrowLeft")),
          dt,
        );
        const f = this.tour.tick(dt);
        this.camera.position.copy(vector(f.position, home.policy.eyeM));
        this.camera.lookAt(
          this.camera.position
            .clone()
            .add(
              new THREE.Vector3(
                Math.sin(f.heading) * Math.cos(f.pitch),
                Math.sin(f.pitch),
                Math.cos(f.heading) * Math.cos(f.pitch),
              ),
            ),
        );
      } else this.controls.update();
      this.drawMeasurements();
      if (this.mode !== "plan") this.renderer.render(this.scene, this.camera);
      if (now - this.notifyAt > 100) {
        this.emit();
        this.notifyAt = now;
      }
    };
    this.raf = requestAnimationFrame(animate);
  }
  private emit() {
    this.changed({
      frame: { ...this.tour.frame },
      playing: this.tour.playing,
      mode: this.mode,
    });
  }
  setMode(mode: ViewMode) {
    this.mode = mode;
    this.controls.enabled = mode === "overview";
    this.architecture.ceilings.visible = mode === "walk";
    if (mode !== "walk") this.tour.pause();
    if (mode === "overview") {
      const b = this.tour.home.bounds;
      this.controls.target.copy(vector(b.center));
      this.camera.position.set(
        b.center[0] + b.span * 0.7,
        b.span * 1.1,
        b.center[1] + b.span * 0.8,
      );
      this.controls.update();
    }
    this.emit();
  }
  toggle() {
    if (this.mode !== "walk") this.setMode("walk");
    if (this.tour.playing) this.tour.pause();
    else this.tour.play();
    this.emit();
  }
  start() {
    this.tour.reset();
    this.setMode("walk");
    this.tour.play();
    this.emit();
  }
  select(index: number) {
    this.tour.select(index);
    this.setMode("walk");
    this.emit();
  }
  step(forward: number, right: number) {
    this.setMode("walk");
    for (let i = 0; i < 8; i++) this.tour.move(forward, right, 0.05);
    this.emit();
  }
  private clearMeasurements() {
    this.measurements.children.forEach((o) => {
      const l = o as THREE.LineSegments;
      l.geometry.dispose();
      (l.material as THREE.Material).dispose();
    });
    this.measurements.clear();
    this.labels.forEach((e) => e.remove());
    this.labels = [];
  }
  private drawMeasurements() {
    const f = this.tour.frame,
      stop = this.tour.home.stops.find((s) => s.roomId === f.roomId);
    const visible =
      this.mode === "walk" &&
      this.dimensions &&
      !!stop &&
      (f.showDimensions || !this.tour.playing);
    this.measurements.visible = visible;
    if (stop && this.measurementRoom !== stop.roomId) {
      this.clearMeasurements();
      this.measurementRoom = stop.roomId;
      for (const d of stop.dimensions) {
        const a = vector(d.a, 0.035),
          b = vector(d.b, 0.035),
          n = new THREE.Vector3().subVectors(b, a).normalize();
        const cross = new THREE.Vector3(-n.z, 0, n.x).multiplyScalar(0.12);
        const points = [
          a,
          b,
          a.clone().sub(cross),
          a.clone().add(cross),
          b.clone().sub(cross),
          b.clone().add(cross),
        ];
        const line = new THREE.LineSegments(
          new THREE.BufferGeometry().setFromPoints(points),
          new THREE.LineBasicMaterial({ color: theme.measurement }),
        );
        this.measurements.add(line);
        const label = document.createElement("span");
        label.className = "he-measure";
        label.textContent = `${d.label} · ≈ ${d.metres.toFixed(2)} m`;
        this.host.appendChild(label);
        this.labels.push(label);
      }
    }
    this.labels.forEach((label, i) => {
      const d = stop?.dimensions[i];
      if (!visible || !d) {
        label.hidden = true;
        return;
      }
      const h = this.tour.frame.heading,
        pos = this.tour.frame.position;
      const ahead = (p: readonly number[]) =>
        (p[0] - pos[0]) * Math.sin(h) + (p[1] - pos[1]) * Math.cos(h);
      const end = ahead(d.a) > ahead(d.b) ? d.a : d.b;
      const p = vector(
        [pos[0] + (end[0] - pos[0]) * 0.82, pos[1] + (end[1] - pos[1]) * 0.82],
        0.18,
      ).project(this.camera);
      label.hidden =
        p.z > 1 || p.z < -1 || Math.abs(p.x) > 0.9 || Math.abs(p.y) > 0.87;
      label.style.left = `${(p.x * 0.5 + 0.5) * 100}%`;
      label.style.top = `${(-p.y * 0.5 + 0.5) * 100}%`;
    });
  }
  dispose() {
    cancelAnimationFrame(this.raf);
    this.abort.abort();
    this.resize.disconnect();
    this.controls.dispose();
    this.clearMeasurements();
    this.architecture.dispose();
    this.scene.traverse((o) => {
      if (o instanceof THREE.Light && "shadow" in o)
        (o as THREE.DirectionalLight).shadow?.dispose();
    });
    this.renderer.dispose();
    this.renderer.domElement.remove();
  }
}
