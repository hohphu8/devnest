import { tauriInvoke } from "@/lib/tauri";
import type { Project } from "@/types/project";
import type { ProjectProfileTransferResult } from "@/types/project-profile";

export const projectProfileApi = {
  exportProject: (projectId: string) =>
    tauriInvoke<ProjectProfileTransferResult | null>("export_project_profile", { projectId }),
  exportTeamProject: (projectId: string) =>
    tauriInvoke<ProjectProfileTransferResult | null>("export_team_project_profile", { projectId }),
  importProject: () => tauriInvoke<Project | null>("import_project_profile"),
  importTeamProject: () => tauriInvoke<Project | null>("import_team_project_profile"),
};
