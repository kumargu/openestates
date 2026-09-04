import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { getProperties } from "../../lib/api.ts";
import { useNotebook } from "../../hooks/useNotebook.ts";
import {
  FOCUS_STORAGE_KEY,
  parseShortlistIds,
  readShortlistIds,
  SHORTLIST_CHANGED_EVENT,
  writeShortlistIds,
} from "../../lib/compare.ts";
import type { PropertyCard } from "../../lib/types.ts";
import { detachNotebookPropertyFromShortlist } from "../../lib/notebook.ts";
import {
  captureDiscoveryDeparture,
  clearDiscoveryContext,
  discoveryReturnHref,
  hasSearchSpanUrlParams,
  hrefWithSearchSpan,
  navigationMode,
  propertyHrefWithSearchSpan,
  readDiscoveryContext,
  reconcileSearchSpanAvailability,
  requestDiscoveryReturn,
  SEARCH_SPAN_TTL_MS,
  searchSpanContextFromLocation,
  searchSpanReferenceForTarget,
  stripSearchSpanUrlParams,
  writeDiscoveryContext,
} from "../../lib/navigationContext.ts";
import {
  activeWorkspaceView,
  activeWorkspaceCompareIds,
  shouldShowWorkspaceSidebar,
  workspaceFocusedHomeId,
} from "../../lib/workspaceNav.ts";
import { WorkspaceSidebar } from "./WorkspaceSidebar.tsx";
import { SearchSpanProvider } from "./SearchSpanProvider.tsx";
import "../../styles/workspace.css";

const SIDEBAR_STORAGE_KEY = "openestates:workspace-sidebar-collapsed";

type WorkspaceFrameProps = {
  children: ReactNode;
};

function routePropertyId(pathname: string): string | null {
  const match = pathname.match(/^\/property\/([^/]+)/)
    ?? pathname.match(/^\/workspace\/buy-vs-rent\/([^/]+)/);
  if (!match?.[1]) return null;
  try {
    return decodeURIComponent(match[1]);
  } catch {
    return null;
  }
}

function sameIds(left: string[], right: string[]): boolean {
  return left.length === right.length && left.every((id, index) => id === right[index]);
}

function writeSidebarCollapsed(collapsed: boolean) {
  window.localStorage.setItem(SIDEBAR_STORAGE_KEY, String(collapsed));
}

export function WorkspaceFrame({ children }: WorkspaceFrameProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const query = useMemo(() => new URLSearchParams(location.search), [location.search]);
  const queryIds = useMemo(() => parseShortlistIds(query.get("ids")), [query]);
  const queryFocus = query.get("focus");
  const propertyId = routePropertyId(location.pathname);
  const shellMode = navigationMode(location.pathname, location.search);
  const [searchSpanRevision, setSearchSpanRevision] = useState(0);
  const storedPropertySearchContext = useMemo(
    () => {
      void searchSpanRevision;
      return searchSpanContextFromLocation(location.pathname, location.search);
    },
    [location.pathname, location.search, searchSpanRevision],
  );
  const hasSearchSpanParams = hasSearchSpanUrlParams(location.search);
  const [properties, setProperties] = useState<PropertyCard[]>([]);
  const [propertyCatalogReady, setPropertyCatalogReady] = useState(false);
  const [shortlistIds, setShortlistIds] = useState<string[]>(() => readShortlistIds());
  const [collapsed, setCollapsed] = useState(() =>
    window.localStorage.getItem(SIDEBAR_STORAGE_KEY) === "true"
  );
  const { compareIds } = useNotebook();

  useEffect(() => {
    const controller = new AbortController();
    getProperties({ signal: controller.signal })
      .then((nextProperties) => {
        setProperties(nextProperties);
        setPropertyCatalogReady(true);
      })
      .catch((error: unknown) => {
        if (error instanceof DOMException && error.name === "AbortError") return;
        setProperties([]);
      });
    return () => controller.abort();
  }, []);

  useEffect(() => {
    function refresh() {
      const next = readShortlistIds();
      setShortlistIds((current) => sameIds(current, next) ? current : next);
    }
    window.addEventListener(SHORTLIST_CHANGED_EVENT, refresh);
    window.addEventListener("storage", refresh);
    return () => {
      window.removeEventListener(SHORTLIST_CHANGED_EVENT, refresh);
      window.removeEventListener("storage", refresh);
    };
  }, []);

  const homeIds = useMemo(() => {
    if (properties.length === 0) return [];
    const availableIds = new Set(properties.map((property) => property.id));
    return shortlistIds.filter((id) => availableIds.has(id));
  }, [properties, shortlistIds]);
  const catalogPropertyIds = useMemo(
    () => new Set(properties.map((property) => property.id)),
    [properties],
  );
  const propertySearchContext = useMemo(
    () => propertyCatalogReady
      ? reconcileSearchSpanAvailability(storedPropertySearchContext, catalogPropertyIds)
      : storedPropertySearchContext,
    [catalogPropertyIds, propertyCatalogReady, storedPropertySearchContext],
  );

  useEffect(() => {
    if (properties.length === 0) return;
    if (queryIds.length === 0 && !sameIds(shortlistIds, homeIds)) {
      writeShortlistIds(homeIds);
    }
  }, [homeIds, properties.length, queryIds.length, shortlistIds]);

  const homes = useMemo(() => {
    const byId = new Map(properties.map((property) => [property.id, property]));
    return homeIds
      .map((id) => byId.get(id))
      .filter((property): property is PropertyCard => Boolean(property));
  }, [homeIds, properties]);

  useLayoutEffect(() => {
    if (shellMode === "landing" && location.pathname === "/") {
      clearDiscoveryContext();
      return undefined;
    }
    if (shellMode !== "discovery") return undefined;
    const url = `${location.pathname}${location.search}`;

    let latestScrollY = window.scrollY;
    const trackScroll = () => {
      latestScrollY = window.scrollY;
    };
    const captureHistoryDeparture = () => {
      latestScrollY = window.scrollY;
      captureDiscoveryDeparture(url, latestScrollY);
    };
    const captureLinkDeparture = (event: MouseEvent) => {
      const target = event.target instanceof Element ? event.target : null;
      const anchor = target?.closest<HTMLAnchorElement>("a[href]");
      if (!anchor) return;
      const destination = new URL(anchor.href, window.location.href);
      if (destination.origin !== window.location.origin) return;
      if (`${destination.pathname}${destination.search}` === url) return;
      captureHistoryDeparture();
    };

    writeDiscoveryContext(url, latestScrollY);
    window.addEventListener("scroll", trackScroll, { passive: true });
    window.addEventListener("popstate", captureHistoryDeparture);
    document.addEventListener("click", captureLinkDeparture, true);
    return () => {
      window.removeEventListener("scroll", trackScroll);
      window.removeEventListener("popstate", captureHistoryDeparture);
      document.removeEventListener("click", captureLinkDeparture, true);
      writeDiscoveryContext(url, latestScrollY);
      requestDiscoveryReturn(url);
    };
  }, [location.pathname, location.search, shellMode]);

  const activeView = activeWorkspaceView(location.pathname);
  const availablePropertyIds = new Set(
    properties.map((property) => property.id),
  );
  const compareFocusIds = queryIds.filter((id) => availablePropertyIds.has(id));
  const activeCompareIds = activeWorkspaceCompareIds(
    activeView === "compare" ? queryIds : [],
    compareIds,
  );
  const storedFocus = window.localStorage.getItem(FOCUS_STORAGE_KEY);
  const workspaceFocusedId = workspaceFocusedHomeId(
    queryFocus,
    storedFocus,
    activeView === "compare" && compareFocusIds.length > 0
      ? compareFocusIds
      : homes.map((home) => home.id),
  );
  const focusedId = shellMode === "property-context"
    ? propertyId ?? ""
    : propertyId ?? (activeView === "compare"
      ? workspaceFocusedId
      : propertySearchContext?.selectedId ?? workspaceFocusedId);

  const searchSpanCreatedAt = propertySearchContext?.createdAt;
  const searchSpanId = propertySearchContext?.id;
  useEffect(() => {
    if (searchSpanCreatedAt == null || !searchSpanId) return undefined;
    const refresh = () => setSearchSpanRevision((revision) => revision + 1);
    const expiresIn = Math.max(
      0,
      searchSpanCreatedAt + SEARCH_SPAN_TTL_MS - Date.now(),
    );
    const expiryTimer = window.setTimeout(refresh, expiresIn + 1);
    window.addEventListener("focus", refresh);
    window.addEventListener("storage", refresh);
    return () => {
      window.clearTimeout(expiryTimer);
      window.removeEventListener("focus", refresh);
      window.removeEventListener("storage", refresh);
    };
  }, [searchSpanCreatedAt, searchSpanId]);

  useEffect(() => {
    if (!hasSearchSpanParams || propertySearchContext) return;
    navigate(`${location.pathname}${stripSearchSpanUrlParams(location.search)}`, {
      replace: true,
    });
  }, [
    location.pathname,
    location.search,
    navigate,
    propertySearchContext,
    hasSearchSpanParams,
  ]);

  useEffect(() => {
    if (focusedId) window.localStorage.setItem(FOCUS_STORAGE_KEY, focusedId);
  }, [focusedId]);

  function writeSelection(nextIds: string[], nextFocus?: string) {
    writeShortlistIds(nextIds);
    const focus = nextFocus
      ?? (nextIds.includes(focusedId) ? focusedId : nextIds[0] ?? "");
    if (focus) window.localStorage.setItem(FOCUS_STORAGE_KEY, focus);

    if (activeView === "compare") {
      const next = new URLSearchParams(location.search);
      const comparedIds = parseShortlistIds(next.get("ids"))
        .filter((id) => nextIds.includes(id));
      if (comparedIds.length > 0) next.set("ids", comparedIds.join(","));
      else next.delete("ids");
      const compareFocus = comparedIds.includes(focus) ? focus : comparedIds[0] ?? "";
      if (compareFocus) next.set("focus", compareFocus);
      else next.delete("focus");
      navigate(hrefWithSearchSpan(
        `/workspace/compare?${next.toString()}`,
        searchSpanReferenceForTarget(propertySearchContext, compareFocus),
      ), { replace: true });
      return;
    }

    if (activeView === "home" && focus) {
      navigate(propertyHrefWithSearchSpan(focus, propertySearchContext));
      return;
    }
    if (activeView === "rera" && focus) {
      navigate(propertyHrefWithSearchSpan(focus, propertySearchContext, "/rera"));
      return;
    }
    if (activeView === "plan" && focus) {
      navigate(hrefWithSearchSpan(
        `/workspace/buy-vs-rent/${encodeURIComponent(focus)}`,
        searchSpanReferenceForTarget(propertySearchContext, focus),
      ));
    }
  }

  function toggleSidebar() {
    setCollapsed((current) => {
      const next = !current;
      writeSidebarCollapsed(next);
      return next;
    });
  }

  function focusHome(nextId: string) {
    window.localStorage.setItem(FOCUS_STORAGE_KEY, nextId);
    if (activeView === "plan") {
      navigate(hrefWithSearchSpan(
        `/workspace/buy-vs-rent/${encodeURIComponent(nextId)}`,
        searchSpanReferenceForTarget(propertySearchContext, nextId),
      ));
      return;
    }
    if (activeView === "rera") {
      navigate(propertyHrefWithSearchSpan(nextId, propertySearchContext, "/rera"));
      return;
    }
    if (activeView === "compare") {
      const next = new URLSearchParams(location.search);
      const focusedHome = homes.find((home) => home.id === nextId);
      next.set("focus", nextId);
      if (focusedHome) next.set("bhk", String(focusedHome.bhk));
      navigate(hrefWithSearchSpan(
        `/workspace/compare?${next.toString()}`,
        searchSpanReferenceForTarget(propertySearchContext, nextId),
      ), { replace: true });
      return;
    }
    navigate(propertyHrefWithSearchSpan(nextId, propertySearchContext));
  }

  function removeHome(propertyIdToRemove: string) {
    const nextIds = homes
      .map((home) => home.id)
      .filter((id) => id !== propertyIdToRemove);
    if (shellMode === "property-context") {
      writeShortlistIds(nextIds);
      detachNotebookPropertyFromShortlist(propertyIdToRemove);
      return;
    }
    writeSelection(nextIds);
    detachNotebookPropertyFromShortlist(propertyIdToRemove);
  }

  const reducedBeforeDecision = shellMode === "workspace"
    && !propertySearchContext
    && homes.length === 0
    && queryIds.length === 0;
  const sidebarReduced = reducedBeforeDecision;
  const sidebarCollapsed = collapsed || sidebarReduced;
  const isInternalRoute = location.pathname.startsWith("/_internal/")
    || location.pathname.startsWith("/dev/");
  const showSidebar = !isInternalRoute
    && shouldShowWorkspaceSidebar(shellMode, homes.length);
  const sidebarMode = shellMode === "property-context"
    ? "property-context"
    : shellMode === "workspace"
      ? "workspace"
      : "discovery";
  const effectiveSidebarCollapsed = sidebarCollapsed;
  const discoveryContext = readDiscoveryContext();
  const discoveryHref = propertySearchContext?.returnUrl ?? discoveryReturnHref();
  const sidebarHomes = homes;
  const shellClassName = [
    "workspace-shell",
    showSidebar ? null : "workspace-shell--plain",
    showSidebar && effectiveSidebarCollapsed ? "workspace-shell--collapsed" : null,
  ].filter(Boolean).join(" ");

  return (
    <SearchSpanProvider value={propertySearchContext}>
      <div className={shellClassName}>
        {showSidebar ? (
          <WorkspaceSidebar
            key={propertySearchContext?.id ?? "no-search"}
            homes={sidebarHomes}
            compareIds={activeCompareIds}
            focusedId={focusedId}
            activeView={activeView}
            collapsed={effectiveSidebarCollapsed}
            reduced={sidebarReduced}
            mode={sidebarMode}
            discoveryHref={discoveryHref}
            discoveryResultCount={propertySearchContext?.results.length
              ?? discoveryContext?.resultCount}
            hasDiscoveryContext={propertySearchContext !== null || discoveryContext !== null}
            searchContext={propertySearchContext}
            onToggle={toggleSidebar}
            onFocus={focusHome}
            onRemove={removeHome}
          />
        ) : null}
        <div className="workspace-view">{children}</div>
      </div>
    </SearchSpanProvider>
  );
}
