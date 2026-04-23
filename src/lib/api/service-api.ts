import { tauriInvoke } from "@/lib/tauri";
import type { PortCheckResult, ServiceLogPayload, ServiceName, ServiceState } from "@/types/service";

export const serviceApi = {
  list: () => tauriInvoke<ServiceState[]>("get_all_service_status"),
  get: (name: ServiceName) => tauriInvoke<ServiceState>("get_service_status", { name }),
  start: (name: ServiceName) => tauriInvoke<ServiceState>("start_service", { name }),
  stop: (name: ServiceName) => tauriInvoke<ServiceState>("stop_service", { name }),
  restart: (name: ServiceName) => tauriInvoke<ServiceState>("restart_service", { name }),
  openDashboard: (name: ServiceName) =>
    tauriInvoke<boolean>("open_service_dashboard", { name }),
  readLogs: (name: ServiceName, lines = 200) =>
    tauriInvoke<ServiceLogPayload>("read_service_logs", { name, lines }),
  clearLogs: (name: ServiceName) => tauriInvoke<boolean>("clear_service_logs", { name }),
  checkPort: (port: number) => tauriInvoke<PortCheckResult>("check_port", { port }),
};
