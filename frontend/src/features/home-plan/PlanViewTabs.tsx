export type PlanView = "netWorth" | "monthly" | "payoff";

const TABS: Array<{ id: PlanView; label: string; shortLabel: string; subtitle: string }> = [
  { id: "netWorth", label: "Net worth", shortLabel: "Net worth", subtitle: "Which path builds more wealth over the years." },
  { id: "monthly", label: "Monthly", shortLabel: "Monthly", subtitle: "What each path costs you every month." },
  { id: "payoff", label: "Pay off loan", shortLabel: "Payoff", subtitle: "How extra payments clear the loan sooner." },
];

type PlanViewTabsProps = {
  view: PlanView;
  onChange: (view: PlanView) => void;
  compact?: boolean;
};

export function PlanViewTabs({ view, onChange, compact = false }: PlanViewTabsProps) {
  const active = TABS.find((tab) => tab.id === view) ?? TABS[0];

  return (
    <div className={`home-plan-view-tabs${compact ? " home-plan-view-tabs--compact" : ""}`}>
      <div className="home-plan-view-tabs__seg" role="tablist" aria-label="What do you want to see?">
        {TABS.map((tab) => (
          <button
            type="button"
            key={tab.id}
            role="tab"
            aria-selected={view === tab.id}
            className={view === tab.id ? "is-active" : ""}
            onClick={() => onChange(tab.id)}
          >
            {compact ? tab.shortLabel : tab.label}
          </button>
        ))}
      </div>
      {!compact ? <p className="home-plan-view-tabs__subtitle">{active.subtitle}</p> : null}
    </div>
  );
}
