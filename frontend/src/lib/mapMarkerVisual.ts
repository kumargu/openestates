export type MapMarkerEmphasis = "active" | "selected" | "subdued";

export type MapMarkerPinOptions = {
  background: string;
  borderColor: string;
  glyphSrc: string;
  scale: number;
};

type MarkerTheme = {
  background: string;
  path: string;
};

const DEFAULT_MARKER_THEME: MarkerTheme = {
  background: "#dedbd5",
  path: '<circle cx="12" cy="12" r="3.2"/>',
};

const MARKER_THEMES: Record<string, MarkerTheme> = {
  "beer": {
    background: "#ecd29a",
    path: '<path d="M7 7h8v10a3 3 0 0 1-3 3h-2a3 3 0 0 1-3-3zM15 10h2a2 2 0 0 1 0 4h-2M8 4h6"/>',
  },
  "briefcase-business": {
    background: "#d9cde9",
    path: '<path d="M4 8h16v11H4zM9 8V5h6v3M4 12h16M10 12v2h4v-2"/>',
  },
  "dumbbell": {
    background: "#d6ddd4",
    path: '<path d="M5 9v6M8 7v10M16 7v10M19 9v6M8 12h8"/>',
  },
  "flag": {
    background: "#e8c8cf",
    path: '<path d="M6 21V4M7 5h10l-2 4 2 4H7"/>',
  },
  "graduation-cap": {
    background: "#ecd9a9",
    path: '<path d="m3 10 9-5 9 5-9 5zM7 13v4c2.6 2 7.4 2 10 0v-4M20 11v5"/>',
  },
  "home": {
    background: "#e5e0d7",
    path: '<path d="m4 11 8-7 8 7v9h-6v-6h-4v6H4z"/>',
  },
  "hospital": {
    background: "#ebccd2",
    path: '<path d="M5 4h14v16H5zM12 8v8M8 12h8"/>',
  },
  "road": {
    background: "#dedbd5",
    path: '<path d="M8 3 5 21M16 3l3 18M12 4v4M12 11v4M12 18v2"/>',
  },
  "train": {
    background: "#ccdcef",
    path: '<path d="M6 4h12v12H6zM6 11h12M9 18l-2 3M15 18l2 3M9 14h.01M15 14h.01"/>',
  },
  "trees": {
    background: "#cfe0c9",
    path: '<path d="m8 4-4 7h3l-3 5h8l-3-5h3zM16 5l-3 6h2l-2 5h7l-3-5h2zM8 16v5M16 16v5"/>',
  },
  "waves": {
    background: "#c9e1e3",
    path: '<path d="M3 9c2 2 4 2 6 0s4-2 6 0 4 2 6 0M3 15c2 2 4 2 6 0s4-2 6 0 4 2 6 0"/>',
  },
};

const EMPHASIS_SCALE: Record<MapMarkerEmphasis, number> = {
  active: 0.95,
  selected: 1.22,
  subdued: 0.74,
};

const glyphUrls = new Map<string, string>();
const markerUrls = new Map<string, string>();

function glyphUrl(path: string): string {
  const cached = glyphUrls.get(path);
  if (cached) return cached;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#292723" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">${path}</svg>`;
  const url = `data:image/svg+xml,${encodeURIComponent(svg)}`;
  glyphUrls.set(path, url);
  return url;
}

export function mapMarkerPinOptions(
  icon: string | undefined,
  emphasis: MapMarkerEmphasis,
): MapMarkerPinOptions {
  const theme = icon ? MARKER_THEMES[icon] ?? DEFAULT_MARKER_THEME : DEFAULT_MARKER_THEME;
  return {
    background: emphasis === "subdued" ? "#e7e4de" : theme.background,
    borderColor: emphasis === "selected" ? "#292723" : "#fffdf8",
    glyphSrc: glyphUrl(theme.path),
    scale: EMPHASIS_SCALE[emphasis],
  };
}

export function mapMarkerIconUrl(
  icon: string | undefined,
  emphasis: MapMarkerEmphasis,
): string {
  const cacheKey = `${icon ?? "default"}:${emphasis}`;
  const cached = markerUrls.get(cacheKey);
  if (cached) return cached;
  const theme = icon ? MARKER_THEMES[icon] ?? DEFAULT_MARKER_THEME : DEFAULT_MARKER_THEME;
  const fill = emphasis === "subdued" ? "#e7e4de" : theme.background;
  const border = emphasis === "selected" ? "#292723" : "#fffdf8";
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 36 36"><circle cx="18" cy="18" r="15" fill="${fill}" stroke="${border}" stroke-width="2"/><g transform="translate(7 7) scale(.92)" fill="none" stroke="#292723" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">${theme.path}</g></svg>`;
  const url = `data:image/svg+xml,${encodeURIComponent(svg)}`;
  markerUrls.set(cacheKey, url);
  return url;
}
