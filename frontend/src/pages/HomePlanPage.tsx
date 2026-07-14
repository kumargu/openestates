import { useEffect, useMemo, useState } from "react";
import { Helmet } from "react-helmet-async";
import { Link, useParams } from "react-router-dom";
import { getProperty } from "../lib/api.ts";
import type { PropertyDetailResponse } from "../lib/types.ts";
import { PageState } from "../components/PageState.tsx";
import { PlanControls, type PlanControlSection } from "../features/home-plan/PlanControls.tsx";
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
type ScenarioPreset = "Base scenario" | "Cautious market" | "Strong growth";

function formatPropertyPrice(price: number): string {
  return price >= 10_000_000
    ? `₹${(price / 10_000_000).toFixed(2)} Cr`
    : `₹${(price / 100_000).toFixed(1)} L`;
}

function applyPreset(baseline: PlanInputs, preset: ScenarioPreset): PlanInputs {
  if (preset === "Cautious market") return { ...baseline, appreciation: 4.5, equityReturn: 9, loanRate: 9.1 };
  if (preset === "Strong growth") return { ...baseline, appreciation: 8, equityReturn: 11, loanRate: 7.8 };
  return baseline;
}

function PlanIcon({ name }: { name: "home" | "back" | "controls" | "chevron" }) {
  const common = { width: 18, height: 18, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 1.8, strokeLinecap: "round" as const, strokeLinejoin: "round" as const, "aria-hidden": true };
  if (name === "home") return <svg {...common}><path d="m3 11 9-8 9 8" /><path d="M5 10v10h14V10" /><path d="M9 20v-6h6v6" /></svg>;
  if (name === "back") return <svg {...common}><path d="m15 18-6-6 6-6" /></svg>;
  if (name === "controls") return <svg {...common}><path d="M4 7h10M18 7h2M14 4v6M4 17h2M10 17h10M8 14v6" /></svg>;
  return <svg {...common}><path d="m8 10 4 4 4-4" /></svg>;
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
  const [scenarioMenuOpen, setScenarioMenuOpen] = useState(false);
  const [preset, setPreset] = useState<ScenarioPreset>("Base scenario");
  const [saved, setSaved] = useState(true);
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
  const winnerLabel = buyWins ? "Buy this home" : "Rent + mutual funds";
  const advantage = Math.abs(activePoint.buyNetWorth - activePoint.rentNetWorth);
  const monthlyRent = activePoint.annualRent / 12;
  const monthlyGap = projection.monthlyEmi - monthlyRent;
  const selectedValue = selectedScenario === "buy" ? activePoint.buyNetWorth : activePoint.rentNetWorth;
  const selectedMonthlyCost = selectedScenario === "buy" ? projection.monthlyEmi : monthlyRent;
  const homeEquity = activePoint.propertyValue - activePoint.loanBalance;
  const selectedOwnership = selectedScenario === "buy" ? homeEquity : activePoint.rentNetWorth;

  const updateInput = <K extends keyof PlanInputs>(key: K, value: PlanInputs[K]) => {
    setInputs((current) => current ? { ...current, [key]: value } : current);
    setPreset("Base scenario");
    setSaved(false);
  };

  const choosePreset = (nextPreset: ScenarioPreset) => {
    setPreset(nextPreset);
    setInputs(applyPreset(baseline, nextPreset));
    setSaved(nextPreset === "Base scenario");
    setScenarioMenuOpen(false);
  };

  const resetPlan = () => {
    setInputs(baseline);
    setPreset("Base scenario");
    setSaved(true);
    setExtraEmisPerYear(2);
  };

  return (
    <div className="home-plan-shell">
      <Helmet>
        <title>{property.title} financial plan | OpenEstates</title>
        <meta name="description" content={`Compare buying ${property.title} with renting and investing over time.`} />
      </Helmet>

      <header className="home-plan-header">
        <div className="home-plan-brand-group">
          <Link to="/" className="home-plan-brand" aria-label="OpenEstates home">
            <span><PlanIcon name="home" /></span>
            <strong>OpenEstates</strong>
          </Link>
          <Link to={`/property/${id}`} className="home-plan-back-link"><PlanIcon name="back" /> Property</Link>
        </div>
        <Link to={`/property/${id}`} className="home-plan-property-context">
          <small>Home plan</small>
          <strong>{property.bhk} BHK · {property.area} · {formatPropertyPrice(property.price)}</strong>
        </Link>
        <div className="home-plan-header-actions">
          <div className="home-plan-scenario-menu">
            <button type="button" onClick={() => setScenarioMenuOpen((open) => !open)} aria-expanded={scenarioMenuOpen}>
              <i />
              <span><strong>{preset}</strong><small>{saved ? "Saved" : "Edited"}</small></span>
              <PlanIcon name="chevron" />
            </button>
            {scenarioMenuOpen && (
              <div className="home-plan-scenario-popover">
                {(["Base scenario", "Cautious market", "Strong growth"] as const).map((item) => (
                  <button type="button" key={item} className={preset === item ? "is-active" : ""} onClick={() => choosePreset(item)}>
                    <span>{item}</span><small>{item === "Base scenario" ? "Property baseline" : item === "Cautious market" ? "Lower growth, higher rates" : "Higher growth, lower rates"}</small>
                  </button>
                ))}
              </div>
            )}
          </div>
          <button
            type="button"
            className="home-plan-controls-launcher"
            onClick={() => { setControlSection("financing"); setControlsOpen(true); }}
          >
            <PlanIcon name="controls" />
            Plan controls
          </button>
        </div>
      </header>

      <div className="home-plan-main">
        <section className={`home-plan-pane ${workspace === "repayment" ? "home-plan-pane--repayment" : ""}`}>
          <div className="home-plan-pane-toolbar">
            <div><span>Home decision</span><h1>See how each choice changes your financial position.</h1></div>
            <div className="home-plan-workspace-switch" aria-label="Planning workspace">
              <button type="button" className={workspace === "decision" ? "is-active" : ""} onClick={() => setWorkspace("decision")}>Buy vs rent</button>
              <button type="button" className={workspace === "repayment" ? "is-active" : ""} onClick={() => setWorkspace("repayment")}>Repayment</button>
            </div>
          </div>

          {workspace === "decision" ? (
            <>
              <section className="home-plan-decision-strip">
                <div className="home-plan-decision-lead"><span>Best at year {horizon}</span><strong>{winnerLabel}</strong><small>Leads by {formatCurrency(advantage, true)}</small></div>
                <div><span>Monthly buy gap</span><strong>{monthlyGap >= 0 ? "+" : "−"}{formatCurrency(Math.abs(monthlyGap))}</strong><small>Compared with renting</small></div>
                <div><span>Break-even</span><strong>{projection.breakEvenYear ? `Year ${projection.breakEvenYear}` : "20y+"}</strong><small>Buying overtakes renting</small></div>
                <div><span>Cash committed</span><strong>₹{inputs.downPaymentLakh.toFixed(0)}L</strong><small>Down payment today</small></div>
              </section>

              <section className="home-plan-comparison-panel">
                <div className="home-plan-panel-heading">
                  <div><span>Scenario comparison</span><h2>{metric === "netWorth" ? "Projected net worth" : "Monthly outflow"}</h2></div>
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

                <div className={`home-plan-scenario-inspector home-plan-scenario-inspector--${selectedScenario}`}>
                  <div><span>Selected option</span><strong>{selectedScenario === "buy" ? "Buy this home" : "Rent + mutual funds"}</strong><small>{selectedScenario === "buy" ? `${formatPropertyPrice(property.price)} home · buy in ${inputs.purchaseYear === 0 ? "year 1" : `year ${inputs.purchaseYear}`}` : "Invest down payment and monthly difference"}</small></div>
                  <dl>
                    <div><dt>Net worth</dt><dd>{formatCurrency(selectedValue, true)}</dd></div>
                    <div><dt>Monthly cost</dt><dd>{formatCurrency(selectedMonthlyCost)}</dd></div>
                    <div><dt>{selectedScenario === "buy" ? "Home equity" : "Fund value"}</dt><dd>{formatCurrency(selectedOwnership, true)}</dd></div>
                    <div><dt>Liquidity</dt><dd>{selectedScenario === "buy" ? formatCurrency(projection.liquidityAfterDownPayment, true) : "High"}</dd></div>
                  </dl>
                  <div className="home-plan-inspector-note"><span>{(selectedScenario === "buy") === buyWins ? "Current leader" : "Alternative"}</span><p>{selectedScenario === "buy" ? "More ownership, with more capital committed upfront." : "More flexibility, with returns tied to the mutual-fund assumption."}</p></div>
                </div>
              </section>

              <section className="home-plan-boundary-card">
                <span>Where the result flips</span>
                <strong>{projection.breakEvenYear ? `Buying moves ahead around year ${projection.breakEvenYear} under the current assumptions.` : "Renting and investing remains ahead across the current horizon."}</strong>
                <button type="button" onClick={() => { setControlSection("market"); setControlsOpen(true); }}>Explore the assumptions</button>
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
        inputs={inputs}
        projection={projection}
        extraEmisPerYear={extraEmisPerYear}
        property={propertyData}
        onClose={() => setControlsOpen(false)}
        onSectionChange={setControlSection}
        onInputChange={updateInput}
        onExtraEmisChange={setExtraEmisPerYear}
        onReset={resetPlan}
        onSave={() => { setSaved(true); setControlsOpen(false); }}
      />
    </div>
  );
}
