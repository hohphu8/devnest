import { tauriInvoke } from "@/lib/tauri";
import type {
  CreateProjectScheduledTaskInput,
  DeleteProjectScheduledTaskResult,
  ProjectScheduledTask,
  ProjectScheduledTaskRun,
  ProjectScheduledTaskRunLogPayload,
  UpdateProjectScheduledTaskPatch,
} from "@/types/project-scheduled-task";

export const projectScheduledTaskApi = {
  listByProject: (projectId: string) =>
    tauriInvoke<ProjectScheduledTask[]>("list_project_scheduled_tasks", { projectId }),
  listAll: () => tauriInvoke<ProjectScheduledTask[]>("list_all_scheduled_tasks"),
  create: (input: CreateProjectScheduledTaskInput) =>
    tauriInvoke<ProjectScheduledTask>("create_project_scheduled_task", { input }),
  update: (taskId: string, patch: UpdateProjectScheduledTaskPatch) =>
    tauriInvoke<ProjectScheduledTask>("update_project_scheduled_task", { taskId, patch }),
  remove: (taskId: string) =>
    tauriInvoke<DeleteProjectScheduledTaskResult>("delete_project_scheduled_task", { taskId }),
  getStatus: (taskId: string) =>
    tauriInvoke<ProjectScheduledTask>("get_project_scheduled_task_status", { taskId }),
  enable: (taskId: string) =>
    tauriInvoke<ProjectScheduledTask>("enable_project_scheduled_task", { taskId }),
  disable: (taskId: string) =>
    tauriInvoke<ProjectScheduledTask>("disable_project_scheduled_task", { taskId }),
  runNow: (taskId: string) =>
    tauriInvoke<ProjectScheduledTask>("run_project_scheduled_task_now", { taskId }),
  listRuns: (taskId: string, limit = 25) =>
    tauriInvoke<ProjectScheduledTaskRun[]>("list_project_scheduled_task_runs", { taskId, limit }),
  readRunLogs: (runId: string, lines = 200) =>
    tauriInvoke<ProjectScheduledTaskRunLogPayload>("read_project_scheduled_task_run_logs", {
      runId,
      lines,
    }),
  clearLogs: (taskId: string) =>
    tauriInvoke<boolean>("clear_project_scheduled_task_logs", { taskId }),
  clearHistory: (taskId: string) =>
    tauriInvoke<ProjectScheduledTask>("clear_project_scheduled_task_history", { taskId }),
};
