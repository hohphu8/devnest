import { tauriInvoke } from "@/lib/tauri";
import type {
  CreateProjectEnvVarInput,
  ProjectEnvInspection,
  ProjectEnvVar,
  UpdateProjectEnvVarInput,
} from "@/types/project-env-var";

export const projectEnvApi = {
  list: (projectId: string) =>
    tauriInvoke<ProjectEnvVar[]>("list_project_env_vars", { projectId }),
  inspect: (projectId: string) =>
    tauriInvoke<ProjectEnvInspection>("inspect_project_env", { projectId }),
  create: (input: CreateProjectEnvVarInput) =>
    tauriInvoke<ProjectEnvVar>("create_project_env_var", { input }),
  update: (input: UpdateProjectEnvVarInput) =>
    tauriInvoke<ProjectEnvVar>("update_project_env_var", { input }),
  remove: (projectId: string, envVarId: string) =>
    tauriInvoke<{ success: true }>("delete_project_env_var", { projectId, envVarId }),
};
