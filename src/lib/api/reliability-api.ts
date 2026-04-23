import { tauriInvoke } from "@/lib/tauri";
import type {
  ActionPreflightReport,
  ReliabilityAction,
  ReliabilityInspectorSnapshot,
  ReliabilityTransferResult,
  RepairExecutionResult,
  RepairWorkflow,
  RepairWorkflowInfo,
} from "@/types/reliability";

export const reliabilityApi = {
  listRepairWorkflows: () =>
    tauriInvoke<RepairWorkflowInfo[]>("list_repair_workflows"),
  runPreflight: (action: ReliabilityAction, projectId?: string) =>
    tauriInvoke<ActionPreflightReport>("run_action_preflight", {
      action,
      projectId,
    }),
  inspectState: (projectId: string) =>
    tauriInvoke<ReliabilityInspectorSnapshot>("inspect_reliability_state", {
      projectId,
    }),
  exportDiagnosticsBundle: (projectId: string) =>
    tauriInvoke<ReliabilityTransferResult | null>("export_diagnostics_bundle", {
      projectId,
    }),
  backupAppMetadata: () =>
    tauriInvoke<ReliabilityTransferResult | null>("backup_app_metadata"),
  restoreAppMetadata: () =>
    tauriInvoke<ReliabilityTransferResult | null>("restore_app_metadata"),
  runRepairWorkflow: (projectId: string, workflow: RepairWorkflow) =>
    tauriInvoke<RepairExecutionResult>("run_repair_workflow", {
      projectId,
      workflow,
    }),
};
