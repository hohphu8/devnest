import { tauriInvoke } from "@/lib/tauri";
import type { ProjectTunnelState } from "@/types/tunnel";

export const tunnelApi = {
  getState: (projectId: string) =>
    tauriInvoke<ProjectTunnelState | null>("get_project_tunnel_state", { projectId }),
  start: (projectId: string) =>
    tauriInvoke<ProjectTunnelState>("start_project_tunnel", { projectId }),
  stop: (projectId: string) =>
    tauriInvoke<ProjectTunnelState>("stop_project_tunnel", { projectId }),
  open: (projectId: string) => tauriInvoke<boolean>("open_project_tunnel_url", { projectId }),
};
