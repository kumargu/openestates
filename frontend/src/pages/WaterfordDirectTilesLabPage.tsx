import { useEffect, useMemo, useRef, useState } from "react";
import "../styles/waterford-direct-tiles.css";

const WATERFORD = {
  name: "Prestige Waterford",
  latitude: 12.9819914,
  longitude: 77.7421819,
};

const MODULES = {
  three: "https://esm.sh/three@0.186.0",
  orbit: "https://esm.sh/three@0.186.0/examples/jsm/controls/OrbitControls.js",
  draco: "https://esm.sh/three@0.186.0/examples/jsm/loaders/DRACOLoader.js",
  tiles: "https://esm.sh/3d-tiles-renderer@0.5.3?deps=three@0.186.0",
  plugins: "https://esm.sh/3d-tiles-renderer@0.5.3/plugins?deps=three@0.186.0",
};

const CAMERA_STOPS = [
  { range: 1200, label: "1,200 m", detail: "Neighbourhood" },
  { range: 700, label: "700 m", detail: "Society context" },
  { range: 500, label: "500 m", detail: "NASA-like" },
  { range: 350, label: "350 m", detail: "Inspection" },
  { range: 200, label: "200 m", detail: "Stress test" },
] as const;

const LOD_TARGETS = [
  { value: 8, label: "Max detail" },
  { value: 20, label: "NASA default" },
  { value: 35, label: "Fast" },
] as const;

type RuntimeStats = {
  queued: number;
  downloading: number;
  parsing: number;
  loaded: number;
  visible: number;
  active: number;
  cacheMb: number;
};

const EMPTY_STATS: RuntimeStats = {
  queued: 0,
  downloading: 0,
  parsing: 0,
  loaded: 0,
  visible: 0,
  active: 0,
  cacheMb: 0,
};

function cameraPosition(THREE: any, range: number) {
  const heading = THREE.MathUtils.degToRad(225);
  const elevation = THREE.MathUtils.degToRad(32);
  const horizontal = range * Math.cos(elevation);
  const vertical = range * Math.sin(elevation);
  const targetY = 30;

  return {
    position: new THREE.Vector3(
      Math.sin(heading) * horizontal,
      targetY + vertical,
      Math.cos(heading) * horizontal,
    ),
    target: new THREE.Vector3(0, targetY, 0),
  };
}

export function WaterfordDirectTilesLabPage() {
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
        setStatus("Loading NASA 3D Tiles renderer…");

        const [THREE, controlsModule, dracoModule, tilesModule, pluginsModule] = await Promise.all([
          import(/* @vite-ignore */ MODULES.three),
          import(/* @vite-ignore */ MODULES.orbit),
          import(/* @vite-ignore */ MODULES.draco),
          import(/* @vite-ignore */ MODULES.tiles),
          import(/* @vite-ignore */ MODULES.plugins),
        ]);
        if (disposed) return;

        const { OrbitControls } = controlsModule;
        const { DRACOLoader } = dracoModule;
        const { TilesRenderer } = tilesModule;
        const {
          GoogleCloudAuthPlugin,
          TileCompressionPlugin,
          TilesFadePlugin,
          GLTFExtensionsPlugin,
          ReorientationPlugin,
        } = pluginsModule;

        const scene = new THREE.Scene();
        scene.background = new THREE.Color(0x11181c);

        renderer = new THREE.WebGLRenderer({
          antialias: true,
          powerPreference: "high-performance",
        });
        renderer.setPixelRatio(window.devicePixelRatio);
        renderer.setSize(host.clientWidth, host.clientHeight, false);
        host.replaceChildren(renderer.domElement);

        const camera = new THREE.PerspectiveCamera(
          60,
          host.clientWidth / Math.max(1, host.clientHeight),
          1,
          1_600_000,
        );

        controls = new OrbitControls(camera, renderer.domElement);
        controls.enableDamping = true;
        controls.dampingFactor = 0.08;
        controls.enablePan = false;
        controls.minDistance = 150;
        controls.maxDistance = 2_500;
        controls.maxPolarAngle = THREE.MathUtils.degToRad(72);

        const initial = cameraPosition(THREE, 500);
        camera.position.copy(initial.position);
        controls.target.copy(initial.target);
        controls.update();

        tiles = new TilesRenderer();
        tiles.registerPlugin(new GoogleCloudAuthPlugin({
          apiToken: apiKey,
          autoRefreshToken: true,
          logoUrl: "https://www.gstatic.com/images/branding/googlelogo/2x/googlelogo_color_92x30dp.png",
          useRecommendedSettings: true,
        }));
        tiles.registerPlugin(new TileCompressionPlugin());
        tiles.registerPlugin(new TilesFadePlugin());

        const dracoLoader = new DRACOLoader();
        dracoLoader.setDecoderPath("https://www.gstatic.com/draco/versioned/decoders/1.5.7/");
        tiles.registerPlugin(new GLTFExtensionsPlugin({ dracoLoader }));

        const reorientation = new ReorientationPlugin({
          lat: THREE.MathUtils.degToRad(WATERFORD.latitude),
          lon: THREE.MathUtils.degToRad(WATERFORD.longitude),
        });
        tiles.registerPlugin(reorientation);

        tiles.errorTarget = 8;
        tiles.loadSiblings = true;
        tiles.loadAncestors = true;
        if (tiles.lruCache && "maxBytesSize" in tiles.lruCache) {
          tiles.lruCache.maxBytesSize = Math.max(
            Number(tiles.lruCache.maxBytesSize) || 0,
            512 * 1024 * 1024,
          );
        }

        scene.add(tiles.group);
        tiles.setResolutionFromRenderer(camera, renderer);
        tiles.setCamera(camera);

        tiles.addEventListener?.("load-model", () => {
          if (!disposed) setStatus("Direct tiles ready");
        });
        tiles.addEventListener?.("load-error", (event: any) => {
          if (disposed) return;
          const message = event?.error?.message || "Google tile request failed";
          setLoadError(message);
          setStatus("Renderer error");
        });

        setStatus("Streaming Waterford photogrammetry…");

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
      <div ref={canvasHostRef} className="waterford-direct-tiles__canvas" aria-label="Direct Google Photorealistic 3D Tiles view of Prestige Waterford" />

      <header className="waterford-direct-tiles__header">
        <div>
          <span className="waterford-direct-tiles__eyebrow">Renderer A/B lab</span>
          <h1>Waterford · direct 3D tiles</h1>
          <p>NASA 3DTilesRendererJS + Three.js · no Google Map3DElement</p>
        </div>
        <a
          className="waterford-direct-tiles__compare"
          href="/property/fixture-prestige-waterford-3bhk?fixture=atlas"
          target="_blank"
          rel="noreferrer"
        >
          Open current Home ↗
        </a>
      </header>

      <section className="waterford-direct-tiles__controls" aria-label="Camera and detail controls">
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
          <strong>{currentStop.label} · {lodTarget}px SSE</strong>
        </div>
        <dl>
          <div><dt>Visible</dt><dd>{stats.visible}</dd></div>
          <div><dt>Active</dt><dd>{stats.active}</dd></div>
          <div><dt>Loading</dt><dd>{stats.downloading + stats.parsing + stats.queued}</dd></div>
          <div><dt>Cache</dt><dd>{stats.cacheMb.toFixed(0)} MB</dd></div>
        </dl>
      </aside>

      {loadError && (
        <div className="waterford-direct-tiles__error" role="alert">
          <strong>Waterford could not start</strong>
          <span>{loadError}</span>
          <small>
            The key must have Google Map Tiles API enabled and allow this preview origin.
          </small>
        </div>
      )}

      <footer className="waterford-direct-tiles__footer">
        <span>Prestige Waterford · 12.9819914, 77.7421819</span>
        <div ref={attributionRef} className="waterford-direct-tiles__attribution" aria-label="Map data attribution" />
      </footer>
    </div>
  );
}
