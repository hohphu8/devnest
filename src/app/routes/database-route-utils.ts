import { formatUpdatedAt } from "@/lib/utils";
import type {
  DatabaseSnapshotSummary,
  DatabaseTimeMachineStatus,
} from "@/types/database";

export function formatFileSize(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  if (value < 1024 * 1024 * 1024)
    return `${(value / (1024 * 1024)).toFixed(1)} MB`;
  return `${(value / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

export function databaseSnapshotTriggerLabel(
  triggerSource: DatabaseSnapshotSummary["triggerSource"],
): string {
  if (triggerSource === "pre-action") return "Pre-action";
  if (triggerSource === "scheduled") return "Scheduled";
  return "Manual";
}

export function databaseSnapshotBackendLabel(
  storageBackend: DatabaseSnapshotSummary["storageBackend"] | undefined,
): string {
  return storageBackend === "restic" ? "Restic dedup" : "SQL";
}

export function formatDatabaseScheduleLabel(
  status: DatabaseTimeMachineStatus,
): string {
  if (!status.enabled || !status.scheduleEnabled)
    return "Scheduled snapshots are off.";
  const nextRun = status.nextScheduledSnapshotAt
    ? formatUpdatedAt(status.nextScheduledSnapshotAt)
    : "waiting for the next interval";
  return `Every ${status.scheduleIntervalMinutes} minutes, next ${nextRun}.`;
}

export function getDatabaseTimeMachinePresentation(
  status: DatabaseTimeMachineStatus | undefined,
  busy: boolean,
  loading = false,
) {
  if (busy) {
    return {
      label: "Busy",
      tone: "warning" as const,
      message: "DevNest is capturing or restoring a managed snapshot.",
    };
  }
  if (loading && !status) {
    return {
      label: "Loading",
      tone: "warning" as const,
      message: "Checking managed snapshot protection.",
    };
  }
  if (!status || status.status === "off") {
    return {
      label: "Off",
      tone: "warning" as const,
      message: "Take the first snapshot to enable rolling protection.",
    };
  }
  if (status.status === "error") {
    return {
      label: "Error",
      tone: "error" as const,
      message: status.lastError ?? "Stored snapshot metadata needs attention.",
    };
  }
  return {
    label: "Protected",
    tone: "success" as const,
    message:
      status.snapshotCount > 0
        ? `${status.snapshotCount} managed snapshot${status.snapshotCount === 1 ? "" : "s"} retained.`
        : "Time Machine is enabled and ready to capture the first snapshot.",
  };
}
