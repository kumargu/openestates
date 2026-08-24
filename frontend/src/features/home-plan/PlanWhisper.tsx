import { planWhisperFor, type PlanWhisperTheme } from "./planWhispers.ts";

type PlanWhisperProps = {
  theme: PlanWhisperTheme;
  activeYear: number;
  loanFreeYear: number | null;
};

export function PlanWhisper({ theme, activeYear, loanFreeYear }: PlanWhisperProps) {
  return (
    <aside className="home-plan-perspective" aria-label="A lighter perspective">
      <p className="home-plan-whisper">
        {planWhisperFor({ theme, activeYear, loanFreeYear })}
      </p>
    </aside>
  );
}
