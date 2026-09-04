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
  readSearchJourneyNotForMeIds,
  SEARCH_JOURNEY_PREFERENCES_CHANGED_EVENT,
  searchJourneyCursor,
} from "../../lib/navigationContext.ts";
import "../../styles/property-search-rail.css";

type Props = {
  context: PropertySearchContext;
};

type PanelProps = Props & {
  notForMeIds: ReadonlySet<string>;
  onMarkNotForMe: (propertyId: string) => void;
  canUndoNotForMe: boolean;
  onUndoNotForMe: () => void;
  onSelect: (propertyId: string) => void;
};

function resultName(result: PropertySearchResult): string {
  return result.societyName || result.title;
}

function resultCompactMeta(result: PropertySearchResult): string {
  return [
    result.area,
    result.bhk ? `${result.bhk}BHK` : null,
    result.price ? formatListingPrice({ price: result.price }) : null,
  ].filter(Boolean).join(" · ");
}

export function PropertySearchPanel({
  context,
  notForMeIds,
  onMarkNotForMe,
  canUndoNotForMe,
  onUndoNotForMe,
  onSelect,
}: PanelProps) {
  const listRef = useRef<HTMLOListElement>(null);
  const visibleResults = context.results.filter((result) =>
    result.propertyId === context.selectedId || !notForMeIds.has(result.propertyId)
  );
  const originalPositions = new Map(
    context.results.map((result, index) => [result.propertyId, index + 1]),
  );
  const cursor = searchJourneyCursor(context, notForMeIds);

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

  if (!cursor) return null;
  const { position: selectedPosition, total } = cursor;

  return (
    <div className="property-search-panel">
      <header className="property-search-panel__header">
        <strong>{context.queryLabel}</strong>
        <span>{selectedPosition} of {total} {total === 1 ? "home" : "homes"}</span>
      </header>
      <ol
        ref={listRef}
        className="property-search-panel__results"
        onKeyDown={handleKeyDown}
      >
        {visibleResults.map((result) => {
          const selected = result.propertyId === context.selectedId;
          const position = originalPositions.get(result.propertyId);
          return (
            <li
              key={result.propertyId}
              className={`workspace-sidebar__home property-search-panel__result${selected ? " is-active" : ""}`}
            >
              <button
                type="button"
                className="workspace-sidebar__home-open property-search-panel__result-open"
                aria-current={selected ? "page" : undefined}
                aria-label={selected
                  ? `Current home, ${resultName(result)}, result ${position} of ${total}`
                  : `${resultName(result)}, result ${position} of ${total}`}
                onClick={() => onSelect(result.propertyId)}
              >
                <strong>{resultName(result)}</strong>
                <span>{resultCompactMeta(result)}</span>
                {result.stateDisplay ? <em>{result.stateDisplay}</em> : null}
              </button>
              {!selected ? (
                <button
                  type="button"
                  className="property-search-panel__preference"
                  aria-label={`Mark ${resultName(result)} as not for me`}
                  onClick={(event) => {
                    const row = event.currentTarget.closest("li");
                    const nextControl = row?.nextElementSibling?.querySelector<HTMLButtonElement>(
                      "button.property-search-panel__result-open",
                    ) ?? row?.previousElementSibling?.querySelector<HTMLButtonElement>(
                      "button.property-search-panel__result-open",
                    );
                    onMarkNotForMe(result.propertyId);
                    window.requestAnimationFrame(() => nextControl?.focus());
                  }}
                >
                  Not for me
                </button>
              ) : null}
            </li>
          );
        })}
      </ol>
      {canUndoNotForMe ? (
        <div className="property-search-panel__undo" role="status">
          <span>Marked not for me</span>
          <button type="button" onClick={onUndoNotForMe}>Undo</button>
        </div>
      ) : null}
    </div>
  );
}

export function PropertySearchStrip({ context }: Props) {
  const [, refreshViewState] = useState(0);
  useEffect(() => {
    const refresh = () => refreshViewState((revision) => revision + 1);
    window.addEventListener(SEARCH_JOURNEY_PREFERENCES_CHANGED_EVENT, refresh);
    window.addEventListener("storage", refresh);
    return () => {
      window.removeEventListener(SEARCH_JOURNEY_PREFERENCES_CHANGED_EVENT, refresh);
      window.removeEventListener("storage", refresh);
    };
  }, []);
  const notForMeIds = new Set(readSearchJourneyNotForMeIds(context));
  const cursor = searchJourneyCursor(context, notForMeIds);
  if (!cursor) return null;
  const { nextResult, position, previousResult, total } = cursor;

  return (
    <nav className="property-search-strip" aria-label="Homes from your search">
      {previousResult ? (
        <Link
          to={propertyHrefWithSearchSpan(previousResult.propertyId, context)}
          aria-label={`Previous result, ${resultName(previousResult)}`}
        >
          ←
        </Link>
      ) : (
        <button type="button" aria-label="No previous result" disabled>←</button>
      )}
      <div className="property-search-strip__summary">
        <strong>{context.queryLabel}</strong>
        <span>{position} of {total} {total === 1 ? "home" : "homes"}</span>
      </div>
      {nextResult ? (
        <Link
          to={propertyHrefWithSearchSpan(nextResult.propertyId, context)}
          aria-label={`Next result, ${resultName(nextResult)}`}
        >
          →
        </Link>
      ) : (
        <button type="button" aria-label="No next result" disabled>→</button>
      )}
    </nav>
  );
}
