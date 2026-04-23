import type { ServiceLogPayload } from "@/types/service";

export type ProjectWorkerPresetType = "queue" | "schedule" | "custom";
export type ProjectWorkerStatus =
  | "running"
  | "stopped"
  | "error"
  | "starting"
  | "restarting";

export interface ProjectWorker {
  id: string;
  projectId: string;
  name: string;
  presetType: ProjectWorkerPresetType;
  command: string;
  args: string[];
  workingDirectory: string;
  autoStart: boolean;
  status: ProjectWorkerStatus;
  pid?: number | null;
  lastStartedAt?: string | null;
  lastStoppedAt?: string | null;
  lastExitCode?: number | null;
  lastError?: string | null;
  logPath: string;
  createdAt: string;
  updatedAt: string;
}

export interface CreateProjectWorkerInput {
  projectId: string;
  name: string;
  presetType: ProjectWorkerPresetType;
  commandLine: string;
  workingDirectory?: string | null;
  autoStart: boolean;
}

export interface UpdateProjectWorkerPatch {
  name?: string;
  presetType?: ProjectWorkerPresetType;
  commandLine?: string;
  workingDirectory?: string | null;
  autoStart?: boolean;
}

export interface DeleteProjectWorkerResult {
  success: true;
}

export type ProjectWorkerLogPayload = ServiceLogPayload;
