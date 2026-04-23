import type { ServiceLogPayload } from "@/types/service";

export type ProjectScheduledTaskType = "command" | "url";
export type ProjectScheduledTaskScheduleMode = "simple" | "cron";
export type ProjectScheduledTaskSimpleScheduleKind =
  | "everySeconds"
  | "everyMinutes"
  | "everyHours"
  | "daily"
  | "weekly";
export type ProjectScheduledTaskOverlapPolicy = "skip_if_running";
export type ProjectScheduledTaskStatus =
  | "idle"
  | "scheduled"
  | "running"
  | "success"
  | "error"
  | "skipped";
export type ProjectScheduledTaskRunStatus = "running" | "success" | "error" | "skipped";

export interface ProjectScheduledTask {
  id: string;
  projectId: string;
  name: string;
  taskType: ProjectScheduledTaskType;
  scheduleMode: ProjectScheduledTaskScheduleMode;
  simpleScheduleKind?: ProjectScheduledTaskSimpleScheduleKind | null;
  scheduleExpression: string;
  intervalSeconds?: number | null;
  dailyTime?: string | null;
  weeklyDay?: number | null;
  url?: string | null;
  command?: string | null;
  args: string[];
  workingDirectory?: string | null;
  enabled: boolean;
  autoResume: boolean;
  overlapPolicy: ProjectScheduledTaskOverlapPolicy;
  status: ProjectScheduledTaskStatus;
  nextRunAt?: string | null;
  lastRunAt?: string | null;
  lastSuccessAt?: string | null;
  lastError?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface ProjectScheduledTaskRun {
  id: string;
  taskId: string;
  projectId: string;
  startedAt: string;
  finishedAt?: string | null;
  durationMs?: number | null;
  status: ProjectScheduledTaskRunStatus;
  exitCode?: number | null;
  responseStatus?: number | null;
  errorMessage?: string | null;
  logPath: string;
  createdAt: string;
}

export interface CreateProjectScheduledTaskInput {
  projectId: string;
  name: string;
  taskType: ProjectScheduledTaskType;
  scheduleMode: ProjectScheduledTaskScheduleMode;
  simpleScheduleKind?: ProjectScheduledTaskSimpleScheduleKind | null;
  scheduleExpression?: string | null;
  intervalSeconds?: number | null;
  dailyTime?: string | null;
  weeklyDay?: number | null;
  url?: string | null;
  commandLine?: string | null;
  workingDirectory?: string | null;
  enabled: boolean;
  autoResume: boolean;
}

export interface UpdateProjectScheduledTaskPatch {
  name?: string;
  taskType?: ProjectScheduledTaskType;
  scheduleMode?: ProjectScheduledTaskScheduleMode;
  simpleScheduleKind?: ProjectScheduledTaskSimpleScheduleKind | null;
  scheduleExpression?: string | null;
  intervalSeconds?: number | null;
  dailyTime?: string | null;
  weeklyDay?: number | null;
  url?: string | null;
  commandLine?: string | null;
  workingDirectory?: string | null;
  enabled?: boolean;
  autoResume?: boolean;
}

export interface DeleteProjectScheduledTaskResult {
  success: true;
}

export type ProjectScheduledTaskRunLogPayload = ServiceLogPayload;
