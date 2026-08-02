import { Link } from "react-router-dom";
import type { ReactNode } from "react";
import { discoveryReturnHref, requestDiscoveryReturn } from "../../lib/navigationContext.ts";

export type WorkspaceMode = "notes" | "compare" | "buy-vs-rent";

type WorkspaceHeaderProps = {
  mode: WorkspaceMode;
  compareHref: string;
  buyVsRentHref: string;
  compareCount?: number;
  context?: ReactNode;
  action?: ReactNode;
};

export function WorkspaceHeader({
  mode,
  compareHref,
  buyVsRentHref,
  compareCount = 0,
  context,
  action,
}: WorkspaceHeaderProps) {
  const addHomesHref = discoveryReturnHref();
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
            {compareCount > 0 && <span>{compareCount}</span>}
          </Link>
          <Link
            to={buyVsRentHref}
            className={mode === "buy-vs-rent" ? "is-active" : undefined}
            aria-current={mode === "buy-vs-rent" ? "page" : undefined}
          >
            Buy vs Rent
          </Link>
        </nav>
        <div className="workspace-header__actions">
          {action}
          <Link
            to={addHomesHref}
            className="workspace-header__add"
            onClick={() => requestDiscoveryReturn(addHomesHref)}
          >
            Add homes
          </Link>
        </div>
      </div>
      {context && <div className="workspace-header__context">{context}</div>}
    </header>
  );
}
