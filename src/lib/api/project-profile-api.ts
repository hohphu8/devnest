import { tauriInvoke } from "@/lib/tauri";
import type { ImportedProjectProfile, ProjectProfileTransferResult } from "@/types/project-profile";

export const projectProfileApi = {
  exportProject: (projectId: string) =>
    tauriInvoke<ProjectProfileTransferResult | null>("export_project_profile", { projectId }),
  exportTeamProject: (projectId: string) =>
    tauriInvoke<ProjectProfileTransferResult | null>("export_team_project_profile", { projectId }),
  importProject: () => tauriInvoke<ImportedProjectProfile | null>("import_project_profile"),
  importTeamProject: () =>
    tauriInvoke<ImportedProjectProfile | null>("import_team_project_profile"),
};
