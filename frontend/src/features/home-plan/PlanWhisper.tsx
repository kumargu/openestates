import { useEffect, useMemo, useState } from "react";
import { PLAN_WHISPERS } from "./planWhispers.ts";

const ROTATE_MS = 11_500;
const FADE_MS = 450;

function shuffled<T>(items: readonly T[]): T[] {
  const copy = [...items];
  for (let i = copy.length - 1; i > 0; i -= 1) {
    const j = Math.floor(Math.random() * (i + 1));
    [copy[i], copy[j]] = [copy[j], copy[i]];
  }
  return copy;
}

export function PlanWhisper() {
  // Shuffle once per mount so grouped source order (rent / buy / invest) reads
  // as a mixed, unbiased rotation instead of themed blocks.
  const whispers = useMemo(() => shuffled(PLAN_WHISPERS), []);
  const [index, setIndex] = useState(0);
  const [fading, setFading] = useState(false);
  const [motionOk, setMotionOk] = useState(true);

  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => setMotionOk(!media.matches);
    update();
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);

  useEffect(() => {
    if (!motionOk || whispers.length <= 1) return undefined;

    const interval = window.setInterval(() => {
      setFading(true);
      window.setTimeout(() => {
        setIndex((current) => (current + 1) % whispers.length);
        setFading(false);
      }, FADE_MS);
    }, ROTATE_MS);

    return () => window.clearInterval(interval);
  }, [motionOk, whispers.length]);

  if (whispers.length === 0) return null;

  return (
    <div className="home-plan-whisper-orbit">
      <p
        className={`home-plan-whisper${fading ? " home-plan-whisper--fading" : ""}`}
        aria-hidden="true"
      >
        {whispers[index]}
      </p>
    </div>
  );
}
