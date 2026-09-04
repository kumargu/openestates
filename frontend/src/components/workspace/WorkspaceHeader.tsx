import { Link } from "react-router-dom";
import type { ReactNode } from "react";
import {
  hrefWithSearchSpan,
  searchSpanReferenceForTarget,
} from "../../lib/navigationContext.ts";
import { useSearchSpan } from "./SearchSpanContext.ts";

export type WorkspaceMode = "notes" | "compare" | "buy-vs-rent";

type WorkspaceHeaderProps = {
  mode: WorkspaceMode;
  compareHref: string;
  buyVsRentHref: string;
  compareCount?: number;
  context?: ReactNode;
  contextDisplay?: "always" | "mobile-only";
};

export function WorkspaceHeader({
  mode,
  compareHref,
  buyVsRentHref,
  compareCount = 0,
  context,
  contextDisplay = "always",
}: WorkspaceHeaderProps) {
  const searchSpan = useSearchSpan();
  const reference = searchSpanReferenceForTarget(searchSpan);
  const notesHref = hrefWithSearchSpan("/workspace", reference);
  const carriedCompareHref = hrefWithSearchSpan(compareHref, reference);
  const carriedBuyVsRentHref = hrefWithSearchSpan(buyVsRentHref, reference);

  return (
    <header className="workspace-header">
      <div className="workspace-header__main">
        <strong className="workspace-header__title">Workspace</strong>
        <nav className="workspace-header__tabs" aria-label="Workspace views">
          <Link
            to={notesHref}
            className={mode === "notes" ? "is-active" : undefined}
            aria-current={mode === "notes" ? "page" : undefined}
          >
            Notes
          </Link>
          <Link
            to={carriedCompareHref}
            className={mode === "compare" ? "is-active" : undefined}
            aria-current={mode === "compare" ? "page" : undefined}
          >
            Compare
            {compareCount > 0 && <span className="workspace-header__count">{compareCount}</span>}
          </Link>
          <Link
            to={carriedBuyVsRentHref}
            className={mode === "buy-vs-rent" ? "is-active" : undefined}
            aria-current={mode === "buy-vs-rent" ? "page" : undefined}
          >
            EMI Plan
          </Link>
        </nav>
      </div>
      {context && (
        <div className={`workspace-header__context workspace-header__context--${contextDisplay}`}>
          {context}
        </div>
      )}
    </header>
  );
}
