import type { ReactNode } from "react";
import { formatCurrency } from "./model.ts";
import type { MonthlyPlanVerdict } from "./monthlyPlanView.ts";

type VerdictBlockProps = {
  verdict: MonthlyPlanVerdict;
  action?: ReactNode;
  aside?: ReactNode;
};

export function VerdictBlock({
  verdict,
  action,
  aside,
}: VerdictBlockProps) {
  return (
    <header className="home-plan-verdict">
      <div className="home-plan-verdict__topline">
        <h1 className="home-plan-verdict__headline">
          {verdict.timeLabel}, you have{" "}
          <span className="home-plan-verdict__amount">{formatCurrency(verdict.advantage, true)} more</span>
          {" "}if you {verdict.choiceLabel}.
        </h1>
        {action && <div className="home-plan-verdict__action">{action}</div>}
        {aside && <aside className="home-plan-verdict__aside">{aside}</aside>}
      </div>
    </header>
  );
}
