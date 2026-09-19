import { useEffect, useMemo, useRef, useState } from "react";
import "../styles/waterford-direct-tiles.css";

const TOKYO_TOWER = {
  name: "Tokyo Tower",
  latitude: 35.6586,
  longitude: 139.7454,
  ellipsoidHeightM: 40,
};

const MODULES = {
  three: "https://esm.sh/three@0.185.0",
  orbit: "https://esm.sh/three@0.185.0/examples/jsm/controls/OrbitControls.js",
  draco: "https://esm.sh/three@0.185.0/examples/jsm/loaders/DRACOLoader.js",
  ktx2: "https://esm.sh/three@0.185.0/examples/jsm/loaders/KTX2Loader.js",
  meshopt: "https://esm.sh/three@0.185.0/examples/jsm/libs/meshopt_decoder.module.js",
  tiles: "https://esm.sh/3d-tiles-renderer@0.5.2?external=three",
  plugins: "https://esm.sh/3d-tiles-renderer@0.5.2/plugins?external=three",
};

const BASIS_TRANSCODER_PATH =
  "https://cdn.jsdelivr.net/npm/three@0.185.0/examples/jsm/libs/basis/";

const CAMERA_STOPS = [
  { range: 1200, label: "1,200 m", detail: "Neighbourhood" },
  { range: 700, label: "700 m", detail: "Society context" },
  { range: 500, label: "500 m", detail: "NASA-like" },
  { range: 350, label: "350 m", detail: "Inspection" },
  { range: 200, label: "200 m", detail: "Stress test" },
] as const;

const LOD_TARGETS = [
  { value: 6, label: "Settled max" },
  { value: 8, label: "High detail" },
  { value: 20, label: "NASA default" },
] as const;

type RuntimeStats = {
  queued: number;
  downloading: number;
  parsing: number;
  loaded: number;
  visible: number;
  active: number;
  cacheMb: number;
  texturedMeshes: number;
  meshes: number;
};

const EMPTY_STATS: RuntimeStats = {
  queued: 0,
  downloading: 0,
  parsing: 0,
  loaded: 0,
  visible: 0,
  active: 0,
  cacheMb: 0,
  texturedMeshes: 0,
  meshes: 0,
};

function runtimeImport(url: string): Promise<any> {
  return import(/* @vite-ignore */ url);
}

function cameraPosition(THREE: any, range: number) {
  const heading = THREE.MathUtils.degToRad(225);
  const elevation = THREE.MathUtils.degToRad(32);
  const horizontal = range * Math.cos(elevation);
  const vertical = range * Math.sin(elevation);
  const targetY = 32;

  return {
    position: new THREE.Vector3(
      Math.sin(heading) * horizontal,
      targetY + vertical,
      Math.cos(heading) * horizontal,
    ),
    target: new THREE.Vector3(0, targetY, 0),
  };
}

export function TokyoTowerDirectTilesLabPage() {
  const canvasHostRef = useRef<HTMLDivElement | null>(null);
  const attributionRef = useRef<HTMLDivElement | null>(null);
  const flyToRef = useRef<(range: number) => void>(() => undefined);
  const setLodRef = useRef<(target: number) => void>(() => undefined);
  const [status, setStatus] = useState("Preparing renderer…");
  const [loadError, setLoadError] = useState<string | null>(null);
  const [range, setRange] = useState(500);
  const [lodTarget, setLodTarget] = useState(8);
  const [stats, setStats] = useState<RuntimeStats>(EMPTY_STATS);

  const apiKey = import.meta.env.VITE_GOOGLE_MAPS_API_KEY?.trim();
  const currentStop = useMemo(
    () => CAMERA_STOPS.find((stop) => stop.range === range) ?? CAMERA_STOPS[2],
    [range],
  );

  useEffect(() => {
    const host = canvasHostRef.current;
    if (!host || !apiKey) {
      setLoadError(
        "VITE_GOOGLE_MAPS_API_KEY is missing. This lab never stores a key in source; configure it in the runtime environment.",
      );
      return undefined;
    }

    let disposed = false;
    let frameId = 0;
    let resizeObserver: ResizeObserver | null = null;
    let renderer: any;
    let controls: any;
    let tiles: any;
    let lastTelemetry = 0;
    let cameraTween = 0;
    let meshCount = 0;
    let texturedMeshCount = 0;

    const renderAttribution = () => {
      const target = attributionRef.current;
      if (!target || !tiles) return;
      target.replaceChildren();

      for (const attribution of tiles.getAttributions?.() ?? []) {
        if (!attribution?.value) continue;
        if (attribution.type === "image") {
          const image = document.createElement("img");
          image.src = attribution.value;
          image.alt = "Google";
          image.className = "waterford-direct-tiles__google-logo";
          target.append(image);
        } else {
          const text = document.createElement("span");
          text.textContent = attribution.value;
          target.append(text);
        }
      }
    };

    void (async () => {
      try {
        setStatus("Checking Google Photorealistic 3D Tiles…");
        const preflight = await fetch(
          `https://tile.googleapis.com/v1/3dtiles/root.json?key=${encodeURIComponent(apiKey)}`,
          { cache: "no-store" },
        );
        if (!preflight.ok) {
          throw new Error(
            `Google Photorealistic 3D Tiles API returned HTTP ${preflight.status}`,
          );
        }

        setStatus("Loading NASA renderer + material decoders…");

        const [
          THREE,
          controlsModule,
          dracoModule,
          ktxModule,
          meshoptModule,
          tilesModule,
          pluginsModule,
        ] = await Promise.all([
          runtimeImport(MODULES.three),
          runtimeImport(MODULES.orbit),
          runtimeImport(MODULES.draco),
          runtimeImport(MODULES.ktx2),
          runtimeImport(MODULES.meshopt),
          runtimeImport(MODULES.tiles),
          runtimeImport(MODULES.plugins),
        ]);
        if (disposed) return;

        const { OrbitControls } = controlsModule;
        const { DRACOLoader } = dracoModule;
        const { KTX2Loader } = ktxModule;
        const { MeshoptDecoder } = meshoptModule;
        const { TilesRenderer } = tilesModule;
        const {
          GoogleCloudAuthPlugin,
          TilesFadePlugin,
          GLTFExtensionsPlugin,
          ReorientationPlugin,
        } = pluginsModule;

        const scene = new THREE.Scene();
        scene.background = new THREE.Color(0x151c1f);

        renderer = new THREE.WebGLRenderer({
          antialias: true,
          alpha: false,
          powerPreference: "high-performance",
        });
        renderer.outputColorSpace = THREE.SRGBColorSpace;
        renderer.setPixelRatio(window.devicePixelRatio);
        renderer.setSize(host.clientWidth, host.clientHeight, false);
        host.replaceChildren(renderer.domElement);

        const camera = new THREE.PerspectiveCamera(
          60,
          host.clientWidth / Math.max(1, host.clientHeight),
          5,
          1_600_000,
        );

        controls = new OrbitControls(camera, renderer.domElement);
        controls.enableDamping = true;
        controls.dampingFactor = 0.055;
        controls.enablePan = false;
        controls.rotateSpeed = 0.55;
        controls.zoomSpeed = 0.72;
        controls.minDistance = 120;
        controls.maxDistance = 2_500;
        controls.minPolarAngle = Math.PI / 3;
        controls.maxPolarAngle = Math.PI / 2 - 0.035;

        const initial = cameraPosition(THREE, 500);
        camera.position.copy(initial.position);
        controls.target.copy(initial.target);
        camera.lookAt(initial.target);
        controls.update();

        tiles = new TilesRenderer();
        tiles.registerPlugin(new GoogleCloudAuthPlugin({
          apiToken: apiKey,
          autoRefreshToken: true,
          logoUrl: "https://www.gstatic.com/images/branding/googlelogo/2x/googlelogo_color_92x30dp.png",
          useRecommendedSettings: true,
        }));
        tiles.registerPlugin(new TilesFadePlugin());

        const dracoLoader = new DRACOLoader();
        dracoLoader.setDecoderPath(
          "https://www.gstatic.com/draco/versioned/decoders/1.5.7/",
        );

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
            lat: THREE.MathUtils.degToRad(TOKYO_TOWER.latitude),
            lon: THREE.MathUtils.degToRad(TOKYO_TOWER.longitude),
            height: TOKYO_TOWER.ellipsoidHeightM,
          }),
        );

        // Keep this A/B fidelity-first. TileCompressionPlugin changes texture
        // mip behavior, so it is intentionally excluded from the baseline.
        tiles.errorTarget = 8;
        tiles.loadSiblings = true;
        tiles.loadAncestors = true;

        scene.add(tiles.group);
        tiles.setResolutionFromRenderer(camera, renderer);
        tiles.setCamera(camera);

        tiles.addEventListener?.("load-model", ({ scene: modelScene }: any) => {
          modelScene?.traverse?.((object: any) => {
            if (!object?.isMesh) return;
            meshCount += 1;
            const materials = Array.isArray(object.material)
              ? object.material
              : [object.material];
            if (
              materials.some(
                (material: any) =>
                  material &&
                  Object.values(material).some((value: any) => value?.isTexture),
              )
            ) {
              texturedMeshCount += 1;
            }
          });
          if (!disposed) setStatus("Direct tiles ready");
        });
        tiles.addEventListener?.("load-error", (event: any) => {
          if (disposed) return;
          const message = event?.error?.message || "Google tile request failed";
          setLoadError(message);
          setStatus("Renderer error");
        });

        setStatus("Streaming Tokyo Tower photogrammetry…");

        flyToRef.current = (nextRange: number) => {
          const startPosition = camera.position.clone();
          const startTarget = controls.target.clone();
          const end = cameraPosition(THREE, nextRange);
          const started = performance.now();
          const duration = 850;
          cameraTween += 1;
          const tweenId = cameraTween;

          const tick = (now: number) => {
            if (disposed || tweenId !== cameraTween) return;
            const raw = Math.min(1, (now - started) / duration);
            const eased = 1 - Math.pow(1 - raw, 3);
            camera.position.lerpVectors(startPosition, end.position, eased);
            controls.target.lerpVectors(startTarget, end.target, eased);
            camera.updateMatrixWorld();
            if (raw < 1) requestAnimationFrame(tick);
          };
          requestAnimationFrame(tick);
        };

        setLodRef.current = (target: number) => {
          tiles.errorTarget = target;
        };

        const resize = () => {
          if (!renderer || !host.clientWidth || !host.clientHeight) return;
          camera.aspect = host.clientWidth / host.clientHeight;
          camera.updateProjectionMatrix();
          renderer.setPixelRatio(window.devicePixelRatio);
          renderer.setSize(host.clientWidth, host.clientHeight, false);
          tiles.setResolutionFromRenderer(camera, renderer);
        };
        resizeObserver = new ResizeObserver(resize);
        resizeObserver.observe(host);

        const animate = (now: number) => {
          if (disposed) return;
          frameId = requestAnimationFrame(animate);
          controls.update();
          camera.updateMatrixWorld();
          tiles.setResolutionFromRenderer(camera, renderer);
          tiles.setCamera(camera);
          tiles.update();
          renderer.render(scene, camera);

          if (now - lastTelemetry > 500) {
            lastTelemetry = now;
            const raw = tiles.stats ?? {};
            const cacheBytes = Number(tiles.lruCache?.cachedBytes) || 0;
            setStats({
              queued: Number(raw.queued) || 0,
              downloading: Number(raw.downloading) || 0,
              parsing: Number(raw.parsing) || 0,
              loaded: Number(raw.loaded) || 0,
              visible: Number(tiles.visibleTiles?.size) || 0,
              active: Number(tiles.activeTiles?.size) || 0,
              cacheMb: cacheBytes / 1024 / 1024,
              texturedMeshes: texturedMeshCount,
              meshes: meshCount,
            });
            renderAttribution();
          }
        };

        frameId = requestAnimationFrame(animate);
      } catch (error) {
        if (disposed) return;
        const message = error instanceof Error ? error.message : String(error);
        setLoadError(message);
        setStatus("Renderer failed to start");
      }
    })();

    return () => {
      disposed = true;
      cancelAnimationFrame(frameId);
      resizeObserver?.disconnect();
      controls?.dispose?.();
      tiles?.dispose?.();
      renderer?.dispose?.();
      host.replaceChildren();
    };
  }, [apiKey]);

  const chooseRange = (nextRange: number) => {
    setRange(nextRange);
    flyToRef.current(nextRange);
  };

  const chooseLod = (target: number) => {
    setLodTarget(target);
    setLodRef.current(target);
  };

  return (
    <div className="waterford-direct-tiles">
      <div
        ref={canvasHostRef}
        className="waterford-direct-tiles__canvas"
        aria-label="Direct Google Photorealistic 3D Tiles view of Tokyo Tower"
      />

      <header className="waterford-direct-tiles__header">
        <div>
          <span className="waterford-direct-tiles__eyebrow">Renderer A/B lab</span>
          <h1>Tokyo Tower · direct 3D tiles</h1>
          <p>NASA 3DTilesRendererJS + Three.js · no Google Map3DElement</p>
        </div>
        <a
          className="waterford-direct-tiles__compare"
          href="/labs/waterford-direct-tiles"
          target="_blank"
          rel="noreferrer"
        >
          Open Waterford ↗
        </a>
      </header>

      <section
        className="waterford-direct-tiles__controls"
        aria-label="Camera and detail controls"
      >
        <div className="waterford-direct-tiles__control-group">
          <span>Camera distance</span>
          <div className="waterford-direct-tiles__segmented">
            {CAMERA_STOPS.map((stop) => (
              <button
                key={stop.range}
                type="button"
                className={stop.range === range ? "is-active" : ""}
                onClick={() => chooseRange(stop.range)}
              >
                <strong>{stop.label}</strong>
                <small>{stop.detail}</small>
              </button>
            ))}
          </div>
        </div>

        <div className="waterford-direct-tiles__control-group waterford-direct-tiles__lod">
          <span>Screen-space error</span>
          <div className="waterford-direct-tiles__segmented waterford-direct-tiles__segmented--compact">
            {LOD_TARGETS.map((target) => (
              <button
                key={target.value}
                type="button"
                className={target.value === lodTarget ? "is-active" : ""}
                onClick={() => chooseLod(target.value)}
              >
                <strong>{target.value}px</strong>
                <small>{target.label}</small>
              </button>
            ))}
          </div>
        </div>
      </section>

      <aside className="waterford-direct-tiles__telemetry">
        <div>
          <span>{status}</span>
          <strong>
            {currentStop.label} · {lodTarget}px SSE
          </strong>
        </div>
        <dl>
          <div><dt>Visible</dt><dd>{stats.visible}</dd></div>
          <div><dt>Active</dt><dd>{stats.active}</dd></div>
          <div><dt>Loading</dt><dd>{stats.downloading + stats.parsing + stats.queued}</dd></div>
          <div><dt>Cache</dt><dd>{stats.cacheMb.toFixed(0)} MB</dd></div>
          <div><dt>Meshes</dt><dd>{stats.meshes}</dd></div>
          <div><dt>Textured</dt><dd>{stats.texturedMeshes}</dd></div>
        </dl>
      </aside>

      {loadError && (
        <div className="waterford-direct-tiles__error" role="alert">
          <strong>Tokyo Tower could not start</strong>
          <span>{loadError}</span>
          <small>
            The key must have Google Map Tiles API enabled and allow this preview origin.
          </small>
        </div>
      )}

      <footer className="waterford-direct-tiles__footer">
        <span>Tokyo Tower · 35.6586, 139.7454</span>
        <div
          ref={attributionRef}
          className="waterford-direct-tiles__attribution"
          aria-label="Map data attribution"
        />
      </footer>
    </div>
  );
}
