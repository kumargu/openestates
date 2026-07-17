import { useEffect, useState } from "react";
import { PLAN_WHISPERS } from "./planWhispers.ts";

const ROTATE_MS = 10_000;
const FADE_MS = 500;

export function PlanWhisper() {
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
    if (!motionOk || PLAN_WHISPERS.length <= 1) return undefined;

    const interval = window.setInterval(() => {
      setFading(true);
      window.setTimeout(() => {
        setIndex((current) => (current + 1) % PLAN_WHISPERS.length);
        setFading(false);
      }, FADE_MS);
    }, ROTATE_MS);

    return () => window.clearInterval(interval);
  }, [motionOk]);

  if (PLAN_WHISPERS.length === 0) return null;

  return (
    <p
      className={`home-plan-whisper${fading ? " home-plan-whisper--fading" : ""}`}
      aria-hidden="true"
    >
      {PLAN_WHISPERS[index]}
    </p>
  );
}
