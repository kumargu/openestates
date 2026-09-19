import { useEffect, useRef, useState } from "react";

const THREE_URL = "https://esm.sh/three@0.185.0";
const ORBIT_CONTROLS_URL = "https://esm.sh/three@0.185.0/examples/jsm/controls/OrbitControls.js";
const DRACO_LOADER_URL = "https://esm.sh/three@0.185.0/examples/jsm/loaders/DRACOLoader.js";
const KTX2_LOADER_URL = "https://esm.sh/three@0.185.0/examples/jsm/loaders/KTX2Loader.js";
const MESHOPT_URL = "https://esm.sh/three@0.185.0/examples/jsm/libs/meshopt_decoder.module.js";
const TILES_RENDERER_URL = "https://esm.sh/3d-tiles-renderer@0.5.2?external=three";
const TILES_PLUGINS_URL = "https://esm.sh/3d-tiles-renderer@0.5.2/plugins?external=three";
const BASIS_TRANSCODER_PATH = "https://cdn.jsdelivr.net/npm/three@0.185.0/examples/jsm/libs/basis/";

const EARTH_RADIUS_M = 6_378_137;

const CAPITOL = {
  name: "Sumadhura Capitol Residences",
  lat: 12.985688035981346,
  lon: 77.75047063207263,
  ellipsoidHeightM: 851,
  rera: "PRM/KA/RERA/1251/446/PR/051024/007125",
  osmWay: "1150896026",
  boundary: [
    { lat: 12.985809, lng: 77.7489587 },
    { lat: 12.9860601, lng: 77.7498094 },
    { lat: 12.9857596, lng: 77.7505443 },
    { lat: 12.9862953, lng: 77.750771 },
    { lat: 12.9860335, lng: 77.7514366 },
    { lat: 12.9850454, lng: 77.7510761 },
  ],
};

const OFFICIAL_PROJECT_URL =
  "https://sumadhuragroup.com/residential/bangalore/sumadhura-capitol-residences";

const OFFICIAL_RENDERS = [
  {
    src: "https://corporatecms.sumadhuragroup.com/uploads/Property_View_1_c5f925429a.png",
    alt: "Official artistic property view for Sumadhura Capitol Residences",
  },
  {
    src: "https://corporatecms.sumadhuragroup.com/uploads/Bird_Eye_View_1_0638f075ae.png",
    alt: "Official artistic bird eye view for Sumadhura Capitol Residences",
  },
  {
    src: "https://corporatecms.sumadhuragroup.com/uploads/Driveway_View_1_06c927395b.png",
    alt: "Official artistic driveway view for Sumadhura Capitol Residences",
  },
];

type LocalPoint = { x: number; z: number };

function runtimeImport(url: string): Promise<any> {
  return import(/* @vite-ignore */ url);
}

function toLocalPoint(lat: number, lng: number): LocalPoint {
  const latRad = CAPITOL.lat * (Math.PI / 180);
  const east =
    (lng - CAPITOL.lon) *
    (Math.PI / 180) *
    EARTH_RADIUS_M *
    Math.cos(latRad);
  const north = (lat - CAPITOL.lat) * (Math.PI / 180) * EARTH_RADIUS_M;
  return { x: east, z: -north };
}

function convexHull(points: LocalPoint[]): LocalPoint[] {
  if (points.length <= 3) return points.slice();

  const sorted = points
    .slice()
    .sort((a, b) => (a.x === b.x ? a.z - b.z : a.x - b.x));

  const cross = (o: LocalPoint, a: LocalPoint, b: LocalPoint) =>
    (a.x - o.x) * (b.z - o.z) - (a.z - o.z) * (b.x - o.x);

  const lower: LocalPoint[] = [];
  for (const point of sorted) {
    while (lower.length >= 2 && cross(lower[lower.length - 2], lower[lower.length - 1], point) <= 0) {
      lower.pop();
    }
    lower.push(point);
  }

  const upper: LocalPoint[] = [];
  for (let index = sorted.length - 1; index >= 0; index -= 1) {
    const point = sorted[index];
    while (upper.length >= 2 && cross(upper[upper.length - 2], upper[upper.length - 1], point) <= 0) {
      upper.pop();
    }
    upper.push(point);
  }

  lower.pop();
  upper.pop();
  return lower.concat(upper);
}

function smoothstep(value: number) {
  const v = Math.min(1, Math.max(0, value));
  return v * v * (3 - 2 * v);
}

export function CapitolFutureLabPage() {
  const mapsKey = import.meta.env.VITE_GOOGLE_MAPS_API_KEY?.trim() ?? "";
  const sceneRootRef = useRef<HTMLDivElement | null>(null);
  const attributionRef = useRef<HTMLDivElement | null>(null);
  const plannedLevelRef = useRef(100);
  const started = useRef(false);

  const [plannedPercent, setPlannedPercent] = useState(100);
  const [status, setStatus] = useState("Preparing Capitol");
  const [detail, setDetail] = useState("Starting Google 3D + future-state reconstruction...");
  const [error, setError] = useState<string | null>(null);
  const [evidenceOpen, setEvidenceOpen] = useState(true);

  useEffect(() => {
    plannedLevelRef.current = plannedPercent;
  }, [plannedPercent]);

  useEffect(() => {
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previousOverflow;
      document.body.classList.remove("capitol-scene-ready");
    };
  }, []);

  useEffect(() => {
    if (started.current) return;
    started.current = true;

    let cancelled = false;
    let teardown: (() => void) | undefined;

    async function start() {
      if (!mapsKey) {
        throw new Error("VITE_GOOGLE_MAPS_API_KEY is not present in this preview build.");
      }

      setStatus("Checking Google 3D Tiles");
      const rootUrl =
        "https://tile.googleapis.com/v1/3dtiles/root.json?key=" + encodeURIComponent(mapsKey);
      const response = await fetch(rootUrl, { cache: "no-store" });
      if (!response.ok) {
        throw new Error("Google Photorealistic 3D Tiles API returned HTTP " + response.status);
      }

      setStatus("Loading Whitefield");
      setDetail("Keeping the real neighbourhood intact while the project parcel is prepared...");

      const [
        THREE,
        orbitModule,
        dracoModule,
        ktxModule,
        meshoptModule,
        tilesModule,
        pluginsModule,
      ] = await Promise.all([
        runtimeImport(THREE_URL),
        runtimeImport(ORBIT_CONTROLS_URL),
        runtimeImport(DRACO_LOADER_URL),
        runtimeImport(KTX2_LOADER_URL),
        runtimeImport(MESHOPT_URL),
        runtimeImport(TILES_RENDERER_URL),
        runtimeImport(TILES_PLUGINS_URL),
      ]);

      if (cancelled) return undefined;

      const root = sceneRootRef.current;
      if (!root) throw new Error("Capitol scene host disappeared before renderer startup.");

      const {
        AmbientLight,
        BoxGeometry,
        BufferGeometry,
        CanvasTexture,
        CircleGeometry,
        Color,
        DirectionalLight,
        DoubleSide,
        Group,
        LineBasicMaterial,
        LineLoop,
        MathUtils,
        Mesh,
        MeshStandardMaterial,
        PerspectiveCamera,
        Plane,
        RepeatWrapping,
        Scene,
        Shape,
        ShapeGeometry,
        SRGBColorSpace,
        Vector3,
        WebGLRenderer,
      } = THREE;

      const { OrbitControls } = orbitModule;
      const { DRACOLoader } = dracoModule;
      const { KTX2Loader } = ktxModule;
      const { MeshoptDecoder } = meshoptModule;
      const { TilesRenderer } = tilesModule;
      const {
        GLTFExtensionsPlugin,
        GoogleCloudAuthPlugin,
        ReorientationPlugin,
        TilesFadePlugin,
      } = pluginsModule;

      const isCoarsePointer = window.matchMedia("(pointer: coarse)").matches;
      const prefersReducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      const deviceDpr = Math.max(1, window.devicePixelRatio || 1);
      const movingDpr = Math.min(deviceDpr, isCoarsePointer ? 1.45 : 1.8);
      const settledDpr = Math.min(deviceDpr, isCoarsePointer ? 1.9 : 2.25);
      let activeDpr = movingDpr;

      const scene = new Scene();
      scene.background = new Color(0x151b1d);

      const renderer = new WebGLRenderer({
        antialias: true,
        alpha: false,
        powerPreference: "high-performance",
      });
      renderer.outputColorSpace = SRGBColorSpace;
      renderer.setPixelRatio(activeDpr);
      renderer.localClippingEnabled = true;
      root.replaceChildren(renderer.domElement);

      scene.add(new AmbientLight(0xffffff, 1.65));
      const sun = new DirectionalLight(0xfff8ed, 2.5);
      sun.position.set(-180, 360, 220);
      scene.add(sun);

      const camera = new PerspectiveCamera(48, 1, 3, 1_600_000);
      camera.position.set(320, 118, 310);

      const controls = new OrbitControls(camera, renderer.domElement);
      controls.target.set(-18, 28, 15);
      controls.enableDamping = true;
      controls.dampingFactor = 0.055;
      controls.enablePan = false;
      controls.rotateSpeed = 0.52;
      controls.zoomSpeed = 0.72;
      controls.minDistance = 145;
      controls.maxDistance = 820;
      controls.minPolarAngle = Math.PI / 3.2;
      controls.maxPolarAngle = Math.PI / 2 - 0.035;
      controls.autoRotate = false;
      controls.autoRotateSpeed = 0.24;
      camera.lookAt(controls.target);
      controls.update();

      const tiles = new TilesRenderer();
      tiles.registerPlugin(
        new GoogleCloudAuthPlugin({
          apiToken: mapsKey,
          autoRefreshToken: true,
          useRecommendedSettings: true,
        }),
      );
      tiles.registerPlugin(new TilesFadePlugin());

      const dracoLoader = new DRACOLoader();
      dracoLoader.setDecoderPath("https://www.gstatic.com/draco/versioned/decoders/1.5.7/");

      const ktxLoader = new KTX2Loader();
      ktxLoader.setTranscoderPath(BASIS_TRANSCODER_PATH);
      ktxLoader.detectSupport(renderer);

      tiles.registerPlugin(
        new GLTFExtensionsPlugin({
          dracoLoader,
          ktxLoader,
          meshoptDecoder: MeshoptDecoder,
        }),
      );
      tiles.registerPlugin(
        new ReorientationPlugin({
          lat: CAPITOL.lat * MathUtils.DEG2RAD,
          lon: CAPITOL.lon * MathUtils.DEG2RAD,
          height: CAPITOL.ellipsoidHeightM,
        }),
      );

      scene.add(tiles.group);
      tiles.setCamera(camera);

      const detailMoving = isCoarsePointer ? 17 : 14;
      const detailSettled = isCoarsePointer ? 9 : 7;
      tiles.errorTarget = detailMoving;

      const boundaryPoints = CAPITOL.boundary.map((point) => toLocalPoint(point.lat, point.lng));
      const clippingHull = convexHull(boundaryPoints);
      const hullCenter = clippingHull.reduce(
        (acc, point) => ({ x: acc.x + point.x / clippingHull.length, z: acc.z + point.z / clippingHull.length }),
        { x: 0, z: 0 },
      );

      const clippingPlanes = clippingHull.map((a, index) => {
        const b = clippingHull[(index + 1) % clippingHull.length];
        const dx = b.x - a.x;
        const dz = b.z - a.z;
        let nx = dz;
        let nz = -dx;
        let constant = -(nx * a.x + nz * a.z);
        const centerDistance = nx * hullCenter.x + nz * hullCenter.z + constant;

        if (centerDistance > 0) {
          nx *= -1;
          nz *= -1;
          constant *= -1;
        }

        const length = Math.hypot(nx, nz) || 1;
        return new Plane(new Vector3(nx / length, 0, nz / length), constant / length);
      });

      const tileMaterials = new Set<any>();
      let clippingActive = false;

      const setMaterialClipping = (material: any, enabled: boolean) => {
        if (!material) return;
        material.clippingPlanes = enabled ? clippingPlanes : null;
        material.clipIntersection = true;
        material.needsUpdate = true;
      };

      const registerTileMaterial = (material: any) => {
        if (!material || tileMaterials.has(material)) return;
        tileMaterials.add(material);
        setMaterialClipping(material, plannedLevelRef.current > 2);
      };

      let modelCount = 0;
      const inspectModel = ({ scene: modelScene }: any) => {
        modelCount += 1;
        modelScene?.traverse?.((object: any) => {
          if (!object?.isMesh) return;
          const materials = Array.isArray(object.material) ? object.material : [object.material];
          materials.forEach(registerTileMaterial);
        });
      };
      tiles.addEventListener("load-model", inspectModel);

      const futureRoot = new Group();
      futureRoot.name = "Capitol future-state reconstruction";
      scene.add(futureRoot);

      const futureMaterials = new Set<any>();
      const trackMaterial = (material: any, baseOpacity = 1) => {
        material.transparent = true;
        material.userData.futureBaseOpacity = baseOpacity;
        futureMaterials.add(material);
        return material;
      };

      const facadeCanvas = document.createElement("canvas");
      facadeCanvas.width = 256;
      facadeCanvas.height = 480;
      const facadeContext = facadeCanvas.getContext("2d");
      if (!facadeContext) throw new Error("Could not create procedural facade texture.");

      facadeContext.fillStyle = "#dcd9d2";
      facadeContext.fillRect(0, 0, 256, 480);

      const floorHeight = 480 / 15;
      for (let floor = 0; floor < 15; floor += 1) {
        const y = floor * floorHeight;
        facadeContext.fillStyle = floor % 2 === 0 ? "#b9b8b4" : "#c8c5bf";
        facadeContext.fillRect(0, y, 256, 2);
        facadeContext.fillStyle = "#273238";
        for (let bay = 0; bay < 5; bay += 1) {
          facadeContext.fillRect(11 + bay * 49, y + 6, 31, 17);
        }
        facadeContext.fillStyle = "rgba(255,255,255,.38)";
        facadeContext.fillRect(0, y + 25, 256, 3);
      }

      facadeContext.fillStyle = "#786858";
      facadeContext.fillRect(112, 0, 28, 480);
      facadeContext.fillStyle = "#eeebe4";
      facadeContext.fillRect(119, 0, 12, 480);

      const facadeTexture = new CanvasTexture(facadeCanvas);
      facadeTexture.colorSpace = SRGBColorSpace;
      facadeTexture.wrapS = RepeatWrapping;
      facadeTexture.wrapT = RepeatWrapping;
      facadeTexture.needsUpdate = true;

      const facadeMaterial = trackMaterial(
        new MeshStandardMaterial({
          map: facadeTexture,
          color: 0xf3efe8,
          roughness: 0.82,
          metalness: 0.02,
        }),
      );
      const charcoalMaterial = trackMaterial(
        new MeshStandardMaterial({
          color: 0x394346,
          roughness: 0.72,
          metalness: 0.08,
        }),
      );
      const bronzeMaterial = trackMaterial(
        new MeshStandardMaterial({
          color: 0x9d7650,
          roughness: 0.62,
          metalness: 0.18,
        }),
      );
      const podiumMaterial = trackMaterial(
        new MeshStandardMaterial({
          color: 0xcac3b5,
          roughness: 0.92,
        }),
      );
      const glassMaterial = trackMaterial(
        new MeshStandardMaterial({
          color: 0x61747b,
          roughness: 0.28,
          metalness: 0.15,
        }),
        0.82,
      );
      const greenMaterial = trackMaterial(
        new MeshStandardMaterial({
          color: 0x607964,
          roughness: 1,
          side: DoubleSide,
        }),
        0.92,
      );

      const makeTower = ({
        x,
        z,
        rotation,
        width,
        depth,
        height,
      }: {
        x: number;
        z: number;
        rotation: number;
        width: number;
        depth: number;
        height: number;
      }) => {
        const tower = new Group();
        tower.position.set(x, 4.2, z);
        tower.rotation.y = rotation;

        const body = new Mesh(new BoxGeometry(width, height, depth), facadeMaterial);
        body.position.y = height / 2;
        tower.add(body);

        const spine = new Mesh(new BoxGeometry(2.1, height + 1, depth + 0.45), charcoalMaterial);
        spine.position.set(width * 0.31, height / 2, 0);
        tower.add(spine);

        const sideGlass = new Mesh(new BoxGeometry(1.3, height * 0.88, depth + 0.7), glassMaterial);
        sideGlass.position.set(-width * 0.31, height * 0.48, 0);
        tower.add(sideGlass);

        const crown = new Mesh(new BoxGeometry(width * 0.8, 2.3, depth + 1.2), bronzeMaterial);
        crown.position.set(0, height + 1.2, 0);
        tower.add(crown);

        return tower;
      };

      const towerHeight = 54;
      futureRoot.add(
        makeTower({ x: 67, z: 6, rotation: -0.32, width: 42, depth: 16, height: towerHeight }),
        makeTower({ x: 24, z: -9, rotation: -0.07, width: 43, depth: 16, height: towerHeight }),
        makeTower({ x: -27, z: 4, rotation: 0.17, width: 43, depth: 16, height: towerHeight }),
        makeTower({ x: -79, z: 26, rotation: 0.31, width: 42, depth: 16, height: towerHeight }),
      );

      const podium = new Mesh(new BoxGeometry(130, 4.1, 55), podiumMaterial);
      podium.position.set(-8, 2.05, 27);
      podium.rotation.y = 0.07;
      futureRoot.add(podium);

      const clubhouse = new Mesh(new BoxGeometry(34, 9.5, 22), podiumMaterial);
      clubhouse.position.set(38, 8.8, 54);
      clubhouse.rotation.y = -0.15;
      futureRoot.add(clubhouse);

      const clubhouseGlass = new Mesh(new BoxGeometry(26, 5.2, 0.7), glassMaterial);
      clubhouseGlass.position.set(38, 9.3, 42.7);
      clubhouseGlass.rotation.y = -0.15;
      futureRoot.add(clubhouseGlass);

      const courtyard = new Mesh(new CircleGeometry(21, 48), greenMaterial);
      courtyard.rotation.x = -Math.PI / 2;
      courtyard.position.set(-20, 4.15, 40);
      futureRoot.add(courtyard);

      const boundaryLineMaterial = trackMaterial(
        new LineBasicMaterial({
          color: 0xffc76d,
          transparent: true,
          opacity: 0.9,
        }),
        0.9,
      );
      const boundaryGeometry = new BufferGeometry().setFromPoints(
        boundaryPoints.map((point) => new Vector3(point.x, 4.5, point.z)),
      );
      const boundaryLine = new LineLoop(boundaryGeometry, boundaryLineMaterial);
      futureRoot.add(boundaryLine);

      const siteShape = new Shape();
      boundaryPoints.forEach((point, index) => {
        const shapeX = point.x;
        const shapeY = -point.z;
        if (index === 0) siteShape.moveTo(shapeX, shapeY);
        else siteShape.lineTo(shapeX, shapeY);
      });
      siteShape.closePath();

      const siteMaterial = trackMaterial(
        new MeshStandardMaterial({
          color: 0xa79b83,
          roughness: 1,
          side: DoubleSide,
          polygonOffset: true,
          polygonOffsetFactor: -1,
          polygonOffsetUnits: -1,
        }),
        0.38,
      );
      const siteSurface = new Mesh(new ShapeGeometry(siteShape), siteMaterial);
      siteSurface.rotation.x = -Math.PI / 2;
      siteSurface.position.y = 0.6;
      futureRoot.add(siteSurface);

      const updateFutureState = () => {
        const rawLevel = Math.min(100, Math.max(0, plannedLevelRef.current)) / 100;
        const level = smoothstep(rawLevel);
        const shouldClip = rawLevel > 0.02;

        if (shouldClip !== clippingActive) {
          clippingActive = shouldClip;
          tileMaterials.forEach((material) => setMaterialClipping(material, clippingActive));
        }

        futureRoot.visible = rawLevel > 0.005;
        futureRoot.scale.y = 0.88 + 0.12 * level;

        futureMaterials.forEach((material) => {
          const baseOpacity = material.userData.futureBaseOpacity ?? 1;
          material.opacity = baseOpacity * level;
        });
      };

      updateFutureState();

      let qualityTimer = 0;
      let autoStartTimer = 0;
      let idleResumeTimer = 0;
      let firstUsefulFrame = false;
      let lastAttribution = "";
      let lastUiUpdate = 0;
      const startedAt = performance.now();
      let raf = 0;

      const resize = () => {
        const width = Math.max(1, root.clientWidth);
        const height = Math.max(1, root.clientHeight);
        camera.aspect = width / height;
        camera.updateProjectionMatrix();
        renderer.setSize(width, height, false);
        tiles.setResolutionFromRenderer(camera, renderer);
      };

      const resizeObserver = new ResizeObserver(resize);
      resizeObserver.observe(root);
      resize();

      const applyPixelRatio = (next: number) => {
        if (Math.abs(activeDpr - next) < 0.05) return;
        activeDpr = next;
        renderer.setPixelRatio(activeDpr);
        resize();
      };

      const setMovingQuality = () => {
        window.clearTimeout(qualityTimer);
        tiles.errorTarget = detailMoving;
        applyPixelRatio(movingDpr);
      };

      const setSettledQuality = () => {
        window.clearTimeout(qualityTimer);
        qualityTimer = window.setTimeout(() => {
          tiles.errorTarget = detailSettled;
          applyPixelRatio(settledDpr);
        }, 280);
      };

      const scheduleAutoMotionResume = () => {
        window.clearTimeout(idleResumeTimer);
        if (prefersReducedMotion) return;
        idleResumeTimer = window.setTimeout(() => {
          controls.autoRotate = true;
        }, 3200);
      };

      const onControlStart = () => {
        controls.autoRotate = false;
        window.clearTimeout(autoStartTimer);
        window.clearTimeout(idleResumeTimer);
        setMovingQuality();
      };
      const onControlEnd = () => {
        setSettledQuality();
        scheduleAutoMotionResume();
      };

      controls.addEventListener("start", onControlStart);
      controls.addEventListener("end", onControlEnd);

      const updateAttribution = () => {
        const values = (tiles.getAttributions?.() ?? [])
          .filter((entry: any) => entry?.type === "string" && entry.value)
          .map((entry: any) => String(entry.value).trim())
          .filter(Boolean);
        const next = values.join(" · ");
        if (next !== lastAttribution) {
          lastAttribution = next;
          if (attributionRef.current) {
            attributionRef.current.textContent = next ? "Google Maps · " + next : "Google Maps";
          }
        }
      };

      const updateUi = (now: number) => {
        if (now - lastUiUpdate < 800) return;
        lastUiUpdate = now;
        updateAttribution();

        if (!firstUsefulFrame) {
          const visible = tiles.visibleTiles?.size ?? 0;
          setDetail("Google 3D · visible " + visible + " · preparing future parcel replacement");
        }
      };

      const frame = (now: number) => {
        if (cancelled) return;
        raf = requestAnimationFrame(frame);

        controls.update();
        updateFutureState();
        tiles.setResolutionFromRenderer(camera, renderer);
        tiles.setCamera(camera);
        camera.updateMatrixWorld();
        tiles.update();
        renderer.render(scene, camera);

        const visibleCount = tiles.visibleTiles?.size ?? 0;
        if (!firstUsefulFrame && visibleCount >= 4 && modelCount > 0) {
          firstUsefulFrame = true;
          const elapsed = Math.max(0, performance.now() - startedAt);
          setStatus("Sumadhura Capitol Residences");
          setDetail(
            "Real surroundings + illustrative future project · loaded in " +
              (elapsed / 1000).toFixed(1) +
              "s",
          );
          document.body.classList.add("capitol-scene-ready");
          setSettledQuality();

          if (!prefersReducedMotion) {
            autoStartTimer = window.setTimeout(() => {
              controls.autoRotate = true;
            }, 1700);
          }
        }

        updateUi(now);
      };

      raf = requestAnimationFrame(frame);

      return () => {
        cancelAnimationFrame(raf);
        window.clearTimeout(qualityTimer);
        window.clearTimeout(autoStartTimer);
        window.clearTimeout(idleResumeTimer);
        resizeObserver.disconnect();
        controls.removeEventListener("start", onControlStart);
        controls.removeEventListener("end", onControlEnd);
        tiles.removeEventListener("load-model", inspectModel);
        controls.dispose();
        tiles.dispose();
        facadeTexture.dispose();
        renderer.dispose();
        root.replaceChildren();
        document.body.classList.remove("capitol-scene-ready");
      };
    }

    void start()
      .then((cleanup) => {
        if (!cleanup) return;
        if (cancelled) cleanup();
        else teardown = cleanup;
      })
      .catch((caught) => {
        if (cancelled) return;
        const message = caught instanceof Error ? caught.message : String(caught);
        setError(message);
        setStatus("Capitol future view could not start");
        setDetail(message);
      });

    return () => {
      cancelled = true;
      teardown?.();
    };
  }, [mapsKey]);

  const viewLabel =
    plannedPercent <= 2 ? "Today" : plannedPercent >= 98 ? "Planned" : "Transition";

  return (
    <div className="capitol-future-lab">
      <style>{[
        "body { background: #151b1d; }",
        ".capitol-future-lab { position: fixed; inset: 0; z-index: 100000; overflow: hidden; background: #151b1d; color: rgba(255,255,255,.96); font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; }",
        "#capitol-scene-root { position: absolute; inset: 0; }",
        "#capitol-scene-root canvas { display: block; width: 100%; height: 100%; touch-action: none; }",
        ".capitol-top { position: absolute; z-index: 4; left: max(22px, env(safe-area-inset-left)); top: max(20px, env(safe-area-inset-top)); pointer-events: none; text-shadow: 0 1px 18px rgba(0,0,0,.78); }",
        ".capitol-kicker { margin: 0 0 6px; font-size: 10px; font-weight: 750; letter-spacing: .16em; text-transform: uppercase; color: #ffc76d; }",
        ".capitol-top h1 { margin: 0; font-size: clamp(18px, 2vw, 27px); font-weight: 650; letter-spacing: -.03em; }",
        ".capitol-detail { margin-top: 7px; max-width: min(760px, 82vw); font-size: 11px; line-height: 1.45; color: rgba(255,255,255,.66); }",
        ".future-control { position: absolute; z-index: 8; left: 50%; bottom: max(28px, calc(env(safe-area-inset-bottom) + 18px)); transform: translateX(-50%); width: min(560px, calc(100vw - 34px)); padding: 12px 14px 13px; border: 1px solid rgba(255,255,255,.16); border-radius: 18px; background: rgba(13,17,18,.76); box-shadow: 0 16px 54px rgba(0,0,0,.28); backdrop-filter: blur(18px); -webkit-backdrop-filter: blur(18px); }",
        ".future-control-row { display: grid; grid-template-columns: auto 1fr auto; gap: 12px; align-items: center; }",
        ".future-chip { min-width: 68px; border: 0; border-radius: 999px; padding: 8px 10px; background: rgba(255,255,255,.08); color: rgba(255,255,255,.9); font: inherit; font-size: 11px; cursor: pointer; }",
        ".future-chip.is-active { background: rgba(255,199,109,.18); color: #ffd99a; }",
        ".future-slider { width: 100%; accent-color: #dca85b; cursor: ew-resize; }",
        ".future-caption { display: flex; justify-content: space-between; gap: 12px; margin-top: 8px; font-size: 10px; color: rgba(255,255,255,.58); }",
        ".future-caption strong { color: rgba(255,255,255,.92); font-weight: 650; }",
        ".evidence-card { position: absolute; z-index: 7; right: max(18px, env(safe-area-inset-right)); top: max(18px, env(safe-area-inset-top)); width: min(360px, calc(100vw - 36px)); border: 1px solid rgba(255,255,255,.14); border-radius: 18px; background: rgba(15,19,20,.77); box-shadow: 0 22px 70px rgba(0,0,0,.26); backdrop-filter: blur(18px); -webkit-backdrop-filter: blur(18px); overflow: hidden; }",
        ".evidence-head { display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 13px 14px; }",
        ".evidence-head button { border: 0; background: transparent; color: rgba(255,255,255,.68); font: inherit; font-size: 11px; cursor: pointer; }",
        ".evidence-title { font-size: 12px; font-weight: 720; }",
        ".evidence-body { padding: 0 14px 14px; }",
        ".evidence-warning { margin: 0 0 11px; padding: 9px 10px; border-radius: 10px; background: rgba(255,199,109,.1); color: #ffe0a7; font-size: 10px; line-height: 1.45; }",
        ".evidence-grid { display: grid; grid-template-columns: 88px 1fr; gap: 6px 10px; font-size: 10px; line-height: 1.4; }",
        ".evidence-grid dt { color: rgba(255,255,255,.46); }",
        ".evidence-grid dd { margin: 0; color: rgba(255,255,255,.82); overflow-wrap: anywhere; }",
        ".render-strip { display: grid; grid-template-columns: repeat(3,1fr); gap: 6px; margin-top: 12px; }",
        ".render-strip a { display: block; overflow: hidden; border-radius: 9px; aspect-ratio: 1.25; border: 1px solid rgba(255,255,255,.1); background: rgba(255,255,255,.04); }",
        ".render-strip img { width: 100%; height: 100%; object-fit: cover; display: block; }",
        ".render-source { display: inline-block; margin-top: 9px; color: #ffd28a; font-size: 10px; text-decoration: none; }",
        "#capitol-attribution { position: absolute; z-index: 9; right: max(8px, env(safe-area-inset-right)); bottom: max(5px, env(safe-area-inset-bottom)); max-width: min(70vw, 820px); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; padding: 3px 6px; border-radius: 4px; background: rgba(255,255,255,.84); color: rgba(0,0,0,.84); font: 9px/1.2 Arial, sans-serif; pointer-events: none; }",
        ".scene-error { position: absolute; z-index: 20; left: 50%; top: 50%; transform: translate(-50%,-50%); width: min(640px, calc(100vw - 36px)); padding: 22px; border: 1px solid rgba(255,255,255,.14); border-radius: 18px; background: rgba(13,17,19,.96); box-shadow: 0 24px 90px rgba(0,0,0,.34); }",
        ".scene-error strong { display: block; font-size: 16px; }",
        ".scene-error p { margin: 9px 0 0; font-size: 12px; line-height: 1.55; color: rgba(255,255,255,.65); overflow-wrap: anywhere; }",
        "@media (max-width: 860px) { .capitol-top { left: 16px; top: 16px; right: 16px; } .capitol-detail { max-width: 65vw; font-size: 10px; } .evidence-card { top: auto; right: 12px; bottom: 118px; width: min(330px, calc(100vw - 24px)); } .evidence-body { max-height: 40vh; overflow: auto; } }",
        "@media (max-width: 640px) { .capitol-top h1 { max-width: 66vw; } .capitol-detail { display: none; } .evidence-card { left: 12px; right: 12px; width: auto; bottom: 115px; } .future-control { bottom: 20px; } .future-control-row { gap: 8px; } .future-chip { min-width: 58px; padding-inline: 8px; } #capitol-attribution { bottom: 2px; max-width: 55vw; } }",
      ].join("\n")}</style>

      <div
        ref={sceneRootRef}
        id="capitol-scene-root"
        aria-label="Interactive 3D future-state view of Sumadhura Capitol Residences"
      />

      <div className="capitol-top">
        <p className="capitol-kicker">Future-state 3D · prototype</p>
        <h1>{status}</h1>
        <div className="capitol-detail">{detail}</div>
      </div>

      <aside className="evidence-card" aria-label="Future view evidence">
        <div className="evidence-head">
          <div className="evidence-title">What is real vs reconstructed</div>
          <button type="button" onClick={() => setEvidenceOpen((open) => !open)}>
            {evidenceOpen ? "Hide" : "Show"}
          </button>
        </div>

        {evidenceOpen && (
          <div className="evidence-body">
            <p className="evidence-warning">
              Illustrative reconstruction. The surroundings are Google photorealistic 3D;
              only the project parcel is replaced. Exact sanctioned tower footprints and
              heights are not asserted by this prototype.
            </p>

            <dl className="evidence-grid">
              <dt>Boundary</dt>
              <dd>OpenStreetMap way {CAPITOL.osmWay}</dd>
              <dt>RERA</dt>
              <dd>{CAPITOL.rera}</dd>
              <dt>Structure</dt>
              <dd>4 towers · 3B + G + 15</dd>
              <dt>Façade</dt>
              <dd>Interpreted from developer artistic renders</dd>
              <dt>Geometry</dt>
              <dd>Approximate massing for this experiment</dd>
              <dt>Surroundings</dt>
              <dd>Google Photorealistic 3D Tiles</dd>
            </dl>

            <div className="render-strip">
              {OFFICIAL_RENDERS.map((render) => (
                <a key={render.src} href={OFFICIAL_PROJECT_URL} target="_blank" rel="noreferrer">
                  <img src={render.src} alt={render.alt} />
                </a>
              ))}
            </div>
            <a className="render-source" href={OFFICIAL_PROJECT_URL} target="_blank" rel="noreferrer">
              Developer source ↗
            </a>
          </div>
        )}
      </aside>

      <div className="future-control" aria-label="Today to planned view control">
        <div className="future-control-row">
          <button
            type="button"
            className={"future-chip " + (plannedPercent <= 2 ? "is-active" : "")}
            onClick={() => setPlannedPercent(0)}
          >
            Today
          </button>

          <input
            className="future-slider"
            type="range"
            min="0"
            max="100"
            step="1"
            value={plannedPercent}
            onChange={(event) => setPlannedPercent(Number(event.target.value))}
            aria-label="Blend between today's site and the planned future view"
          />

          <button
            type="button"
            className={"future-chip " + (plannedPercent >= 98 ? "is-active" : "")}
            onClick={() => setPlannedPercent(100)}
          >
            Planned
          </button>
        </div>
        <div className="future-caption">
          <span>Current Google 3D site</span>
          <strong>{viewLabel} · {plannedPercent}%</strong>
          <span>Evidence-backed massing</span>
        </div>
      </div>

      <div ref={attributionRef} id="capitol-attribution">Google Maps</div>

      {error && (
        <div className="scene-error" role="alert">
          <strong>Capitol future view could not start</strong>
          <p>{error}</p>
        </div>
      )}
    </div>
  );
}
