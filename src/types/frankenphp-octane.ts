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

export interface FrankenphpRuntimeExtensionHealth {
  extensionName: string;
  available: boolean;
  enabled: boolean;
}

export interface FrankenphpRuntimeHealth {
  runtimeId: string;
  version: string;
  phpFamily?: string | null;
  path: string;
  managedPhpConfigPath?: string | null;
  extensions: FrankenphpRuntimeExtensionHealth[];
}

export interface FrankenphpOctaneWorkerHealth {
  projectId: string;
  status: FrankenphpOctaneWorkerStatus;
  pid?: number | null;
  uptimeSeconds?: number | null;
  workerPort: number;
  adminPort: number;
  lastStartedAt?: string | null;
  lastRestartedAt?: string | null;
  lastError?: string | null;
  requestCount?: number | null;
  metricsAvailable: boolean;
  logTail: string;
  restartRecommended: boolean;
  restartReason?: string | null;
  runtime?: FrankenphpRuntimeHealth | null;
  generatedAt: string;
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
