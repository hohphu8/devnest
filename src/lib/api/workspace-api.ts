import { tauriInvoke } from "@/lib/tauri";
import type { ServiceState } from "@/types/service";
import type { WorkspaceOverviewPayload, WorkspacePortStatus } from "@/types/workspace";

export const workspaceApi = {
  overview: () => tauriInvoke<WorkspaceOverviewPayload>("get_workspace_overview"),
  portSummary: (serviceSnapshot: ServiceState[]) =>
    tauriInvoke<WorkspacePortStatus[]>("get_workspace_port_summary", { serviceSnapshot }),
};
