import { Link } from "react-router-dom";
import type { ReactNode } from "react";

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
  return (
    <header className="workspace-header">
      <div className="workspace-header__main">
        <strong className="workspace-header__title">Workspace</strong>
        <nav className="workspace-header__tabs" aria-label="Workspace views">
          <Link
            to="/workspace"
            className={mode === "notes" ? "is-active" : undefined}
            aria-current={mode === "notes" ? "page" : undefined}
          >
            Notes
          </Link>
          <Link
            to={compareHref}
            className={mode === "compare" ? "is-active" : undefined}
            aria-current={mode === "compare" ? "page" : undefined}
          >
            Compare
            {compareCount > 0 && <span className="workspace-header__count">{compareCount}</span>}
          </Link>
          <Link
            to={buyVsRentHref}
            className={mode === "buy-vs-rent" ? "is-active" : undefined}
            aria-current={mode === "buy-vs-rent" ? "page" : undefined}
          >
            Rent vs buy
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
