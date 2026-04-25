import { tauriInvoke } from "@/lib/tauri";
import type {
  FrankenphpOctanePreflight,
  FrankenphpOctaneWorkerSettings,
  UpdateFrankenphpOctaneWorkerSettingsInput,
} from "@/types/frankenphp-octane";
import type { ProjectWorkerLogPayload } from "@/types/project-worker";

export const frankenphpOctaneApi = {
  getSettings: (projectId: string) =>
    tauriInvoke<FrankenphpOctaneWorkerSettings>("get_project_frankenphp_worker_settings", {
      projectId,
    }),
  updateSettings: (projectId: string, input: UpdateFrankenphpOctaneWorkerSettingsInput) =>
    tauriInvoke<FrankenphpOctaneWorkerSettings>("update_project_frankenphp_worker_settings", {
      projectId,
      input,
    }),
  preflight: (projectId: string) =>
    tauriInvoke<FrankenphpOctanePreflight>("get_project_frankenphp_octane_preflight", {
      projectId,
    }),
  status: (projectId: string) =>
    tauriInvoke<FrankenphpOctaneWorkerSettings>("get_project_frankenphp_worker_status", {
      projectId,
    }),
  start: (projectId: string) =>
    tauriInvoke<FrankenphpOctaneWorkerSettings>("start_project_frankenphp_worker", {
      projectId,
    }),
  stop: (projectId: string) =>
    tauriInvoke<FrankenphpOctaneWorkerSettings>("stop_project_frankenphp_worker", {
      projectId,
    }),
  restart: (projectId: string) =>
    tauriInvoke<FrankenphpOctaneWorkerSettings>("restart_project_frankenphp_worker", {
      projectId,
    }),
  reload: (projectId: string) =>
    tauriInvoke<FrankenphpOctaneWorkerSettings>("reload_project_frankenphp_worker", {
      projectId,
    }),
  readLogs: (projectId: string, lines = 120) =>
    tauriInvoke<ProjectWorkerLogPayload>("read_project_frankenphp_worker_logs", {
      projectId,
      lines,
    }),
};
