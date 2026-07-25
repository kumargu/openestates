import { useEffect, useMemo, useState } from "react";
import { Helmet } from "react-helmet-async";
import { useParams } from "react-router-dom";
import { getProperty } from "../lib/api.ts";
import type { PropertyDetailResponse } from "../lib/types.ts";
import { PageState } from "../components/PageState.tsx";
import { PlanAssumptionRail } from "../features/home-plan/PlanAssumptionRail.tsx";
import { PlanGraph } from "../features/home-plan/PlanGraph.tsx";
import { PlanViewTabs, type PlanView } from "../features/home-plan/PlanViewTabs.tsx";
import { PropertyOrigin } from "../features/home-plan/PropertyOrigin.tsx";
import { RepaymentJourney } from "../features/home-plan/RepaymentJourney.tsx";
import { VerdictBlock } from "../features/home-plan/VerdictBlock.tsx";
import { PlanWhisper } from "../features/home-plan/PlanWhisper.tsx";
import {
  buildBaselinePlanInputs,
  calculateLoanJourney,
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
  const [view, setView] = useState<PlanView>("netWorth");
  const [horizon, setHorizon] = useState(10);
  const [previewYear, setPreviewYear] = useState<number | null>(null);
  const [extraEmisPerYear, setExtraEmisPerYear] = useState(2);
  const [loanYear, setLoanYear] = useState(5);

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

  const projection = useMemo(() => inputs ? calculateProjection(inputs) : null, [inputs]);
  const loanJourney = useMemo(() => inputs ? calculateLoanJourney(inputs, extraEmisPerYear) : null, [inputs, extraEmisPerYear]);
  const baselineLoanJourney = useMemo(() => inputs ? calculateLoanJourney(inputs, 0) : null, [inputs]);

  if (!id) return <PageState variant="not_found" context="property" message={BUY_VS_RENT.pickProperty} />;
  if (status === "loading") return <LoadingPlan />;
  if (status === "not_found") return <PageState variant="not_found" context="property" message={BUY_VS_RENT.unavailable} />;
  if (status === "error") return <PageState variant="error" context="property" message={BUY_VS_RENT.loadError} />;
  if (!propertyData || !inputs || !projection || !loanJourney || !baselineLoanJourney) return null;

  const property = propertyData.property;
  const baseline = buildBaselinePlanInputs(property.price, constructionProfileFor(propertyData));
  const activeYear = previewYear ?? horizon;
  const activePoint = projection.points[Math.min(activeYear, projection.points.length - 1)];
  const buyWins = activePoint.buyNetWorth >= activePoint.rentNetWorth;
  const advantage = Math.abs(activePoint.buyNetWorth - activePoint.rentNetWorth);

  const updateInput = <K extends keyof PlanInputs>(key: K, value: PlanInputs[K]) => {
    setInputs((current) => current ? { ...current, [key]: value } : current);
  };

  const resetPlan = () => {
    setInputs(baseline);
    setExtraEmisPerYear(2);
  };

  const chooseHorizon = (year: number) => {
    setHorizon(year);
    setPreviewYear(null);
  };

  const changeView = (nextView: PlanView) => {
    setView(nextView);
    setPreviewYear(null);
  };

  const viewChapterClass = view === "netWorth" ? "net-worth" : view;

  return (
    <div className={`home-plan-shell home-plan-shell--view-${viewChapterClass}`}>
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
                <div className="home-plan-hero__actions">
                  <PlanViewTabs view={view} onChange={changeView} compact />
                </div>
              </div>

              <div className="home-plan-hero__stage">
                <PlanWhisper />
              </div>
            </section>

            {view === "payoff" ? (
              <RepaymentJourney
                journey={loanJourney}
                baselineJourney={baselineLoanJourney}
                extraEmisPerYear={extraEmisPerYear}
                selectedYear={loanYear}
                onExtraEmisChange={setExtraEmisPerYear}
                onSelectYear={setLoanYear}
              />
            ) : (
              <>
                <VerdictBlock
                  activeYear={activeYear}
                  horizon={horizon}
                  buyWins={buyWins}
                  advantage={advantage}
                  paymentSchedule={projection.paymentSchedule}
                  possessionDate={projection.possessionDate}
                  constructionDateSource={projection.constructionDateSource}
                  isUnderConstruction={projection.possessionMonth > inputs.purchaseYear * 12}
                  onHorizonChange={chooseHorizon}
                />

                <PlanAssumptionRail
                  inputs={inputs}
                  onInputChange={updateInput}
                  onReset={resetPlan}
                />

                <section className="home-plan-stage" aria-label="Projection over time">
                  <PlanGraph
                    projection={projection}
                    horizon={horizon}
                    onHorizonChange={chooseHorizon}
                    onPreviewYearChange={setPreviewYear}
                  />
                </section>
              </>
            )}
          </div>
        </div>
      </div>

    </div>
  );
}
