import { useEffect, useMemo, useState } from "react";
import { Helmet } from "react-helmet-async";
import { useParams } from "react-router-dom";
import { getProperty } from "../lib/api.ts";
import type { PropertyDetailResponse } from "../lib/types.ts";
import { useNotebook } from "../hooks/useNotebook.ts";
import { PageState } from "../components/PageState.tsx";
import { NotebookSaveIcon } from "../components/notebook/NotebookSaveIcon.tsx";
import { PlanAssumptionRail } from "../features/home-plan/PlanAssumptionRail.tsx";
import { PlanGraph } from "../features/home-plan/PlanGraph.tsx";
import { PropertyOrigin } from "../features/home-plan/PropertyOrigin.tsx";
import { VerdictBlock } from "../features/home-plan/VerdictBlock.tsx";
import { PlanWhisper } from "../features/home-plan/PlanWhisper.tsx";
import {
  buildBaselinePlanInputs,
  calculateProjection,
  type ConstructionProfile,
  type PlanInputs,
} from "../features/home-plan/model.ts";
import "../features/home-plan/home-plan.css";
import { BUY_VS_RENT } from "../features/home-plan/labels.ts";
import {
  isExplicitlyReadyStatus,
  parsePlanDate,
} from "../features/home-plan/financeEngine.ts";
import { buildPlanSnapshotNote } from "../features/home-plan/planSnapshot.ts";

function constructionProfileFor(data: PropertyDetailResponse): ConstructionProfile {
  const asOfDate = new Date().toISOString().slice(0, 10);
  const stateText = [
    data.project_status,
    data.property.possession_status,
    data.rera?.status,
    data.home_state_display,
  ].filter(Boolean).join(" ").toLowerCase();
  const explicitlyReady = [
    data.project_status,
    data.property.possession_status,
    data.rera?.status,
    data.home_state_display,
  ].filter(Boolean).some((value) => (
    isExplicitlyReadyStatus(String(value))
  ));
  const completionDate = data.rera?.completion_date;
  const parsedCompletion = parsePlanDate(completionDate);
  const completionIsFuture = parsedCompletion ? parsedCompletion.getTime() > Date.now() : false;
  const underConstruction = !explicitlyReady && (
    completionIsFuture
    || ["under construction", "under_construction", "ongoing", "new launch"]
      .some((term) => stateText.includes(term))
  );

  return {
    state: underConstruction ? "under_construction" : "ready",
    asOfDate,
    startDate: data.rera?.start_date,
    completionDate,
    dateSource: completionDate ? "rera" : underConstruction ? "estimated" : "not_applicable",
  };
}

function LoadingPlan() {
  return (
    <div className="home-plan-loading" aria-label={BUY_VS_RENT.loading}>
      <div className="home-plan-loading-pane">
        <div />
        <div />
        <div />
      </div>
    </div>
  );
}

export function HomePlanPage() {
  const { id } = useParams<{ id: string }>();
  const [propertyData, setPropertyData] = useState<PropertyDetailResponse | null>(null);
  const [status, setStatus] = useState<"loading" | "ready" | "not_found" | "error">("loading");
  const [inputs, setInputs] = useState<PlanInputs | null>(null);
  const [previewYear, setPreviewYear] = useState<number | null>(null);
  const [pinnedYear, setPinnedYear] = useState<number | null>(null);
  const [extraEmisPerYear, setExtraEmisPerYear] = useState(0);
  const { isPinned, toggleFact } = useNotebook();

  useEffect(() => {
    if (!id) return;
    let active = true;
    getProperty(id)
      .then((data) => {
        if (!active) return;
        setPropertyData(data);
        setInputs(buildBaselinePlanInputs(data.property.price, constructionProfileFor(data)));
        setStatus("ready");
      })
      .catch((error: unknown) => {
        if (!active) return;
        const message = error instanceof Error ? error.message : "";
        setStatus(message.includes("404") ? "not_found" : "error");
      });
    return () => { active = false; };
  }, [id]);

  const projection = useMemo(
    () => inputs ? calculateProjection(inputs, extraEmisPerYear) : null,
    [inputs, extraEmisPerYear],
  );

  if (!id) return <PageState variant="not_found" context="property" message={BUY_VS_RENT.pickProperty} />;
  if (status === "loading") return <LoadingPlan />;
  if (status === "not_found") return <PageState variant="not_found" context="property" message={BUY_VS_RENT.unavailable} />;
  if (status === "error") return <PageState variant="error" context="property" message={BUY_VS_RENT.loadError} />;
  if (!propertyData || !inputs || !projection) return null;

  const property = propertyData.property;
  const baseline = buildBaselinePlanInputs(property.price, constructionProfileFor(propertyData));
  const defaultYear = Math.min(inputs.holdingPeriodYears, projection.points.length - 1);
  const activeYear = previewYear ?? pinnedYear ?? defaultYear;
  const activePoint = projection.points[Math.min(activeYear, projection.points.length - 1)];
  const buyWins = activePoint.buyNetWorth >= activePoint.rentNetWorth;
  const advantage = Math.abs(activePoint.buyNetWorth - activePoint.rentNetWorth);
  const planSnapshot = buildPlanSnapshotNote({
    propertyId: id,
    inputs,
    projection,
    activeYear,
    activePoint,
    extraEmisPerYear,
  });
  const snapshotSaved = isPinned(planSnapshot.catalogKey);

  const updateInput = <K extends keyof PlanInputs>(key: K, value: PlanInputs[K]) => {
    setInputs((current) => current ? { ...current, [key]: value } : current);
  };

  const resetPlan = () => {
    setInputs(baseline);
    setExtraEmisPerYear(0);
    setPinnedYear(null);
    setPreviewYear(null);
  };

  const savePlanSnapshot = () => {
    toggleFact({
      propertyId: id,
      catalogKey: planSnapshot.catalogKey,
      title: planSnapshot.title,
      detail: planSnapshot.detail,
      source: planSnapshot.source,
      labels: planSnapshot.labels,
      kind: "plan",
    });
  };

  return (
    <div className="home-plan-shell home-plan-shell--view-net-worth">
      <Helmet>
        <title>{property.title} — {BUY_VS_RENT.pageTitle} | OpenEstates</title>
        <meta name="description" content={`Compare buying ${property.title} with renting and investing over time.`} />
      </Helmet>

      <div className="home-plan-body">
        <div className="home-plan-main">
          <div className="home-plan-canvas">
            <section className="home-plan-hero" aria-label="Buy vs rent overview">
              <div className="home-plan-hero__rail">
                <PropertyOrigin
                  propertyId={id}
                  title={property.title}
                  area={property.area}
                  price={property.price}
                />
              </div>
            </section>

            <VerdictBlock
              activeYear={activeYear}
              buyWins={buyWins}
              advantage={advantage}
              aside={<PlanWhisper />}
              action={(
                <button
                  type="button"
                  className={`home-plan-snapshot-button${snapshotSaved ? " is-saved" : ""}`}
                  aria-label={snapshotSaved ? "Remove plan snapshot" : "Save plan snapshot"}
                  aria-pressed={snapshotSaved}
                  title={snapshotSaved ? "Saved" : "Save"}
                  onClick={savePlanSnapshot}
                >
                  <NotebookSaveIcon filled={snapshotSaved} size={18} />
                </button>
              )}
            />

            <PlanAssumptionRail
              inputs={inputs}
              extraEmisPerYear={extraEmisPerYear}
              onInputChange={updateInput}
              onExtraEmisChange={setExtraEmisPerYear}
              onReset={resetPlan}
            />

            <section className="home-plan-stage" aria-label="Projection over time">
              <PlanGraph
                projection={projection}
                activeYear={activeYear}
                onPreviewYearChange={setPreviewYear}
                onPinYear={setPinnedYear}
              />
            </section>
          </div>
        </div>
      </div>
    </div>
  );
}
