import type { ReactNode } from "react";
import type { SearchResultItem } from "../../lib/types.ts";
import type { PropertyEvidenceResponse } from "../../lib/types.ts";
import { clusterSearchResults, type UniverseCluster } from "../../lib/evidence.ts";

type RenderResult = (result: SearchResultItem) => ReactNode;

type Props = {
  results: SearchResultItem[];
  evidenceById: Map<string, PropertyEvidenceResponse>;
  renderResult: RenderResult;
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
        <h2 className="universe-cluster__title">{cluster.label}</h2>
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

export function UniverseBoard({ results, evidenceById, renderResult }: Props) {
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
