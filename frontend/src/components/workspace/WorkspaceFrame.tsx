import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { getProperties } from "../../lib/api.ts";
import {
  defaultComparedHomes,
  normalizeComparedSocieties,
} from "../../lib/compare.ts";
import type { PropertyCard } from "../../lib/types.ts";
import {
  WorkspaceSidebar,
  type WorkspaceView,
} from "./WorkspaceSidebar.tsx";
import "../../styles/workspace.css";

const MAX_WORKSPACE_HOMES = 10;
const DEFAULT_WORKSPACE_HOMES = 3;
const MIN_WORKSPACE_SOCIETIES = 2;
const SIDEBAR_STORAGE_KEY = "openestates:workspace-sidebar-collapsed";
const HOMES_STORAGE_KEY = "openestates:workspace-home-ids";
const FOCUS_STORAGE_KEY = "openestates:workspace-focused-home";

type WorkspaceFrameProps = {
  children: ReactNode;
};

function parseIds(value: string | null): string[] {
  if (!value) return [];
  return [...new Set(value.split(",").map((id) => id.trim()).filter(Boolean))]
    .slice(0, MAX_WORKSPACE_HOMES);
}

function storedIds(): string[] {
  return parseIds(window.localStorage.getItem(HOMES_STORAGE_KEY));
}

function routePropertyId(pathname: string): string | null {
  const match = pathname.match(/^\/property\/([^/]+)/);
  return match?.[1] ? decodeURIComponent(match[1]) : null;
}

function activeWorkspaceView(pathname: string): WorkspaceView {
  if (pathname === "/compare") return "compare";
  if (/^\/property\/[^/]+\/plan$/.test(pathname)) return "plan";
  if (/^\/property\/[^/]+$/.test(pathname)) return "home";
  return "discover";
}

export function WorkspaceFrame({ children }: WorkspaceFrameProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const isLanding = location.pathname === "/";
  const query = useMemo(() => new URLSearchParams(location.search), [location.search]);
  const queryIds = useMemo(() => parseIds(query.get("ids")), [query]);
  const queryFocus = query.get("focus");
  const propertyId = routePropertyId(location.pathname);
  const [properties, setProperties] = useState<PropertyCard[]>([]);
  const [collapsed, setCollapsed] = useState(() =>
    window.localStorage.getItem(SIDEBAR_STORAGE_KEY) === "true"
  );

  useEffect(() => {
    if (isLanding) return undefined;
    const controller = new AbortController();
    getProperties({ signal: controller.signal })
      .then(setProperties)
      .catch((error: unknown) => {
        if (error instanceof DOMException && error.name === "AbortError") return;
        setProperties([]);
      });
    return () => controller.abort();
  }, [isLanding]);

  const homeIds = useMemo(() => {
    if (properties.length === 0 || isLanding) return [];
    const availableIds = new Set(properties.map((property) => property.id));
    const hasExplicitSelection = queryIds.length > 0;
    const requested = hasExplicitSelection ? queryIds : storedIds();
    let next = requested.filter((id) => availableIds.has(id));

    if (next.length < MIN_WORKSPACE_SOCIETIES && !hasExplicitSelection) {
      next = defaultComparedHomes(properties, DEFAULT_WORKSPACE_HOMES)
        .map((property) => property.id);
    } else if (next.length >= MIN_WORKSPACE_SOCIETIES) {
      const byId = new Map(properties.map((property) => [property.id, property]));
      const selectedHomes = next
        .map((id) => byId.get(id))
        .filter((property): property is PropertyCard => Boolean(property));
      next = normalizeComparedSocieties(
        selectedHomes,
        properties,
        MIN_WORKSPACE_SOCIETIES,
        MAX_WORKSPACE_HOMES,
      ).map((property) => property.id);
    }

    if (
      propertyId
      && availableIds.has(propertyId)
      && !next.includes(propertyId)
      && !hasExplicitSelection
    ) {
      next = [propertyId, ...next].slice(0, MAX_WORKSPACE_HOMES);
    }

    return next;
  }, [isLanding, properties, propertyId, queryIds]);

  useEffect(() => {
    if (homeIds.length > 0) {
      window.localStorage.setItem(HOMES_STORAGE_KEY, homeIds.join(","));
    }
  }, [homeIds]);

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

  if (isLanding) return children;

  const idsValue = homes.map((home) => home.id).join(",");
  const compareHref = idsValue
    ? `/compare?ids=${encodeURIComponent(idsValue)}&focus=${encodeURIComponent(focusedId)}`
    : "/compare";
  const activeView = activeWorkspaceView(location.pathname);

  function writeSelection(nextIds: string[], nextFocus?: string) {
    window.localStorage.setItem(HOMES_STORAGE_KEY, nextIds.join(","));
    const focus = nextFocus
      ?? (nextIds.includes(focusedId) ? focusedId : nextIds[0] ?? "");
    if (focus) window.localStorage.setItem(FOCUS_STORAGE_KEY, focus);

    if (activeView === "compare") {
      const next = new URLSearchParams(location.search);
      next.set("ids", nextIds.join(","));
      if (focus) next.set("focus", focus);
      navigate(`/compare?${next.toString()}`, { replace: true });
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
      navigate(`/compare?ids=${encodeURIComponent(idsValue)}&focus=${encodeURIComponent(nextId)}`, {
        replace: true,
      });
      return;
    }
    navigate(`/property/${nextId}`);
  }

  function removeHome(propertyId: string) {
    if (homes.length <= MIN_WORKSPACE_SOCIETIES) return;
    const nextIds = homes
      .map((home) => home.id)
      .filter((id) => id !== propertyId);
    writeSelection(nextIds);
  }

  return (
    <div className={`workspace-shell${collapsed ? " workspace-shell--collapsed" : ""}`}>
      <WorkspaceSidebar
        homes={homes}
        focusedId={focusedId}
        compareHref={compareHref}
        activeView={activeView}
        collapsed={collapsed}
        onToggle={toggleSidebar}
        onFocus={focusHome}
        onRemove={removeHome}
      />
      <div className="workspace-view">{children}</div>
    </div>
  );
}
