import type { ReactNode } from "react";
import type { SearchResultFocus, SearchResultItem } from "../../lib/types.ts";

type RenderResult = (result: SearchResultItem) => ReactNode;

type Props = {
  focus: SearchResultFocus;
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
export function SearchFocusBoard({ focus, renderResult }: Props) {
  const siblings = focus.sibling_configs ?? [];
  const moreHomes = focus.more_homes ?? [];

  return (
    <div className="search-focus-board">
      <ResultRail
        results={focus.focus_results}
        siblings={siblings}
        renderResult={renderResult}
      />
      {moreHomes.length > 0 ? (
        <ResultRail title="More homes" results={moreHomes} renderResult={renderResult} />
      ) : null}
    </div>
  );
}
