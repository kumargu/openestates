import { useEffect, useMemo, useState } from "react";
import { PLAN_WHISPERS } from "./planWhispers.ts";

const ROTATE_MS = 11_500;
const FADE_MS = 450;

function shuffled<T>(items: readonly T[]): T[] {
  const copy = [...items];
  for (let index = copy.length - 1; index > 0; index -= 1) {
    const nextIndex = Math.floor(Math.random() * (index + 1));
    [copy[index], copy[nextIndex]] = [copy[nextIndex], copy[index]];
  }
  return copy;
}

export function PlanWhisper() {
  const whispers = useMemo(() => shuffled(PLAN_WHISPERS), []);
  const [index, setIndex] = useState(0);
  const [fading, setFading] = useState(false);
  const [motionAllowed, setMotionAllowed] = useState(true);

  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => setMotionAllowed(!media.matches);
    update();
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);

  useEffect(() => {
    if (!motionAllowed || whispers.length <= 1) return undefined;
    let fadeTimeout: number | undefined;
    const interval = window.setInterval(() => {
      setFading(true);
      fadeTimeout = window.setTimeout(() => {
        setIndex((current) => (current + 1) % whispers.length);
        setFading(false);
      }, FADE_MS);
    }, ROTATE_MS);
    return () => {
      window.clearInterval(interval);
      if (fadeTimeout !== undefined) window.clearTimeout(fadeTimeout);
    };
  }, [motionAllowed, whispers.length]);

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
