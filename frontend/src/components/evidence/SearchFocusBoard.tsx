import type { ReactNode } from "react";
import type { SearchResultItem, SearchResultSet } from "../../lib/types.ts";

type RenderResult = (result: SearchResultItem) => ReactNode;

type Props = {
  resultSets: SearchResultSet[];
  renderResult: RenderResult;
};

function ResultRail({
  title,
  results,
  renderResult,
  siblings = [],
}: {
  title?: string;
  results: SearchResultItem[];
  renderResult: RenderResult;
  siblings?: SearchResultItem[];
}) {
  const hasSiblings = siblings.length > 0;

  if (results.length === 0 && !hasSiblings) return null;

  return (
    <section className="search-focus-rail">
      {title ? (
        <header className="search-focus-rail__header">
          <h2 className="search-focus-rail__title">{title}</h2>
        </header>
      ) : null}
      <div className="search-focus-rail__row">
        {results.map((result) => (
          <div key={result.id} className="search-focus-rail__cell">
            {renderResult(result)}
          </div>
        ))}
        {hasSiblings ? (
          <>
            <div
              className="search-focus-rail__plus"
              role="separator"
              aria-label="Also available at this project"
            >
              <span className="search-focus-rail__plus-mark" aria-hidden="true">
                +
              </span>
            </div>
            {siblings.map((result) => (
              <div key={result.id} className="search-focus-rail__cell search-focus-rail__cell--sibling">
                {renderResult(result)}
              </div>
            ))}
          </>
        ) : null}
      </div>
    </section>
  );
}

/**
 * Journey rails for search: asked config, small +, sibling configs, then More homes.
 */
export function SearchFocusBoard({ resultSets, renderResult }: Props) {
  return (
    <div className="search-focus-board">
      {resultSets.map((set) => {
        const results = set.results.filter((result) => result.match_tier !== "supported");
        const siblings = set.results.filter((result) => result.match_tier === "supported");
        return (
          <ResultRail
            key={set.branchId}
            title={set.label === "Matches" ? undefined : set.label}
            results={results}
            siblings={siblings}
            renderResult={renderResult}
          />
        );
      })}
    </div>
  );
}
