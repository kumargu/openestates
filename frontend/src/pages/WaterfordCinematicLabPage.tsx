import { useEffect, useMemo } from "react";

function htmlEscapeJson(value: unknown): string {
  return JSON.stringify(value).replace(/</g, "\\u003c");
}

export function WaterfordCinematicLabPage() {
  const mapsKey = import.meta.env.VITE_GOOGLE_MAPS_API_KEY?.trim() ?? "";

  useEffect(() => {
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previousOverflow;
    };
  }, []);

  const srcDoc = useMemo(() => {
    const runtimeConfig = htmlEscapeJson({
      apiKey: mapsKey,
      lat: 12.98142,
      lon: 77.74156,
      height: 850,
      maxDpr: 1.65,
    });

    return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
  <title>Prestige Waterford · Cinematic 3D Lab</title>
  <style>
    :root {
      color-scheme: dark;
      font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: #101518;
    }
    * { box-sizing: border-box; }
    html, body, #scene-root { width: 100%; height: 100%; margin: 0; overflow: hidden; }
    body { background: #101518; color: rgba(255,255,255,.94); }
    #scene-root { position: fixed; inset: 0; }
    #scene-root canvas { display: block; width: 100%; height: 100%; touch-action: none; }

    .scene-vignette {
      position: fixed;
      inset: 0;
      z-index: 2;
      pointer-events: none;
      background:
        linear-gradient(180deg, rgba(4,7,8,.26) 0%, transparent 24%, transparent 74%, rgba(4,7,8,.30) 100%),
        radial-gradient(ellipse at center, transparent 54%, rgba(5,8,9,.18) 100%);
    }

    .scene-copy {
      position: fixed;
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
      transition: opacity .5s ease;
    }
    body.scene-ready #scene-detail { opacity: .72; }

    #scene-hint {
      position: fixed;
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
      position: fixed;
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

    #scene-error {
      position: fixed;
      z-index: 6;
      left: 50%;
      top: 50%;
      transform: translate(-50%, -50%);
      display: none;
      width: min(640px, calc(100vw - 36px));
      padding: 22px;
      border: 1px solid rgba(255,255,255,.14);
      border-radius: 18px;
      background: rgba(13,17,19,.90);
      box-shadow: 0 24px 90px rgba(0,0,0,.32);
      backdrop-filter: blur(18px);
      -webkit-backdrop-filter: blur(18px);
    }
    body.scene-failed #scene-error { display: block; }
    #scene-error strong { display: block; font-size: 16px; }
    #scene-error-message {
      margin-top: 9px;
      font: 12px/1.55 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
      color: rgba(255,255,255,.62);
      overflow-wrap: anywhere;
    }

    @media (max-width: 680px) {
      .scene-copy { left: 16px; top: 16px; }
      #scene-hint { bottom: 26px; }
      #scene-detail { max-width: 76vw; }
      #scene-attribution { max-width: 64vw; font-size: 9px; }
    }

    @media (prefers-reduced-motion: reduce) {
      #scene-hint, #scene-detail { transition: none; }
    }
  </style>
  <script type="importmap">
  {
    "imports": {
      "three": "https://esm.sh/three@0.185.0",
      "three/addons/": "https://esm.sh/three@0.185.0/examples/jsm/",
      "3d-tiles-renderer": "https://esm.sh/3d-tiles-renderer@0.5.2?external=three",
      "3d-tiles-renderer/plugins": "https://esm.sh/3d-tiles-renderer@0.5.2/plugins?external=three"
    }
  }
  </script>
</head>
<body>
  <div id="scene-root" aria-label="Interactive photorealistic 3D view of Prestige Waterford"></div>
  <div class="scene-vignette" aria-hidden="true"></div>

  <div class="scene-copy">
    <h1 id="scene-status">Checking Google 3D Tiles</h1>
    <div id="scene-detail">Verifying the preview key before starting the renderer…</div>
  </div>

  <div id="scene-hint">Drag to look · pinch or scroll to move closer</div>
  <div id="scene-attribution">Google Maps</div>

  <div id="scene-error" role="alert">
    <strong id="scene-error-title">Waterford could not start</strong>
    <div id="scene-error-message"></div>
  </div>

  <script>
    window.__WATERFORD_LAB_CONFIG__ = ${runtimeConfig};

    const statusEl = document.getElementById("scene-status");
    const detailEl = document.getElementById("scene-detail");
    const errorTitleEl = document.getElementById("scene-error-title");
    const errorMessageEl = document.getElementById("scene-error-message");

    function fail(title, error) {
      const message = error instanceof Error ? error.message : String(error || "Unknown error");
      statusEl.textContent = title;
      detailEl.textContent = message;
      errorTitleEl.textContent = title;
      errorMessageEl.textContent = message;
      document.body.classList.add("scene-failed");
    }

    window.addEventListener("error", (event) => {
      if (!document.body.classList.contains("scene-ready")) {
        fail("Renderer error", event.error || event.message);
      }
    });

    window.addEventListener("unhandledrejection", (event) => {
      if (!document.body.classList.contains("scene-ready")) {
        fail("Renderer error", event.reason);
      }
    });

    async function startWaterford() {
      const key = String(window.__WATERFORD_LAB_CONFIG__.apiKey || "").trim();
      if (!key) {
        fail("Google Maps key missing", "VITE_GOOGLE_MAPS_API_KEY is not present in this Vercel preview build.");
        return;
      }

      try {
        const rootUrl = "https://tile.googleapis.com/v1/3dtiles/root.json?key=" + encodeURIComponent(key);
        const response = await fetch(rootUrl, { cache: "no-store" });
        if (!response.ok) {
          let reason = "";
          try { reason = (await response.text()).slice(0, 500); } catch { /* ignore */ }
          throw new Error("Google Photorealistic 3D Tiles API returned HTTP " + response.status + (reason ? ": " + reason : ""));
        }

        statusEl.textContent = "Loading Waterford";
        detailEl.textContent = "Google Tiles API verified · loading Three.js renderer…";

        await import("/labs/waterford-cinematic/scene.js");
      } catch (error) {
        fail("Waterford could not start", error);
      }
    }

    startWaterford();
  </script>
</body>
</html>`;
  }, [mapsKey]);

  return (
    <iframe
      title="Prestige Waterford cinematic 3D lab"
      srcDoc={srcDoc}
      allow="fullscreen"
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 100_000,
        width: "100vw",
        height: "100dvh",
        border: 0,
        background: "#101518",
      }}
    />
  );
}
