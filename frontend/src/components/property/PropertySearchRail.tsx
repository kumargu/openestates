import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { Link } from "react-router-dom";
import { formatListingPrice } from "../../lib/listing-price.ts";
import type {
  PropertySearchContext,
  PropertySearchResult,
} from "../../lib/navigationContext.ts";
import {
  propertyHrefWithSearchSpan,
  readSearchSpanDismissedIds,
  rotatePropertySearchResults,
  SEARCH_SPAN_VIEW_CHANGED_EVENT,
} from "../../lib/navigationContext.ts";
import "../../styles/property-search-rail.css";

type Props = {
  context: PropertySearchContext;
};

type PanelProps = Props & {
  dismissedIds: ReadonlySet<string>;
  onDismiss: (result: PropertySearchResult) => void;
  canUndoDismissal: boolean;
  onUndoDismissal: () => void;
  onSelect: (propertyId: string) => void;
};

function resultName(result: PropertySearchResult): string {
  return result.societyName || result.title;
}

function resultMeta(result: PropertySearchResult): string {
  return [
    result.area,
    result.bhk ? `${result.bhk} BHK` : null,
    result.sqft ? `${result.sqft.toLocaleString("en-IN")} sqft` : null,
  ].filter(Boolean).join(" · ");
}

function resultCompactMeta(result: PropertySearchResult): string {
  return [
    result.area,
    result.bhk ? `${result.bhk}BHK` : null,
    result.price ? formatListingPrice({ price: result.price }) : null,
  ].filter(Boolean).join(" · ");
}

function resultHref(
  context: PropertySearchContext,
  result: PropertySearchResult,
): string {
  return propertyHrefWithSearchSpan(result.propertyId, context);
}

export function PropertySearchPanel({
  context,
  dismissedIds,
  onDismiss,
  canUndoDismissal,
  onUndoDismissal,
  onSelect,
}: PanelProps) {
  const listRef = useRef<HTMLOListElement>(null);
  const [previewResult, setPreviewResult] = useState<PropertySearchResult | null>(null);
  const availableResults = context.results.filter((result) =>
    result.propertyId === context.selectedId || !dismissedIds.has(result.propertyId)
  );
  const visibleResults = rotatePropertySearchResults(availableResults, context.selectedId);
  const visiblePositions = new Map(
    availableResults.map((result, index) => [result.propertyId, index + 1]),
  );
  const selectedResult = visibleResults[0];
  const previewMeta = previewResult ? resultMeta(previewResult) : "";

  useLayoutEffect(() => {
    listRef.current
      ?.querySelector<HTMLElement>("[aria-current='page']")
      ?.scrollIntoView({ block: "nearest" });
  }, [context.selectedId]);

  function handleKeyDown(event: KeyboardEvent<HTMLOListElement>) {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    const controls = Array.from(
      listRef.current?.querySelectorAll<HTMLButtonElement>(
        "button.property-search-panel__result-open",
      ) ?? [],
    );
    if (controls.length === 0) return;
    const focusedIndex = controls.findIndex((control) => control === document.activeElement);
    let nextIndex = focusedIndex < 0 ? 0 : focusedIndex;
    if (event.key === "ArrowDown") nextIndex = Math.min(controls.length - 1, nextIndex + 1);
    if (event.key === "ArrowUp") nextIndex = Math.max(0, nextIndex - 1);
    if (event.key === "Home") nextIndex = 0;
    if (event.key === "End") nextIndex = controls.length - 1;
    event.preventDefault();
    controls[nextIndex]?.focus();
  }

  if (!selectedResult) return null;
  const total = availableResults.length;

  return (
    <div className="property-search-panel">
      <ol
        ref={listRef}
        className="property-search-panel__results"
        onKeyDown={handleKeyDown}
      >
        {visibleResults.map((result) => {
          const selected = result.propertyId === context.selectedId;
          return (
            <li
              key={result.propertyId}
              className={`workspace-sidebar__home property-search-panel__result${selected ? " is-active" : ""}`}
              onMouseEnter={() => setPreviewResult(result)}
              onMouseLeave={() => setPreviewResult(null)}
              onFocus={() => setPreviewResult(result)}
              onBlur={(event) => {
                if (!event.currentTarget.contains(event.relatedTarget)) {
                  setPreviewResult(null);
                }
              }}
            >
              <button
                type="button"
                className="workspace-sidebar__home-open property-search-panel__result-open"
                aria-current={selected ? "page" : undefined}
                aria-label={selected
                  ? `Current home, ${resultName(result)}, result ${visiblePositions.get(result.propertyId)} of ${total}`
                  : `${resultName(result)}, result ${visiblePositions.get(result.propertyId)} of ${total}`}
                onClick={() => onSelect(result.propertyId)}
              >
                <strong>{resultName(result)}</strong>
                <span>{resultCompactMeta(result)}</span>
                {result.stateDisplay ? <em>{result.stateDisplay}</em> : null}
              </button>
              {!selected ? (
                <button
                  type="button"
                  className="workspace-sidebar__home-remove property-search-panel__dismiss"
                  aria-label={`Hide ${resultName(result)} from this search`}
                  title="Hide from this search"
                  onClick={(event) => {
                    const row = event.currentTarget.closest("li");
                    const nextControl = row?.nextElementSibling?.querySelector<HTMLButtonElement>(
                      "button.property-search-panel__result-open",
                    ) ?? row?.previousElementSibling?.querySelector<HTMLButtonElement>(
                      "button.property-search-panel__result-open",
                    );
                    setPreviewResult(null);
                    onDismiss(result);
                    window.requestAnimationFrame(() => nextControl?.focus());
                  }}
                >
                  ×
                </button>
              ) : null}
            </li>
          );
        })}
      </ol>
      {previewResult && previewResult.propertyId !== context.selectedId ? (
        <div className="property-search-panel__preview" aria-hidden="true">
          <strong>{resultName(previewResult)}</strong>
          {previewMeta ? <span>{previewMeta}</span> : null}
          {previewResult.price ? (
            <em>{formatListingPrice({ price: previewResult.price })}</em>
          ) : null}
        </div>
      ) : null}
      {canUndoDismissal ? (
        <div className="property-search-panel__undo" role="status">
          <span>Home hidden</span>
          <button type="button" onClick={onUndoDismissal}>Undo</button>
        </div>
      ) : null}
    </div>
  );
}

export function PropertySearchStrip({ context }: Props) {
  const [, refreshViewState] = useState(0);
  useEffect(() => {
    const refresh = () => refreshViewState((revision) => revision + 1);
    window.addEventListener(SEARCH_SPAN_VIEW_CHANGED_EVENT, refresh);
    window.addEventListener("storage", refresh);
    return () => {
      window.removeEventListener(SEARCH_SPAN_VIEW_CHANGED_EVENT, refresh);
      window.removeEventListener("storage", refresh);
    };
  }, []);
  const dismissedIds = new Set(readSearchSpanDismissedIds(context));
  const results = context.results.filter((result) =>
    result.propertyId === context.selectedId || !dismissedIds.has(result.propertyId)
  );
  const selectedIndex = results.findIndex((result) =>
    result.propertyId === context.selectedId
  );
  const selectedResult = results[selectedIndex];
  if (!selectedResult) return null;
  const position = selectedIndex + 1;
  const total = results.length;
  const visibleTotal = results.length;
  const previousResult = visibleTotal > 1
    ? results[(selectedIndex - 1 + visibleTotal) % visibleTotal]
    : undefined;
  const nextResult = visibleTotal > 1
    ? results[(selectedIndex + 1) % visibleTotal]
    : undefined;

  return (
    <nav className="property-search-strip" aria-label="Homes from your search">
      {previousResult ? (
        <Link
          to={resultHref(context, previousResult)}
          aria-label={`Previous result, ${resultName(previousResult)}`}
        >
          ←
        </Link>
      ) : <span aria-hidden="true" />}
      <div className="property-search-strip__summary">
        <span>{resultName(selectedResult)}</span>
        <strong>{position} of {total} {total === 1 ? "home" : "homes"}</strong>
      </div>
      {nextResult ? (
        <Link
          to={resultHref(context, nextResult)}
          aria-label={`Next result, ${resultName(nextResult)}`}
        >
          →
        </Link>
      ) : <span aria-hidden="true" />}
    </nav>
  );
}
