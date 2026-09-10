import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import type { PreparedHome, TourFrame } from "../core/types.ts";
import { HomeTour } from "../core/playback.ts";
import { buildArchitecture, vector } from "./architecture.ts";
import { INTERIOR_THEME as theme } from "./theme.ts";
import { bounds, mix } from "../core/geometry.ts";

export type ViewMode = "overview" | "walk" | "plan";
export interface ViewerState {
  frame: TourFrame;
  playing: boolean;
  mode: ViewMode;
  selectedRoomId?: string | null;
}
/** The renderer owns pixels and input; HomeTour owns all movement and timing. */
export class HomeViewer {
  readonly tour: HomeTour;
  mode: ViewMode = "overview";
  dimensions = true;
  cutaway = true;
  private selectedRoomId: string | null = null;
  private orbitGoal: { position: THREE.Vector3; target: THREE.Vector3 } | null =
    null;
  private touchMove: readonly [number, number] = [0, 0];
  private raycaster = new THREE.Raycaster();
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
    this.renderer.localClippingEnabled = true;
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
    this.controls.addEventListener("start", () => {
      this.orbitGoal = null;
    });
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
        this.touchMove = [0, 0];
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
          this.touchMove = [0, 0];
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
      if (this.mode === "overview") this.frameOverview();
    });
    this.resize.observe(host);
    const animate = (now: number) => {
      this.raf = requestAnimationFrame(animate);
      const dt = this.last ? Math.min((now - this.last) / 1000, 0.05) : 0;
      this.last = now;
      if (this.mode === "walk") {
        const k = this.keys;
        this.tour.move(
          this.touchMove[0] +
            Number(k.has("w") || k.has("ArrowUp")) -
            Number(k.has("s") || k.has("ArrowDown")),
          this.touchMove[1] +
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
      } else {
        if (this.orbitGoal && this.mode === "overview") {
          const a = this.tour.reducedMotion ? 1 : 1 - Math.exp(-dt * 4);
          this.camera.position.lerp(this.orbitGoal.position, a);
          this.controls.target.lerp(this.orbitGoal.target, a);
          if (this.camera.position.distanceTo(this.orbitGoal.position) < 0.01)
            this.orbitGoal = null;
        }
        this.controls.update();
      }
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
      selectedRoomId: this.selectedRoomId,
    });
  }
  setMode(mode: ViewMode) {
    const previous = this.mode;
    this.mode = mode;
    this.controls.enabled = mode === "overview";
    this.architecture.ceilings.visible = mode === "walk";
    this.architecture.setCutaway(mode === "overview" && this.cutaway);
    this.architecture.selectRoom(
      mode === "overview" ? this.selectedRoomId : null,
    );
    if (mode === "walk") {
      this.camera.fov =
        this.host.clientWidth < 600 ? theme.mobileFov : theme.eyeFov;
      this.camera.updateProjectionMatrix();
    }
    this.touchMove = [0, 0];
    this.keys.clear();
    if (mode !== "walk") this.tour.pause();
    if (mode === "overview") this.frameOverview(previous !== "overview");
    if (previous !== mode && !this.tour.reducedMotion)
      this.host.animate([{ opacity: 0 }, { opacity: 1 }], {
        duration: 450,
        easing: "ease-out",
      });
    this.emit();
  }
  private frameOverview(immediate = false) {
    const room = this.tour.home.plan.rooms.find(
      (r) => r.id === this.selectedRoomId,
    );
    const b = room ? bounds(room.polygon) : this.tour.home.bounds;
    const target = vector(b.center, 0.35),
      direction = new THREE.Vector3(0.55, 1.35, 0.75).normalize();
    const vertical = (58 * Math.PI) / 180,
      horizontal = 2 * Math.atan(Math.tan(vertical / 2) * this.camera.aspect);
    const distance = Math.max(
      4,
      (b.span * 0.8) / Math.sin(Math.min(vertical, horizontal) / 2),
    );
    this.camera.fov = 58;
    this.camera.far = Math.max(250, distance * 3);
    this.camera.updateProjectionMatrix();
    this.orbitGoal = {
      target,
      position: target.clone().addScaledVector(direction, distance),
    };
    if (immediate || this.camera.position.length() < 0.1) {
      this.camera.position.copy(this.orbitGoal.position);
      this.controls.target.copy(target);
      this.orbitGoal = null;
      this.controls.update();
    }
  }
  setCutaway(enabled: boolean) {
    this.cutaway = enabled;
    this.architecture.setCutaway(this.mode === "overview" && enabled);
  }
  resetOverview() {
    this.selectedRoomId = null;
    this.setMode("overview");
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
    if (this.mode !== "walk") {
      this.selectedRoomId = this.tour.home.stops[index]?.roomId ?? null;
      this.setMode("overview");
      return;
    }
    this.tour.select(index);
    this.setMode("walk");
    this.emit();
  }
  move(forward: number, right: number) {
    this.touchMove = [forward, right];
    if (forward || right) this.tour.pause();
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
      // Anchor to the actual tape, never interpolate from the camera onto a different line.
      const anchors = [0.5, 0.25, 0.75, 0.12, 0.88].map((t) =>
        vector(mix(d.a, d.b, t), 0.12),
      );
      const anchor = anchors.find((a) => {
        const clip = a.clone().project(this.camera);
        if (
          clip.z > 1 ||
          clip.z < -1 ||
          Math.abs(clip.x) > 0.8 ||
          Math.abs(clip.y) > 0.75
        )
          return false;
        const dir = a.clone().sub(this.camera.position),
          length = dir.length();
        this.raycaster.set(this.camera.position, dir.normalize());
        this.raycaster.far = length - 0.04;
        return !this.raycaster.intersectObject(this.architecture.group, true)
          .length;
      });
      label.hidden = !anchor;
      if (!anchor) return;
      const p = anchor.clone().project(this.camera);
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
