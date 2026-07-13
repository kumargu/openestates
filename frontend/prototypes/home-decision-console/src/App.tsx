import { useMemo, useState } from "react";
import {
  BASE_INPUTS,
  calculateProjection,
  formatCurrency,
  type PlanInputs,
} from "./model.ts";

type ScenarioId = "buy" | "rent" | "smaller";

type Scenario = {
  id: ScenarioId;
  label: string;
  description: string;
  value: number;
  monthlyCost: number;
  liquidity: string;
  ownership: string;
  color: string;
};

function Icon({ name }: { name: "home" | "tune" | "save" | "close" }) {
  const common = { width: 18, height: 18, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 1.8, strokeLinecap: "round" as const, strokeLinejoin: "round" as const, "aria-hidden": true };
  if (name === "home") return <svg {...common}><path d="m3 11 9-8 9 8" /><path d="M5 10v10h14V10" /><path d="M9 20v-6h6v6" /></svg>;
  if (name === "tune") return <svg {...common}><path d="M4 7h10" /><path d="M18 7h2" /><path d="M14 4v6" /><path d="M4 17h2" /><path d="M10 17h10" /><path d="M8 14v6" /></svg>;
  if (name === "save") return <svg {...common}><path d="M6 3h12l2 2v16H4V3h2Z" /><path d="M8 3v6h8V3" /><path d="M8 21v-7h8v7" /></svg>;
  if (name === "close") return <svg {...common}><path d="m6 6 12 12" /><path d="M18 6 6 18" /></svg>;
  return null;
}

function NetWorthGraph({
  scenarios,
  buyProjection,
  smallerProjection,
  horizon,
  selected,
  onSelect,
}: {
  scenarios: Scenario[];
  buyProjection: ReturnType<typeof calculateProjection>;
  smallerProjection: ReturnType<typeof calculateProjection>;
  horizon: number;
  selected: ScenarioId;
  onSelect: (id: ScenarioId) => void;
}) {
  const width = 760;
  const height = 300;
  const inset = { left: 58, right: 20, top: 18, bottom: 35 };
  const plotWidth = width - inset.left - inset.right;
  const plotHeight = height - inset.top - inset.bottom;
  const series = [
    { id: "buy" as const, values: buyProjection.points.map((point) => point.buyNetWorth), color: "#f18a68" },
    { id: "rent" as const, values: buyProjection.points.map((point) => point.rentNetWorth), color: "#69aedd" },
    { id: "smaller" as const, values: smallerProjection.points.map((point) => point.buyNetWorth), color: "#9aafa4" },
  ];
  const maxValue = Math.max(...series.flatMap((item) => item.values)) * 1.08;
  const x = (year: number) => inset.left + year / 20 * plotWidth;
  const y = (value: number) => inset.top + plotHeight - value / maxValue * plotHeight;
  const line = (values: number[]) => values.map((value, year) => `${year === 0 ? "M" : "L"}${x(year).toFixed(1)},${y(value).toFixed(1)}`).join(" ");
  const cursorX = x(horizon);

  return (
    <div className="net-worth-graph">
      <div className="graph-legend">
        {scenarios.map((scenario) => <button key={scenario.id} className={selected === scenario.id ? "selected" : ""} onClick={() => onSelect(scenario.id)}><i className={`legend-dot legend-dot--${scenario.id}`} /><span><strong>{scenario.label}</strong><small>{formatCurrency(scenario.value, true)} at {horizon}y</small></span></button>)}
      </div>
      <svg viewBox={`0 0 ${width} ${height}`} role="img" aria-label="Projected net worth over twenty years">
        {[.25, .5, .75, 1].map((ratio) => {
          const value = maxValue * ratio;
          const lineY = y(value);
          return <g key={ratio}><line x1={inset.left} x2={width - inset.right} y1={lineY} y2={lineY} className="graph-gridline" /><text x={inset.left - 9} y={lineY + 3} className="graph-y-label">{formatCurrency(value, true)}</text></g>;
        })}
        {[0, 5, 10, 15, 20].map((year) => <text key={year} x={x(year)} y={height - 9} className="graph-x-label">{year === 0 ? "Now" : `${year}y`}</text>)}
        {series.map((item) => <path key={item.id} d={line(item.values)} className={`graph-line graph-line--${item.id} ${selected === item.id ? "selected" : ""}`} />)}
        <line x1={cursorX} x2={cursorX} y1={inset.top} y2={inset.top + plotHeight} className="graph-cursor" />
        <text x={cursorX} y={inset.top - 5} className="graph-cursor-label">{horizon} YEARS</text>
        {series.map((item) => <circle key={item.id} cx={cursorX} cy={y(item.values[horizon])} r={selected === item.id ? 6 : 4} className={`graph-point graph-point--${item.id}`} />)}
      </svg>
    </div>
  );
}

function Driver({ label, value, helper, min, max, step, current, onChange }: { label: string; value: string; helper: string; min: number; max: number; step: number; current: number; onChange: (value: number) => void }) {
  return (
    <label className="driver">
      <span className="driver__top"><span><strong>{label}</strong><small>{helper}</small></span><b>{value}</b></span>
      <input type="range" min={min} max={max} step={step} value={current} onChange={(event) => onChange(Number(event.target.value))} />
    </label>
  );
}

export function App() {
  const [inputs, setInputs] = useState<PlanInputs>({ ...BASE_INPUTS, purchaseYear: 1, holdingPeriodYears: 20 });
  const [horizon, setHorizon] = useState(10);
  const [selected, setSelected] = useState<ScenarioId>("buy");
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [saved, setSaved] = useState(false);

  const buyProjection = useMemo(() => calculateProjection(inputs), [inputs]);
  const smallerInputs = useMemo<PlanInputs>(() => ({ ...inputs, propertyPriceLakh: 115, downPaymentLakh: 30, purchaseYear: Math.min(inputs.holdingPeriodYears - 1, inputs.purchaseYear + 1) }), [inputs]);
  const smallerProjection = useMemo(() => calculateProjection(smallerInputs), [smallerInputs]);
  const buyPoint = buyProjection.points[horizon];
  const smallerPoint = smallerProjection.points[horizon];

  const scenarios: Scenario[] = [
    {
      id: "buy",
      label: "Buy this home",
      description: `₹${inputs.propertyPriceLakh}L home · buy in year ${inputs.purchaseYear}`,
      value: buyPoint.buyNetWorth,
      monthlyCost: buyPoint.annualEmi / 12,
      liquidity: formatCurrency(buyProjection.liquidityAfterDownPayment, true),
      ownership: formatCurrency(Math.max(0, buyPoint.propertyValue - buyPoint.loanBalance), true),
      color: "#f18a68",
    },
    {
      id: "rent",
      label: "Keep renting and invest",
      description: `₹${inputs.currentRentThousands}K rent · invest the EMI difference`,
      value: buyPoint.rentNetWorth,
      monthlyCost: buyPoint.annualRent / 12,
      liquidity: "High",
      ownership: formatCurrency(buyPoint.rentNetWorth, true),
      color: "#69aedd",
    },
    {
      id: "smaller",
      label: "Buy a smaller home",
      description: "₹115L home · ₹30L down payment",
      value: smallerPoint.buyNetWorth,
      monthlyCost: smallerPoint.annualEmi / 12,
      liquidity: formatCurrency(smallerProjection.liquidityAfterDownPayment, true),
      ownership: formatCurrency(Math.max(0, smallerPoint.propertyValue - smallerPoint.loanBalance), true),
      color: "#9aafa4",
    },
  ];

  const ranked = [...scenarios].sort((first, second) => second.value - first.value);
  const winner = ranked[0];
  const advantage = winner.value - ranked[1].value;
  const selectedScenario = scenarios.find((scenario) => scenario.id === selected) ?? scenarios[0];
  const selectedProjection = selected === "smaller" ? smallerProjection : buyProjection;

  const update = (key: keyof PlanInputs, value: number) => setInputs((current) => ({ ...current, [key]: value }));

  return (
    <div className="dashboard-shell">
      <header className="dashboard-header">
        <div className="dashboard-brand"><span><Icon name="home" /></span><strong>OpenEstates</strong></div>
        <div className="dashboard-property"><small>Home plan</small><strong>3 BHK · Whitefield · ₹1.50 Cr</strong></div>
        <div className="dashboard-actions"><button onClick={() => setDetailsOpen(true)}><Icon name="tune" />Details</button><button className={`save-button ${saved ? "saved" : ""}`} onClick={() => setSaved((current) => !current)}><Icon name="save" />{saved ? "Saved" : "Save"}</button></div>
      </header>

      <main className="dashboard-main">
        <section className="dashboard-pane">
          <div className="pane-toolbar">
            <div><span>BUY VS RENT</span><h1>Which choice builds the strongest position?</h1></div>
            <div className="horizon-control"><span>View at</span><div>{[5, 10, 15, 20].map((year) => <button key={year} className={horizon === year ? "active" : ""} onClick={() => setHorizon(year)}>{year} years</button>)}</div></div>
          </div>

          <div className="pane-answer"><span>Current result</span><strong>At {horizon} years, {winner.label.toLowerCase()} leads by {formatCurrency(advantage, true)}.</strong><p>Change a driver below. Every value updates in place.</p></div>

          <div className="pane-grid">
            <section className="comparison-panel">
              <div className="panel-heading"><div><span>SCENARIO COMPARISON</span><h2>Projected net worth</h2></div><small>Select a series for details</small></div>
              <NetWorthGraph scenarios={scenarios} buyProjection={buyProjection} smallerProjection={smallerProjection} horizon={horizon} selected={selected} onSelect={setSelected} />
            </section>

            <section className={`breakdown-panel breakdown-panel--${selected}`}>
              <div className="panel-heading"><div><span>SELECTED OPTION</span><h2>{selectedScenario.label}</h2></div><i /> </div>
              <div className="selected-value"><span>Projected net worth</span><strong>{formatCurrency(selectedScenario.value, true)}</strong><small>at year {horizon}</small></div>
              <div className="metric-grid"><div><span>Monthly housing cost</span><strong>{formatCurrency(selectedScenario.monthlyCost)}</strong></div><div><span>Liquid after decision</span><strong>{selectedScenario.liquidity}</strong></div><div><span>{selected === "rent" ? "Investment portfolio" : "Home equity"}</span><strong>{selectedScenario.ownership}</strong></div><div><span>Break-even</span><strong>{selected === "rent" ? "Not applicable" : selectedProjection.breakEvenYear ? `Year ${selectedProjection.breakEvenYear}` : "Beyond 20 years"}</strong></div></div>
              <p className="breakdown-note">{selected === "buy" ? "Higher ownership, but more capital is committed upfront." : selected === "rent" ? "Highest flexibility, but the outcome depends more on investment returns." : "Lower monthly pressure with less property exposure."}</p>
            </section>
          </div>

          <section className="driver-panel">
            <div className="panel-heading"><div><span>PRIMARY DRIVERS</span><h2>Test the decision</h2></div><small>Advanced assumptions stay in Details</small></div>
            <div className="driver-grid"><Driver label="Buy timing" value={inputs.purchaseYear === 0 ? "Now" : `Year ${inputs.purchaseYear}`} helper="When the home purchase happens" min={0} max={7} step={1} current={inputs.purchaseYear} onChange={(value) => update("purchaseYear", value)} /><Driver label="Down payment" value={`₹${inputs.downPaymentLakh}L`} helper="Cash committed upfront" min={20} max={80} step={5} current={inputs.downPaymentLakh} onChange={(value) => update("downPaymentLakh", value)} /><Driver label="Property growth" value={`${inputs.appreciation.toFixed(1)}%`} helper="Annual model assumption" min={2} max={10} step={.5} current={inputs.appreciation} onChange={(value) => update("appreciation", value)} /></div>
          </section>
        </section>
      </main>

      {detailsOpen && <button className="details-backdrop" onClick={() => setDetailsOpen(false)} aria-label="Close details" />}
      <aside className={`details-drawer ${detailsOpen ? "open" : ""}`}>
        <div className="details-header"><div><span>MODEL DETAILS</span><h2>Assumptions and evidence</h2></div><button onClick={() => setDetailsOpen(false)} aria-label="Close"><Icon name="close" /></button></div>
        <div className="detail-section"><h3>Advanced assumptions</h3><Assumption label="Loan rate" value={`${inputs.loanRate.toFixed(1)}%`} min={6.5} max={11} step={.1} current={inputs.loanRate} onChange={(value) => update("loanRate", value)} /><Assumption label="Equity return" value={`${inputs.equityReturn.toFixed(1)}%`} min={6} max={14} step={.5} current={inputs.equityReturn} onChange={(value) => update("equityReturn", value)} /><Assumption label="Rent inflation" value={`${inputs.rentInflation.toFixed(1)}%`} min={3} max={10} step={.5} current={inputs.rentInflation} onChange={(value) => update("rentInflation", value)} /></div>
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
