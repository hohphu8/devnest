export interface DownloadCacheBucket {
  id: string;
  displayName: string;
  path: string;
  sizeBytes: number;
  fileCount: number;
}

export interface DownloadCacheSummary {
  totalSizeBytes: number;
  fileCount: number;
  buckets: DownloadCacheBucket[];
}

export interface ClearDownloadCacheResult {
  deletedBytes: number;
  deletedFiles: number;
  summary: DownloadCacheSummary;
}
