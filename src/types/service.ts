export type ServiceName = "apache" | "nginx" | "frankenphp" | "mysql" | "mailpit" | "redis";
export type ServiceStatus = "running" | "stopped" | "error";
export type ServiceLogSeverity = "error" | "warning" | "info";

export interface ServiceState {
  name: ServiceName;
  enabled: boolean;
  autoStart: boolean;
  port?: number | null;
  pid?: number | null;
  status: ServiceStatus;
  lastError?: string | null;
  updatedAt: string;
}

export interface PortCheckResult {
  port: number;
  available: boolean;
  pid?: number | null;
  processName?: string | null;
}

export interface ServiceLogLine {
  id: string;
  text: string;
  severity: ServiceLogSeverity;
  lineNumber?: number | null;
}

export interface ServiceLogPayload {
  name: string;
  totalLines: number;
  truncated: boolean;
  lines: ServiceLogLine[];
  content?: string;
}
