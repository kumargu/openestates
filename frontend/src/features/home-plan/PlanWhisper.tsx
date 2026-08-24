import { useEffect, useMemo, useRef, useState } from "react";
import { planWhispersForContext, type PlanWhisperTheme } from "./planWhispers.ts";

const ROTATE_MS = 9_000;

type PlanWhisperProps = {
  theme: PlanWhisperTheme;
  activeYear: number;
  loanFreeYear: number | null;
};

export function PlanWhisper({ theme, activeYear, loanFreeYear }: PlanWhisperProps) {
  const hostRef = useRef<HTMLElement>(null);
  const whispers = useMemo(
    () => planWhispersForContext({ theme, activeYear, loanFreeYear }),
    [activeYear, loanFreeYear, theme],
  );
  const [index, setIndex] = useState(0);
  const [motionAllowed, setMotionAllowed] = useState(true);
  const [hostVisible, setHostVisible] = useState(true);
  const [documentVisible, setDocumentVisible] = useState(
    () => typeof document === "undefined" || !document.hidden,
  );

  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => setMotionAllowed(!media.matches);
    update();
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);

  useEffect(() => {
    const host = hostRef.current;
    if (!host || typeof IntersectionObserver === "undefined") return undefined;
    const observer = new IntersectionObserver(([entry]) => {
      setHostVisible(entry?.isIntersecting ?? true);
    });
    observer.observe(host);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const update = () => setDocumentVisible(!document.hidden);
    document.addEventListener("visibilitychange", update);
    return () => document.removeEventListener("visibilitychange", update);
  }, []);

  useEffect(() => {
    if (!motionAllowed || !hostVisible || !documentVisible || whispers.length <= 1) {
      return undefined;
    }
    const interval = window.setInterval(() => {
      setIndex((current) => (current + 1) % whispers.length);
    }, ROTATE_MS);
    return () => window.clearInterval(interval);
  }, [documentVisible, hostVisible, motionAllowed, whispers.length]);

  const whisper = whispers[index % whispers.length];
  return (
    <aside ref={hostRef} className="home-plan-perspective" aria-label="A lighter perspective">
      <p
        key={whisper}
        className="home-plan-whisper"
      >
        {whisper}
      </p>
    </aside>
  );
}
