import { tauriInvoke } from "@/lib/tauri";
import type {
  ClearDownloadCacheResult,
  DownloadCacheSummary,
} from "@/types/download-cache";

export const downloadCacheApi = {
  summary: () => tauriInvoke<DownloadCacheSummary>("get_download_cache_summary"),
  clear: () => tauriInvoke<ClearDownloadCacheResult>("clear_download_cache"),
};
