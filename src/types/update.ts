export type AppUpdateState =
  | "idle"
  | "checking"
  | "noUpdate"
  | "updateAvailable"
  | "downloading"
  | "installing"
  | "restartRequired"
  | "failed";

export interface AppReleaseInfo {
  appName: string;
  currentVersion: string;
  releaseChannel: string;
  updateEndpoint?: string | null;
  updaterConfigured: boolean;
}

export interface AppUpdateCheckResult {
  status: "upToDate" | "updateAvailable";
  currentVersion: string;
  latestVersion?: string | null;
  releaseChannel: string;
  checkedAt: string;
  notes?: string | null;
  pubDate?: string | null;
  updateEndpoint?: string | null;
}

export interface AppUpdateInstallResult {
  status: "restartRequired";
  targetVersion: string;
}
