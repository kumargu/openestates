import { useEffect, useMemo, useState } from "react";
import { planWhispersFor, type PlanWhisperTheme } from "./planWhispers.ts";

const ROTATE_MS = 11_500;
const FADE_MS = 450;

type PlanWhisperProps = {
  theme: PlanWhisperTheme;
};

export function PlanWhisper({ theme }: PlanWhisperProps) {
  const whispers = useMemo(() => planWhispersFor(theme), [theme]);
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
      <p className={`home-plan-whisper${fading ? " home-plan-whisper--fading" : ""}`}>
        {whispers[index]}
      </p>
    </div>
  );
}
