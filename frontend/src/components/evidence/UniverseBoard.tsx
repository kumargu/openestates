import type { ReactNode } from "react";
import type { SearchResultItem } from "../../lib/types.ts";
import type { PropertyEvidenceResponse } from "../../lib/types.ts";
import { clusterSearchResults, type UniverseCluster } from "../../lib/evidence.ts";

type RenderResult = (result: SearchResultItem) => ReactNode;

type Props = {
  results: SearchResultItem[];
  evidenceById: Map<string, PropertyEvidenceResponse>;
  renderResult: RenderResult;
  learningGaps?: string[];
};

function UniverseClusterSection({
  cluster,
  renderResult,
}: {
  cluster: UniverseCluster;
  renderResult: RenderResult;
}) {
  return (
    <section className={`universe-cluster universe-cluster--${cluster.id}`}>
      <header className="universe-cluster__header">
        <div>
          <h2 className="universe-cluster__title">{cluster.label}</h2>
          <p className="universe-cluster__hint">{cluster.hint}</p>
        </div>
        <span className="universe-cluster__count">
          {cluster.results.length} home{cluster.results.length === 1 ? "" : "s"}
        </span>
      </header>
      <div className="universe-cluster__grid">
        {cluster.results.map((result) => (
          <div key={result.id} className="universe-cluster__cell">
            {renderResult(result)}
          </div>
        ))}
      </div>
    </section>
  );
}

export function UniverseBoard({ results, evidenceById, renderResult, learningGaps }: Props) {
  const clusters = clusterSearchResults(results, evidenceById);

  if (clusters.length === 0) {
    return (
      <div className="results-grid">
        {results.map((result) => (
          <div key={result.id}>{renderResult(result)}</div>
        ))}
      </div>
    );
  }

  return (
    <div className="universe-board">
      <header className="universe-board__intro">
        <h2 className="universe-board__title">Your property universe</h2>
        <p className="universe-board__subtitle">
          Grouped by fit, proof strength, and value angle — not a flat broker list.
        </p>
        {learningGaps && learningGaps.length > 0 && (
          <p className="universe-board__gaps">
            Still learning: {learningGaps.slice(0, 2).join(" · ")}
          </p>
        )}
      </header>

      {clusters.map((cluster) => (
        <UniverseClusterSection
          key={cluster.id}
          cluster={cluster}
          renderResult={renderResult}
        />
      ))}
    </div>
  );
}
