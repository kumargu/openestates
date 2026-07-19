import { useEffect, useMemo, useRef, useState } from "react";
import { Helmet } from "react-helmet-async";
import { Link, useParams } from "react-router-dom";
import { getProperty } from "../lib/api.ts";
import type { PropertyDetailResponse } from "../lib/types.ts";
import { PageState } from "../components/PageState.tsx";
import { AssumptionStrip } from "../features/home-plan/AssumptionStrip.tsx";
import { MilestoneHint } from "../features/home-plan/MilestoneHint.tsx";
import { PlanControls, type PlanControlSection, type PlanPreset } from "../features/home-plan/PlanControls.tsx";
import { PlanGraph, type PlanScenarioId } from "../features/home-plan/PlanGraph.tsx";
import { PlanViewTabs, type PlanView } from "../features/home-plan/PlanViewTabs.tsx";
import { PropertyOrigin } from "../features/home-plan/PropertyOrigin.tsx";
import { RepaymentJourney } from "../features/home-plan/RepaymentJourney.tsx";
import { TimeRail } from "../features/home-plan/TimeRail.tsx";
import { VerdictBlock } from "../features/home-plan/VerdictBlock.tsx";
import { PlanWhisper } from "../features/home-plan/PlanWhisper.tsx";
import {
  buildBaselinePlanInputs,
  calculateLoanJourney,
  calculateProjection,
  formatCurrency,
  type PlanInputs,
  type PlanProjection,
} from "../features/home-plan/model.ts";
import {
  buildMilestones,
  describePlanChange,
  type PlanControlField,
  type PlanMilestone,
} from "../features/home-plan/planFields.ts";
import { BUY_VS_RENT } from "../features/home-plan/labels.ts";

function applyPreset(baseline: PlanInputs, preset: Exclude<PlanPreset, "Custom">): PlanInputs {
  if (preset === "Cautious market") return { ...baseline, appreciation: 4.5, equityReturn: 9, loanRate: 9.1 };
  if (preset === "Strong growth") return { ...baseline, appreciation: 8, equityReturn: 11, loanRate: 7.8 };
  return baseline;
}

function BackIcon() {
  const common = { width: 18, height: 18, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 1.8, strokeLinecap: "round" as const, strokeLinejoin: "round" as const, "aria-hidden": true };
  return <svg {...common}><path d="m15 18-6-6 6-6" /></svg>;
}

function LoadingPlan() {
  return (
    <div className="home-plan-loading" aria-label={BUY_VS_RENT.loading}>
      <div className="home-plan-loading-header" />
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
  const [controlsOpen, setControlsOpen] = useState(false);
  const [controlSection, setControlSection] = useState<PlanControlSection>("financing");
  const [focusField, setFocusField] = useState<PlanControlField | null>(null);
  const [preset, setPreset] = useState<PlanPreset>("Base scenario");
  const [extraEmisPerYear, setExtraEmisPerYear] = useState(2);
  const [loanYear, setLoanYear] = useState(5);
  const [changeNote, setChangeNote] = useState<string | null>(null);
  const [milestoneHint, setMilestoneHint] = useState<PlanMilestone | null>(null);
  const [showFocusHint, setShowFocusHint] = useState(true);
  const prevProjectionRef = useRef<PlanProjection | null>(null);
  const changeNoteTimer = useRef<number | null>(null);

  useEffect(() => {
    if (!id) return;
    let active = true;
    getProperty(id)
      .then((data) => {
        if (!active) return;
        setPropertyData(data);
        setInputs(buildBaselinePlanInputs(data.property.price));
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
  const baseline = buildBaselinePlanInputs(property.price);
  const activeYear = previewYear ?? horizon;
  const activePoint = projection.points[Math.min(activeYear, projection.points.length - 1)];
  const pinnedPoint = projection.points[Math.min(horizon, projection.points.length - 1)];
  const pinnedDifference = pinnedPoint.buyNetWorth - pinnedPoint.rentNetWorth;
  const pinnedScale = Math.max(Math.abs(pinnedPoint.buyNetWorth), Math.abs(pinnedPoint.rentNetWorth), 1);
  const decisionTheme = Math.abs(pinnedDifference) / pinnedScale <= 0.02
    ? "balanced"
    : pinnedDifference > 0 ? "buy" : "rent";
  const buyWins = activePoint.buyNetWorth >= activePoint.rentNetWorth;
  const advantage = Math.abs(activePoint.buyNetWorth - activePoint.rentNetWorth);
  const monthlyRent = activePoint.annualRent / 12;
  const monthlyGap = projection.monthlyEmi - monthlyRent;
  const homeEquity = activePoint.propertyValue - activePoint.loanBalance;
  const monthlyGapSummary = monthlyGap >= 0
    ? `Buying costs ${formatCurrency(monthlyGap)} more per month than renting`
    : `Buying costs ${formatCurrency(Math.abs(monthlyGap))} less per month than renting`;
  const maxYear = projection.points.length - 1;
  const loanFreeYear = projection.points.find((point, index, points) => (
    point.year > inputs.purchaseYear
    && point.loanBalance <= 0
    && (points[index - 1]?.loanBalance ?? 0) > 0
  ))?.year ?? null;
  const milestones = buildMilestones(inputs.purchaseYear, projection.breakEvenYear, loanFreeYear);

  const updateInput = <K extends keyof PlanInputs>(key: K, value: PlanInputs[K]) => {
    setInputs((current) => current ? { ...current, [key]: value } : current);
    setPreset("Custom");
  };

  const choosePreset = (nextPreset: Exclude<PlanPreset, "Custom">) => {
    setPreset(nextPreset);
    setInputs(applyPreset(baseline, nextPreset));
  };

  const resetPlan = () => {
    setInputs(baseline);
    setPreset("Base scenario");
    setExtraEmisPerYear(2);
  };

  const chooseHorizon = (year: number) => {
    setHorizon(year);
    setPreviewYear(null);
    setMilestoneHint(null);
  };

  const openControls = (section: PlanControlSection, field: PlanControlField) => {
    setControlSection(section);
    setFocusField(field);
    setControlsOpen(true);
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

  const metric = view === "monthly" ? "monthlyOutflow" : "netWorth";

  const viewChapterClass = view === "netWorth" ? "net-worth" : view;

  return (
    <div className={`home-plan-shell home-plan-shell--${decisionTheme} home-plan-shell--view-${viewChapterClass}`}>
      <Helmet>
        <title>{property.title} — {BUY_VS_RENT.pageTitle} | OpenEstates</title>
        <meta name="description" content={`Compare buying ${property.title} with renting and investing over time.`} />
      </Helmet>

      <header className="home-plan-header">
        <Link to="/" className="home-plan-brand" aria-label="OpenEstates home">OpenEstates</Link>
        <Link to={`/property/${id}`} className="home-plan-back-link"><BackIcon /> Property</Link>
      </header>

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
              <PlanViewTabs view={view} onChange={changeView} compact />
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
                view={view}
                activeYear={activeYear}
                buyWins={buyWins}
                advantage={advantage}
                isPreview={previewYear !== null}
                breakEvenYear={projection.breakEvenYear}
                homeEquity={homeEquity}
                monthlyGap={monthlyGap}
                monthlyGapSummary={monthlyGapSummary}
                changeNote={changeNote}
                monthlyEmi={projection.monthlyEmi}
                monthlyRent={monthlyRent}
                buyNetWorth={activePoint.buyNetWorth}
                rentNetWorth={activePoint.rentNetWorth}
              />

              <MilestoneHint milestone={milestoneHint} />

              <section className="home-plan-stage" aria-label="Projection over time">
                <PlanGraph
                  projection={projection}
                  horizon={horizon}
                  metric={metric}
                  selected={selectedScenario}
                  milestones={milestones}
                  hintedMilestoneYear={milestoneHint?.year ?? null}
                  showFocusHint={showFocusHint}
                  onHorizonChange={chooseHorizon}
                  onPreviewYearChange={setPreviewYear}
                  onSelect={setSelectedScenario}
                  onMilestonePress={handleMilestonePress}
                  onDismissFocusHint={() => setShowFocusHint(false)}
                />

                <TimeRail
                  horizon={horizon}
                  maxYear={maxYear}
                  milestones={milestones}
                  hintedMilestoneYear={milestoneHint?.year ?? null}
                  onChange={chooseHorizon}
                  onMilestonePress={handleMilestonePress}
                />
              </section>

              <AssumptionStrip
                inputs={inputs}
                activeField={controlsOpen ? focusField : null}
                onEdit={openControls}
              />
            </>
          )}
        </div>
      </div>

      <PlanControls
        open={controlsOpen}
        section={controlSection}
        focusField={focusField}
        preset={preset}
        inputs={inputs}
        projection={projection}
        extraEmisPerYear={extraEmisPerYear}
        property={propertyData}
        onClose={() => { setControlsOpen(false); setFocusField(null); }}
        onSectionChange={setControlSection}
        onPresetChange={choosePreset}
        onInputChange={updateInput}
        onExtraEmisChange={setExtraEmisPerYear}
        onReset={resetPlan}
      />
    </div>
  );
}
