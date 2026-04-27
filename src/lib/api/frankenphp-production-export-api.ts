import { tauriInvoke } from "@/lib/tauri";
import type {
  FrankenphpProductionExportPreview,
  FrankenphpProductionExportWriteResult,
} from "@/types/frankenphp-production-export";

export const frankenphpProductionExportApi = {
  preview: (projectId: string) =>
    tauriInvoke<FrankenphpProductionExportPreview>("preview_frankenphp_production_export", {
      projectId,
    }),
  write: (projectId: string) =>
    tauriInvoke<FrankenphpProductionExportWriteResult | null>(
      "write_frankenphp_production_export",
      { projectId },
    ),
};
