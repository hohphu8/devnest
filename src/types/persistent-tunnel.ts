export type PersistentTunnelProvider = "cloudflared";
export type PersistentTunnelStatus = "stopped" | "starting" | "running" | "error";

export interface PersistentTunnelSetupStatus {
  provider: PersistentTunnelProvider;
  ready: boolean;
  managed: boolean;
  binaryPath?: string | null;
  authCertPath?: string | null;
  credentialsPath?: string | null;
  tunnelId?: string | null;
  tunnelName?: string | null;
  defaultHostnameZone?: string | null;
  details: string;
  guidance?: string | null;
}

export interface PersistentTunnelNamedTunnelSummary {
  tunnelId: string;
  tunnelName: string;
  credentialsPath?: string | null;
  selected: boolean;
}

export interface CreatePersistentNamedTunnelInput {
  name: string;
}

export interface SelectPersistentNamedTunnelInput {
  tunnelId: string;
}

export interface UpdatePersistentTunnelSetupInput {
  defaultHostnameZone?: string | null;
}

export interface ProjectPersistentHostname {
  id: string;
  projectId: string;
  provider: PersistentTunnelProvider;
  hostname: string;
  createdAt: string;
  updatedAt: string;
}

export interface UpsertProjectPersistentHostnameInput {
  projectId: string;
  hostname: string;
}

export interface ApplyProjectPersistentHostnameInput {
  projectId: string;
  hostname?: string | null;
}

export interface ProjectPersistentTunnelState {
  projectId: string;
  provider: PersistentTunnelProvider;
  status: PersistentTunnelStatus;
  hostname: string;
  localUrl: string;
  publicUrl: string;
  logPath: string;
  binaryPath?: string | null;
  tunnelId?: string | null;
  credentialsPath?: string | null;
  updatedAt: string;
  details?: string | null;
}

export interface ApplyProjectPersistentHostnameResult {
  hostname: ProjectPersistentHostname;
  tunnel: ProjectPersistentTunnelState;
}

export interface DeleteProjectPersistentHostnameResult {
  hostname: string;
}

export interface PersistentTunnelHealthCheck {
  code: string;
  label: string;
  status: PersistentTunnelStatus;
  message: string;
}

export interface PersistentTunnelHealthReport {
  projectId: string;
  hostname?: string | null;
  overallStatus: PersistentTunnelStatus;
  checks: PersistentTunnelHealthCheck[];
  updatedAt: string;
}
