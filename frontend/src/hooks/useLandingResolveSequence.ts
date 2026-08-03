import { useEffect, useState } from "react";

const RESOLVE_PHASES = ["rest", "query", "candidates", "selection", "proof"] as const;

type ResolvePhase = typeof RESOLVE_PHASES[number];

type ResolveSequenceInput = {
  active: boolean;
  paused: boolean;
  reducedMotion: boolean;
};

const PHASE_DELAY_MS: Partial<Record<ResolvePhase, number>> = {
  query: 460,
  candidates: 520,
  selection: 440,
};

function nextPhase(phase: ResolvePhase): ResolvePhase {
  const currentIndex = RESOLVE_PHASES.indexOf(phase);
  return RESOLVE_PHASES[Math.min(currentIndex + 1, RESOLVE_PHASES.length - 1)];
}

export function useLandingResolveSequence({
  active,
  paused,
  reducedMotion,
}: ResolveSequenceInput) {
  const [phase, setPhase] = useState<ResolvePhase>(() => {
    if (reducedMotion) return "proof";
    return active ? "query" : "rest";
  });

  useEffect(() => {
    const delay = PHASE_DELAY_MS[phase];
    if (!active || paused || reducedMotion || delay == null) return undefined;

    const timer = window.setTimeout(() => setPhase(nextPhase(phase)), delay);
    return () => window.clearTimeout(timer);
  }, [active, paused, phase, reducedMotion]);

  const phaseIndex = RESOLVE_PHASES.indexOf(phase);
  return {
    phase,
    queryVisible: phaseIndex >= RESOLVE_PHASES.indexOf("query"),
    candidatesVisible: phaseIndex >= RESOLVE_PHASES.indexOf("candidates"),
    selectionVisible: phaseIndex >= RESOLVE_PHASES.indexOf("selection"),
    proofVisible: phaseIndex >= RESOLVE_PHASES.indexOf("proof"),
  };
}
