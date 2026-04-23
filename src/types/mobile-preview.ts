export type MobilePreviewStatus = "stopped" | "starting" | "running" | "error";

export interface ProjectMobilePreviewState {
  projectId: string;
  status: MobilePreviewStatus;
  localProjectUrl: string;
  lanIp?: string | null;
  port?: number | null;
  proxyUrl?: string | null;
  qrUrl?: string | null;
  updatedAt: string;
  details?: string | null;
}
