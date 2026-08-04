import { useEffect, useRef } from "react";

function clamp(value: number): number {
  return Math.min(1, Math.max(0, value));
}

export function useLandingStoryMotion(reducedMotion: boolean) {
  const storyRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const story = storyRef.current;
    if (!story || reducedMotion) return undefined;

    let animationFrame = 0;

    const updateMotion = () => {
      animationFrame = 0;
      const viewportHeight = window.innerHeight;

      story.querySelectorAll<HTMLElement>("[data-scene-id]").forEach((scene) => {
        const bounds = scene.getBoundingClientRect();
        const centerOffset = (bounds.top + bounds.height / 2 - viewportHeight / 2) / viewportHeight;
        const boundedOffset = Math.max(-1, Math.min(1, centerOffset));
        const distance = clamp(Math.abs(centerOffset) / 0.72);
        const emphasis = 1 - distance;
        const sceneProgress = clamp(0.5 - centerOffset / 1.4);

        scene.style.setProperty("--landing-scene-lift", `${(-boundedOffset * 12).toFixed(2)}px`);
        scene.style.setProperty("--landing-scene-content-lift", `${(boundedOffset * 4).toFixed(2)}px`);
        scene.style.setProperty("--landing-scene-scale", (0.985 + emphasis * 0.015).toFixed(4));
        scene.style.setProperty("--landing-scene-glint", `${(-48 + sceneProgress * 96).toFixed(2)}%`);
        scene.style.setProperty("--landing-copy-blur", `${(distance * 4.5).toFixed(2)}px`);
        scene.style.setProperty("--landing-copy-lift", `${(boundedOffset * 14).toFixed(2)}px`);
        scene.style.setProperty("--landing-copy-opacity", (0.42 + emphasis * 0.58).toFixed(3));
      });
    };

    const requestUpdate = () => {
      if (animationFrame === 0) animationFrame = window.requestAnimationFrame(updateMotion);
    };

    const resizeObserver = new ResizeObserver(requestUpdate);
    resizeObserver.observe(story);
    window.addEventListener("scroll", requestUpdate, { passive: true });
    window.addEventListener("resize", requestUpdate);
    requestUpdate();

    return () => {
      resizeObserver.disconnect();
      window.removeEventListener("scroll", requestUpdate);
      window.removeEventListener("resize", requestUpdate);
      if (animationFrame !== 0) window.cancelAnimationFrame(animationFrame);
    };
  }, [reducedMotion]);

  return storyRef;
}
