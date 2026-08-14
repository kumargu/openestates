import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { getProperties } from "../../lib/api.ts";
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
  navigationMode,
  requestDiscoveryReturn,
  writeDiscoveryContext,
} from "../../lib/navigationContext.ts";
import {
  activeWorkspaceView,
  shouldShowWorkspaceSidebar,
  workspaceBuyVsRentHref,
  workspaceFocusedHomeId,
} from "../../lib/workspaceNav.ts";
import { WorkspaceSidebar } from "./WorkspaceSidebar.tsx";
import "../../styles/workspace.css";

const SIDEBAR_STORAGE_KEY = "openestates:workspace-sidebar-collapsed";

type WorkspaceFrameProps = {
  children: ReactNode;
};

function routePropertyId(pathname: string): string | null {
  const match = pathname.match(/^\/property\/([^/]+)/)
    ?? pathname.match(/^\/workspace\/buy-vs-rent\/([^/]+)/);
  return match?.[1] ? decodeURIComponent(match[1]) : null;
}

function sameIds(left: string[], right: string[]): boolean {
  return left.length === right.length && left.every((id, index) => id === right[index]);
}

function writeSidebarCollapsed(collapsed: boolean) {
  window.localStorage.setItem(SIDEBAR_STORAGE_KEY, String(collapsed));
}

function propertyPath(propertyId: string, suffix = ""): string {
  return `/property/${encodeURIComponent(propertyId)}${suffix}`;
}

export function WorkspaceFrame({ children }: WorkspaceFrameProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const query = useMemo(() => new URLSearchParams(location.search), [location.search]);
  const queryIds = useMemo(() => parseShortlistIds(query.get("ids")), [query]);
  const queryFocus = query.get("focus");
  const propertyId = routePropertyId(location.pathname);
  const shellMode = navigationMode(location.pathname, location.search);
  const [properties, setProperties] = useState<PropertyCard[]>([]);
  const [shortlistIds, setShortlistIds] = useState<string[]>(() => readShortlistIds());
  const [collapsed, setCollapsed] = useState(() =>
    window.localStorage.getItem(SIDEBAR_STORAGE_KEY) === "true"
  );

  useEffect(() => {
    const controller = new AbortController();
    getProperties({ signal: controller.signal })
      .then((nextProperties) => {
        setProperties(nextProperties);
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

  const storedFocus = window.localStorage.getItem(FOCUS_STORAGE_KEY);
  const workspaceFocusedId = workspaceFocusedHomeId(
    queryFocus,
    storedFocus,
    homes.map((home) => home.id),
  );
  const focusedId = shellMode === "property-context"
    ? propertyId ?? ""
    : propertyId ?? workspaceFocusedId;

  useEffect(() => {
    if (focusedId) window.localStorage.setItem(FOCUS_STORAGE_KEY, focusedId);
  }, [focusedId]);

  const activeView = activeWorkspaceView(location.pathname);

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
      navigate(`/workspace/compare?${next.toString()}`, { replace: true });
      return;
    }

    if (activeView === "home" && focus) {
      navigate(propertyPath(focus));
      return;
    }
    if (activeView === "rera" && focus) {
      navigate(propertyPath(focus, "/rera"));
      return;
    }
    if (activeView === "plan" && focus) {
      navigate(workspaceBuyVsRentHref(focus, queryIds.filter((id) => nextIds.includes(id))));
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
      navigate(workspaceBuyVsRentHref(nextId, queryIds));
      return;
    }
    if (activeView === "rera") {
      navigate(propertyPath(nextId, "/rera"));
      return;
    }
    if (activeView === "compare") {
      const next = new URLSearchParams(location.search);
      const focusedHome = homes.find((home) => home.id === nextId);
      next.set("focus", nextId);
      if (focusedHome) next.set("bhk", String(focusedHome.bhk));
      navigate(`/workspace/compare?${next.toString()}`, { replace: true });
      return;
    }
    navigate(propertyPath(nextId));
  }

  function removeHome(propertyIdToRemove: string) {
    const nextIds = homes
      .map((home) => home.id)
      .filter((id) => id !== propertyIdToRemove);
    writeSelection(nextIds);
    detachNotebookPropertyFromShortlist(propertyIdToRemove);
  }

  const reducedBeforeDecision = shellMode === "workspace" && homes.length === 0 && queryIds.length === 0;
  const sidebarCollapsed = collapsed || reducedBeforeDecision;
  const showSidebar = shouldShowWorkspaceSidebar(
    location.pathname,
    shellMode,
    homes.length,
  );
  const sidebarMode = shellMode === "property-context"
    ? "property-context"
    : shellMode === "workspace"
      ? "workspace"
      : "discovery";
  const effectiveSidebarCollapsed = sidebarMode === "workspace" && sidebarCollapsed;
  const sidebarHomes = homes;

  return (
    <div className={`workspace-shell${showSidebar ? "" : " workspace-shell--plain"}${showSidebar && effectiveSidebarCollapsed ? " workspace-shell--collapsed" : ""}`}>
      {showSidebar ? (
        <WorkspaceSidebar
          homes={sidebarHomes}
          focusedId={focusedId}
          activeView={activeView}
          collapsed={effectiveSidebarCollapsed}
          reduced={reducedBeforeDecision}
          mode={sidebarMode}
          onToggle={toggleSidebar}
          onFocus={focusHome}
          onRemove={removeHome}
        />
      ) : null}
      <div className="workspace-view">{children}</div>
    </div>
  );
}
