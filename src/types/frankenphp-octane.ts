export type FrankenphpOctaneWorkerStatus =
  | "running"
  | "stopped"
  | "error"
  | "starting"
  | "restarting";

export interface FrankenphpOctaneWorkerSettings {
  projectId: string;
  mode: "octane" | "symfony" | "custom";
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
  customWorkerRelativePath?: string | null;
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
  mode: "octane" | "symfony" | "custom";
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
  mode?: "octane" | "symfony" | "custom";
  workerPort?: number;
  adminPort?: number;
  workers?: number;
  maxRequests?: number;
  customWorkerRelativePath?: string | null;
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
  mode: "octane" | "symfony" | "custom";
  ready: boolean;
  summary: string;
  installCommands: string[];
  checks: FrankenphpOctanePreflightCheck[];
  generatedAt: string;
}

export type FrankenphpWorkerSettings = FrankenphpOctaneWorkerSettings;
export type FrankenphpWorkerHealth = FrankenphpOctaneWorkerHealth;
export type FrankenphpWorkerPreflight = FrankenphpOctanePreflight;
export type UpdateFrankenphpWorkerSettingsInput = UpdateFrankenphpOctaneWorkerSettingsInput;
