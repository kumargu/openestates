type MapsLibraryImporter = (
  library: "core" | "maps" | "maps3d" | "elevation" | "marker" | "streetView",
) => Promise<unknown>;

type ElevationLibrary = {
  ElevationService: new () => {
    getElevationForLocations: (request: {
      locations: Array<{ lat: number; lng: number }>;
    }) => Promise<{ results?: Array<{ elevation?: number }> }>;
  };
};

type GoogleMapsWindow = Window & {
  __openestatesGoogleMaps3dReady?: () => void;
  google?: {
    maps?: {
      importLibrary?: MapsLibraryImporter;
    };
  };
};

const GOOGLE_MAPS_CALLBACK = "__openestatesGoogleMaps3dReady";
const GOOGLE_MAPS_LOAD_TIMEOUT_MS = 12_000;

let googleMaps3dPromise: Promise<unknown> | null = null;
let googleMaps2dPromise: Promise<unknown> | null = null;
let googleMarkerPromise: Promise<unknown> | null = null;
let googleStreetViewPromise: Promise<unknown> | null = null;
const terrainElevationPromises = new Map<string, Promise<number>>();

export function googleMaps3dApiKey(): string | null {
  const key = import.meta.env.VITE_GOOGLE_MAPS_API_KEY?.trim();
  return key || null;
}

export function loadGoogleMaps3dLibrary(): Promise<unknown> {
  const key = googleMaps3dApiKey();
  if (!key) return Promise.reject(new Error("google_maps_3d_not_configured"));
  if (googleMaps3dPromise) return googleMaps3dPromise;

  googleMaps3dPromise = new Promise<void>((resolve, reject) => {
    const browserWindow = window as GoogleMapsWindow;
    if (browserWindow.google?.maps?.importLibrary) {
      resolve();
      return;
    }

    const existing = document.querySelector<HTMLScriptElement>(
      'script[data-openestates-google-maps="true"]',
    );
    existing?.remove();

    const script = document.createElement("script");
    const timeoutId = window.setTimeout(() => {
      delete browserWindow.__openestatesGoogleMaps3dReady;
      script.remove();
      reject(new Error("google_maps_3d_load_timeout"));
    }, GOOGLE_MAPS_LOAD_TIMEOUT_MS);
    const finish = (callback: () => void) => {
      window.clearTimeout(timeoutId);
      delete browserWindow.__openestatesGoogleMaps3dReady;
      callback();
    };

    browserWindow.__openestatesGoogleMaps3dReady = () => finish(resolve);
    script.async = true;
    script.dataset.openestatesGoogleMaps = "true";
    script.src = `https://maps.googleapis.com/maps/api/js?key=${encodeURIComponent(key)}&loading=async&v=weekly&libraries=maps3d&callback=${GOOGLE_MAPS_CALLBACK}`;
    script.addEventListener(
      "error",
      () => finish(() => reject(new Error("google_maps_3d_load_failed"))),
      { once: true },
    );
    document.head.append(script);
  }).then(() => {
    const importer = (window as GoogleMapsWindow).google?.maps?.importLibrary;
    if (!importer) throw new Error("google_maps_3d_import_unavailable");
    return importer("maps3d");
  });

  return googleMaps3dPromise;
}

export function loadGoogleStreetViewLibrary(): Promise<unknown> {
  if (googleStreetViewPromise) return googleStreetViewPromise;
  googleStreetViewPromise = loadGoogleMaps3dLibrary().then(() => {
    const importer = (window as GoogleMapsWindow).google?.maps?.importLibrary;
    if (!importer) throw new Error("google_maps_street_view_import_unavailable");
    return importer("streetView");
  }).catch((error: unknown) => {
    googleStreetViewPromise = null;
    throw error;
  });
  return googleStreetViewPromise;
}

export function loadGoogleMaps2dLibrary(): Promise<unknown> {
  if (googleMaps2dPromise) return googleMaps2dPromise;
  googleMaps2dPromise = loadGoogleMaps3dLibrary().then(() => {
    const importer = (window as GoogleMapsWindow).google?.maps?.importLibrary;
    if (!importer) throw new Error("google_maps_2d_import_unavailable");
    return Promise.all([importer("maps"), importer("core")]).then(
      ([mapsLibrary, coreLibrary]) => ({
        ...(mapsLibrary as Record<string, unknown>),
        ...(coreLibrary as Record<string, unknown>),
      }),
    );
  }).catch((error: unknown) => {
    googleMaps2dPromise = null;
    throw error;
  });
  return googleMaps2dPromise;
}

export function loadGoogleMarkerLibrary(): Promise<unknown> {
  if (googleMarkerPromise) return googleMarkerPromise;
  googleMarkerPromise = loadGoogleMaps3dLibrary().then(() => {
    const importer = (window as GoogleMapsWindow).google?.maps?.importLibrary;
    if (!importer) throw new Error("google_maps_marker_import_unavailable");
    return importer("marker");
  }).catch((error: unknown) => {
    googleMarkerPromise = null;
    throw error;
  });
  return googleMarkerPromise;
}

export function loadGoogleTerrainElevation(
  latitude: number,
  longitude: number,
): Promise<number> {
  const cacheKey = `${latitude.toFixed(6)},${longitude.toFixed(6)}`;
  const cached = terrainElevationPromises.get(cacheKey);
  if (cached) return cached;

  const promise = loadGoogleMaps3dLibrary().then(async () => {
    const importer = (window as GoogleMapsWindow).google?.maps?.importLibrary;
    if (!importer) throw new Error("google_maps_elevation_import_unavailable");
    const library = await importer("elevation") as ElevationLibrary;
    const response = await new library.ElevationService().getElevationForLocations({
      locations: [{ lat: latitude, lng: longitude }],
    });
    const elevation = response.results?.[0]?.elevation;
    if (typeof elevation !== "number" || !Number.isFinite(elevation)) {
      throw new Error("google_maps_elevation_unavailable");
    }
    return elevation;
  }).catch((error: unknown) => {
    terrainElevationPromises.delete(cacheKey);
    throw error;
  });
  terrainElevationPromises.set(cacheKey, promise);
  return promise;
}
