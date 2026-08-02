import { createContext, useContext } from "react";

export type WorkspaceRouteProperty = {
  id: string;
  label: string;
};

export const WorkspaceRoutePropertyContext = createContext<
  (property: WorkspaceRouteProperty) => void
>(() => {});

export function useRegisterWorkspaceRouteProperty() {
  return useContext(WorkspaceRoutePropertyContext);
}
