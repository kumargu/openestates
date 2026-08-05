import { useEffect, useState } from "react";

type LandingChapterSequenceInput = {
  active: boolean;
  delays: readonly number[];
  paused: boolean;
  reducedMotion: boolean;
};

type LandingLoopSequenceInput = {
  active: boolean;
  durations: readonly number[];
  paused: boolean;
  reducedMotion: boolean;
};

export function useLandingChapterSequence({
  active,
  delays,
  paused,
  reducedMotion,
}: LandingChapterSequenceInput): number {
  const finalPhase = delays.length;
  const [phase, setPhase] = useState(reducedMotion ? finalPhase : 0);

  useEffect(() => {
    const timer = window.setTimeout(() => setPhase(reducedMotion ? finalPhase : 0), 0);
    return () => window.clearTimeout(timer);
  }, [active, finalPhase, reducedMotion]);

  useEffect(() => {
    if (!active || paused || reducedMotion || phase >= finalPhase) return undefined;
    const timer = window.setTimeout(() => setPhase((current) => current + 1), delays[phase]);
    return () => window.clearTimeout(timer);
  }, [active, delays, finalPhase, paused, phase, reducedMotion]);

  return phase;
}

export function useLandingLoopSequence({
  active,
  durations,
  paused,
  reducedMotion,
}: LandingLoopSequenceInput): number {
  const finalPhase = Math.max(0, durations.length - 1);
  const [phase, setPhase] = useState(reducedMotion ? finalPhase : 0);

  useEffect(() => {
    const timer = window.setTimeout(() => setPhase(reducedMotion ? finalPhase : 0), 0);
    return () => window.clearTimeout(timer);
  }, [active, finalPhase, reducedMotion]);

  useEffect(() => {
    if (!active || paused || reducedMotion || durations.length <= 1) return undefined;
    const timer = window.setTimeout(() => {
      setPhase((current) => (current + 1) % durations.length);
    }, durations[phase] ?? durations[0]);
    return () => window.clearTimeout(timer);
  }, [active, durations, paused, phase, reducedMotion]);

  return phase;
}
