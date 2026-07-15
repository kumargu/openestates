export type PlanView = "netWorth" | "monthly" | "payoff";

const TABS: Array<{ id: PlanView; label: string; subtitle: string }> = [
  { id: "netWorth", label: "Net worth", subtitle: "Which path builds more wealth over the years." },
  { id: "monthly", label: "Monthly", subtitle: "What each path costs you every month." },
  { id: "payoff", label: "Pay off loan", subtitle: "How extra payments clear the loan sooner." },
];

type PlanViewTabsProps = {
  view: PlanView;
  onChange: (view: PlanView) => void;
};

export function PlanViewTabs({ view, onChange }: PlanViewTabsProps) {
  const active = TABS.find((tab) => tab.id === view) ?? TABS[0];

  return (
    <div className="home-plan-view-tabs">
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
            {tab.label}
          </button>
        ))}
      </div>
      <p className="home-plan-view-tabs__subtitle">{active.subtitle}</p>
    </div>
  );
}
