import { useEffect, useRef, useState } from "react";

const THREE_URL = "https://esm.sh/three@0.185.0";
const ORBIT_CONTROLS_URL = "https://esm.sh/three@0.185.0/examples/jsm/controls/OrbitControls.js";
const DRACO_LOADER_URL = "https://esm.sh/three@0.185.0/examples/jsm/loaders/DRACOLoader.js";
const KTX2_LOADER_URL = "https://esm.sh/three@0.185.0/examples/jsm/loaders/KTX2Loader.js";
const MESHOPT_URL = "https://esm.sh/three@0.185.0/examples/jsm/libs/meshopt_decoder.module.js";
const TILES_RENDERER_URL = "https://esm.sh/3d-tiles-renderer@0.5.2?external=three";
const TILES_PLUGINS_URL = "https://esm.sh/3d-tiles-renderer@0.5.2/plugins?external=three";
const BASIS_TRANSCODER_PATH = "https://cdn.jsdelivr.net/npm/three@0.185.0/examples/jsm/libs/basis/";

const WATERFORD = {
  lat: 12.9819914,
  lon: 77.7421819,
  // ReorientationPlugin expects WGS84 ellipsoid height. Google ElevationService's
  // ~921 m value is orthometric, so use a conservative local ellipsoid approximation
  // for this rendering lab instead of treating the elevation sample as ellipsoid height.
  ellipsoidHeightM: 850,
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
        throw new Error("VITE_GOOGLE_MAPS_API_KEY is not present in this preview build.");
      }

      setStatus("Checking Google 3D Tiles");
      setDetail("Verifying Photorealistic 3D Tiles access…");

      const rootUrl = `https://tile.googleapis.com/v1/3dtiles/root.json?key=${encodeURIComponent(mapsKey)}`;
      const response = await fetch(rootUrl, { cache: "no-store" });
      if (!response.ok) {
        throw new Error(`Google Photorealistic 3D Tiles API returned HTTP ${response.status}`);
      }

      setStatus("Loading Waterford");
      setDetail("Google Tiles verified · loading geometry + material decoders…");

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
      if (!root) throw new Error("Waterford scene host disappeared before renderer startup.");

      const {
        AmbientLight,
        DirectionalLight,
        MathUtils,
        PerspectiveCamera,
        Scene,
        SRGBColorSpace,
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

      const prefersReducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      const isCoarsePointer = window.matchMedia("(pointer: coarse)").matches;
      const deviceDpr = Math.max(1, window.devicePixelRatio || 1);
      const movingDpr = Math.min(deviceDpr, isCoarsePointer ? 1.6 : 2.0);
      const settledDpr = Math.min(deviceDpr, isCoarsePointer ? 2.25 : 2.5);
      let activeDpr = movingDpr;

      const scene = new Scene();
      const renderer = new WebGLRenderer({
        antialias: true,
        alpha: false,
        powerPreference: "high-performance",
      });
      renderer.outputColorSpace = SRGBColorSpace;
      renderer.setClearColor(0x151c1f, 1);
      renderer.setPixelRatio(activeDpr);
      root.replaceChildren(renderer.domElement);

      // Harmless for Google's unlit photogrammetry materials, but essential if a tile
      // arrives with a lit PBR material. This rules out the white/flat result being a
      // missing-scene-light problem.
      scene.add(new AmbientLight(0xffffff, 2.0));
      const keyLight = new DirectionalLight(0xffffff, 2.2);
      keyLight.position.set(500, 900, 400);
      scene.add(keyLight);

      // Start deliberately close to the NASA reference geometry. We want to prove the
      // material pipeline before doing any authored OpenEstates camera choreography.
      const camera = new PerspectiveCamera(60, 1, 10, 1_600_000);
      camera.position.set(500, 430, 500);

      const controls = new OrbitControls(camera, renderer.domElement);
      controls.target.set(0, 25, 0);
      controls.enableDamping = true;
      controls.dampingFactor = 0.05;
      controls.enablePan = false;
      controls.minDistance = 180;
      controls.maxDistance = 3_000;
      controls.minPolarAngle = 0;
      controls.maxPolarAngle = 3 * Math.PI / 8;
      controls.autoRotate = false;
      controls.autoRotateSpeed = 0.5;
      camera.lookAt(controls.target);
      controls.update();

      const tiles = new TilesRenderer();
      tiles.registerPlugin(new GoogleCloudAuthPlugin({
        apiToken: mapsKey,
        autoRefreshToken: true,
        useRecommendedSettings: true,
      }));
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
          lat: WATERFORD.lat * MathUtils.DEG2RAD,
          lon: WATERFORD.lon * MathUtils.DEG2RAD,
          height: WATERFORD.ellipsoidHeightM,
        }),
      );

      // Keep the diagnostic baseline lossless. We intentionally do NOT use
      // TileCompressionPlugin yet because it disables texture mipmaps and changes the
      // loaded material state. Once this looks right we can benchmark it separately.
      scene.add(tiles.group);
      tiles.setCamera(camera);

      const detailMoving = isCoarsePointer ? 16 : 14;
      const detailSettled = isCoarsePointer ? 8 : 6;
      tiles.errorTarget = detailMoving;

      let qualityTimer = 0;
      let idleResumeTimer = 0;
      let autoStartTimer = 0;
      let firstUsefulFrame = false;
      let lastAttribution = "";
      let lastUiUpdate = 0;
      let modelCount = 0;
      let meshCount = 0;
      let texturedMeshCount = 0;
      let textureSlotCount = 0;
      let failedResources = 0;
      const startedAt = performance.now();
      let raf = 0;

      const inspectModel = ({ scene: modelScene }: any) => {
        modelCount += 1;
        modelScene?.traverse?.((object: any) => {
          if (!object?.isMesh) return;
          meshCount += 1;
          const materials = Array.isArray(object.material) ? object.material : [object.material];
          let meshHasTexture = false;
          for (const material of materials) {
            if (!material) continue;
            for (const key of Object.keys(material)) {
              const value = material[key];
              if (value?.isTexture) {
                textureSlotCount += 1;
                meshHasTexture = true;
              }
            }
          }
          if (meshHasTexture) texturedMeshCount += 1;
        });
      };
      tiles.addEventListener("load-model", inspectModel);

      const previousManagerError = tiles.manager.onError;
      tiles.manager.onError = () => {
        failedResources += 1;
      };

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
        }, 250);
      };

      const scheduleAutoMotionResume = () => {
        window.clearTimeout(idleResumeTimer);
        if (prefersReducedMotion) return;
        idleResumeTimer = window.setTimeout(() => {
          controls.autoRotate = true;
          tiles.errorTarget = Math.max(detailSettled, 8);
        }, 2600);
      };

      const onControlStart = () => {
        controls.autoRotate = false;
        window.clearTimeout(autoStartTimer);
        window.clearTimeout(idleResumeTimer);
        hintRef.current?.classList.add("is-quiet");
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
            attributionRef.current.textContent = next ? `Google Maps · ${next}` : "Google Maps";
          }
        }
      };

      const updateUi = (now: number) => {
        if (now - lastUiUpdate < 650) return;
        lastUiUpdate = now;
        const visible = tiles.visibleTiles?.size ?? 0;
        const stats = tiles.stats ?? {};
        const gpuTextures = renderer.info?.memory?.textures ?? 0;
        const loaded = stats.loaded ?? 0;
        const downloading = stats.downloading ?? 0;
        const parsing = stats.parsing ?? 0;

        if (!firstUsefulFrame) {
          setDetail(
            `Google 3D · visible ${visible} · loaded ${loaded} · downloading ${downloading} · parsing ${parsing}`,
          );
        } else {
          setDetail(
            `models ${modelCount} · meshes ${meshCount} · textured ${texturedMeshCount} · texture slots ${textureSlotCount} · GPU textures ${gpuTextures} · failed ${failedResources}`,
          );
        }
        updateAttribution();
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
        if (!firstUsefulFrame && visibleCount >= 4 && modelCount > 0) {
          firstUsefulFrame = true;
          const elapsed = Math.max(0, performance.now() - startedAt);
          setStatus("Prestige Waterford");
          setDetail(`Photorealistic scene loaded · ${(elapsed / 1000).toFixed(1)}s`);
          document.body.classList.add("scene-ready");
          setSettledQuality();

          if (!prefersReducedMotion) {
            autoStartTimer = window.setTimeout(() => {
              controls.autoRotate = true;
            }, 1300);
          }
        }

        updateUi(now);
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
        tiles.removeEventListener("load-model", inspectModel);
        tiles.manager.onError = previousManagerError;
        controls.dispose();
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
          text-shadow: 0 1px 18px rgba(0,0,0,.75);
          transition: opacity .6s ease;
        }
        body.scene-ready .scene-copy { opacity: .82; }
        #scene-status {
          margin: 0;
          font-size: clamp(18px, 2vw, 26px);
          font-weight: 600;
          letter-spacing: -.025em;
        }
        #scene-detail {
          margin-top: 7px;
          max-width: min(900px, 90vw);
          font: 11px/1.5 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
          color: rgba(255,255,255,.64);
        }
        #scene-hint {
          position: absolute;
          z-index: 3;
          left: 50%;
          bottom: max(30px, calc(env(safe-area-inset-bottom) + 20px));
          transform: translateX(-50%);
          white-space: nowrap;
          padding: 9px 13px;
          border: 1px solid rgba(255,255,255,.15);
          border-radius: 999px;
          background: rgba(12,16,18,.42);
          backdrop-filter: blur(10px);
          -webkit-backdrop-filter: blur(10px);
          font-size: 11px;
          color: rgba(255,255,255,.86);
          pointer-events: none;
          opacity: 0;
          transition: opacity .6s ease;
        }
        body.scene-ready #scene-hint { opacity: 1; }
        #scene-hint.is-quiet { opacity: .2; }
        #scene-attribution {
          position: absolute;
          z-index: 4;
          right: max(10px, env(safe-area-inset-right));
          bottom: max(6px, env(safe-area-inset-bottom));
          max-width: min(72vw, 900px);
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
          padding: 3px 6px;
          border-radius: 4px;
          background: rgba(255,255,255,.84);
          color: rgba(0,0,0,.84);
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
          background: rgba(13,17,19,.94);
          box-shadow: 0 24px 90px rgba(0,0,0,.32);
        }
        .scene-error strong { display: block; font-size: 16px; }
        .scene-error p {
          margin: 9px 0 0;
          font: 12px/1.55 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
          color: rgba(255,255,255,.65);
          overflow-wrap: anywhere;
        }
        @media (max-width: 680px) {
          .scene-copy { left: 16px; top: 16px; right: 16px; }
          #scene-detail { max-width: 92vw; font-size: 10px; }
          #scene-attribution { max-width: 68vw; font-size: 9px; }
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
