import { formatCurrency } from "./model.ts";
import type { PlanScenarioId } from "./PlanGraph.tsx";

type VerdictBlockProps = {
  activeYear: number;
  buyWins: boolean;
  advantage: number;
  isPreview: boolean;
  breakEvenYear: number | null;
  homeEquity: number;
  monthlyGapSummary: string;
  changeNote: string | null;
  monthlyEmi: number;
  monthlyRent: number;
  buyNetWorth: number;
  rentNetWorth: number;
  selectedScenario: PlanScenarioId;
  onSelectScenario: (scenario: PlanScenarioId) => void;
};

export function VerdictBlock({
  activeYear,
  buyWins,
  advantage,
  isPreview,
  breakEvenYear,
  homeEquity,
  monthlyGapSummary,
  changeNote,
  monthlyEmi,
  monthlyRent,
  buyNetWorth,
  rentNetWorth,
  selectedScenario,
  onSelectScenario,
}: VerdictBlockProps) {
  const leader = buyWins ? "Buying" : "Renting and investing";
  const detailLine = breakEvenYear
    ? `Buying catches up in year ${breakEvenYear}. You would hold ${formatCurrency(homeEquity, true)} in home equity by year ${activeYear}.`
    : `Renting and investing stays ahead for the full 20 years. Home equity would be ${formatCurrency(homeEquity, true)} by year ${activeYear}.`;

  return (
    <header className="home-plan-verdict">
      <p className="home-plan-verdict__year">
        {isPreview ? `Year ${activeYear} (preview)` : `Year ${activeYear}`}
        {isPreview && <span className="home-plan-verdict__hint"> — tap the chart to keep this year</span>}
      </p>
      <h1 className="home-plan-verdict__headline">
        <span className="home-plan-verdict__leader">{leader}</span>
        {" "}leads by{" "}
        <span className="home-plan-verdict__amount">{formatCurrency(advantage, true)}</span>
      </h1>
      <p className="home-plan-verdict__detail">{detailLine}</p>
      <p className="home-plan-verdict__cashflow">{monthlyGapSummary}.</p>
      {changeNote && <p className="home-plan-verdict__change">{changeNote}</p>}
      <div className="home-plan-verdict__breakdown" role="group" aria-label={`Year ${activeYear} paths`}>
        <button
          type="button"
          className={`home-plan-verdict__tile home-plan-verdict__tile--buy ${selectedScenario === "buy" ? "is-selected" : ""}`}
          onClick={() => onSelectScenario("buy")}
          aria-pressed={selectedScenario === "buy"}
        >
          <span className="home-plan-verdict__tile-label"><i />Buy path</span>
          <strong>{formatCurrency(buyNetWorth, true)}</strong>
          <small>EMI {formatCurrency(monthlyEmi)}/mo</small>
        </button>
        <button
          type="button"
          className={`home-plan-verdict__tile home-plan-verdict__tile--rent ${selectedScenario === "rent" ? "is-selected" : ""}`}
          onClick={() => onSelectScenario("rent")}
          aria-pressed={selectedScenario === "rent"}
        >
          <span className="home-plan-verdict__tile-label"><i />Rent + SIP</span>
          <strong>{formatCurrency(rentNetWorth, true)}</strong>
          <small>Rent {formatCurrency(monthlyRent)}/mo</small>
        </button>
      </div>
    </header>
  );
}
