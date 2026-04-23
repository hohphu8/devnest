export interface DatabaseActionResult {
  success: true;
  name: string;
}

export interface DatabaseTransferResult {
  success: true;
  name: string;
  path: string;
}

export type DatabaseTimeMachineState = "off" | "protected" | "busy" | "error";
export type DatabaseSnapshotTriggerSource = "manual" | "pre-action" | "scheduled";

export interface DatabaseTimeMachineStatus {
  name: string;
  enabled: boolean;
  status: DatabaseTimeMachineState;
  snapshotCount: number;
  scheduleEnabled: boolean;
  scheduleIntervalMinutes: number;
  linkedProjectActionSnapshotsEnabled: boolean;
  latestSnapshotAt?: string | null;
  nextScheduledSnapshotAt?: string | null;
  lastError?: string | null;
}

export interface DatabaseSnapshotSummary {
  id: string;
  databaseName: string;
  createdAt: string;
  triggerSource: DatabaseSnapshotTriggerSource;
  sizeBytes: number;
  linkedProjectNames: string[];
  scheduledIntervalMinutes?: number | null;
  note?: string | null;
}

export interface DatabaseSnapshotResult {
  success: true;
  name: string;
  snapshot: DatabaseSnapshotSummary;
  status: DatabaseTimeMachineStatus;
}

export interface DatabaseSnapshotRollbackResult {
  success: true;
  name: string;
  snapshotId: string;
  restoredAt: string;
  restoredSnapshot: DatabaseSnapshotSummary;
  safetySnapshotId?: string | null;
}
