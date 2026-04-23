import { tauriInvoke } from "@/lib/tauri";
import type { WorkspaceOverviewPayload } from "@/types/workspace";

export const workspaceApi = {
  overview: () => tauriInvoke<WorkspaceOverviewPayload>("get_workspace_overview"),
};
