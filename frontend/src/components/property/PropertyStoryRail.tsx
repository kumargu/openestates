import { useEffect, useState } from "react";
import "../../styles/property-story-navigation.css";

export type PropertyStoryChapter = {
  id: string;
  label: string;
};

type Props = {
  chapters: PropertyStoryChapter[];
};

export function PropertyStoryRail({ chapters }: Props) {
  const [activeIndex, setActiveIndex] = useState(0);

  useEffect(() => {
    let frame = 0;
    const update = () => {
      frame = 0;
      const marker = window.innerHeight * 0.38;
      let nextIndex = 0;
      chapters.forEach((chapter, index) => {
        const element = document.getElementById(chapter.id);
        if (element && element.getBoundingClientRect().top <= marker) {
          nextIndex = index;
        }
      });
      setActiveIndex((current) => current === nextIndex ? current : nextIndex);
    };
    const schedule = () => {
      if (frame) return;
      frame = window.requestAnimationFrame(update);
    };
    update();
    window.addEventListener("scroll", schedule, { passive: true });
    window.addEventListener("resize", schedule);
    return () => {
      if (frame) window.cancelAnimationFrame(frame);
      window.removeEventListener("scroll", schedule);
      window.removeEventListener("resize", schedule);
    };
  }, [chapters]);

  if (chapters.length <= 2) return null;

  function goToChapter(index: number) {
    const chapter = chapters[index];
    if (!chapter) return;
    const reducedMotion = window.matchMedia(
      "(prefers-reduced-motion: reduce)",
    ).matches;
    document.getElementById(chapter.id)?.scrollIntoView({
      behavior: reducedMotion ? "auto" : "smooth",
      block: "start",
    });
    setActiveIndex(index);
  }

  const activeChapter = chapters[activeIndex] ?? chapters[0];

  return (
    <>
      <aside className="property-story-rail" aria-label="Property story chapters">
        {chapters.map((chapter, index) => (
          <button
            key={chapter.id}
            type="button"
            className={index === activeIndex ? "is-active" : ""}
            aria-label={`Go to ${chapter.label}`}
            aria-current={index === activeIndex ? "step" : undefined}
            onClick={() => goToChapter(index)}
          >
            <span aria-hidden="true" />
            {index === activeIndex && <strong>{chapter.label}</strong>}
          </button>
        ))}
      </aside>

      <nav
        className={`property-story-mobile-progress${
          activeIndex === 0 ? " is-overview" : ""
        }`}
        aria-label="Story progress"
      >
        <button
          type="button"
          aria-label="Previous chapter"
          disabled={activeIndex === 0}
          onClick={() => goToChapter(activeIndex - 1)}
        >
          ←
        </button>
        <span aria-live="polite">
          <small>{activeIndex + 1} / {chapters.length}</small>
          <strong>{activeChapter?.label}</strong>
        </span>
        <button
          type="button"
          aria-label="Next chapter"
          disabled={activeIndex === chapters.length - 1}
          onClick={() => goToChapter(activeIndex + 1)}
        >
          →
        </button>
      </nav>
    </>
  );
}
