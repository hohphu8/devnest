export type OptionalToolType = "mailpit" | "cloudflared" | "phpmyadmin";
export type OptionalToolHealthStatus = "available" | "missing";
export type OptionalToolArchiveKind = "zip" | "binary";
export type OptionalToolInstallStage =
  | "queued"
  | "downloading"
  | "verifying"
  | "extracting"
  | "registering"
  | "completed"
  | "failed";

export interface OptionalToolInventoryItem {
  id: string;
  toolType: OptionalToolType;
  version: string;
  path: string;
  isActive: boolean;
  status: OptionalToolHealthStatus;
  createdAt: string;
  updatedAt: string;
  details?: string | null;
}

export interface OptionalToolPackage {
  id: string;
  toolType: OptionalToolType;
  version: string;
  platform: string;
  arch: string;
  displayName: string;
  downloadUrl: string;
  checksumSha256?: string | null;
  archiveKind: OptionalToolArchiveKind;
  entryBinary: string;
  notes?: string | null;
}

export interface OptionalToolInstallTask {
  packageId: string;
  displayName: string;
  toolType: OptionalToolType;
  version: string;
  stage: OptionalToolInstallStage;
  message: string;
  updatedAt: string;
  errorCode?: string | null;
}
