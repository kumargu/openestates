import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import type { PropertyCard } from "../../lib/types.ts";
import { useNotebook } from "../../hooks/useNotebook.ts";
import {
  workspaceNavItems,
  type WorkspaceView,
} from "../../lib/workspaceNav.ts";
import { BrandMark } from "../brand/BrandMark.tsx";
import {
  readSearchSpanDismissedIds,
  requestDiscoveryReturn,
  requestSearchSpanReturn,
  searchSpanReturnDelta,
  writeSearchSpanDismissedIds,
} from "../../lib/navigationContext.ts";
import type {
  PropertySearchContext,
  PropertySearchResult,
} from "../../lib/navigationContext.ts";
import { PUBLIC_BRAND_NAME } from "../../lib/brand.ts";
import { PropertySearchPanel } from "../property/PropertySearchRail.tsx";

type WorkspaceIconName =
  | "back"
  | "browse"
  | "listing"
  | "notebook"
  | "compare"
  | "rera"
  | "plan"
  | "toggle";

type WorkspaceSidebarProps = {
  homes: PropertyCard[];
  compareIds: string[];
  focusedId: string;
  activeView: WorkspaceView;
  collapsed: boolean;
  reduced: boolean;
  mode: "discovery" | "property-context" | "workspace";
  discoveryHref: string;
  discoveryResultCount?: number;
  hasDiscoveryContext: boolean;
  searchContext: PropertySearchContext | null;
  onToggle: () => void;
  onFocus: (propertyId: string) => void;
  onRemove: (propertyId: string) => void;
};

function WorkspaceIcon({
  name,
  size = 20,
}: {
  name: WorkspaceIconName;
  size?: number;
}) {
  const common = {
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };

  if (name === "browse") {
    return (
      <svg {...common}>
        <circle cx="12" cy="12" r="8" />
        <path d="m16 8-2.6 5.4L8 16l2.6-5.4Z" />
      </svg>
    );
  }
  if (name === "back") {
    return (
      <svg {...common}>
        <path d="m14.5 6.5-5.5 5.5 5.5 5.5" />
        <path d="M9 12h10" />
      </svg>
    );
  }
  if (name === "listing") {
    return (
      <svg {...common}>
        <path d="M4.5 10.2 12 4.25l7.5 5.95V19a1.5 1.5 0 0 1-1.5 1.5H6A1.5 1.5 0 0 1 4.5 19Z" />
        <path d="M9.5 20.5v-6h5v6" />
      </svg>
    );
  }
  if (name === "notebook") {
    return (
      <svg {...common}>
        <path d="M8 4.75h7.25A1.75 1.75 0 0 1 17 6.5v13H8A1.75 1.75 0 0 1 6.25 17.75v-11A2 2 0 0 1 8 4.75Z" />
        <path d="M9.75 9.25h4.5M9.75 12.5h4.5M9.75 15.75h3" />
      </svg>
    );
  }
  if (name === "compare") {
    return (
      <svg {...common}>
        <rect x="3.5" y="5" width="7" height="14" rx="1.4" />
        <rect x="13.5" y="5" width="7" height="14" rx="1.4" />
      </svg>
    );
  }
  if (name === "rera") {
    return (
      <svg {...common}>
        <path d="M12 3.75 18.25 6.2v5.3c0 4-2.5 6.7-6.25 8.75C8.25 18.2 5.75 15.5 5.75 11.5V6.2Z" />
        <path d="m9.15 12 1.85 1.85 3.85-4" />
      </svg>
    );
  }
  if (name === "plan") {
    return (
      <svg {...common}>
        <rect x="4.5" y="4" width="15" height="16" rx="2" />
        <path d="M8 15.5 11 12l2.25 2 3.25-4M8 8h4" />
      </svg>
    );
  }
  if (name === "toggle") {
    return (
      <svg {...common}>
        <path d="m14.5 7-5 5 5 5" />
      </svg>
    );
  }
  return (
    <svg {...common}>
      <rect x="4.5" y="5" width="6.25" height="14" rx="1.3" />
      <path d="M13.25 8.5h6.25M13.25 12h6.25M13.25 15.5h4.25" />
    </svg>
  );
}

function societyLabel(home: PropertyCard): string {
  return home.society_name?.trim() || home.title;
}

function formatCompactPrice(price: number): string {
  if (price <= 0 || !Number.isFinite(price)) return "Price unavailable";
  if (price >= 10_000_000) return `₹${(price / 10_000_000).toFixed(2)} Cr`;
  if (price >= 100_000) return `₹${(price / 100_000).toFixed(1)} L`;
  return `₹${price.toLocaleString("en-IN")}`;
}

function homeStateHint(home: PropertyCard): string | null {
  return home.home_state_display || home.project_status_display || null;
}

type SavedHomesPanelProps = {
  homes: PropertyCard[];
  focusedId: string;
  mode: "discovery" | "property-context" | "workspace";
  onFocus: (propertyId: string) => void;
  onRemove: (propertyId: string) => void;
};

function SavedHomesPanel({
  homes,
  focusedId,
  mode,
  onFocus,
  onRemove,
}: SavedHomesPanelProps) {
  const [showAllHomes, setShowAllHomes] = useState(false);
  const focusedHome = homes.find((home) => home.id === focusedId);
  const previewHomes = homes.slice(0, 4);
  if (
    mode !== "property-context"
    && focusedHome
    && !previewHomes.some((home) => home.id === focusedHome.id)
  ) {
    previewHomes[previewHomes.length - 1] = focusedHome;
  }
  const visibleHomes = showAllHomes ? homes : previewHomes;

  return (
    <div className="workspace-sidebar__saved-panel">
      <div className="workspace-sidebar__shortlist-list">
        {visibleHomes.length === 0 ? (
          <div className="workspace-sidebar__empty">
            <strong>
              {mode === "property-context"
                ? "No other saved homes"
                : "No saved homes yet"}
            </strong>
            <p>Save another home to compare.</p>
          </div>
        ) : null}
        {visibleHomes.map((home) => {
          const name = societyLabel(home);
          const state = homeStateHint(home);
          return (
            <div
              key={home.id}
              className={`workspace-sidebar__home${home.id === focusedId ? " is-active" : ""}`}
            >
              <button
                type="button"
                className="workspace-sidebar__home-open"
                title={name}
                onClick={() => onFocus(home.id)}
              >
                <strong>{name}</strong>
                <span>
                  {[home.area, home.bhk > 0 ? `${home.bhk}BHK` : null]
                    .filter(Boolean)
                    .join(" · ")} · {formatCompactPrice(home.price)}
                </span>
                {state ? <em>{state}</em> : null}
              </button>
              <button
                type="button"
                className="workspace-sidebar__home-remove"
                aria-label={`Remove ${name} from shortlist`}
                title="Remove"
                onClick={() => onRemove(home.id)}
              >
                ×
              </button>
            </div>
          );
        })}
      </div>
      {homes.length > previewHomes.length ? (
        <button
          type="button"
          className="workspace-sidebar__shortlist-toggle"
          aria-expanded={showAllHomes}
          onClick={() => setShowAllHomes((current) => !current)}
        >
          {showAllHomes ? "Show fewer" : "More saved homes"}
        </button>
      ) : null}
    </div>
  );
}

export function WorkspaceSidebar({
  homes,
  compareIds,
  focusedId,
  activeView,
  collapsed,
  reduced,
  mode,
  discoveryHref,
  discoveryResultCount,
  hasDiscoveryContext,
  searchContext,
  onToggle,
  onFocus,
  onRemove,
}: WorkspaceSidebarProps) {
  const navigate = useNavigate();
  const navItems = workspaceNavItems(focusedId, activeView, {
    mode,
    discoveryHref,
    discoveryResultCount: searchContext ? undefined : discoveryResultCount,
    hasDiscoveryContext,
    compareIds,
    propertySearchContext: searchContext,
  });
  const [preferredPanel, setPreferredPanel] = useState<"search" | "saved">(
    () => searchContext && searchContext.selectedId !== focusedId ? "saved" : "search",
  );
  const [, refreshSearchView] = useState(0);
  const [lastDismissed, setLastDismissed] = useState<{
    contextId: string;
    result: PropertySearchResult;
  } | null>(null);
  const { notes } = useNotebook();
  const noteCount = notes.length;
  const savedHomes = mode === "property-context"
    ? homes.filter((home) => home.id !== focusedId)
    : homes;
  const activePanel = searchContext && preferredPanel === "search"
    ? "search"
    : "saved";
  const dismissedPropertyIds = readSearchSpanDismissedIds(searchContext);
  const dismissedIdSet = new Set(dismissedPropertyIds);
  const visibleSearchCount = searchContext
    ? searchContext.results.filter((result) =>
      result.propertyId === searchContext.selectedId
      || !dismissedIdSet.has(result.propertyId)
    ).length
    : 0;
  const canUndoDismissal = Boolean(
    searchContext
    && lastDismissed?.contextId === searchContext.id
    && dismissedIdSet.has(lastDismissed.result.propertyId),
  );

  function dismissSearchResult(result: PropertySearchResult) {
    if (!searchContext || result.propertyId === searchContext.selectedId) return;
    writeSearchSpanDismissedIds(searchContext, [
      ...dismissedPropertyIds,
      result.propertyId,
    ]);
    refreshSearchView((revision) => revision + 1);
    setLastDismissed({ contextId: searchContext.id, result });
  }

  function undoSearchDismissal() {
    if (!searchContext || lastDismissed?.contextId !== searchContext.id) return;
    const propertyId = lastDismissed.result.propertyId;
    writeSearchSpanDismissedIds(
      searchContext,
      dismissedPropertyIds.filter((id) => id !== propertyId),
    );
    refreshSearchView((revision) => revision + 1);
    setLastDismissed(null);
  }

  return (
    <aside
      className={`workspace-sidebar workspace-sidebar--${mode}${collapsed ? " workspace-sidebar--collapsed" : ""}${reduced ? " workspace-sidebar--reduced" : ""}`}
    >
      <div className="workspace-sidebar__brand-row">
        <Link to="/" className="workspace-sidebar__brand" aria-label={`${PUBLIC_BRAND_NAME} home`}>
          <BrandMark size={28} className="workspace-sidebar__mark" />
          {!collapsed && <strong>{PUBLIC_BRAND_NAME}</strong>}
        </Link>
      </div>

      <nav className="workspace-sidebar__nav" aria-label={mode === "property-context" ? "Property navigation" : "Buyer workspace"}>
        {navItems.map((item) => {
          const isDiscoveryReturn = mode === "property-context"
            && hasDiscoveryContext
            && item.view === "browse";
          const title = !item.available ? `${item.label} — save a home first` : undefined;
          const badge = item.view === "notebook" && noteCount > 0 ? noteCount : null;
          const body = (
            <>
              <span className="workspace-sidebar__nav-icon">
                <WorkspaceIcon name={item.icon} />
                {badge ? <em className="workspace-sidebar__nav-badge">{badge}</em> : null}
              </span>
              <span className="workspace-sidebar__nav-label">{item.label}</span>
            </>
          );

          if (!item.available) {
            return (
              <span
                key={item.label}
                className="workspace-sidebar__nav-item is-disabled"
                aria-disabled="true"
                title={title}
              >
                {body}
              </span>
            );
          }

          return (
            <Link
              key={item.label}
              to={item.to}
              className={`workspace-sidebar__nav-item${item.active ? " is-active" : ""}${isDiscoveryReturn ? " workspace-sidebar__nav-item--discovery-return" : ""}`}
              aria-current={item.active ? "page" : undefined}
              title={title}
              onClick={(event) => {
                if (item.view !== "browse") return;
                if (!searchContext) {
                  requestDiscoveryReturn(item.to);
                  return;
                }
                event.preventDefault();
                requestSearchSpanReturn(searchContext);
                const delta = searchSpanReturnDelta(searchContext);
                if (delta !== null) navigate(delta);
                else navigate(searchContext.returnUrl, { replace: true });
              }}
            >
              {body}
            </Link>
          );
        })}
      </nav>

      {!collapsed && (
        <section
          className="workspace-sidebar__shortlist"
          aria-label={searchContext ? "Search and saved homes" : "Saved homes"}
        >
          {searchContext ? (
            <div className="workspace-sidebar__panel-tabs" role="group" aria-label="Home lists">
              <button
                type="button"
                aria-pressed={activePanel === "search"}
                onClick={() => setPreferredPanel("search")}
              >
                Search <span>{visibleSearchCount}</span>
              </button>
              <button
                type="button"
                aria-pressed={activePanel === "saved"}
                onClick={() => setPreferredPanel("saved")}
              >
                Saved <span>{savedHomes.length}</span>
              </button>
            </div>
          ) : (
            <div className="workspace-sidebar__section-title">
              <span>Saved homes</span>
              <strong>{savedHomes.length}</strong>
            </div>
          )}

          {activePanel === "search" && searchContext ? (
            <div className="workspace-sidebar__context-panel">
              <PropertySearchPanel
                context={searchContext}
                dismissedIds={dismissedIdSet}
                onDismiss={dismissSearchResult}
                canUndoDismissal={canUndoDismissal}
                onUndoDismissal={undoSearchDismissal}
              />
            </div>
          ) : (
            <div className="workspace-sidebar__context-panel">
              <SavedHomesPanel
                homes={savedHomes}
                focusedId={focusedId}
                mode={mode}
                onFocus={onFocus}
                onRemove={onRemove}
              />
            </div>
          )}
        </section>
      )}

      <div className="workspace-sidebar__footer">
        <button
          type="button"
          className="workspace-sidebar__rail-toggle"
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          aria-expanded={!collapsed}
          title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          disabled={reduced}
          onClick={onToggle}
        >
          <span className="workspace-sidebar__nav-icon workspace-sidebar__collapse-icon">
            <WorkspaceIcon name="toggle" />
          </span>
        </button>
      </div>
    </aside>
  );
}
