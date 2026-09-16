import { useEffect, useRef, useState } from "react";

const THREE_URL = "https://esm.sh/three@0.185.0";
const ORBIT_CONTROLS_URL = "https://esm.sh/three@0.185.0/examples/jsm/controls/OrbitControls.js";
const DRACO_LOADER_URL = "https://esm.sh/three@0.185.0/examples/jsm/loaders/DRACOLoader.js";
const TILES_RENDERER_URL = "https://esm.sh/3d-tiles-renderer@0.5.2?external=three";
const TILES_PLUGINS_URL = "https://esm.sh/3d-tiles-renderer@0.5.2/plugins?external=three";

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
      setDetail("Google Tiles verified · loading Three.js + 3DTilesRendererJS…");

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
        SRGBColorSpace,
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
      const pixelRatio = Math.min(window.devicePixelRatio || 1, isCoarsePointer ? 1.35 : 1.65);

      const scene = new Scene();
      const renderer = new WebGLRenderer({
        antialias: true,
        alpha: false,
        powerPreference: "high-performance",
      });
      renderer.outputColorSpace = SRGBColorSpace;
      renderer.setClearColor(0x101518, 1);
      renderer.setPixelRatio(pixelRatio);
      root.replaceChildren(renderer.domElement);

      // Tighter than the NASA reference: Waterford should read as the subject, not as
      // one object in an infinite globe.
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
        lat: 12.98142 * MathUtils.DEG2RAD,
        lon: 77.74156 * MathUtils.DEG2RAD,
        height: 850,
      }));

      scene.add(tiles.group);
      tiles.setCamera(camera);

      const detailMoving = isCoarsePointer ? 20 : 17;
      const detailSettled = isCoarsePointer ? 15 : 11;
      let qualityTimer = 0;
      let idleResumeTimer = 0;
      let firstUsefulFrame = false;
      let lastAttribution = "";
      let lastAttributionUpdate = 0;
      const startedAt = performance.now();
      let raf = 0;

      const setMovingQuality = () => {
        window.clearTimeout(qualityTimer);
        tiles.errorTarget = detailMoving;
      };

      const setSettledQuality = () => {
        window.clearTimeout(qualityTimer);
        qualityTimer = window.setTimeout(() => {
          tiles.errorTarget = detailSettled;
        }, 180);
      };

      const scheduleAutoMotionResume = () => {
        window.clearTimeout(idleResumeTimer);
        if (prefersReducedMotion) return;
        idleResumeTimer = window.setTimeout(() => {
          controls.autoRotate = true;
          tiles.errorTarget = isCoarsePointer ? 18 : 14;
        }, 4200);
      };

      const onControlStart = () => {
        controls.autoRotate = false;
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

      const frame = (now: number) => {
        if (cancelled) return;
        raf = requestAnimationFrame(frame);

        controls.update();
        tiles.setResolutionFromRenderer(camera, renderer);
        tiles.setCamera(camera);
        camera.updateMatrixWorld();
        tiles.update();
        renderer.render(scene, camera);

        if (!firstUsefulFrame && (tiles.visibleTiles?.size ?? 0) >= 4) {
          firstUsefulFrame = true;
          const elapsed = Math.max(0, performance.now() - startedAt);
          setStatus("Prestige Waterford");
          setDetail(`Live photorealistic 3D · first scene ${(elapsed / 1000).toFixed(1)}s`);
          document.body.classList.add("scene-ready");
          setSettledQuality();
        }

        updateAttribution(now);
      };

      raf = requestAnimationFrame(frame);

      return () => {
        cancelAnimationFrame(raf);
        window.clearTimeout(qualityTimer);
        window.clearTimeout(idleResumeTimer);
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
        body { background: #101518; }
        .waterford-cinematic-lab {
          position: fixed;
          inset: 0;
          z-index: 100000;
          overflow: hidden;
          background: #101518;
          color: rgba(255,255,255,.94);
          font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        }
        #scene-root { position: absolute; inset: 0; }
        #scene-root canvas { display: block; width: 100%; height: 100%; touch-action: none; }
        .scene-vignette {
          position: absolute;
          inset: 0;
          z-index: 2;
          pointer-events: none;
          background:
            linear-gradient(180deg, rgba(4,7,8,.20) 0%, transparent 22%, transparent 76%, rgba(4,7,8,.24) 100%),
            radial-gradient(ellipse at center, transparent 60%, rgba(5,8,9,.12) 100%);
        }
        .scene-copy {
          position: absolute;
          z-index: 3;
          left: max(22px, env(safe-area-inset-left));
          top: max(20px, env(safe-area-inset-top));
          pointer-events: none;
          text-shadow: 0 1px 18px rgba(0,0,0,.72);
        }
        #scene-status {
          margin: 0;
          font-size: clamp(18px, 2vw, 26px);
          font-weight: 560;
          letter-spacing: -.025em;
        }
        #scene-detail {
          margin-top: 6px;
          max-width: min(760px, 82vw);
          font-size: 12px;
          line-height: 1.45;
          letter-spacing: .02em;
          color: rgba(255,255,255,.66);
        }
        #scene-hint {
          position: absolute;
          z-index: 3;
          left: 50%;
          bottom: max(26px, calc(env(safe-area-inset-bottom) + 18px));
          transform: translateX(-50%);
          white-space: nowrap;
          padding: 8px 12px;
          border: 1px solid rgba(255,255,255,.12);
          border-radius: 999px;
          background: rgba(12,16,18,.45);
          backdrop-filter: blur(12px);
          -webkit-backdrop-filter: blur(12px);
          font-size: 11px;
          letter-spacing: .02em;
          color: rgba(255,255,255,.76);
          pointer-events: none;
          opacity: 0;
          transition: opacity .8s ease;
        }
        body.scene-ready #scene-hint { opacity: 1; }
        #scene-hint.is-quiet { opacity: .22; }
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
          #scene-detail { max-width: 76vw; }
          #scene-attribution { max-width: 64vw; font-size: 9px; }
        }
      `}</style>

      <div ref={sceneRootRef} id="scene-root" aria-label="Interactive photorealistic 3D view of Prestige Waterford" />
      <div className="scene-vignette" aria-hidden="true" />

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
