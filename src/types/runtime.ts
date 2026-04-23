export type RuntimeType = "php" | "apache" | "nginx" | "mysql";
export type RuntimeSource = "bundled" | "downloaded" | "imported" | "external";
export type RuntimeHealthStatus = "available" | "missing";
export type RuntimeArchiveKind = "zip";
export type RuntimeInstallStage =
  | "queued"
  | "downloading"
  | "verifying"
  | "extracting"
  | "registering"
  | "completed"
  | "failed";

export interface RuntimeInventoryItem {
  id: string;
  runtimeType: RuntimeType;
  version: string;
  path: string;
  isActive: boolean;
  source: RuntimeSource;
  status: RuntimeHealthStatus;
  createdAt: string;
  updatedAt: string;
  details?: string | null;
}

export interface RuntimePackage {
  id: string;
  runtimeType: RuntimeType;
  version: string;
  platform: string;
  arch: string;
  displayName: string;
  downloadUrl: string;
  checksumSha256: string;
  archiveKind: RuntimeArchiveKind;
  entryBinary: string;
  notes?: string | null;
}

export interface RuntimeInstallTask {
  packageId: string;
  displayName: string;
  runtimeType: RuntimeType;
  version: string;
  stage: RuntimeInstallStage;
  message: string;
  updatedAt: string;
  errorCode?: string | null;
}

export interface PhpExtensionState {
  runtimeId: string;
  runtimeVersion: string;
  extensionName: string;
  dllFile: string;
  enabled: boolean;
  updatedAt: string;
}

export interface PhpExtensionInstallResult {
  runtimeId: string;
  runtimeVersion: string;
  installedExtensions: string[];
  sourcePath: string;
}

export type PhpExtensionPackageKind = "zip" | "binary";

export interface PhpExtensionPackage {
  id: string;
  extensionName: string;
  phpFamily: string;
  version: string;
  platform: string;
  arch: string;
  displayName: string;
  downloadUrl: string;
  checksumSha256?: string | null;
  packageKind: PhpExtensionPackageKind;
  dllFile: string;
  notes?: string | null;
}

export interface PhpFunctionState {
  runtimeId: string;
  runtimeVersion: string;
  functionName: string;
  enabled: boolean;
  updatedAt: string;
}
