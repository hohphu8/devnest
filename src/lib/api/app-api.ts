import { tauriInvoke } from "@/lib/tauri";
import type {
  AppReleaseInfo,
  AppUpdateCheckResult,
  AppUpdateInstallResult,
} from "@/types/update";

export const appApi = {
  getReleaseInfo: () => tauriInvoke<AppReleaseInfo>("get_app_release_info"),
  checkForUpdate: () => tauriInvoke<AppUpdateCheckResult>("check_for_app_update"),
  installUpdate: () => tauriInvoke<AppUpdateInstallResult>("install_app_update"),
};
