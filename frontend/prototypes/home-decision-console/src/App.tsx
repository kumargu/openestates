import { useMemo, useState } from "react";
import {
  BASE_INPUTS,
  calculateLoanJourney,
  calculateProjection,
  calculateScenarioGap,
  formatCurrency,
  type PlanInputs,
} from "./model.ts";
import { LoanJourneyView } from "./LoanJourneyView.tsx";
import { PlanLab, type ExperimentId, type ExperimentImpact, type PlanExperiment } from "./PlanLab.tsx";

type ScenarioId = "buy" | "rent";
type WorkspaceView = "decision" | "loan";

type Scenario = {
  id: ScenarioId;
  label: string;
  description: string;
  value: number;
  monthlyCost: number;
  liquidity: string;
  ownership: string;
};

function Icon({ name }: { name: "home" | "tune" | "save" | "close" }) {
  const common = { width: 18, height: 18, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 1.8, strokeLinecap: "round" as const, strokeLinejoin: "round" as const, "aria-hidden": true };
  if (name === "home") return <svg {...common}><path d="m3 11 9-8 9 8" /><path d="M5 10v10h14V10" /><path d="M9 20v-6h6v6" /></svg>;
  if (name === "tune") return <svg {...common}><path d="M4 7h10" /><path d="M18 7h2" /><path d="M14 4v6" /><path d="M4 17h2" /><path d="M10 17h10" /><path d="M8 14v6" /></svg>;
  if (name === "save") return <svg {...common}><path d="M6 3h12l2 2v16H4V3h2Z" /><path d="M8 3v6h8V3" /><path d="M8 21v-7h8v7" /></svg>;
  if (name === "close") return <svg {...common}><path d="m6 6 12 12" /><path d="M18 6 6 18" /></svg>;
  return null;
}

function NetWorthGraph({ scenarios, projection, horizon, selected, onSelect, onHorizonChange }: { scenarios: Scenario[]; projection: ReturnType<typeof calculateProjection>; horizon: number; selected: ScenarioId; onSelect: (id: ScenarioId) => void; onHorizonChange: (year: number) => void }) {
  const width = 960;
  const height = 340;
  const inset = { left: 66, right: 24, top: 24, bottom: 38 };
  const plotWidth = width - inset.left - inset.right;
  const plotHeight = height - inset.top - inset.bottom;
  const series = [
    { id: "buy" as const, values: projection.points.map((point) => point.buyNetWorth) },
    { id: "rent" as const, values: projection.points.map((point) => point.rentNetWorth) },
  ];
  const maxValue = Math.max(...series.flatMap((item) => item.values)) * 1.08;
  const x = (year: number) => inset.left + year / 20 * plotWidth;
  const y = (value: number) => inset.top + plotHeight - value / maxValue * plotHeight;
  const line = (values: number[]) => values.map((value, year) => `${year === 0 ? "M" : "L"}${x(year).toFixed(1)},${y(value).toFixed(1)}`).join(" ");
  const cursorX = x(horizon);

  return (
    <div className="net-worth-graph">
      <div className="graph-legend">
        {scenarios.map((scenario) => <button key={scenario.id} className={selected === scenario.id ? "selected" : ""} onClick={() => onSelect(scenario.id)}><i className={`legend-dot legend-dot--${scenario.id}`} /><span><strong>{scenario.label}</strong><small>{formatCurrency(scenario.value, true)}</small></span></button>)}
      </div>
      <svg viewBox={`0 0 ${width} ${height}`} role="img" aria-label="Projected net worth over twenty years">
        {[.25, .5, .75, 1].map((ratio) => {
          const value = maxValue * ratio;
          const lineY = y(value);
          return <g key={ratio}><line x1={inset.left} x2={width - inset.right} y1={lineY} y2={lineY} className="graph-gridline" /><text x={inset.left - 10} y={lineY + 3} className="graph-y-label">{formatCurrency(value, true)}</text></g>;
        })}
        {[0, 5, 10, 15, 20].map((year) => <text key={year} x={x(year)} y={height - 9} className="graph-x-label">{year === 0 ? "Now" : `${year}y`}</text>)}
        {series.map((item) => <path key={item.id} d={line(item.values)} className={`graph-line graph-line--${item.id} ${selected === item.id ? "selected" : ""}`} />)}
        <line x1={cursorX} x2={cursorX} y1={inset.top} y2={inset.top + plotHeight} className="graph-cursor" />
        <text x={cursorX} y={inset.top - 8} className="graph-cursor-label">YEAR {horizon}</text>
        {series.map((item) => <circle key={item.id} cx={cursorX} cy={y(item.values[horizon])} r={selected === item.id ? 6 : 4} className={`graph-point graph-point--${item.id}`} />)}
      </svg>
      <label className="year-scrubber"><span>Now</span><input type="range" min={0} max={20} step={1} value={horizon} onChange={(event) => onHorizonChange(Number(event.target.value))} aria-label="Projection year" /><span>20 years</span></label>
    </div>
  );
}

function applyExperiment(inputs: PlanInputs, id: ExperimentId, value: number): PlanInputs {
  if (id === "delay") return { ...inputs, purchaseYear: value };
  if (id === "downPayment") return { ...inputs, downPaymentLakh: value };
  if (id === "growth") return { ...inputs, appreciation: value };
  if (id === "loanRate") return { ...inputs, loanRate: value };
  if (id === "fundReturn") return { ...inputs, equityReturn: value };
  return { ...inputs, monthlyExtraInvestmentThousands: value };
}

function experimentDefault(inputs: PlanInputs, id: ExperimentId): number {
  if (id === "delay") return Math.min(7, inputs.purchaseYear + 2);
  if (id === "downPayment") return Math.min(80, inputs.downPaymentLakh + 10);
  if (id === "growth") return Math.max(2, inputs.appreciation - 1);
  if (id === "loanRate") return Math.min(11, inputs.loanRate + 1);
  if (id === "fundReturn") return Math.min(14, inputs.equityReturn + 1);
  return Math.min(100, inputs.monthlyExtraInvestmentThousands + 20);
}

function findFirstRate(start: number, end: number, step: number, predicate: (rate: number) => boolean): number | null {
  for (let rate = start; rate <= end + .001; rate += step) {
    const roundedRate = Math.round(rate * 10) / 10;
    if (predicate(roundedRate)) return roundedRate;
  }
  return null;
}

export function App() {
  const [baseInputs, setBaseInputs] = useState<PlanInputs>({ ...BASE_INPUTS, purchaseYear: 1, holdingPeriodYears: 20 });
  const [workspaceView, setWorkspaceView] = useState<WorkspaceView>("decision");
  const [horizon, setHorizon] = useState(10);
  const [selected, setSelected] = useState<ScenarioId>("buy");
  const [extraEmisPerYear, setExtraEmisPerYear] = useState(2);
  const [loanYear, setLoanYear] = useState(5);
  const [activeExperimentId, setActiveExperimentId] = useState<ExperimentId | null>(null);
  const [experimentValue, setExperimentValue] = useState(0);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [saved, setSaved] = useState(false);

  const inputs = activeExperimentId ? applyExperiment(baseInputs, activeExperimentId, experimentValue) : baseInputs;
  const projection = useMemo(() => calculateProjection(inputs), [inputs]);
  const baselineProjection = useMemo(() => calculateProjection(baseInputs), [baseInputs]);
  const loanJourney = useMemo(() => calculateLoanJourney(inputs, extraEmisPerYear), [inputs, extraEmisPerYear]);
  const point = projection.points[horizon];
  const scenarios: Scenario[] = [
    { id: "buy", label: "Buy this home", description: `₹${inputs.propertyPriceLakh}L home · buy in year ${inputs.purchaseYear}`, value: point.buyNetWorth, monthlyCost: point.annualEmi / 12, liquidity: formatCurrency(projection.liquidityAfterDownPayment, true), ownership: formatCurrency(Math.max(0, point.propertyValue - point.loanBalance), true) },
    { id: "rent", label: "Rent + mutual funds", description: `₹${inputs.currentRentThousands}K rent${inputs.monthlyExtraInvestmentThousands > 0 ? ` · ₹${inputs.monthlyExtraInvestmentThousands}K extra SIP` : " · invest the monthly difference"}`, value: point.rentNetWorth, monthlyCost: point.annualRent / 12, liquidity: "High", ownership: formatCurrency(point.rentNetWorth, true) },
  ];
  const ranked = [...scenarios].sort((first, second) => second.value - first.value);
  const winner = ranked[0];
  const advantage = winner.value - ranked[1].value;
  const selectedScenario = scenarios.find((scenario) => scenario.id === selected) ?? scenarios[0];
  const monthlyGap = scenarios[0].monthlyCost - scenarios[1].monthlyCost;
  const experiments: PlanExperiment[] = [
    { id: "delay", title: "Buy later", description: "See the cost of waiting two years", controlLabel: "Purchase timing", value: activeExperimentId === "delay" ? experimentValue : experimentDefault(baseInputs, "delay"), min: 0, max: 7, step: 1, displayValue: `Year ${activeExperimentId === "delay" ? experimentValue : experimentDefault(baseInputs, "delay")}` },
    { id: "downPayment", title: "Use more cash", description: "Add ₹10L to the down payment", controlLabel: "Down payment", value: activeExperimentId === "downPayment" ? experimentValue : experimentDefault(baseInputs, "downPayment"), min: 20, max: 80, step: 5, displayValue: `₹${activeExperimentId === "downPayment" ? experimentValue : experimentDefault(baseInputs, "downPayment")}L` },
    { id: "growth", title: "Slower market", description: "Reduce property growth by 1%", controlLabel: "Property growth", value: activeExperimentId === "growth" ? experimentValue : experimentDefault(baseInputs, "growth"), min: 2, max: 10, step: .5, displayValue: `${(activeExperimentId === "growth" ? experimentValue : experimentDefault(baseInputs, "growth")).toFixed(1)}%` },
    { id: "loanRate", title: "Rate shock", description: "Increase the loan rate by 1%", controlLabel: "Loan interest rate", value: activeExperimentId === "loanRate" ? experimentValue : experimentDefault(baseInputs, "loanRate"), min: 6.5, max: 11, step: .1, displayValue: `${(activeExperimentId === "loanRate" ? experimentValue : experimentDefault(baseInputs, "loanRate")).toFixed(1)}%` },
    { id: "fundReturn", title: "Stronger funds", description: "Increase mutual-fund returns by 1%", controlLabel: "Mutual-fund return", value: activeExperimentId === "fundReturn" ? experimentValue : experimentDefault(baseInputs, "fundReturn"), min: 6, max: 14, step: .5, displayValue: `${(activeExperimentId === "fundReturn" ? experimentValue : experimentDefault(baseInputs, "fundReturn")).toFixed(1)}%` },
    { id: "extraSip", title: "Invest more", description: "Add ₹20K to the monthly SIP", controlLabel: "Additional monthly SIP", value: activeExperimentId === "extraSip" ? experimentValue : experimentDefault(baseInputs, "extraSip"), min: 0, max: 100, step: 5, displayValue: `₹${activeExperimentId === "extraSip" ? experimentValue : experimentDefault(baseInputs, "extraSip")}K / month` },
  ];
  const activeExperiment = experiments.find((experiment) => experiment.id === activeExperimentId) ?? null;
  const baselinePoint = baselineProjection.points[horizon];
  const baselineGap = baselinePoint.buyNetWorth - baselinePoint.rentNetWorth;
  const currentGap = point.buyNetWorth - point.rentNetWorth;
  const experimentImpact: ExperimentImpact | null = activeExperiment ? {
    winnerLabel: winner.label,
    advantage,
    monthlyCostDelta: point.annualEmi / 12 - baselinePoint.annualEmi / 12,
    liquidityDelta: projection.liquidityAfterDownPayment - baselineProjection.liquidityAfterDownPayment,
    buyRentGap: currentGap - baselineGap,
    breakEven: projection.breakEvenYear ? `Year ${projection.breakEvenYear}` : "20y+",
    baselineBreakEven: baselineProjection.breakEvenYear ? `Year ${baselineProjection.breakEvenYear}` : "20y+",
  } : null;

  const reversalInsight = useMemo(() => {
    const savedGap = calculateScenarioGap(baseInputs, horizon);
    const growthThreshold = findFirstRate(2, 10, .1, (rate) => calculateScenarioGap({ ...baseInputs, appreciation: rate }, horizon) >= 0);
    const fundThreshold = findFirstRate(6, 14, .1, (rate) => calculateScenarioGap({ ...baseInputs, equityReturn: rate }, horizon) < 0);
    if (savedGap >= 0 && growthThreshold !== null && fundThreshold !== null) return `Renting becomes stronger below ${growthThreshold.toFixed(1)}% property growth or above ${fundThreshold.toFixed(1)}% mutual-fund returns.`;
    if (savedGap < 0 && growthThreshold !== null && fundThreshold !== null) return `Buying becomes stronger above ${growthThreshold.toFixed(1)}% property growth or below ${fundThreshold.toFixed(1)}% mutual-fund returns.`;
    return "The current decision remains stable across the assumptions tested.";
  }, [baseInputs, horizon]);

  const update = (key: keyof PlanInputs, value: number) => {
    setBaseInputs((current) => ({ ...current, [key]: value }));
    setActiveExperimentId(null);
  };
  const selectExperiment = (id: ExperimentId) => {
    setActiveExperimentId(id);
    setExperimentValue(experimentDefault(baseInputs, id));
  };
  const keepExperiment = () => {
    setBaseInputs(inputs);
    setActiveExperimentId(null);
  };

  return (
    <div className="dashboard-shell">
      <header className="dashboard-header">
        <div className="dashboard-brand"><span><Icon name="home" /></span><strong>OpenEstates</strong></div>
        <div className="dashboard-property"><small>Home plan</small><strong>3 BHK · Whitefield · ₹1.50 Cr</strong></div>
        <div className="dashboard-actions"><button onClick={() => setDetailsOpen(true)}><Icon name="tune" />Details</button><button className={`save-button ${saved ? "saved" : ""}`} onClick={() => setSaved((current) => !current)}><Icon name="save" />{saved ? "Saved" : "Save"}</button></div>
      </header>

      <main className="dashboard-main">
        <section className={`dashboard-pane dashboard-pane--${workspaceView}`}>
          <div className="pane-toolbar">
            <div><span>{workspaceView === "decision" ? "HOME DECISION" : "LOAN JOURNEY"}</span><h1>{workspaceView === "decision" ? "See how each choice changes your financial position." : "See how extra EMIs change the life of your loan."}</h1></div>
            <div className="view-switch" aria-label="Planning view"><button className={workspaceView === "decision" ? "active" : ""} onClick={() => setWorkspaceView("decision")}>Decision</button><button className={workspaceView === "loan" ? "active" : ""} onClick={() => setWorkspaceView("loan")}>Loan journey</button></div>
          </div>

          {workspaceView === "decision" ? (
            <>
              <section className="decision-strip" aria-label="Decision summary">
                <div className="decision-lead"><span>Best at year {horizon}</span><strong>{winner.label}</strong><small>Leads by {formatCurrency(advantage, true)}</small></div>
                <div><span>Monthly buy gap</span><strong>{monthlyGap >= 0 ? "+" : "−"}{formatCurrency(Math.abs(monthlyGap))}</strong><small>Compared with renting</small></div>
                <div><span>Break-even</span><strong>{projection.breakEvenYear ? `Year ${projection.breakEvenYear}` : "20y+"}</strong><small>Buying overtakes renting</small></div>
                <div><span>Cash committed</span><strong>₹{inputs.downPaymentLakh}L</strong><small>Down payment today</small></div>
              </section>

              <section className="comparison-panel">
                <div className="panel-heading"><div><span>SCENARIO COMPARISON</span><h2>Projected net worth</h2></div><div className="horizon-control"><span>Quick view</span><div>{[5, 10, 15, 20].map((year) => <button key={year} className={horizon === year ? "active" : ""} onClick={() => setHorizon(year)}>{year}y</button>)}</div></div></div>
                <NetWorthGraph scenarios={scenarios} projection={projection} horizon={horizon} selected={selected} onSelect={setSelected} onHorizonChange={setHorizon} />
                <div className={`scenario-inspector scenario-inspector--${selected}`}>
                  <div><span>Selected option</span><strong>{selectedScenario.label}</strong><small>{selectedScenario.description}</small></div>
                  <dl><div><dt>Net worth</dt><dd>{formatCurrency(selectedScenario.value, true)}</dd></div><div><dt>Monthly cost</dt><dd>{formatCurrency(selectedScenario.monthlyCost)}</dd></div><div><dt>{selected === "rent" ? "Fund value" : "Home equity"}</dt><dd>{selectedScenario.ownership}</dd></div><div><dt>Liquidity</dt><dd>{selectedScenario.liquidity}</dd></div></dl>
                  <p>{selected === "buy" ? "More ownership, with more capital committed upfront." : "More flexibility, with returns tied to the mutual-fund assumption."}</p>
                </div>
              </section>

              <PlanLab experiments={experiments} activeExperiment={activeExperiment} impact={experimentImpact} reversalInsight={reversalInsight} onSelect={selectExperiment} onValueChange={setExperimentValue} onKeep={keepExperiment} onReset={() => setActiveExperimentId(null)} />
            </>
          ) : (
            <LoanJourneyView journey={loanJourney} extraEmisPerYear={extraEmisPerYear} selectedYear={loanYear} onExtraEmisChange={setExtraEmisPerYear} onSelectYear={setLoanYear} />
          )}
        </section>
      </main>

      {detailsOpen && <button className="details-backdrop" onClick={() => setDetailsOpen(false)} aria-label="Close details" />}
      <aside className={`details-drawer ${detailsOpen ? "open" : ""}`}>
        <div className="details-header"><div><span>MODEL DETAILS</span><h2>Assumptions and evidence</h2></div><button onClick={() => setDetailsOpen(false)} aria-label="Close"><Icon name="close" /></button></div>
        <div className="detail-section"><h3>Advanced assumptions</h3><Assumption label="Loan rate" value={`${inputs.loanRate.toFixed(1)}%`} min={6.5} max={11} step={.1} current={inputs.loanRate} onChange={(value) => update("loanRate", value)} /><Assumption label="Mutual-fund return" value={`${inputs.equityReturn.toFixed(1)}%`} min={6} max={14} step={.5} current={inputs.equityReturn} onChange={(value) => update("equityReturn", value)} /><Assumption label="Rent inflation" value={`${inputs.rentInflation.toFixed(1)}%`} min={3} max={10} step={.5} current={inputs.rentInflation} onChange={(value) => update("rentInflation", value)} /></div>
        <div className="detail-section"><h3>Evidence used</h3><Evidence label="Property price" value="₹12,450 / sq.ft." note="42 comparable observations · medium confidence" /><Evidence label="Rental yield" value="3.1%" note="Observed Whitefield range · medium confidence" /><Evidence label="RERA completion" value="December 2027" note="Karnataka RERA · verified" /></div>
      </aside>
    </div>
  );
}

function Assumption({ label, value, min, max, step, current, onChange }: { label: string; value: string; min: number; max: number; step: number; current: number; onChange: (value: number) => void }) {
  return <label className="assumption"><span><span>{label}</span><strong>{value}</strong></span><input type="range" min={min} max={max} step={step} value={current} onChange={(event) => onChange(Number(event.target.value))} /></label>;
}

function Evidence({ label, value, note }: { label: string; value: string; note: string }) {
  return <div className="evidence-row"><span>{label}</span><strong>{value}</strong><small>{note}</small></div>;
}
