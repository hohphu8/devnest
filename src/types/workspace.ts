import type { Project } from "@/types/project";
import type { ProjectScheduledTask } from "@/types/project-scheduled-task";
import type { ProjectWorker } from "@/types/project-worker";
import type { ServiceName, ServiceState } from "@/types/service";

export interface BootState {
  appName: string;
  environment: "tauri" | "browser";
  dbPath: string;
  startedAt: string;
}

export interface WorkspacePortStatus {
  port: number;
  available: boolean;
  pid?: number | null;
  processName?: string | null;
  managedOwner?: ServiceName | null;
  expectedServices: ServiceName[];
}

export interface WorkspaceOverviewPayload {
  bootState: BootState;
  projects: Project[];
  services: ServiceState[];
  workers: ProjectWorker[];
  scheduledTasks: ProjectScheduledTask[];
  portSummary: WorkspacePortStatus[];
}
