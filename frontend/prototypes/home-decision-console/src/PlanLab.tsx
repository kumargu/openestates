import { useState } from "react";
import { formatCurrency } from "./model.ts";

export type ExperimentId = "delay" | "downPayment" | "growth" | "loanRate" | "fundReturn" | "extraSip";

export type PlanExperiment = {
  id: ExperimentId;
  title: string;
  description: string;
  controlLabel: string;
  value: number;
  min: number;
  max: number;
  step: number;
  displayValue: string;
};

export type ExperimentImpact = {
  winnerLabel: string;
  advantage: number;
  monthlyCostDelta: number;
  liquidityDelta: number;
  buyRentGap: number;
  breakEven: string;
  baselineBreakEven: string;
};

function signedCurrency(value: number): string {
  if (Math.abs(value) < 1) return "No change";
  return `${value > 0 ? "+" : "−"}${formatCurrency(Math.abs(value), true)}`;
}

export function PlanLab({ experiments, activeExperiment, impact, reversalInsight, onSelect, onValueChange, onKeep, onReset }: { experiments: PlanExperiment[]; activeExperiment: PlanExperiment | null; impact: ExperimentImpact | null; reversalInsight: string; onSelect: (id: ExperimentId) => void; onValueChange: (value: number) => void; onKeep: () => void; onReset: () => void }) {
  const [open, setOpen] = useState(false);

  return (
    <section className={`plan-lab ${open ? "open" : ""}`}>
      <button className="plan-lab__summary" onClick={() => setOpen((current) => !current)} aria-expanded={open}>
        <span><small>TEST THE PLAN</small><strong>What could change the answer?</strong><em>{reversalInsight}</em></span>
        <span>{open ? "Close" : "Run experiments"}</span>
      </button>

      <div className="plan-lab__body">
        <div className="experiment-grid">
          {experiments.map((experiment) => <button key={experiment.id} className={activeExperiment?.id === experiment.id ? "active" : ""} onClick={() => onSelect(experiment.id)}><strong>{experiment.title}</strong><small>{experiment.description}</small></button>)}
        </div>

        {activeExperiment && impact ? (
          <div className="experiment-workbench">
            <label className="experiment-control">
              <span><span><small>ACTIVE EXPERIMENT</small><strong>{activeExperiment.controlLabel}</strong></span><b>{activeExperiment.displayValue}</b></span>
              <input type="range" min={activeExperiment.min} max={activeExperiment.max} step={activeExperiment.step} value={activeExperiment.value} onChange={(event) => onValueChange(Number(event.target.value))} />
              <small>{activeExperiment.description}</small>
            </label>

            <div className="experiment-impact">
              <span>LIVE IMPACT AT CURRENT HORIZON</span>
              <h3>{impact.winnerLabel} leads by {formatCurrency(impact.advantage, true)}</h3>
              <dl><div><dt>Monthly buy cost</dt><dd>{signedCurrency(impact.monthlyCostDelta)}</dd></div><div><dt>Liquid savings</dt><dd>{signedCurrency(impact.liquidityDelta)}</dd></div><div><dt>Buy versus rent gap</dt><dd>{signedCurrency(impact.buyRentGap)}</dd></div><div><dt>Break-even</dt><dd>{impact.breakEven}<small>Was {impact.baselineBreakEven}</small></dd></div></dl>
              <div className="experiment-actions"><button onClick={onReset}>Reset</button><button onClick={onKeep}>Keep this change</button></div>
            </div>
          </div>
        ) : (
          <div className="experiment-empty"><strong>Choose one question to test.</strong><span>The graph and decision summary will preview the change without replacing your saved plan.</span></div>
        )}

        <div className="reversal-insight"><span>DECISION BOUNDARY</span><strong>{reversalInsight}</strong><small>This is a model threshold, not a market prediction.</small></div>
      </div>
    </section>
  );
}
