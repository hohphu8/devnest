import { tauriInvoke } from "@/lib/tauri";
import type { ProjectMobilePreviewState } from "@/types/mobile-preview";

export const mobilePreviewApi = {
  getState: (projectId: string) =>
    tauriInvoke<ProjectMobilePreviewState | null>("get_project_mobile_preview_state", { projectId }),
  start: (projectId: string) =>
    tauriInvoke<ProjectMobilePreviewState>("start_project_mobile_preview", { projectId }),
  stop: (projectId: string) =>
    tauriInvoke<ProjectMobilePreviewState>("stop_project_mobile_preview", { projectId }),
};
