import { formatCurrency } from "./model.ts";

type VerdictBlockProps = {
  activeYear: number;
  buyWins: boolean;
  advantage: number;
};

export function VerdictBlock({
  activeYear,
  buyWins,
  advantage,
}: VerdictBlockProps) {
  const timeLabel = activeYear === 0
    ? "Today"
    : `After ${activeYear} ${activeYear === 1 ? "year" : "years"}`;
  const choice = buyWins ? "buy" : "rent and invest";

  return (
    <header className="home-plan-verdict">
      <div className="home-plan-verdict__topline">
        <h1 className="home-plan-verdict__headline">
          {timeLabel}, you have{" "}
          <span className="home-plan-verdict__amount">{formatCurrency(advantage, true)} more</span>
          {" "}if you {choice}.
        </h1>
      </div>
    </header>
  );
}
