export type TunnelProvider = "cloudflared";
export type TunnelStatus = "stopped" | "starting" | "running" | "error";

export interface ProjectTunnelState {
  projectId: string;
  provider: TunnelProvider;
  status: TunnelStatus;
  localUrl: string;
  publicUrl?: string | null;
  publicHostAliasSynced?: boolean;
  logPath: string;
  binaryPath?: string | null;
  updatedAt: string;
  details?: string | null;
}
