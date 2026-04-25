export type FrankenphpOctaneWorkerStatus =
  | "running"
  | "stopped"
  | "error"
  | "starting"
  | "restarting";

export interface FrankenphpOctaneWorkerSettings {
  projectId: string;
  workerPort: number;
  adminPort: number;
  workers: number;
  maxRequests: number;
  status: FrankenphpOctaneWorkerStatus;
  pid?: number | null;
  lastStartedAt?: string | null;
  lastStoppedAt?: string | null;
  lastError?: string | null;
  logPath: string;
  createdAt: string;
  updatedAt: string;
}

export interface UpdateFrankenphpOctaneWorkerSettingsInput {
  workerPort?: number;
  adminPort?: number;
  workers?: number;
  maxRequests?: number;
}

export type FrankenphpOctanePreflightLevel = "ok" | "warning" | "error";

export interface FrankenphpOctanePreflightCheck {
  code: string;
  level: FrankenphpOctanePreflightLevel;
  title: string;
  message: string;
  suggestion?: string | null;
  blocking: boolean;
}

export interface FrankenphpOctanePreflight {
  projectId: string;
  ready: boolean;
  summary: string;
  installCommands: string[];
  checks: FrankenphpOctanePreflightCheck[];
  generatedAt: string;
}
