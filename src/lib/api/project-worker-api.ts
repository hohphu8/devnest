import { tauriInvoke } from "@/lib/tauri";
import type {
  CreateProjectWorkerInput,
  DeleteProjectWorkerResult,
  ProjectWorker,
  ProjectWorkerLogPayload,
  UpdateProjectWorkerPatch,
} from "@/types/project-worker";

export const projectWorkerApi = {
  listByProject: (projectId: string) =>
    tauriInvoke<ProjectWorker[]>("list_project_workers", { projectId }),
  listAll: () => tauriInvoke<ProjectWorker[]>("list_all_workers"),
  create: (input: CreateProjectWorkerInput) =>
    tauriInvoke<ProjectWorker>("create_project_worker", { input }),
  update: (workerId: string, patch: UpdateProjectWorkerPatch) =>
    tauriInvoke<ProjectWorker>("update_project_worker", { workerId, patch }),
  remove: (workerId: string) =>
    tauriInvoke<DeleteProjectWorkerResult>("delete_project_worker", { workerId }),
  getStatus: (workerId: string) =>
    tauriInvoke<ProjectWorker>("get_project_worker_status", { workerId }),
  start: (workerId: string) =>
    tauriInvoke<ProjectWorker>("start_project_worker", { workerId }),
  stop: (workerId: string) =>
    tauriInvoke<ProjectWorker>("stop_project_worker", { workerId }),
  restart: (workerId: string) =>
    tauriInvoke<ProjectWorker>("restart_project_worker", { workerId }),
  readLogs: (workerId: string, lines = 200) =>
    tauriInvoke<ProjectWorkerLogPayload>("read_project_worker_logs", { workerId, lines }),
  clearLogs: (workerId: string) =>
    tauriInvoke<boolean>("clear_project_worker_logs", { workerId }),
};
