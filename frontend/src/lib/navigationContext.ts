export type NavigationMode = "landing" | "discovery" | "property-context" | "workspace";

export type DiscoveryContext = {
  version: 1;
  url: string;
  scrollY: number;
};

const DISCOVERY_STORAGE_KEY = "openestates:last-discovery:v1";

export function navigationMode(pathname: string, search = ""): NavigationMode {
  if (pathname.startsWith("/workspace") || pathname === "/notebook" || pathname === "/compare") {
    return "workspace";
  }
  if (/^\/property\/[^/]+(?:\/rera)?$/.test(pathname)) return "property-context";
  if (pathname === "/" && new URLSearchParams(search).get("q")?.trim()) return "discovery";
  return "landing";
}

export function readDiscoveryContext(): DiscoveryContext | null {
  if (typeof window === "undefined") return null;
  try {
    const parsed: unknown = JSON.parse(window.sessionStorage.getItem(DISCOVERY_STORAGE_KEY) ?? "null");
    if (!parsed || typeof parsed !== "object") return null;
    const candidate = parsed as Partial<DiscoveryContext>;
    if (
      candidate.version !== 1
      || typeof candidate.url !== "string"
      || !candidate.url.startsWith("/?")
      || !new URLSearchParams(candidate.url.slice(2)).get("q")?.trim()
    ) return null;
    return {
      version: 1,
      url: candidate.url,
      scrollY: typeof candidate.scrollY === "number" && candidate.scrollY >= 0
        ? candidate.scrollY
        : 0,
    };
  } catch {
    return null;
  }
}

export function writeDiscoveryContext(url: string, scrollY = 0): void {
  if (typeof window === "undefined") return;
  const params = url.startsWith("/?") ? new URLSearchParams(url.slice(2)) : null;
  if (!params?.get("q")?.trim()) return;
  const context: DiscoveryContext = {
    version: 1,
    url,
    scrollY: Math.max(0, Math.round(scrollY)),
  };
  window.sessionStorage.setItem(DISCOVERY_STORAGE_KEY, JSON.stringify(context));
}

export function discoveryReturnHref(): string {
  return readDiscoveryContext()?.url ?? "/";
}
