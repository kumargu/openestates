import { formatCurrency } from "./model.ts";
import type { MonthlyPlanVerdict } from "./monthlyPlanView.ts";

type VerdictBlockProps = {
  verdict: MonthlyPlanVerdict;
};

export function VerdictBlock({ verdict }: VerdictBlockProps) {
  return (
    <header className="home-plan-verdict">
      <h1 className="home-plan-verdict__headline">
        {verdict.timeLabel}, you have{" "}
        <span className="home-plan-verdict__amount">{formatCurrency(verdict.advantage, true)} more</span>
        {" "}if you {verdict.choiceLabel}.
      </h1>
    </header>
  );
}
