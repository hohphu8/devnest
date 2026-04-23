import { tauriInvoke } from "@/lib/tauri";
import type { DiagnosticFixResult, DiagnosticItem } from "@/types/diagnostics";

export const diagnosticsApi = {
  run: (projectId: string) => tauriInvoke<DiagnosticItem[]>("run_diagnostics", { projectId }),
  fix: (projectId: string, code: string) =>
    tauriInvoke<DiagnosticFixResult>("apply_diagnostic_fix", { projectId, code }),
};
