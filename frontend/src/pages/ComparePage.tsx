import { useEffect, useMemo, useState } from "react";
import { Helmet } from "react-helmet-async";
import { Link, useSearchParams } from "react-router-dom";
import { SocietyComparisonMatrix } from "../components/compare/SocietyComparisonMatrix.tsx";
import { getProperties, getProperty } from "../lib/api.ts";
import type { PropertyCard, PropertyDetailResponse } from "../lib/types.ts";
import "../styles/workspace.css";

const MAX_COMPARE_HOMES = 4;

type LoadStatus = "loading" | "ready" | "error" | "empty";

type CompareLoadState = {
  requestKey: string;
  status: LoadStatus;
  selectedHomes: PropertyCard[];
  catalog: PropertyCard[];
  details: PropertyDetailResponse[];
};

function parseComparedIds(value: string): string[] {
  return [...new Set(value.split(",").map((id) => id.trim()).filter(Boolean))]
    .slice(0, MAX_COMPARE_HOMES);
}

function CompareLoading() {
  return (
    <div className="compare-loading" aria-label="Loading compared homes">
      <div className="compare-loading__main">
        <div className="compare-loading__line compare-loading__line--short" />
        <div className="compare-loading__line compare-loading__line--title" />
        <div className="compare-loading__table" />
        <div className="compare-loading__table compare-loading__table--small" />
      </div>
    </div>
  );
}

function CompareUnavailable({
  variant,
  selectedCount = 0,
}: {
  variant: "error" | "empty";
  selectedCount?: number;
}) {
  const needsOneMore = variant === "empty" && selectedCount === 1;
  return (
    <div className="compare-unavailable">
      <span>{variant === "error" ? "Comparison unavailable" : "Compare"}</span>
      <h1>
        {variant === "error"
          ? "We couldn't load the decision workspace."
          : needsOneMore
            ? "Add one more home to compare."
            : "No homes to compare yet."}
      </h1>
      <p>
        {variant === "error"
          ? "Property data could not be loaded. Try again or return to discovery."
          : needsOneMore
            ? "Compare starts when at least two shortlisted homes are available side by side."
            : "Save two homes to evaluate price, society, and evidence side by side."}
      </p>
      <Link to="/">Browse homes</Link>
    </div>
  );
}

export function ComparePage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const idsParam = searchParams.get("ids") ?? "";
  const requestedIds = useMemo(() => parseComparedIds(idsParam), [idsParam]);
  const requestKey = requestedIds.join(",") || "default";
  const [loadState, setLoadState] = useState<CompareLoadState>({
    requestKey,
    status: "loading",
    selectedHomes: [],
    catalog: [],
    details: [],
  });
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    const controller = new AbortController();

    getProperties({ signal: controller.signal })
      .then(async (properties) => {
        const byId = new Map(properties.map((property) => [property.id, property]));
        const requestedHomes = requestedIds
          .map((id) => byId.get(id))
          .filter((property): property is PropertyCard => Boolean(property));
        const selectedHomes = requestedIds.length > 0 ? requestedHomes : [];

        if (selectedHomes.length < 2) {
          setLoadState({
            requestKey,
            status: "empty",
            selectedHomes,
            catalog: properties,
            details: [],
          });
          return;
        }

        const detailResults = await Promise.allSettled(
          selectedHomes.map((home) => getProperty(home.id, { signal: controller.signal })),
        );
        if (controller.signal.aborted) return;
        const details = detailResults
          .filter((result): result is PromiseFulfilledResult<PropertyDetailResponse> =>
            result.status === "fulfilled"
          )
          .map((result) => result.value);

        setLoadState({
          requestKey,
          status: "ready",
          selectedHomes,
          catalog: properties,
          details,
        });

        const selectedIds = selectedHomes.map((property) => property.id).join(",");
        if (requestedIds.length > 0 && selectedIds !== idsParam) {
          const next = new URLSearchParams(window.location.search);
          next.set("ids", selectedIds);
          next.set("focus", selectedHomes[0].id);
          setSearchParams(next, { replace: true });
        }
      })
      .catch((error: unknown) => {
        if (error instanceof DOMException && error.name === "AbortError") return;
        setLoadState({
          requestKey,
          status: "error",
          selectedHomes: [],
          catalog: [],
          details: [],
        });
      });

    return () => controller.abort();
  }, [idsParam, requestKey, requestedIds, setSearchParams]);

  const isCurrentRequest = loadState.requestKey === requestKey;
  const status = isCurrentRequest ? loadState.status : "loading";
  const selectedHomes = isCurrentRequest ? loadState.selectedHomes : [];
  const catalog = isCurrentRequest ? loadState.catalog : [];
  const details = isCurrentRequest ? loadState.details : [];

  if (status === "loading") return <CompareLoading />;
  if (status === "error" || status === "empty") {
    return <CompareUnavailable variant={status} selectedCount={selectedHomes.length} />;
  }

  function copyComparisonLink() {
    void navigator.clipboard.writeText(window.location.href)
      .then(() => setCopied(true));
  }

  return (
    <div className="compare-workspace">
      <Helmet>
        <title>Compare shortlisted societies | OpenEstates</title>
        <meta
          name="description"
          content="Compare society and BHK price ranges on shared decision scales."
        />
        <meta name="robots" content="noindex" />
      </Helmet>

      <header className="compare-workspace__bar">
        <div>
          <span>Shortlist</span>
          <i>/</i>
          <strong>Compare</strong>
        </div>
        <button type="button" onClick={copyComparisonLink}>
          {copied ? "Link copied" : "Share"}
        </button>
      </header>

      <div className="compare-workspace__content">
        <header className="compare-workspace__intro">
          <div>
            <span>Side by side</span>
            <h1>Same budget. Different tradeoffs.</h1>
            <p>Saved notes beside the facts that stay comparable.</p>
          </div>
        </header>

        <SocietyComparisonMatrix
          selectedHomes={selectedHomes}
          catalog={catalog}
          details={details}
        />
      </div>
    </div>
  );
}
