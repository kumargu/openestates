import { useEffect, useState, type MouseEvent } from "react";
import {
  isShortlisted,
  SHORTLIST_CHANGED_EVENT,
  toggleShortlistId,
} from "../lib/compare.ts";

type SaveHeartButtonProps = {
  propertyId: string;
  className?: string;
  label?: string;
};

export function SaveHeartButton({ propertyId, className = "", label }: SaveHeartButtonProps) {
  const [saved, setSaved] = useState(() => isShortlisted(propertyId));

  useEffect(() => {
    function sync() {
      setSaved(isShortlisted(propertyId));
    }
    sync();
    window.addEventListener(SHORTLIST_CHANGED_EVENT, sync);
    window.addEventListener("storage", sync);
    return () => {
      window.removeEventListener(SHORTLIST_CHANGED_EVENT, sync);
      window.removeEventListener("storage", sync);
    };
  }, [propertyId]);

  function handleClick(event: MouseEvent<HTMLButtonElement>) {
    event.preventDefault();
    event.stopPropagation();
    const next = toggleShortlistId(propertyId);
    setSaved(next.includes(propertyId));
  }

  return (
    <button
      type="button"
      className={`save-heart${saved ? " is-saved" : ""}${className ? ` ${className}` : ""}`}
      aria-label={saved ? "Remove from shortlist" : "Save for later"}
      aria-pressed={saved}
      title={saved ? "Saved" : "Save for later"}
      onClick={handleClick}
    >
      <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true">
        <path
          d="M12 20.5s-6.8-4.2-9.1-8.1C1.2 9.7 2.1 6.2 5.2 5.1c1.8-.6 3.7.1 4.8 1.5C11.1 5.2 13 4.5 14.8 5.1c3.1 1.1 4 4.6 2.3 7.3C18.8 16.3 12 20.5 12 20.5Z"
          fill={saved ? "currentColor" : "none"}
          stroke="currentColor"
          strokeWidth="1.8"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
      {label && <span>{label}</span>}
    </button>
  );
}
