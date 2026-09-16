import {
  MathUtils,
  PerspectiveCamera,
  Scene,
  SRGBColorSpace,
  WebGLRenderer,
} from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { DRACOLoader } from "three/addons/loaders/DRACOLoader.js";
import { TilesRenderer } from "3d-tiles-renderer";
import {
  GLTFExtensionsPlugin,
  GoogleCloudAuthPlugin,
  ReorientationPlugin,
  TileCompressionPlugin,
  TilesFadePlugin,
} from "3d-tiles-renderer/plugins";

const config = window.__WATERFORD_LAB_CONFIG__ || {};
const apiKey = String(config.apiKey || "").trim();

const root = document.getElementById("scene-root");
const status = document.getElementById("scene-status");
const detail = document.getElementById("scene-detail");
const attribution = document.getElementById("scene-attribution");
const hint = document.getElementById("scene-hint");

if (!root || !status || !detail || !attribution || !hint) {
  throw new Error("Waterford cinematic lab shell is incomplete.");
}

if (!apiKey) {
  status.textContent = "Google Maps key missing";
  detail.textContent = "Configure VITE_GOOGLE_MAPS_API_KEY for this preview.";
  hint.hidden = true;
  throw new Error("VITE_GOOGLE_MAPS_API_KEY is required for the Waterford cinematic lab.");
}

const WATERFORD = {
  // OSM/Mapcarta society centroid. Keep this input isolated so we can refine it
  // without changing renderer behavior.
  lat: Number(config.lat ?? 12.98142),
  lon: Number(config.lon ?? 77.74156),
  // Approximate local ellipsoid height. The reorientation plugin turns the global
  // Google tileset into a compact local Three.js scene around this point.
  height: Number(config.height ?? 850),
};

const prefersReducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
const isCoarsePointer = window.matchMedia("(pointer: coarse)").matches;
const maxDpr = Number(config.maxDpr ?? (isCoarsePointer ? 1.35 : 1.65));
const pixelRatio = Math.min(window.devicePixelRatio || 1, maxDpr);

const scene = new Scene();
const renderer = new WebGLRenderer({
  antialias: true,
  alpha: false,
  powerPreference: "high-performance",
});
renderer.outputColorSpace = SRGBColorSpace;
renderer.setClearColor(0x101518, 1);
renderer.setPixelRatio(pixelRatio);
root.appendChild(renderer.domElement);

// A slightly tighter lens than the NASA reference keeps Waterford visually dominant
// and produces more of an architectural-visualization feel than an infinite globe.
const camera = new PerspectiveCamera(50, 1, 20, 1_600_000);
camera.position.set(720, 430, -900);

const controls = new OrbitControls(camera, renderer.domElement);
controls.target.set(0, 48, 0);
controls.enableDamping = true;
controls.dampingFactor = 0.055;
controls.enablePan = false;
controls.enableZoom = true;
controls.zoomSpeed = 0.65;
controls.rotateSpeed = 0.42;
controls.minDistance = 240;
controls.maxDistance = 1_850;
controls.minPolarAngle = MathUtils.degToRad(30);
controls.maxPolarAngle = MathUtils.degToRad(72);
controls.autoRotate = !prefersReducedMotion;
controls.autoRotateSpeed = 0.32;

const tiles = new TilesRenderer();
tiles.registerPlugin(new GoogleCloudAuthPlugin({
  apiToken: apiKey,
  autoRefreshToken: true,
  useRecommendedSettings: true,
}));
tiles.registerPlugin(new TileCompressionPlugin());
tiles.registerPlugin(new TilesFadePlugin());

const dracoLoader = new DRACOLoader();
dracoLoader.setDecoderPath("https://www.gstatic.com/draco/versioned/decoders/1.5.7/");
tiles.registerPlugin(new GLTFExtensionsPlugin({ dracoLoader }));

tiles.registerPlugin(new ReorientationPlugin({
  lat: WATERFORD.lat * MathUtils.DEG2RAD,
  lon: WATERFORD.lon * MathUtils.DEG2RAD,
  height: WATERFORD.height,
}));

scene.add(tiles.group);
tiles.setCamera(camera);

// Lower values ask the renderer for sharper tiles. During active movement we allow
// slightly coarser LOD, then quickly sharpen once the user settles.
const DETAIL_MOVING = isCoarsePointer ? 20 : 17;
const DETAIL_SETTLED = isCoarsePointer ? 15 : 11;
let qualityTimer = 0;
let idleResumeTimer = 0;
let firstUsefulFrame = false;
let lastAttribution = "";
let lastAttributionUpdate = 0;
let startedAt = performance.now();

function setMovingQuality() {
  window.clearTimeout(qualityTimer);
  tiles.errorTarget = DETAIL_MOVING;
}

function setSettledQuality() {
  window.clearTimeout(qualityTimer);
  qualityTimer = window.setTimeout(() => {
    tiles.errorTarget = DETAIL_SETTLED;
  }, 180);
}

function scheduleAutoMotionResume() {
  window.clearTimeout(idleResumeTimer);
  if (prefersReducedMotion) return;
  idleResumeTimer = window.setTimeout(() => {
    controls.autoRotate = true;
    tiles.errorTarget = isCoarsePointer ? 18 : 14;
  }, 4200);
}

controls.addEventListener("start", () => {
  controls.autoRotate = false;
  setMovingQuality();
  window.clearTimeout(idleResumeTimer);
  hint.classList.add("is-quiet");
});

controls.addEventListener("end", () => {
  setSettledQuality();
  scheduleAutoMotionResume();
});

function resize() {
  const width = Math.max(1, root.clientWidth);
  const height = Math.max(1, root.clientHeight);
  camera.aspect = width / height;
  camera.updateProjectionMatrix();
  renderer.setSize(width, height, false);
  tiles.setResolutionFromRenderer(camera, renderer);
}

const resizeObserver = new ResizeObserver(resize);
resizeObserver.observe(root);
resize();

function updateAttribution(now) {
  if (now - lastAttributionUpdate < 500) return;
  lastAttributionUpdate = now;

  const values = tiles
    .getAttributions()
    .filter((entry) => entry && entry.type === "string" && entry.value)
    .map((entry) => String(entry.value).trim())
    .filter(Boolean);
  const next = values.join(" · ");
  if (next !== lastAttribution) {
    lastAttribution = next;
    attribution.textContent = next ? `Google Maps · ${next}` : "Google Maps";
  }
}

function updateReadyState() {
  if (firstUsefulFrame || tiles.visibleTiles.size < 4) return;
  firstUsefulFrame = true;
  const elapsed = Math.max(0, performance.now() - startedAt);
  status.textContent = "Prestige Waterford";
  detail.textContent = `Live photorealistic 3D · first scene ${(elapsed / 1000).toFixed(1)}s`;
  document.body.classList.add("scene-ready");
  setSettledQuality();
}

let raf = 0;
function frame(now) {
  raf = requestAnimationFrame(frame);

  controls.update();
  tiles.setResolutionFromRenderer(camera, renderer);
  tiles.setCamera(camera);
  camera.updateMatrixWorld();
  tiles.update();

  renderer.render(scene, camera);
  updateReadyState();
  updateAttribution(now);
}

// Expose only a tiny debug surface. This is deliberately not a generic Atlas API yet.
window.__WATERFORD_CINEMATIC__ = {
  camera,
  controls,
  tiles,
  renderer,
  reset() {
    controls.autoRotate = false;
    camera.position.set(720, 430, -900);
    controls.target.set(0, 48, 0);
    controls.update();
    setSettledQuality();
    scheduleAutoMotionResume();
  },
};

window.addEventListener("pagehide", () => {
  cancelAnimationFrame(raf);
  window.clearTimeout(qualityTimer);
  window.clearTimeout(idleResumeTimer);
  resizeObserver.disconnect();
  controls.dispose();
  dracoLoader.dispose();
  tiles.dispose();
  renderer.dispose();
});

frame(performance.now());
