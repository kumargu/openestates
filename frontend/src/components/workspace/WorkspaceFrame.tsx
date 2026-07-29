import { useEffect, useMemo, useState, type ReactNode } from "react";
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
import {
  WorkspaceSidebar,
  type WorkspaceView,
} from "./WorkspaceSidebar.tsx";
import "../../styles/workspace.css";

const SIDEBAR_STORAGE_KEY = "openestates:workspace-sidebar-collapsed";

type WorkspaceFrameProps = {
  children: ReactNode;
};

function routePropertyId(pathname: string): string | null {
  const match = pathname.match(/^\/property\/([^/]+)/);
  return match?.[1] ? decodeURIComponent(match[1]) : null;
}

function activeWorkspaceView(pathname: string): WorkspaceView {
  if (pathname === "/workspace" || pathname === "/notebook") return "notebook";
  if (pathname === "/workspace/compare" || pathname === "/compare") return "compare";
  if (/^\/property\/[^/]+\/plan$/.test(pathname)) return "plan";
  if (/^\/property\/[^/]+$/.test(pathname)) return "home";
  return "browse";
}

function sameIds(left: string[], right: string[]): boolean {
  return left.length === right.length && left.every((id, index) => id === right[index]);
}

export function WorkspaceFrame({ children }: WorkspaceFrameProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const query = useMemo(() => new URLSearchParams(location.search), [location.search]);
  const queryIds = useMemo(() => parseShortlistIds(query.get("ids")), [query]);
  const queryFocus = query.get("focus");
  const propertyId = routePropertyId(location.pathname);
  const [properties, setProperties] = useState<PropertyCard[]>([]);
  const [shortlistIds, setShortlistIds] = useState<string[]>(() => readShortlistIds());
  const [collapsed, setCollapsed] = useState(() =>
    window.localStorage.getItem(SIDEBAR_STORAGE_KEY) === "true"
  );

  useEffect(() => {
    const controller = new AbortController();
    getProperties({ signal: controller.signal })
      .then(setProperties)
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

  const storedFocus = window.localStorage.getItem(FOCUS_STORAGE_KEY);
  const focusedId = (
    propertyId
    ?? (queryFocus && homes.some((home) => home.id === queryFocus) ? queryFocus : null)
    ?? (storedFocus && homes.some((home) => home.id === storedFocus) ? storedFocus : null)
    ?? homes[0]?.id
    ?? ""
  );

  useEffect(() => {
    if (focusedId) window.localStorage.setItem(FOCUS_STORAGE_KEY, focusedId);
  }, [focusedId]);

  useEffect(() => {
    if (homes.length === 0) return;
    setCollapsed(false);
    window.localStorage.setItem(SIDEBAR_STORAGE_KEY, "false");
  }, [homes.length]);

  const activeView = activeWorkspaceView(location.pathname);

  function writeSelection(nextIds: string[], nextFocus?: string) {
    writeShortlistIds(nextIds);
    const focus = nextFocus
      ?? (nextIds.includes(focusedId) ? focusedId : nextIds[0] ?? "");
    if (focus) window.localStorage.setItem(FOCUS_STORAGE_KEY, focus);

    if (activeView === "compare") {
      const next = new URLSearchParams(location.search);
      if (focus) next.set("focus", focus);
      else next.delete("focus");
      navigate(`/workspace/compare?${next.toString()}`, { replace: true });
      return;
    }

    if (activeView === "home" && focus) {
      navigate(`/property/${focus}`);
      return;
    }
    if (activeView === "plan" && focus) {
      navigate(`/property/${focus}/plan`);
    }
  }

  function toggleSidebar() {
    setCollapsed((current) => {
      const next = !current;
      window.localStorage.setItem(SIDEBAR_STORAGE_KEY, String(next));
      return next;
    });
  }

  function focusHome(nextId: string) {
    window.localStorage.setItem(FOCUS_STORAGE_KEY, nextId);
    if (activeView === "plan") {
      navigate(`/property/${nextId}/plan`);
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
    navigate(`/property/${nextId}`);
  }

  function removeHome(propertyIdToRemove: string) {
    const nextIds = homes
      .map((home) => home.id)
      .filter((id) => id !== propertyIdToRemove);
    writeSelection(nextIds);
  }

  const reducedBeforeDecision = homes.length === 0 && queryIds.length === 0;
  const sidebarCollapsed = collapsed || reducedBeforeDecision;

  return (
    <div className={`workspace-shell${sidebarCollapsed ? " workspace-shell--collapsed" : ""}`}>
      <WorkspaceSidebar
        homes={homes}
        focusedId={focusedId}
        activeView={activeView}
        collapsed={sidebarCollapsed}
        reduced={reducedBeforeDecision}
        onToggle={toggleSidebar}
        onFocus={focusHome}
        onRemove={removeHome}
      />
      <div className="workspace-view">{children}</div>
    </div>
  );
}
