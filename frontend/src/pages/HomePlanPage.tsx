import { useEffect, useMemo, useState } from "react";
import { Helmet } from "react-helmet-async";
import { Link, useParams } from "react-router-dom";
import { getProperty } from "../lib/api.ts";
import type { PropertyDetailResponse } from "../lib/types.ts";
import { PageState } from "../components/PageState.tsx";
import { PlanControls, type PlanControlSection, type PlanPreset } from "../features/home-plan/PlanControls.tsx";
import { PlanGraph, type PlanGraphMetric, type PlanScenarioId } from "../features/home-plan/PlanGraph.tsx";
import { RepaymentJourney } from "../features/home-plan/RepaymentJourney.tsx";
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
  const activePoint = projection.points[Math.min(horizon, projection.points.length - 1)];
  const buyWins = activePoint.buyNetWorth >= activePoint.rentNetWorth;
  const advantage = Math.abs(activePoint.buyNetWorth - activePoint.rentNetWorth);
  const monthlyRent = activePoint.annualRent / 12;
  const monthlyGap = projection.monthlyEmi - monthlyRent;
  const homeEquity = activePoint.propertyValue - activePoint.loanBalance;
  const monthlyGapSummary = monthlyGap >= 0
    ? `The buying path costs ${formatCurrency(monthlyGap)} more per month than renting`
    : `The buying path costs ${formatCurrency(Math.abs(monthlyGap))} less per month than renting`;
  const decisionHeadline = buyWins
    ? `At year ${horizon}, buying is ahead by ${formatCurrency(advantage, true)}.`
    : `At year ${horizon}, rent + mutual funds is ahead by ${formatCurrency(advantage, true)}.`;
  const boundaryHeadline = projection.breakEvenYear
    ? `Buying catches up in year ${projection.breakEvenYear}.`
    : "Buying does not catch up within 20 years.";
  const boundaryDetail = projection.breakEvenYear
    ? "At that point, estimated home equity becomes greater than the rent-and-invest portfolio."
    : "With the assumptions below, the rent-and-invest portfolio remains higher throughout the 20-year view.";

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

  return (
    <div className="home-plan-shell">
      <Helmet>
        <title>{property.title} financial plan | OpenEstates</title>
        <meta name="description" content={`Compare buying ${property.title} with renting and investing over time.`} />
      </Helmet>

      <header className="home-plan-header">
        <Link to="/" className="home-plan-brand" aria-label="OpenEstates home">OpenEstates</Link>
        <Link to={`/property/${id}`} className="home-plan-back-link"><BackIcon /> Back to property</Link>
      </header>

      <div className="home-plan-main">
        <section className={`home-plan-pane ${workspace === "repayment" ? "home-plan-pane--repayment" : ""}`}>
          <div className="home-plan-pane-toolbar">
            <div>
              <span>{property.area} · {preset === "Base scenario" ? "Base case" : preset}</span>
              <strong className="home-plan-selected-home">{property.title}</strong>
              <h1>{workspace === "decision" ? decisionHeadline : "See how prepayments shorten this loan."}</h1>
              <p>{workspace === "decision" ? "This result uses the assumptions shown below. Change them to test another outcome." : "Choose how many extra EMIs to pay each year and see when the loan ends."}</p>
            </div>
            <div className="home-plan-workspace-switch" aria-label="Planning workspace">
              <button type="button" className={workspace === "decision" ? "is-active" : ""} onClick={() => setWorkspace("decision")}>Compare choices</button>
              <button type="button" className={workspace === "repayment" ? "is-active" : ""} onClick={() => setWorkspace("repayment")}>Pay off loan</button>
            </div>
          </div>

          {workspace === "decision" ? (
            <>
              <section className="home-plan-boundary-card">
                <div className="home-plan-boundary-icon" aria-hidden="true">↗</div>
                <div>
                  <span>When does buying catch up?</span>
                  <strong>{boundaryHeadline}</strong>
                  <p>{boundaryDetail}</p>
                </div>
              </section>

              <section className="home-plan-assumption-bar" aria-label="Current assumptions">
                <span>Current assumptions</span>
                <div>
                  <button type="button" onClick={() => { setControlSection("financing"); setControlsOpen(true); }}><small>Down payment</small><strong>₹{inputs.downPaymentLakh.toFixed(0)}L</strong></button>
                  <button type="button" onClick={() => { setControlSection("financing"); setControlsOpen(true); }}><small>Loan rate</small><strong>{inputs.loanRate.toFixed(1)}%</strong></button>
                  <button type="button" onClick={() => { setControlSection("market"); setControlsOpen(true); }}><small>Home growth</small><strong>{inputs.appreciation.toFixed(1)}%</strong></button>
                  <button type="button" onClick={() => { setControlSection("market"); setControlsOpen(true); }}><small>Fund return</small><strong>{inputs.equityReturn.toFixed(1)}%</strong></button>
                </div>
                <button type="button" onClick={() => { setControlSection("financing"); setControlsOpen(true); }}>Edit assumptions</button>
              </section>

              <section className="home-plan-comparison-panel">
                <div className="home-plan-panel-heading">
                  <div>
                    <span>Compare the two paths</span>
                    <h2>{metric === "netWorth" ? "Projected net worth" : "Monthly outflow"}</h2>
                    <p className="home-plan-panel-context">
                      By year {horizon}, buying creates {formatCurrency(homeEquity, true)} in home equity. {monthlyGapSummary} and requires ₹{inputs.downPaymentLakh.toFixed(0)}L upfront.
                    </p>
                  </div>
                  <div className="home-plan-chart-tools">
                    <label>
                      <span className="home-plan-visually-hidden">Chart metric</span>
                      <select value={metric} onChange={(event) => setMetric(event.target.value as PlanGraphMetric)}>
                        <option value="netWorth">Net worth</option>
                        <option value="monthlyOutflow">Monthly outflow</option>
                      </select>
                    </label>
                    <div className="home-plan-horizon-control">
                      <span>Horizon</span>
                      <div>{[5, 10, 15, 20].map((year) => <button type="button" key={year} className={horizon === year ? "is-active" : ""} onClick={() => setHorizon(year)}>{year}y</button>)}</div>
                    </div>
                  </div>
                </div>
                <PlanGraph projection={projection} horizon={horizon} metric={metric} selected={selectedScenario} purchaseYear={inputs.purchaseYear} onHorizonChange={setHorizon} onSelect={setSelectedScenario} />
              </section>

            </>
          ) : (
            <RepaymentJourney journey={loanJourney} extraEmisPerYear={extraEmisPerYear} selectedYear={loanYear} onExtraEmisChange={setExtraEmisPerYear} onSelectYear={setLoanYear} />
          )}
        </section>
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
