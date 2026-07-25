import type { PlanMilestone } from "./planFields.ts";

type MilestoneHintProps = {
  milestone: PlanMilestone | null;
};

export function MilestoneHint({ milestone }: MilestoneHintProps) {
  if (!milestone) return null;

  return (
    <div className="home-plan-milestone-hint" role="status">
      <strong>{milestone.label}</strong>
      <span>{milestone.definition}</span>
    </div>
  );
}
