import { useEffect, useMemo, useRef, useState } from "react";
import { Helmet } from "react-helmet-async";
import { useParams } from "react-router-dom";
import { getProperty } from "../lib/api.ts";
import type { PropertyDetailResponse } from "../lib/types.ts";
import { PageState } from "../components/PageState.tsx";
import { MilestoneHint } from "../features/home-plan/MilestoneHint.tsx";
import { PlanAssumptionRail } from "../features/home-plan/PlanAssumptionRail.tsx";
import { PlanGraph, type PlanScenarioId } from "../features/home-plan/PlanGraph.tsx";
import { PlanViewTabs, type PlanView } from "../features/home-plan/PlanViewTabs.tsx";
import { PropertyOrigin } from "../features/home-plan/PropertyOrigin.tsx";
import { RepaymentJourney } from "../features/home-plan/RepaymentJourney.tsx";
import { VerdictBlock } from "../features/home-plan/VerdictBlock.tsx";
import { PlanWhisper } from "../features/home-plan/PlanWhisper.tsx";
import {
  buildBaselinePlanInputs,
  calculateLoanJourney,
  calculateProjection,
  formatCurrency,
  maximumDownPaymentLakh,
  type ConstructionProfile,
  type PlanInputs,
  type PlanProjection,
} from "../features/home-plan/model.ts";
import {
  buildMilestones,
  describePlanChange,
  type PlanMilestone,
} from "../features/home-plan/planFields.ts";
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
  const [selectedScenario, setSelectedScenario] = useState<PlanScenarioId>("buy");
  const [extraEmisPerYear, setExtraEmisPerYear] = useState(2);
  const [loanYear, setLoanYear] = useState(5);
  const [changeNote, setChangeNote] = useState<string | null>(null);
  const [milestoneHint, setMilestoneHint] = useState<PlanMilestone | null>(null);
  const [assumptionsOpen, setAssumptionsOpen] = useState(false);
  const prevProjectionRef = useRef<PlanProjection | null>(null);
  const changeNoteTimer = useRef<number | null>(null);

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

  useEffect(() => () => {
    if (changeNoteTimer.current !== null) window.clearTimeout(changeNoteTimer.current);
  }, []);

  useEffect(() => {
    if (!assumptionsOpen) return undefined;
    const previousOverflow = document.body.style.overflow;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setAssumptionsOpen(false);
    };
    document.body.style.overflow = "hidden";
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.body.style.overflow = previousOverflow;
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [assumptionsOpen]);

  const projection = useMemo(() => inputs ? calculateProjection(inputs) : null, [inputs]);
  const loanJourney = useMemo(() => inputs ? calculateLoanJourney(inputs, extraEmisPerYear) : null, [inputs, extraEmisPerYear]);
  const baselineLoanJourney = useMemo(() => inputs ? calculateLoanJourney(inputs, 0) : null, [inputs]);

  useEffect(() => {
    if (!projection) return;
    if (prevProjectionRef.current) {
      const note = describePlanChange(prevProjectionRef.current, projection, horizon);
      if (note) {
        setChangeNote(note);
        if (changeNoteTimer.current !== null) window.clearTimeout(changeNoteTimer.current);
        changeNoteTimer.current = window.setTimeout(() => setChangeNote(null), 5000);
      }
    }
    prevProjectionRef.current = projection;
  }, [projection, horizon]);

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
  const monthlyRent = activePoint.annualRent / 12;
  const monthlyGap = activePoint.monthlyBuyerHousingCost - monthlyRent;
  const homeEquity = activePoint.propertyValue - activePoint.loanBalance - activePoint.builderBalance;
  const monthlyGapSummary = monthlyGap >= 0
    ? `Buying costs ${formatCurrency(monthlyGap)} more per month than renting`
    : `Buying costs ${formatCurrency(Math.abs(monthlyGap))} less per month than renting`;
  const loanFreeYear = projection.points.find((point, index, points) => (
    point.year > inputs.purchaseYear
    && point.loanBalance <= 0
    && (points[index - 1]?.loanBalance ?? 0) > 0
  ))?.year ?? null;
  const milestones = buildMilestones(inputs.purchaseYear, projection.breakEvenYear, loanFreeYear);

  const updateInput = <K extends keyof PlanInputs>(key: K, value: PlanInputs[K]) => {
    setInputs((current) => {
      if (!current) return current;
      const next = { ...current, [key]: value };
      if (key === "startingSavingsLakh") {
        next.downPaymentLakh = Math.min(next.downPaymentLakh, maximumDownPaymentLakh(next));
      }
      return next;
    });
  };

  const resetPlan = () => {
    setInputs(baseline);
    setExtraEmisPerYear(2);
  };

  const chooseHorizon = (year: number) => {
    setHorizon(year);
    setPreviewYear(null);
    setMilestoneHint(null);
  };

  const handleMilestonePress = (milestone: PlanMilestone) => {
    if (milestoneHint?.year === milestone.year) {
      chooseHorizon(milestone.year);
      return;
    }
    setMilestoneHint(milestone);
  };

  const changeView = (nextView: PlanView) => {
    setView(nextView);
    setPreviewYear(null);
    setMilestoneHint(null);
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
                  {view === "netWorth" && (
                    <button
                      type="button"
                      className="home-plan-assumptions-trigger"
                      aria-expanded={assumptionsOpen}
                      onClick={() => setAssumptionsOpen(true)}
                    >
                      Tune plan
                    </button>
                  )}
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
                  buyWins={buyWins}
                  advantage={advantage}
                  isPreview={previewYear !== null}
                  breakEvenYear={projection.breakEvenYear}
                  homeEquity={homeEquity}
                  monthlyGapSummary={monthlyGapSummary}
                  changeNote={changeNote}
                  monthlyEmi={projection.monthlyEmi}
                  monthlyRent={monthlyRent}
                  buyNetWorth={activePoint.buyNetWorth}
                  rentNetWorth={activePoint.rentNetWorth}
                  selectedScenario={selectedScenario}
                  paymentSchedule={projection.paymentSchedule}
                  possessionDate={projection.possessionDate}
                  constructionDateSource={projection.constructionDateSource}
                  isUnderConstruction={projection.possessionMonth > inputs.purchaseYear * 12}
                  isBeforePossession={activeYear * 12 < projection.possessionMonth}
                  onSelectScenario={setSelectedScenario}
                />

                <MilestoneHint milestone={milestoneHint} />

                <section className="home-plan-stage" aria-label="Projection over time">
                  <PlanGraph
                    projection={projection}
                    horizon={horizon}
                    selected={selectedScenario}
                    milestones={milestones}
                    hintedMilestoneYear={milestoneHint?.year ?? null}
                    onHorizonChange={chooseHorizon}
                    onPreviewYearChange={setPreviewYear}
                    onMilestonePress={handleMilestonePress}
                  />
                </section>
              </>
            )}
          </div>
        </div>
      </div>

      {assumptionsOpen && (
        <div
          className="home-plan-assumptions-backdrop"
          role="presentation"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setAssumptionsOpen(false);
          }}
        >
          <aside
            className="home-plan-assumptions-drawer"
            role="dialog"
            aria-modal="true"
            aria-labelledby="home-plan-inputs-title"
          >
            <header>
              <div>
                <span>Your plan</span>
                <h2 id="home-plan-inputs-title">Tune your plan</h2>
              </div>
              <button
                type="button"
                aria-label="Close plan controls"
                onClick={() => setAssumptionsOpen(false)}
              >
                ×
              </button>
            </header>
            <PlanAssumptionRail
              inputs={inputs}
              onInputChange={updateInput}
              onReset={resetPlan}
            />
          </aside>
        </div>
      )}
    </div>
  );
}
