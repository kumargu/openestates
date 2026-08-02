import { useCallback, useEffect, useRef, useState } from "react";

function prefersReducedMotion(): boolean {
  return typeof window !== "undefined"
    && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

export function useLandingSceneController(sceneIds: readonly string[]) {
  const sceneNodes = useRef(new Map<string, HTMLElement>());
  const sceneRatios = useRef(new Map<string, number>());
  const sceneRefCallbacks = useRef(new Map<string, (node: HTMLElement | null) => void>());
  const [activeSceneId, setActiveSceneId] = useState<string | null>(
    () => prefersReducedMotion() ? sceneIds[0] ?? null : null,
  );
  const [enteredSceneIds, setEnteredSceneIds] = useState<Set<string>>(
    () => prefersReducedMotion() ? new Set(sceneIds) : new Set(),
  );
  const [isReducedMotion, setIsReducedMotion] = useState(prefersReducedMotion);
  const [pausedSceneId, setPausedSceneId] = useState<string | null>(null);
  const [isDocumentHidden, setIsDocumentHidden] = useState(
    () => typeof document !== "undefined" && document.visibilityState !== "visible",
  );

  const sceneRef = useCallback((sceneId: string) => {
    const existing = sceneRefCallbacks.current.get(sceneId);
    if (existing) return existing;

    const callback = (node: HTMLElement | null) => {
      if (node) {
        sceneNodes.current.set(sceneId, node);
      } else {
        sceneNodes.current.delete(sceneId);
        sceneRatios.current.delete(sceneId);
      }
    };
    sceneRefCallbacks.current.set(sceneId, callback);
    return callback;
  }, []);

  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const handleChange = () => {
      setIsReducedMotion(media.matches);
      if (media.matches) {
        setEnteredSceneIds(new Set(sceneIds));
      }
    };

    handleChange();
    media.addEventListener("change", handleChange);
    return () => media.removeEventListener("change", handleChange);
  }, [sceneIds]);

  useEffect(() => {
    if (isReducedMotion) return undefined;

    const observer = new IntersectionObserver((entries) => {
      for (const entry of entries) {
        const sceneId = (entry.target as HTMLElement).dataset.sceneId;
        if (!sceneId) continue;
        sceneRatios.current.set(sceneId, entry.isIntersecting ? entry.intersectionRatio : 0);
      }

      let nextSceneId: string | null = null;
      let highestRatio = 0;
      for (const sceneId of sceneIds) {
        const ratio = sceneRatios.current.get(sceneId) ?? 0;
        if (ratio > highestRatio) {
          highestRatio = ratio;
          nextSceneId = sceneId;
        }
      }

      setActiveSceneId((current) => current === nextSceneId ? current : nextSceneId);
      if (nextSceneId) {
        setEnteredSceneIds((current) => {
          if (current.has(nextSceneId)) return current;
          const next = new Set(current);
          next.add(nextSceneId);
          return next;
        });
      }
    }, {
      rootMargin: "-22% 0px -22% 0px",
      threshold: [0, 0.15, 0.3, 0.45, 0.6, 0.75, 0.9],
    });

    for (const sceneId of sceneIds) {
      const node = sceneNodes.current.get(sceneId);
      if (node) observer.observe(node);
    }

    return () => observer.disconnect();
  }, [isReducedMotion, sceneIds]);

  useEffect(() => {
    const handleVisibilityChange = () => {
      setIsDocumentHidden(document.visibilityState !== "visible");
    };

    document.addEventListener("visibilitychange", handleVisibilityChange);
    return () => document.removeEventListener("visibilitychange", handleVisibilityChange);
  }, []);

  const pauseScene = useCallback((sceneId: string) => {
    setPausedSceneId(sceneId);
  }, []);

  const resumeScene = useCallback((sceneId: string) => {
    setPausedSceneId((current) => current === sceneId ? null : current);
  }, []);

  return {
    activeSceneId,
    hasEntered: (sceneId: string) => enteredSceneIds.has(sceneId),
    isPaused: (sceneId: string) => isDocumentHidden || pausedSceneId === sceneId,
    isReducedMotion,
    pauseScene,
    resumeScene,
    sceneRef,
  };
}
