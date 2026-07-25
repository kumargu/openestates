import { useEffect, useMemo, useState } from "react";
import { Helmet } from "react-helmet-async";
import { Link, useSearchParams } from "react-router-dom";
import { SocietyComparisonChart } from "../components/compare/SocietyComparisonChart.tsx";
import { getProperties } from "../lib/api.ts";
import {
  defaultComparedHomes,
  normalizeComparedSocieties,
} from "../lib/compare.ts";
import type { PropertyCard } from "../lib/types.ts";
import "../styles/workspace.css";

const MAX_COMPARE_HOMES = 10;
const DEFAULT_COMPARE_HOMES = 3;
const MIN_COMPARE_SOCIETIES = 2;

type LoadStatus = "loading" | "ready" | "error" | "empty";

type CompareLoadState = {
  requestKey: string;
  status: LoadStatus;
  selectedHomes: PropertyCard[];
  catalog: PropertyCard[];
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

function CompareUnavailable({ variant }: { variant: "error" | "empty" }) {
  return (
    <div className="compare-unavailable">
      <span>{variant === "error" ? "Comparison unavailable" : "Add another home"}</span>
      <h1>
        {variant === "error"
          ? "We couldn't load the decision workspace."
          : "Compare needs at least two homes."}
      </h1>
      <p>
        {variant === "error"
          ? "Property data could not be loaded. Try again or return to discovery."
          : "Choose two or more homes before opening the shared decision view."}
      </p>
      <Link to="/">Browse homes</Link>
    </div>
  );
}

function societyIdentity(property: PropertyCard): string {
  return property.society_name?.trim().toLocaleLowerCase()
    || property.title.trim().toLocaleLowerCase();
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
  });
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    const controller = new AbortController();

    getProperties({ signal: controller.signal })
      .then((properties) => {
        const byId = new Map(properties.map((property) => [property.id, property]));
        const requestedHomes = requestedIds
          .map((id) => byId.get(id))
          .filter((property): property is PropertyCard => Boolean(property));
        const selectedHomes = requestedHomes.length >= 2
          ? normalizeComparedSocieties(
            requestedHomes,
            properties,
            MIN_COMPARE_SOCIETIES,
            MAX_COMPARE_HOMES,
          )
          : defaultComparedHomes(properties, DEFAULT_COMPARE_HOMES);

        if (selectedHomes.length < 2) {
          setLoadState({
            requestKey,
            status: "empty",
            selectedHomes: [],
            catalog: properties,
          });
          return;
        }

        setLoadState({
          requestKey,
          status: "ready",
          selectedHomes,
          catalog: properties,
        });

        const selectedIds = selectedHomes.map((property) => property.id).join(",");
        if (selectedIds !== idsParam) {
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
        });
      });

    return () => controller.abort();
  }, [idsParam, requestKey, requestedIds, setSearchParams]);

  const isCurrentRequest = loadState.requestKey === requestKey;
  const status = isCurrentRequest ? loadState.status : "loading";
  const selectedHomes = isCurrentRequest ? loadState.selectedHomes : [];
  const catalog = isCurrentRequest ? loadState.catalog : [];

  if (status === "loading") return <CompareLoading />;
  if (status === "error" || status === "empty") {
    return <CompareUnavailable variant={status} />;
  }

  const societyCount = new Set(selectedHomes.map(societyIdentity)).size;

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
            <span>Decision view</span>
            <h1>Same budget. Different tradeoffs.</h1>
            <p>
              {societyCount} societ{societyCount === 1 ? "y" : "ies"} on one shared price scale
            </p>
          </div>
        </header>

        <SocietyComparisonChart
          selectedHomes={selectedHomes}
          catalog={catalog}
        />
      </div>
    </div>
  );
}
