import { useEffect, useRef, useState } from "react";

const THREE_URL = "https://esm.sh/three@0.185.0";
const ORBIT_CONTROLS_URL = "https://esm.sh/three@0.185.0/examples/jsm/controls/OrbitControls.js";
const DRACO_LOADER_URL = "https://esm.sh/three@0.185.0/examples/jsm/loaders/DRACOLoader.js";
const TILES_RENDERER_URL = "https://esm.sh/3d-tiles-renderer@0.5.2?external=three";
const TILES_PLUGINS_URL = "https://esm.sh/3d-tiles-renderer@0.5.2/plugins?external=three";

const WATERFORD = {
  // Existing OpenEstates Atlas fixture point + Google ElevationService sample.
  lat: 12.9819914,
  lon: 77.7421819,
  elevationM: 921.2407,
};

function runtimeImport(url: string): Promise<any> {
  return import(/* @vite-ignore */ url);
}

export function WaterfordCinematicLabPage() {
  const mapsKey = import.meta.env.VITE_GOOGLE_MAPS_API_KEY?.trim() ?? "";
  const sceneRootRef = useRef<HTMLDivElement | null>(null);
  const attributionRef = useRef<HTMLDivElement | null>(null);
  const hintRef = useRef<HTMLDivElement | null>(null);
  const started = useRef(false);
  const [status, setStatus] = useState("Preparing Waterford");
  const [detail, setDetail] = useState("Starting the cinematic 3D renderer…");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previousOverflow;
      document.body.classList.remove("scene-ready");
    };
  }, []);

  useEffect(() => {
    if (started.current) return;
    started.current = true;

    let cancelled = false;
    let teardown: (() => void) | undefined;

    async function start() {
      if (!mapsKey) {
        throw new Error("VITE_GOOGLE_MAPS_API_KEY is not present in this Vercel preview build.");
      }

      setStatus("Checking Google 3D Tiles");
      setDetail("Verifying Photorealistic 3D Tiles access…");

      const rootUrl = `https://tile.googleapis.com/v1/3dtiles/root.json?key=${encodeURIComponent(mapsKey)}`;
      const response = await fetch(rootUrl, { cache: "no-store" });
      if (!response.ok) {
        let reason = "";
        try {
          reason = (await response.text()).slice(0, 500);
        } catch {
          // Status alone is enough to diagnose the API gate.
        }
        throw new Error(
          `Google Photorealistic 3D Tiles API returned HTTP ${response.status}${reason ? `: ${reason}` : ""}`,
        );
      }

      setStatus("Loading Waterford");
      setDetail("Google Tiles verified · loading the cinematic renderer…");

      const [THREE, orbitModule, dracoModule, tilesModule, pluginsModule] = await Promise.all([
        runtimeImport(THREE_URL),
        runtimeImport(ORBIT_CONTROLS_URL),
        runtimeImport(DRACO_LOADER_URL),
        runtimeImport(TILES_RENDERER_URL),
        runtimeImport(TILES_PLUGINS_URL),
      ]);

      if (cancelled) return undefined;

      const root = sceneRootRef.current;
      if (!root) throw new Error("Waterford scene host disappeared before renderer startup.");

      const {
        MathUtils,
        PerspectiveCamera,
        Scene,
        WebGLRenderer,
      } = THREE;
      const { OrbitControls } = orbitModule;
      const { DRACOLoader } = dracoModule;
      const { TilesRenderer } = tilesModule;
      const {
        GLTFExtensionsPlugin,
        GoogleCloudAuthPlugin,
        ReorientationPlugin,
        TileCompressionPlugin,
        TilesFadePlugin,
      } = pluginsModule;

      const prefersReducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      const isCoarsePointer = window.matchMedia("(pointer: coarse)").matches;
      const deviceDpr = Math.max(1, window.devicePixelRatio || 1);

      // NASA's reference uses devicePixelRatio directly. The old lab capped mobile at
      // 1.35, which made photogrammetry look soft on high-density phones. Keep motion
      // affordable, but let a settled scene become genuinely sharp.
      const movingDpr = Math.min(deviceDpr, isCoarsePointer ? 1.75 : 2.0);
      const settledDpr = Math.min(deviceDpr, isCoarsePointer ? 2.5 : 2.25);
      let activeDpr = movingDpr;

      const scene = new Scene();
      const renderer = new WebGLRenderer({
        antialias: true,
        alpha: false,
        powerPreference: "high-performance",
      });
      renderer.setClearColor(0x151c1f, 1);
      renderer.setPixelRatio(activeDpr);
      root.replaceChildren(renderer.domElement);

      // Match the NASA reference lens. Waterford is much shorter than Tokyo Tower, so
      // the camera is adapted to the site's ~16 acre footprint rather than copied by
      // distance alone.
      const camera = new PerspectiveCamera(60, 1, 30, 1_600_000);

      const controls = new OrbitControls(camera, renderer.domElement);
      controls.target.set(0, 42, 0);
      controls.enableDamping = true;
      controls.enablePan = false;
      controls.minDistance = 190;
      controls.maxDistance = 1_600;
      controls.minPolarAngle = 0;
      controls.maxPolarAngle = 3 * Math.PI / 8;
      controls.autoRotate = false;
      controls.autoRotateSpeed = 0.5;

      const tiles = new TilesRenderer();
      tiles.registerPlugin(new GoogleCloudAuthPlugin({
        apiToken: mapsKey,
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
        height: WATERFORD.elevationM,
      }));

      scene.add(tiles.group);
      tiles.setCamera(camera);

      const detailMoving = isCoarsePointer ? 16 : 14;
      const detailSettled = isCoarsePointer ? 9 : 7;
      tiles.errorTarget = detailMoving;

      let qualityTimer = 0;
      let idleResumeTimer = 0;
      let autoStartTimer = 0;
      let firstUsefulFrame = false;
      let lastAttribution = "";
      let lastAttributionUpdate = 0;
      let lastProgressUpdate = 0;
      let initialCameraPlaced = false;
      const startedAt = performance.now();
      let raf = 0;

      const resize = () => {
        const width = Math.max(1, root.clientWidth);
        const height = Math.max(1, root.clientHeight);
        const aspect = width / height;
        camera.aspect = aspect;
        camera.updateProjectionMatrix();
        renderer.setSize(width, height, false);
        tiles.setResolutionFromRenderer(camera, renderer);

        if (!initialCameraPlaced) {
          // Portrait needs extra range because horizontal FOV is much narrower. This
          // still keeps the society dominant instead of showing a generic city view.
          if (aspect < 0.72) {
            camera.position.set(430, 330, 430);
          } else if (aspect < 1.1) {
            camera.position.set(390, 310, 390);
          } else {
            camera.position.set(340, 285, 340);
          }
          camera.lookAt(controls.target);
          controls.update();
          initialCameraPlaced = true;
        }
      };

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
        }, 240);
      };

      const scheduleAutoMotionResume = () => {
        window.clearTimeout(idleResumeTimer);
        if (prefersReducedMotion) return;
        idleResumeTimer = window.setTimeout(() => {
          controls.autoRotate = true;
          // Preserve the sharp settled scene during the slow cinematic orbit.
          tiles.errorTarget = Math.max(detailSettled, 9);
        }, 2600);
      };

      const onControlStart = () => {
        controls.autoRotate = false;
        window.clearTimeout(autoStartTimer);
        setMovingQuality();
        window.clearTimeout(idleResumeTimer);
        hintRef.current?.classList.add("is-quiet");
      };

      const onControlEnd = () => {
        setSettledQuality();
        scheduleAutoMotionResume();
      };

      controls.addEventListener("start", onControlStart);
      controls.addEventListener("end", onControlEnd);

      const resizeObserver = new ResizeObserver(resize);
      resizeObserver.observe(root);
      resize();

      const updateAttribution = (now: number) => {
        if (now - lastAttributionUpdate < 500) return;
        lastAttributionUpdate = now;
        const values = (tiles.getAttributions?.() ?? [])
          .filter((entry: any) => entry && entry.type === "string" && entry.value)
          .map((entry: any) => String(entry.value).trim())
          .filter(Boolean);
        const next = values.join(" · ");
        if (next !== lastAttribution) {
          lastAttribution = next;
          if (attributionRef.current) {
            attributionRef.current.textContent = next ? `Google Maps · ${next}` : "Google Maps";
          }
        }
      };

      const updateLoadingProgress = (now: number) => {
        if (firstUsefulFrame || now - lastProgressUpdate < 700) return;
        lastProgressUpdate = now;
        const stats = tiles.stats ?? {};
        const visible = tiles.visibleTiles?.size ?? 0;
        const loaded = stats.loaded ?? 0;
        const downloading = stats.downloading ?? 0;
        const parsing = stats.parsing ?? 0;
        setDetail(`Google 3D · visible ${visible} · loaded ${loaded} · downloading ${downloading} · parsing ${parsing}`);
      };

      const frame = (now: number) => {
        if (cancelled) return;
        raf = requestAnimationFrame(frame);

        controls.update();
        tiles.setResolutionFromRenderer(camera, renderer);
        tiles.setCamera(camera);
        camera.updateMatrixWorld();
        tiles.update();
        renderer.render(scene, camera);

        const visibleCount = tiles.visibleTiles?.size ?? 0;
        if (!firstUsefulFrame && visibleCount >= 4) {
          firstUsefulFrame = true;
          const elapsed = Math.max(0, performance.now() - startedAt);
          setStatus("Prestige Waterford");
          setDetail(`High-resolution Google 3D · ${visibleCount} tiles · first view ${(elapsed / 1000).toFixed(1)}s`);
          document.body.classList.add("scene-ready");
          setSettledQuality();

          if (!prefersReducedMotion) {
            autoStartTimer = window.setTimeout(() => {
              controls.autoRotate = true;
            }, 900);
          }
        }

        updateLoadingProgress(now);
        updateAttribution(now);
      };

      raf = requestAnimationFrame(frame);

      return () => {
        cancelAnimationFrame(raf);
        window.clearTimeout(qualityTimer);
        window.clearTimeout(idleResumeTimer);
        window.clearTimeout(autoStartTimer);
        resizeObserver.disconnect();
        controls.removeEventListener("start", onControlStart);
        controls.removeEventListener("end", onControlEnd);
        controls.dispose();
        dracoLoader.dispose();
        tiles.dispose();
        renderer.dispose();
        root.replaceChildren();
        document.body.classList.remove("scene-ready");
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
        setStatus("Waterford could not start");
        setDetail(message);
      });

    return () => {
      cancelled = true;
      teardown?.();
    };
  }, [mapsKey]);

  return (
    <div className="waterford-cinematic-lab">
      <style>{`
        body { background: #151c1f; }
        .waterford-cinematic-lab {
          position: fixed;
          inset: 0;
          z-index: 100000;
          overflow: hidden;
          background: #151c1f;
          color: rgba(255,255,255,.94);
          font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        }
        #scene-root { position: absolute; inset: 0; }
        #scene-root canvas { display: block; width: 100%; height: 100%; touch-action: none; }
        .scene-copy {
          position: absolute;
          z-index: 3;
          left: max(22px, env(safe-area-inset-left));
          top: max(20px, env(safe-area-inset-top));
          pointer-events: none;
          text-shadow: 0 1px 18px rgba(0,0,0,.72);
          transition: opacity .8s ease;
        }
        body.scene-ready .scene-copy { opacity: .72; }
        #scene-status {
          margin: 0;
          font-size: clamp(18px, 2vw, 26px);
          font-weight: 560;
          letter-spacing: -.025em;
        }
        #scene-detail {
          margin-top: 6px;
          max-width: min(760px, 82vw);
          font-size: 11px;
          line-height: 1.45;
          letter-spacing: .02em;
          color: rgba(255,255,255,.62);
        }
        #scene-hint {
          position: absolute;
          z-index: 3;
          left: 50%;
          bottom: max(26px, calc(env(safe-area-inset-bottom) + 18px));
          transform: translateX(-50%);
          white-space: nowrap;
          padding: 8px 12px;
          border: 1px solid rgba(255,255,255,.14);
          border-radius: 999px;
          background: rgba(12,16,18,.38);
          backdrop-filter: blur(10px);
          -webkit-backdrop-filter: blur(10px);
          font-size: 11px;
          letter-spacing: .02em;
          color: rgba(255,255,255,.82);
          pointer-events: none;
          opacity: 0;
          transition: opacity .8s ease;
        }
        body.scene-ready #scene-hint { opacity: 1; }
        #scene-hint.is-quiet { opacity: .18; }
        #scene-attribution {
          position: absolute;
          z-index: 4;
          right: max(10px, env(safe-area-inset-right));
          bottom: max(6px, env(safe-area-inset-bottom));
          max-width: min(70vw, 860px);
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
          padding: 3px 6px;
          border-radius: 4px;
          background: rgba(255,255,255,.82);
          color: rgba(0,0,0,.82);
          font: 10px/1.2 Arial, sans-serif;
          pointer-events: none;
        }
        .scene-error {
          position: absolute;
          z-index: 6;
          left: 50%;
          top: 50%;
          transform: translate(-50%, -50%);
          width: min(640px, calc(100vw - 36px));
          padding: 22px;
          border: 1px solid rgba(255,255,255,.14);
          border-radius: 18px;
          background: rgba(13,17,19,.92);
          box-shadow: 0 24px 90px rgba(0,0,0,.32);
          backdrop-filter: blur(18px);
          -webkit-backdrop-filter: blur(18px);
        }
        .scene-error strong { display: block; font-size: 16px; }
        .scene-error p {
          margin: 9px 0 0;
          font: 12px/1.55 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
          color: rgba(255,255,255,.62);
          overflow-wrap: anywhere;
        }
        @media (max-width: 680px) {
          .scene-copy { left: 16px; top: 16px; }
          #scene-detail { max-width: 84vw; }
          #scene-attribution { max-width: 64vw; font-size: 9px; }
        }
      `}</style>

      <div ref={sceneRootRef} id="scene-root" aria-label="Interactive photorealistic 3D view of Prestige Waterford" />

      <div className="scene-copy">
        <h1 id="scene-status">{status}</h1>
        <div id="scene-detail">{detail}</div>
      </div>

      <div ref={hintRef} id="scene-hint">Drag to look · pinch or scroll to move closer</div>
      <div ref={attributionRef} id="scene-attribution">Google Maps</div>

      {error && (
        <div className="scene-error" role="alert">
          <strong>Waterford could not start</strong>
          <p>{error}</p>
        </div>
      )}
    </div>
  );
}
