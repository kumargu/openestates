import type { ReactNode } from "react";

export function WorkspaceHeader({ context }: { context?: ReactNode }) {
  if (!context) return null;
  return (
    <header className="workspace-header workspace-header--context">
      <div className="workspace-header__context">{context}</div>
    </header>
  );
}
