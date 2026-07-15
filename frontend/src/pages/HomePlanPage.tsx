import { useEffect, useMemo, useState } from "react";
import { Helmet } from "react-helmet-async";
import { Link, useParams } from "react-router-dom";
import { getProperty } from "../lib/api.ts";
import type { PropertyDetailResponse } from "../lib/types.ts";
import { PageState } from "../components/PageState.tsx";
import { AssumptionStrip } from "../features/home-plan/AssumptionStrip.tsx";
import { PlanControls, type PlanControlSection, type PlanPreset } from "../features/home-plan/PlanControls.tsx";
import { PlanGraph, type PlanGraphMetric, type PlanScenarioId } from "../features/home-plan/PlanGraph.tsx";
import { PropertyOrigin } from "../features/home-plan/PropertyOrigin.tsx";
import { RepaymentJourney } from "../features/home-plan/RepaymentJourney.tsx";
import { TimeRail } from "../features/home-plan/TimeRail.tsx";
import { VerdictBlock } from "../features/home-plan/VerdictBlock.tsx";
import {
  buildBaselinePlanInputs,
  calculateLoanJourney,
  calculateProjection,
  formatCurrency,
  type PlanInputs,
} from "../features/home-plan/model.ts";
import "../features/home-plan/home-plan.css";

type WorkspaceView = "decision" | "repayment";

function applyPreset(baseline: PlanInputs, preset: Exclude<PlanPreset, "Custom">): PlanInputs {
  if (preset === "Cautious market") return { ...baseline, appreciation: 4.5, equityReturn: 9, loanRate: 9.1 };
  if (preset === "Strong growth") return { ...baseline, appreciation: 8, equityReturn: 11, loanRate: 7.8 };
  return baseline;
}

function BackIcon() {
  const common = { width: 18, height: 18, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 1.8, strokeLinecap: "round" as const, strokeLinejoin: "round" as const, "aria-hidden": true };
  return <svg {...common}><path d="m15 18-6-6 6-6" /></svg>;
}

function MetricIcon({ metric }: { metric: PlanGraphMetric }) {
  const common = { width: 16, height: 16, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 1.8, strokeLinecap: "round" as const, strokeLinejoin: "round" as const, "aria-hidden": true };
  if (metric === "monthlyOutflow") {
    return <svg {...common}><path d="M12 2v20M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6" /></svg>;
  }
  return <svg {...common}><path d="M3 3v18h18" /><path d="m7 15 4-4 3 3 5-6" /></svg>;
}

function LoadingPlan() {
  return (
    <div className="home-plan-loading" aria-label="Loading home plan">
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
  const [workspace, setWorkspace] = useState<WorkspaceView>("decision");
  const [horizon, setHorizon] = useState(10);
  const [previewYear, setPreviewYear] = useState<number | null>(null);
  const [selectedScenario, setSelectedScenario] = useState<PlanScenarioId>("buy");
  const [metric, setMetric] = useState<PlanGraphMetric>("netWorth");
  const [controlsOpen, setControlsOpen] = useState(false);
  const [controlSection, setControlSection] = useState<PlanControlSection>("financing");
  const [preset, setPreset] = useState<PlanPreset>("Base scenario");
  const [extraEmisPerYear, setExtraEmisPerYear] = useState(2);
  const [loanYear, setLoanYear] = useState(5);

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

  const projection = useMemo(() => inputs ? calculateProjection(inputs) : null, [inputs]);
  const loanJourney = useMemo(() => inputs ? calculateLoanJourney(inputs, extraEmisPerYear) : null, [inputs, extraEmisPerYear]);

  if (!id) return <PageState variant="not_found" context="property" message="Choose a home before opening its plan." />;
  if (status === "loading") return <LoadingPlan />;
  if (status === "not_found") return <PageState variant="not_found" context="property" message="This home is no longer available for planning." />;
  if (status === "error") return <PageState variant="error" context="property" message="We could not load this plan. Return to the property and try again." />;
  if (!propertyData || !inputs || !projection || !loanJourney) return null;

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
  const presetLabel = preset === "Base scenario" ? "Base case" : preset;
  const maxYear = projection.points.length - 1;
  const loanFreeYear = projection.points.find((point, index, points) => (
    point.year > inputs.purchaseYear
    && point.loanBalance <= 0
    && (points[index - 1]?.loanBalance ?? 0) > 0
  ))?.year ?? null;
  const timeMilestones = [
    { year: inputs.purchaseYear, label: inputs.purchaseYear === 0 ? "Buy" : `Buy Y${inputs.purchaseYear}` },
    ...(projection.breakEvenYear !== null ? [{ year: projection.breakEvenYear, label: "Break-even" }] : []),
    ...(loanFreeYear !== null ? [{ year: loanFreeYear, label: "Loan free" }] : []),
  ];

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
  };

  const openControls = (section: PlanControlSection) => {
    setControlSection(section);
    setControlsOpen(true);
  };

  return (
    <div className={`home-plan-shell home-plan-shell--${decisionTheme}`}>
      <Helmet>
        <title>{property.title} financial plan | OpenEstates</title>
        <meta name="description" content={`Compare buying ${property.title} with renting and investing over time.`} />
      </Helmet>

      <header className="home-plan-header">
        <Link to="/" className="home-plan-brand" aria-label="OpenEstates home">OpenEstates</Link>
        <div className="home-plan-header__actions">
          <nav className="home-plan-workspace-switch" aria-label="Planning workspace">
            <button type="button" className={workspace === "decision" ? "is-active" : ""} onClick={() => { setPreviewYear(null); setWorkspace("decision"); }}>Compare</button>
            <button type="button" className={workspace === "repayment" ? "is-active" : ""} onClick={() => { setPreviewYear(null); setWorkspace("repayment"); }}>Pay off loan</button>
          </nav>
          <Link to={`/property/${id}`} className="home-plan-back-link"><BackIcon /> Property</Link>
        </div>
      </header>

      <div className="home-plan-main">
        {workspace === "decision" ? (
          <div className="home-plan-canvas">
            <PropertyOrigin
              propertyId={id}
              title={property.title}
              area={property.area}
              bhk={property.bhk}
              price={property.price}
              inputs={inputs}
              presetLabel={presetLabel}
            />

            <VerdictBlock
              activeYear={activeYear}
              buyWins={buyWins}
              advantage={advantage}
              isPreview={previewYear !== null}
              breakEvenYear={projection.breakEvenYear}
              homeEquity={homeEquity}
              monthlyGapSummary={monthlyGapSummary}
            />

            <section className="home-plan-stage" aria-label="Net worth projection">
              <div className="home-plan-stage__toolbar">
                <span className="home-plan-stage__label">
                  {metric === "netWorth" ? "Projected net worth" : "Monthly outflow"}
                </span>
                <div className="home-plan-metric-toggle" role="group" aria-label="Chart metric">
                  {(["netWorth", "monthlyOutflow"] as const).map((item) => (
                    <button
                      type="button"
                      key={item}
                      className={metric === item ? "is-active" : ""}
                      onClick={() => setMetric(item)}
                      aria-pressed={metric === item}
                      title={item === "netWorth" ? "Net worth" : "Monthly outflow"}
                    >
                      <MetricIcon metric={item} />
                      <span>{item === "netWorth" ? "Net worth" : "Monthly"}</span>
                    </button>
                  ))}
                </div>
              </div>

              <PlanGraph
                projection={projection}
                horizon={horizon}
                metric={metric}
                selected={selectedScenario}
                purchaseYear={inputs.purchaseYear}
                onHorizonChange={chooseHorizon}
                onPreviewYearChange={setPreviewYear}
                onSelect={setSelectedScenario}
              />

              <TimeRail
                horizon={horizon}
                maxYear={maxYear}
                milestones={timeMilestones}
                onChange={chooseHorizon}
              />
            </section>

            <AssumptionStrip inputs={inputs} onEdit={openControls} />
          </div>
        ) : (
          <section className="home-plan-repayment-canvas">
            <header className="home-plan-repayment-header">
              <PropertyOrigin
                propertyId={id}
                title={property.title}
                area={property.area}
                bhk={property.bhk}
                price={property.price}
                inputs={inputs}
                presetLabel={presetLabel}
              />
              <h1>See how prepayments shorten this loan.</h1>
              <p>Keep the regular EMI unchanged. Each annual prepayment goes directly toward principal.</p>
            </header>
            <RepaymentJourney
              journey={loanJourney}
              extraEmisPerYear={extraEmisPerYear}
              selectedYear={loanYear}
              onExtraEmisChange={setExtraEmisPerYear}
              onSelectYear={setLoanYear}
            />
          </section>
        )}
      </div>

      <PlanControls
        open={controlsOpen}
        section={controlSection}
        preset={preset}
        inputs={inputs}
        projection={projection}
        extraEmisPerYear={extraEmisPerYear}
        property={propertyData}
        onClose={() => setControlsOpen(false)}
        onSectionChange={setControlSection}
        onPresetChange={choosePreset}
        onInputChange={updateInput}
        onExtraEmisChange={setExtraEmisPerYear}
        onReset={resetPlan}
      />
    </div>
  );
}
