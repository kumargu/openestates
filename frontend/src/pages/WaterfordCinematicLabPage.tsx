import { useEffect, useRef, useState } from "react";

type LabConfig = {
  apiKey: string;
  lat: number;
  lon: number;
  height: number;
  maxDpr: number;
};

type LabWindow = Window & {
  __WATERFORD_LAB_CONFIG__?: LabConfig;
};

export function WaterfordCinematicLabPage() {
  const mapsKey = import.meta.env.VITE_GOOGLE_MAPS_API_KEY?.trim() ?? "";
  const started = useRef(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previousOverflow;
    };
  }, []);

  useEffect(() => {
    if (started.current) return;
    started.current = true;

    const labWindow = window as LabWindow;
    labWindow.__WATERFORD_LAB_CONFIG__ = {
      apiKey: mapsKey,
      lat: 12.98142,
      lon: 77.74156,
      height: 850,
      maxDpr: 1.65,
    };

    const status = document.getElementById("scene-status");
    const detail = document.getElementById("scene-detail");

    async function start() {
      try {
        if (!mapsKey) {
          throw new Error("VITE_GOOGLE_MAPS_API_KEY is not present in this preview build.");
        }

        if (status) status.textContent = "Checking Google 3D Tiles";
        if (detail) detail.textContent = "Verifying Photorealistic 3D Tiles access…";

        const rootUrl = `https://tile.googleapis.com/v1/3dtiles/root.json?key=${encodeURIComponent(mapsKey)}`;
        const response = await fetch(rootUrl, { cache: "no-store" });
        if (!response.ok) {
          let reason = "";
          try {
            reason = (await response.text()).slice(0, 500);
          } catch {
            // Ignore body parsing failures; status is enough to diagnose the API gate.
          }
          throw new Error(
            `Google Photorealistic 3D Tiles API returned HTTP ${response.status}${reason ? `: ${reason}` : ""}`,
          );
        }

        if (status) status.textContent = "Loading Waterford";
        if (detail) detail.textContent = "Google Tiles verified · starting cinematic renderer…";

        const sceneUrl: string = "/labs/waterford-cinematic/scene.js";
        await import(/* @vite-ignore */ sceneUrl);
      } catch (caught) {
        const message = caught instanceof Error ? caught.message : String(caught);
        setError(message);
        if (status) status.textContent = "Waterford could not start";
        if (detail) detail.textContent = message;
      }
    }

    void start();
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
            linear-gradient(180deg, rgba(4,7,8,.26) 0%, transparent 24%, transparent 74%, rgba(4,7,8,.30) 100%),
            radial-gradient(ellipse at center, transparent 54%, rgba(5,8,9,.18) 100%);
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

      <div id="scene-root" aria-label="Interactive photorealistic 3D view of Prestige Waterford" />
      <div className="scene-vignette" aria-hidden="true" />

      <div className="scene-copy">
        <h1 id="scene-status">Preparing Waterford</h1>
        <div id="scene-detail">Starting the cinematic 3D renderer…</div>
      </div>

      <div id="scene-hint">Drag to look · pinch or scroll to move closer</div>
      <div id="scene-attribution">Google Maps</div>

      {error && (
        <div className="scene-error" role="alert">
          <strong>Waterford could not start</strong>
          <p>{error}</p>
        </div>
      )}
    </div>
  );
}
