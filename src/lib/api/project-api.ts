import { tauriInvoke } from "@/lib/tauri";
import type { CreateProjectInput, Project, ScanResult, UpdateProjectPatch } from "@/types/project";

export const projectApi = {
  list: () => tauriInvoke<Project[]>("list_projects"),
  get: (projectId: string) => tauriInvoke<Project>("get_project", { projectId }),
  openFolder: (projectId: string) =>
    tauriInvoke<{ success: true }>("open_project_folder", { projectId }),
  openTerminal: (projectId: string) =>
    tauriInvoke<{ success: true }>("open_project_terminal", { projectId }),
  openVsCode: (projectId: string) =>
    tauriInvoke<{ success: true }>("open_project_vscode", { projectId }),
  pickFolder: () => tauriInvoke<string | null>("pick_project_folder"),
  scan: (path: string) => tauriInvoke<ScanResult>("scan_project", { path }),
  create: (input: CreateProjectInput) => tauriInvoke<Project>("create_project", { input }),
  update: (projectId: string, patch: UpdateProjectPatch) =>
    tauriInvoke<Project>("update_project", { projectId, patch }),
  remove: (projectId: string) => tauriInvoke<{ success: true }>("delete_project", { projectId }),
};
