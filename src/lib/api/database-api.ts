import { tauriInvoke } from "@/lib/tauri";
import type {
  DatabaseActionResult,
  DatabaseSnapshotResult,
  DatabaseSnapshotRollbackResult,
  DatabaseSnapshotSummary,
  DatabaseTimeMachineStatus,
  DatabaseTransferResult,
} from "@/types/database";

export const databaseApi = {
  list: () => tauriInvoke<string[]>("list_databases"),
  create: (name: string) => tauriInvoke<DatabaseActionResult>("create_database", { name }),
  drop: (name: string) => tauriInvoke<DatabaseActionResult>("drop_database", { name }),
  backup: (name: string) =>
    tauriInvoke<DatabaseTransferResult | null>("backup_database", { name }),
  restore: (name: string) =>
    tauriInvoke<DatabaseTransferResult | null>("restore_database", { name }),
  getTimeMachineStatus: (name: string) =>
    tauriInvoke<DatabaseTimeMachineStatus>("get_database_time_machine_status", { name }),
  enableTimeMachine: (name: string) =>
    tauriInvoke<DatabaseTimeMachineStatus>("enable_database_time_machine", { name }),
  disableTimeMachine: (name: string) =>
    tauriInvoke<DatabaseTimeMachineStatus>("disable_database_time_machine", { name }),
  takeSnapshot: (name: string) =>
    tauriInvoke<DatabaseSnapshotResult>("take_database_snapshot", { name }),
  listSnapshots: (name: string) =>
    tauriInvoke<DatabaseSnapshotSummary[]>("list_database_snapshots", { name }),
  rollbackSnapshot: (name: string, snapshotId: string) =>
    tauriInvoke<DatabaseSnapshotRollbackResult>("rollback_database_snapshot", {
      name,
      snapshotId,
    }),
};
