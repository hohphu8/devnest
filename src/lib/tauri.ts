import { invoke } from "@tauri-apps/api/core";
import type {
  DatabaseSnapshotResult,
  DatabaseSnapshotRollbackResult,
  DatabaseSnapshotSummary,
  DatabaseTimeMachineStatus,
} from "@/types/database";
import type { DiagnosticItem } from "@/types/diagnostics";
import type {
  OptionalToolInstallTask,
  OptionalToolInventoryItem,
  OptionalToolPackage,
} from "@/types/optional-tool";
import type {
  ApplyProjectPersistentHostnameInput,
  ApplyProjectPersistentHostnameResult,
  CreatePersistentNamedTunnelInput,
  DeleteProjectPersistentHostnameResult,
  PersistentTunnelHealthReport,
  PersistentTunnelNamedTunnelSummary,
  ProjectPersistentTunnelState,
  PersistentTunnelSetupStatus,
  ProjectPersistentHostname,
  SelectPersistentNamedTunnelInput,
  UpdatePersistentTunnelSetupInput,
} from "@/types/persistent-tunnel";
import {
  documentRootSchema,
  domainSchema,
  gitRepositoryUrlSchema,
  projectNameSchema,
  projectPathSchema,
  recipeTargetPathSchema,
} from "@/lib/validators";
import { serializeCommandLine } from "@/lib/utils";
import type { CreateProjectInput, Project, ScanResult, UpdateProjectPatch } from "@/types/project";
import type {
  ProjectDiskEnvVar,
  ProjectEnvComparisonItem,
  ProjectEnvInspection,
  ProjectEnvVar,
} from "@/types/project-env-var";
import type {
  FrankenphpOctanePreflight,
  FrankenphpOctaneWorkerHealth,
  FrankenphpOctaneWorkerSettings,
  UpdateFrankenphpOctaneWorkerSettingsInput,
} from "@/types/frankenphp-octane";
import type {
  FrankenphpProductionExportPreview,
  FrankenphpProductionExportWriteResult,
} from "@/types/frankenphp-production-export";
import type {
  ActionPreflightReport,
  ReliabilityInspectorSnapshot,
  ReliabilityTransferResult,
  RepairExecutionResult,
  RepairWorkflowInfo,
} from "@/types/reliability";
import type { ProjectMobilePreviewState } from "@/types/mobile-preview";
import type {
  CreateProjectScheduledTaskInput,
  DeleteProjectScheduledTaskResult,
  ProjectScheduledTask,
  ProjectScheduledTaskRun,
  ProjectScheduledTaskRunLogPayload,
  UpdateProjectScheduledTaskPatch,
} from "@/types/project-scheduled-task";
import type {
  CreateProjectWorkerInput,
  DeleteProjectWorkerResult,
  ProjectWorker,
  ProjectWorkerLogPayload,
  UpdateProjectWorkerPatch,
} from "@/types/project-worker";
import type {
  PhpExtensionInstallResult,
  PhpExtensionPackage,
  PhpExtensionState,
  PhpFunctionState,
  RuntimeInstallTask,
  RuntimeInventoryItem,
  RuntimePackage,
} from "@/types/runtime";
import type { RuntimeConfigSchema, RuntimeConfigValues } from "@/types/runtime-config";
import type { PortCheckResult, ServiceLogPayload, ServiceState } from "@/types/service";
import type { ProjectTunnelState } from "@/types/tunnel";
import type { BootState, WorkspaceOverviewPayload } from "@/types/workspace";

export interface AppError {
  code: string;
  message: string;
  details?: unknown;
}

export function getAppErrorMessage(error: unknown, fallback: string): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    return String(error.message);
  }

  return fallback;
}

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

const MOCK_PROJECTS_KEY = "devnest.mock.projects";
const MOCK_HOSTS_KEY = "devnest.mock.hosts";
const MOCK_SERVICES_KEY = "devnest.mock.services";
const MOCK_SERVICE_LOGS_KEY = "devnest.mock.service-logs";
const MOCK_DATABASES_KEY = "devnest.mock.databases";
const MOCK_DATABASE_SNAPSHOTS_KEY = "devnest.mock.database-snapshots";
const MOCK_DATABASE_TIME_MACHINE_KEY = "devnest.mock.database-time-machine";
const MOCK_PROJECT_ENV_VARS_KEY = "devnest.mock.project-env-vars";
const MOCK_PROJECT_SCHEDULED_TASKS_KEY = "devnest.mock.project-scheduled-tasks";
const MOCK_PROJECT_SCHEDULED_TASK_RUNS_KEY = "devnest.mock.project-scheduled-task-runs";
const MOCK_PROJECT_WORKERS_KEY = "devnest.mock.project-workers";
const MOCK_FRANKENPHP_OCTANE_WORKERS_KEY = "devnest.mock.frankenphp-octane-workers";
const MOCK_RUNTIMES_KEY = "devnest.mock.runtimes";
const MOCK_RUNTIME_PACKAGES_KEY = "devnest.mock.runtime-packages";
const MOCK_RUNTIME_INSTALL_TASK_KEY = "devnest.mock.runtime-install-task";
const MOCK_OPTIONAL_TOOLS_KEY = "devnest.mock.optional-tools";
const MOCK_OPTIONAL_TOOL_PACKAGES_KEY = "devnest.mock.optional-tool-packages";
const MOCK_OPTIONAL_TOOL_INSTALL_TASK_KEY = "devnest.mock.optional-tool-install-task";
const MOCK_SSL_AUTHORITY_TRUSTED_KEY = "devnest.mock.ssl-authority-trusted";
const MOCK_PHP_EXTENSION_OVERRIDES_KEY = "devnest.mock.php-extension-overrides";
const MOCK_PHP_FUNCTION_OVERRIDES_KEY = "devnest.mock.php-function-overrides";
const MOCK_PHP_AVAILABLE_EXTENSIONS_KEY = "devnest.mock.php-available-extensions";
const MOCK_RUNTIME_CONFIG_OVERRIDES_KEY = "devnest.mock.runtime-config-overrides";
const MOCK_LAST_EXPORTED_PROJECT_PROFILE_KEY = "devnest.mock.last-exported-project-profile";
const MOCK_LAST_EXPORTED_TEAM_PROJECT_PROFILE_KEY =
  "devnest.mock.last-exported-team-project-profile";
const MOCK_PROJECT_MOBILE_PREVIEWS_KEY = "devnest.mock.project-mobile-previews";
const MOCK_PROJECT_TUNNELS_KEY = "devnest.mock.project-tunnels";
const MOCK_PROJECT_PERSISTENT_HOSTNAMES_KEY = "devnest.mock.project-persistent-hostnames";
const MOCK_PROJECT_PERSISTENT_TUNNELS_KEY = "devnest.mock.project-persistent-tunnels";
const MOCK_PERSISTENT_TUNNEL_SETUP_KEY = "devnest.mock.persistent-tunnel-setup";
const MOCK_PERSISTENT_NAMED_TUNNELS_KEY = "devnest.mock.persistent-named-tunnels";
const MOCK_APP_DATA_ROOT = "C:/DevNest/mock-app-data";
const MOCK_SHARED_ROOT = "C:/DevNest/mock-shared";
const BROWSER_PREVIEW_UPDATE_ENDPOINT =
  "https://github.com/hohphu8/devnest/releases/latest/download/stable.json";

function readMockProjects(): Project[] {
  if (typeof window === "undefined") {
    return [];
  }

  const stored = window.localStorage.getItem(MOCK_PROJECTS_KEY);
  return stored
    ? (JSON.parse(stored) as Project[]).map((project) => ({
        ...project,
        frankenphpMode: project.frankenphpMode ?? "classic",
      }))
    : [];
}

function writeMockProjects(projects: Project[]) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_PROJECTS_KEY, JSON.stringify(projects));
}

function readMockHosts(): Record<string, string> {
  if (typeof window === "undefined") {
    return {};
  }

  const stored = window.localStorage.getItem(MOCK_HOSTS_KEY);
  return stored ? (JSON.parse(stored) as Record<string, string>) : {};
}

function writeMockHosts(hosts: Record<string, string>) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_HOSTS_KEY, JSON.stringify(hosts));
}

function defaultMockDatabases(): string[] {
  return Array.from(
    new Set(
      readMockProjects()
        .map((project) => project.databaseName?.trim())
        .filter((name): name is string => Boolean(name)),
    ),
  ).sort((left, right) => left.localeCompare(right));
}

function readMockDatabases(): string[] {
  if (typeof window === "undefined") {
    return defaultMockDatabases();
  }

  const stored = window.localStorage.getItem(MOCK_DATABASES_KEY);
  if (!stored) {
    const defaults = defaultMockDatabases();
    writeMockDatabases(defaults);
    return defaults;
  }

  return JSON.parse(stored) as string[];
}

function writeMockDatabases(databases: string[]) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_DATABASES_KEY, JSON.stringify(databases));
}

function readMockDatabaseSnapshots(): Record<string, DatabaseSnapshotSummary[]> {
  if (typeof window === "undefined") {
    return {};
  }

  const stored = window.localStorage.getItem(MOCK_DATABASE_SNAPSHOTS_KEY);
  if (!stored) {
    return {};
  }

  const parsed = JSON.parse(stored) as Record<string, DatabaseSnapshotSummary[]>;
  return Object.fromEntries(
    Object.entries(parsed).map(([databaseName, snapshots]) => [
      databaseName,
      snapshots.map((snapshot) => ({
        ...snapshot,
        storageBackend: snapshot.storageBackend ?? "sql",
        resticSnapshotId: snapshot.resticSnapshotId ?? null,
        logicalDumpPath: snapshot.logicalDumpPath ?? null,
        linkedProjectNames: snapshot.linkedProjectNames ?? [],
        scheduledIntervalMinutes: snapshot.scheduledIntervalMinutes ?? null,
      })),
    ]),
  );
}

function writeMockDatabaseSnapshots(snapshots: Record<string, DatabaseSnapshotSummary[]>) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_DATABASE_SNAPSHOTS_KEY, JSON.stringify(snapshots));
}

interface MockDatabaseTimeMachineState {
  enabled: boolean;
  scheduleEnabled: boolean;
  scheduleIntervalMinutes: number;
  linkedProjectActionSnapshotsEnabled: boolean;
  updatedAt: string;
}

function defaultMockDatabaseTimeMachineState(enabled: boolean): MockDatabaseTimeMachineState {
  return {
    enabled,
    scheduleEnabled: true,
    scheduleIntervalMinutes: 5,
    linkedProjectActionSnapshotsEnabled: true,
    updatedAt: new Date().toISOString(),
  };
}

function readMockDatabaseTimeMachine(): Record<string, MockDatabaseTimeMachineState> {
  if (typeof window === "undefined") {
    return {};
  }

  const stored = window.localStorage.getItem(MOCK_DATABASE_TIME_MACHINE_KEY);
  if (!stored) {
    return {};
  }

  const parsed = JSON.parse(stored) as Record<
    string,
    Partial<MockDatabaseTimeMachineState> & { enabled?: boolean }
  >;
  return Object.fromEntries(
    Object.entries(parsed).map(([databaseName, state]) => [
      databaseName,
      {
        ...defaultMockDatabaseTimeMachineState(Boolean(state.enabled)),
        ...state,
        enabled: Boolean(state.enabled),
      },
    ]),
  );
}

function writeMockDatabaseTimeMachine(statuses: Record<string, MockDatabaseTimeMachineState>) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_DATABASE_TIME_MACHINE_KEY, JSON.stringify(statuses));
}

function mockLinkedProjectNamesForDatabase(name: string): string[] {
  return readMockProjects()
    .filter((project) => project.databaseName === name)
    .map((project) => project.name);
}

function mockNextScheduledSnapshotAt(
  machineState: MockDatabaseTimeMachineState,
  latestSnapshotAt?: string | null,
): string | null {
  if (!machineState.enabled || !machineState.scheduleEnabled) {
    return null;
  }

  const base = latestSnapshotAt ?? machineState.updatedAt;
  const timestamp = new Date(base);
  if (Number.isNaN(timestamp.getTime())) {
    return null;
  }

  const intervalMs = machineState.scheduleIntervalMinutes * 60 * 1000;
  let nextTimestamp = timestamp.getTime() + intervalMs;
  const now = Date.now();
  if (nextTimestamp <= now) {
    const elapsedIntervals = Math.floor((now - timestamp.getTime()) / intervalMs);
    nextTimestamp = timestamp.getTime() + (elapsedIntervals + 1) * intervalMs;
  }

  timestamp.setTime(nextTimestamp);
  return timestamp.toISOString();
}

function ensureMockDatabaseExists(name: string) {
  if (!readMockDatabases().includes(name)) {
    throw {
      code: "DATABASE_NOT_FOUND",
      message: "The selected database does not exist anymore.",
    } satisfies AppError;
  }
}

function mockDatabaseTimeMachineStatus(name: string): DatabaseTimeMachineStatus {
  const machineState = readMockDatabaseTimeMachine()[name] ?? defaultMockDatabaseTimeMachineState(false);
  const snapshots = (readMockDatabaseSnapshots()[name] ?? []).sort((left, right) =>
    right.createdAt.localeCompare(left.createdAt),
  );
  const latestSnapshotAt = snapshots[0]?.createdAt ?? null;

  return {
    name,
    enabled: machineState.enabled,
    status: machineState.enabled ? "protected" : "off",
    snapshotCount: snapshots.length,
    scheduleEnabled: machineState.scheduleEnabled,
    scheduleIntervalMinutes: machineState.scheduleIntervalMinutes,
    linkedProjectActionSnapshotsEnabled: machineState.linkedProjectActionSnapshotsEnabled,
    latestSnapshotAt,
    nextScheduledSnapshotAt: mockNextScheduledSnapshotAt(machineState, latestSnapshotAt),
    lastError: null,
  };
}

function createMockSnapshot(
  name: string,
  triggerSource: DatabaseSnapshotSummary["triggerSource"],
  options?: {
    note?: string | null;
    linkedProjectNames?: string[];
    scheduledIntervalMinutes?: number | null;
  },
): DatabaseSnapshotResult {
  const timestamp = new Date().toISOString();
  const snapshots = readMockDatabaseSnapshots();
  const current = snapshots[name] ?? [];
  const machineState = readMockDatabaseTimeMachine()[name] ?? defaultMockDatabaseTimeMachineState(true);
  const snapshot: DatabaseSnapshotSummary = {
    id: `${timestamp.replace(/[:.]/g, "-")}-${current.length + 1}`,
    databaseName: name,
    createdAt: timestamp,
    triggerSource,
    sizeBytes: 1024 * Math.max(1, current.length + 1),
    storageBackend: "sql",
    resticSnapshotId: null,
    logicalDumpPath: null,
    linkedProjectNames: options?.linkedProjectNames ?? mockLinkedProjectNamesForDatabase(name),
    scheduledIntervalMinutes: options?.scheduledIntervalMinutes ?? null,
    note: options?.note ?? null,
  };
  snapshots[name] = [snapshot, ...current]
    .sort((left, right) => right.createdAt.localeCompare(left.createdAt))
    .slice(0, 3);
  writeMockDatabaseSnapshots(snapshots);

  const timeMachine = readMockDatabaseTimeMachine();
  timeMachine[name] = {
    ...machineState,
    enabled: true,
    updatedAt: timestamp,
  };
  writeMockDatabaseTimeMachine(timeMachine);

  return {
    success: true,
    name,
    snapshot,
    status: mockDatabaseTimeMachineStatus(name),
  };
}

function takeMockPreActionSnapshotIfEnabled(
  name: string,
  note: string,
  linkedProjectNames?: string[],
) {
  const status = mockDatabaseTimeMachineStatus(name);
  if (!status.enabled) {
    return null;
  }

  return createMockSnapshot(name, "pre-action", {
    note,
    linkedProjectNames,
  }).snapshot;
}

function takeMockProjectLinkedSnapshotIfEnabled(project: Project, note: string) {
  const databaseName = project.databaseName?.trim();
  if (!databaseName) {
    return null;
  }

  const status = mockDatabaseTimeMachineStatus(databaseName);
  if (!status.enabled || !status.linkedProjectActionSnapshotsEnabled) {
    return null;
  }

  return takeMockPreActionSnapshotIfEnabled(databaseName, note);
}

function readMockProjectEnvVars(): ProjectEnvVar[] {
  if (typeof window === "undefined") {
    return [];
  }

  const stored = window.localStorage.getItem(MOCK_PROJECT_ENV_VARS_KEY);
  return stored ? (JSON.parse(stored) as ProjectEnvVar[]) : [];
}

function readMockProjectScheduledTasks(): ProjectScheduledTask[] {
  if (typeof window === "undefined") {
    return [];
  }

  const stored = window.localStorage.getItem(MOCK_PROJECT_SCHEDULED_TASKS_KEY);
  return stored ? (JSON.parse(stored) as ProjectScheduledTask[]) : [];
}

function writeMockProjectScheduledTasks(tasks: ProjectScheduledTask[]) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_PROJECT_SCHEDULED_TASKS_KEY, JSON.stringify(tasks));
}

function readMockProjectScheduledTaskRuns(): ProjectScheduledTaskRun[] {
  if (typeof window === "undefined") {
    return [];
  }

  const stored = window.localStorage.getItem(MOCK_PROJECT_SCHEDULED_TASK_RUNS_KEY);
  return stored ? (JSON.parse(stored) as ProjectScheduledTaskRun[]) : [];
}

function writeMockProjectScheduledTaskRuns(runs: ProjectScheduledTaskRun[]) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_PROJECT_SCHEDULED_TASK_RUNS_KEY, JSON.stringify(runs));
}

function writeMockProjectEnvVars(envVars: ProjectEnvVar[]) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_PROJECT_ENV_VARS_KEY, JSON.stringify(envVars));
}

function buildMockProjectEnvInspection(projectId: string): ProjectEnvInspection {
  const project = getMockProjectOrThrow(projectId);
  const trackedVars = readMockProjectEnvVars()
    .filter((item) => item.projectId === projectId)
    .sort((left, right) => left.envKey.localeCompare(right.envKey));
  const diskMap = new Map<string, ProjectDiskEnvVar>();

  for (const item of trackedVars) {
    diskMap.set(item.envKey, {
      key: item.envKey,
      value: item.envValue,
      sourceLine: diskMap.size + 1,
    });
  }

  if (!diskMap.has("APP_NAME")) {
    diskMap.set("APP_NAME", {
      key: "APP_NAME",
      value: project.name,
      sourceLine: diskMap.size + 1,
    });
  }

  if (!diskMap.has("APP_URL")) {
    diskMap.set("APP_URL", {
      key: "APP_URL",
      value: `https://${project.domain}`,
      sourceLine: diskMap.size + 1,
    });
  }

  const diskVars = Array.from(diskMap.values()).sort((left, right) => left.key.localeCompare(right.key));
  const trackedMap = new Map(trackedVars.map((item) => [item.envKey, item.envValue]));
  const diskValueMap = new Map(diskVars.map((item) => [item.key, item.value]));
  const allKeys = Array.from(new Set([...trackedMap.keys(), ...diskValueMap.keys()])).sort((left, right) =>
    left.localeCompare(right),
  );
  const comparison: ProjectEnvComparisonItem[] = allKeys.map((key) => {
    const trackedValue = trackedMap.get(key);
    const diskValue = diskValueMap.get(key);
    let status: ProjectEnvComparisonItem["status"] = "match";

    if (trackedValue !== undefined && diskValue !== undefined) {
      status = trackedValue === diskValue ? "match" : "valueMismatch";
    } else if (trackedValue !== undefined) {
      status = "onlyTracked";
    } else {
      status = "onlyDisk";
    }

    return {
      key,
      trackedValue,
      diskValue,
      status,
    };
  });

  return {
    projectId,
    envFilePath: `${project.path}\\.env`,
    envFileExists: true,
    trackedCount: trackedVars.length,
    diskCount: diskVars.length,
    diskVars,
    comparison,
  };
}

function readMockProjectWorkers(): ProjectWorker[] {
  if (typeof window === "undefined") {
    return [];
  }

  const stored = window.localStorage.getItem(MOCK_PROJECT_WORKERS_KEY);
  return stored ? (JSON.parse(stored) as ProjectWorker[]) : [];
}

function writeMockProjectWorkers(workers: ProjectWorker[]) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_PROJECT_WORKERS_KEY, JSON.stringify(workers));
}

function readMockFrankenphpOctaneWorkers(): Record<string, FrankenphpOctaneWorkerSettings> {
  if (typeof window === "undefined") {
    return {};
  }

  const stored = window.localStorage.getItem(MOCK_FRANKENPHP_OCTANE_WORKERS_KEY);
  return stored ? (JSON.parse(stored) as Record<string, FrankenphpOctaneWorkerSettings>) : {};
}

function writeMockFrankenphpOctaneWorkers(workers: Record<string, FrankenphpOctaneWorkerSettings>) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_FRANKENPHP_OCTANE_WORKERS_KEY, JSON.stringify(workers));
}

function mockFrankenphpOctaneLogKey(projectId: string) {
  return `frankenphp-octane:${projectId}`;
}

function getMockFrankenphpOctaneSettings(projectId: string): FrankenphpOctaneWorkerSettings {
  const stored = readMockFrankenphpOctaneWorkers();
  if (stored[projectId]) {
    const project = readMockProjects().find((item) => item.id === projectId);
    return {
      ...stored[projectId],
      mode: stored[projectId].mode ?? (project?.frankenphpMode === "symfony" || project?.frankenphpMode === "custom" ? project.frankenphpMode : "octane"),
      customWorkerRelativePath: stored[projectId].customWorkerRelativePath ?? null,
    };
  }

  const timestamp = new Date().toISOString();
  const project = readMockProjects().find((item) => item.id === projectId);
  const usedWorkerPorts = new Set(Object.values(stored).map((worker) => worker.workerPort));
  const usedAdminPorts = new Set(Object.values(stored).map((worker) => worker.adminPort));
  const workerPort =
    Array.from({ length: 100 }, (_, index) => 8100 + index).find((port) => !usedWorkerPorts.has(port)) ?? 8100;
  const adminPort =
    Array.from({ length: 100 }, (_, index) => 9100 + index).find((port) => !usedAdminPorts.has(port)) ?? 9100;
  const settings: FrankenphpOctaneWorkerSettings = {
    projectId,
    mode: project?.frankenphpMode === "symfony" || project?.frankenphpMode === "custom" ? project.frankenphpMode : "octane",
    workerPort,
    adminPort,
    workers: 1,
    maxRequests: 500,
    status: "stopped",
    pid: null,
    lastStartedAt: null,
    lastStoppedAt: null,
    lastError: null,
    logPath: `${MOCK_APP_DATA_ROOT}/runtime-logs/frankenphp-octane/${projectId}.log`,
    customWorkerRelativePath: null,
    createdAt: timestamp,
    updatedAt: timestamp,
  };
  stored[projectId] = settings;
  writeMockFrankenphpOctaneWorkers(stored);
  return settings;
}

function updateMockFrankenphpOctaneSettings(
  projectId: string,
  patch: Partial<FrankenphpOctaneWorkerSettings>,
): FrankenphpOctaneWorkerSettings {
  const stored = readMockFrankenphpOctaneWorkers();
  const current = getMockFrankenphpOctaneSettings(projectId);
  const cleanedPatch = Object.fromEntries(
    Object.entries(patch).filter(([, value]) => value !== undefined),
  ) as Partial<FrankenphpOctaneWorkerSettings>;
  const next = {
    ...current,
    ...cleanedPatch,
    updatedAt: new Date().toISOString(),
  };
  stored[projectId] = next;
  writeMockFrankenphpOctaneWorkers(stored);
  return next;
}

function buildMockFrankenphpOctaneHealth(projectId: string): FrankenphpOctaneWorkerHealth {
  const project = getMockProjectOrThrow(projectId);
  const settings = getMockFrankenphpOctaneSettings(projectId);
  const runtime = readMockRuntimes().find((item) => item.runtimeType === "frankenphp" && item.isActive);
  const runtimeLabel = runtime
    ? `FrankenPHP ${runtime.version} (PHP ${runtime.phpFamily ?? mockRuntimePhpFamilyForItem(runtime)})`
    : "FrankenPHP";
  const extensionStates = runtime ? mockPhpExtensionsForRuntime(runtime.id, runtimeLabel) : [];
  const selectedExtensions = ["redis", "mbstring", "pdo_mysql"].map((extensionName) => {
    const state = extensionStates.find((item) => item.extensionName === extensionName);
    return {
      extensionName,
      available: Boolean(state),
      enabled: Boolean(state?.enabled),
    };
  });
  const lastStartedAt = settings.lastStartedAt ?? null;
  const uptimeSeconds =
    lastStartedAt && settings.status === "running"
      ? Math.max(0, Math.floor((Date.now() - new Date(lastStartedAt).getTime()) / 1000))
      : null;
  const logKey = mockFrankenphpOctaneLogKey(projectId);
  const logs = readMockServiceLogs()[logKey] ?? [];

  return {
    projectId,
    mode: settings.mode,
    status: settings.status,
    pid: settings.pid,
    uptimeSeconds,
    workerPort: settings.workerPort,
    adminPort: settings.adminPort,
    lastStartedAt,
    lastRestartedAt: null,
    lastError: settings.lastError,
    requestCount: settings.status === "running" ? Math.max(1, logs.length * 12) : null,
    metricsAvailable: settings.status === "running",
    logTail: logs.slice(-80).join("\n"),
    restartRecommended: project.name.toLowerCase().includes("restart"),
    restartReason: project.name.toLowerCase().includes("restart")
      ? "Browser mock detected a project metadata change after worker start."
      : null,
    runtime: runtime
      ? {
          runtimeId: runtime.id,
          version: runtime.version,
          phpFamily: runtime.phpFamily ?? mockRuntimePhpFamilyForItem(runtime),
          path: runtime.path,
          managedPhpConfigPath: `${MOCK_APP_DATA_ROOT}/runtime-config/frankenphp/php.ini`,
          extensions: selectedExtensions,
        }
      : null,
    generatedAt: new Date().toISOString(),
  };
}

function defaultMockServices(): ServiceState[] {
  const timestamp = new Date().toISOString();

  return [
    {
      name: "apache",
      enabled: true,
      autoStart: false,
      port: 80,
      pid: null,
      status: "stopped",
      lastError: null,
      updatedAt: timestamp,
    },
    {
      name: "nginx",
      enabled: true,
      autoStart: false,
      port: 80,
      pid: null,
      status: "stopped",
      lastError: null,
      updatedAt: timestamp,
    },
    {
      name: "frankenphp",
      enabled: true,
      autoStart: false,
      port: 80,
      pid: null,
      status: "stopped",
      lastError: null,
      updatedAt: timestamp,
    },
    {
      name: "mysql",
      enabled: true,
      autoStart: false,
      port: 3306,
      pid: null,
      status: "stopped",
      lastError: null,
      updatedAt: timestamp,
    },
    {
      name: "mailpit",
      enabled: true,
      autoStart: false,
      port: 8025,
      pid: null,
      status: "stopped",
      lastError: null,
      updatedAt: timestamp,
    },
    {
      name: "redis",
      enabled: true,
      autoStart: false,
      port: 6379,
      pid: null,
      status: "stopped",
      lastError: null,
      updatedAt: timestamp,
    },
  ];
}

function defaultMockRuntimeInventory(): RuntimeInventoryItem[] {
  const timestamp = new Date().toISOString();

  return [
    {
      id: "apache-2.4",
      runtimeType: "apache",
      version: "2.4",
      path: "D:/laragon/bin/apache/httpd-2.4/bin/httpd.exe",
      isActive: true,
      source: "external",
      status: "available",
      createdAt: timestamp,
      updatedAt: timestamp,
      details: null,
    },
    {
      id: "nginx-1.25",
      runtimeType: "nginx",
      version: "1.25",
      path: "D:/laragon/bin/nginx/nginx.exe",
      isActive: false,
      source: "external",
      status: "available",
      createdAt: timestamp,
      updatedAt: timestamp,
      details: null,
    },
    {
      id: "frankenphp-1.5.3",
      runtimeType: "frankenphp",
      version: "1.5.3",
      phpFamily: "8.4",
      path: "D:/tools/frankenphp/frankenphp.exe",
      isActive: false,
      source: "external",
      status: "available",
      createdAt: timestamp,
      updatedAt: timestamp,
      details: "Embedded PHP 8.4 runtime with managed Caddy config.",
    },
    {
      id: "mysql-8.0",
      runtimeType: "mysql",
      version: "8.0",
      path: "D:/laragon/bin/mysql/mysql-8.0/bin/mysqld.exe",
      isActive: true,
      source: "external",
      status: "available",
      createdAt: timestamp,
      updatedAt: timestamp,
      details: null,
    },
    {
      id: "php-8.2",
      runtimeType: "php",
      version: "8.2",
      phpFamily: "8.2",
      path: "D:/laragon/bin/php/php-8.2/php.exe",
      isActive: true,
      source: "external",
      status: "available",
      createdAt: timestamp,
      updatedAt: timestamp,
      details: null,
    },
  ];
}

function defaultMockRuntimePackages(): RuntimePackage[] {
  return [
    {
      id: "php-7.4.32-win-x64",
      runtimeType: "php",
      version: "7.4.32",
      phpFamily: "7.4",
      platform: "windows",
      arch: "x64",
      displayName: "PHP 7.4.32",
      downloadUrl: "https://downloads.devnest.invalid/php-7.4.32-win-x64.zip",
      checksumSha256: "demo-checksum",
      archiveKind: "zip",
      entryBinary: "php.exe",
      notes: "Preview catalog entry.",
    },
    {
      id: "php-8.0.30-win-x64",
      runtimeType: "php",
      version: "8.0.30",
      phpFamily: "8.0",
      platform: "windows",
      arch: "x64",
      displayName: "PHP 8.0.30",
      downloadUrl: "https://downloads.devnest.invalid/php-8.0.30-win-x64.zip",
      checksumSha256: "demo-checksum",
      archiveKind: "zip",
      entryBinary: "php.exe",
      notes: "Preview catalog entry.",
    },
    {
      id: "php-8.1.34-win-x64",
      runtimeType: "php",
      version: "8.1.34",
      phpFamily: "8.1",
      platform: "windows",
      arch: "x64",
      displayName: "PHP 8.1.34",
      downloadUrl: "https://downloads.devnest.invalid/php-8.1.34-win-x64.zip",
      checksumSha256: "demo-checksum",
      archiveKind: "zip",
      entryBinary: "php.exe",
      notes: "Preview catalog entry.",
    },
    {
      id: "php-8.2.30-win-x64",
      runtimeType: "php",
      version: "8.2.30",
      phpFamily: "8.2",
      platform: "windows",
      arch: "x64",
      displayName: "PHP 8.2.30",
      downloadUrl: "https://downloads.devnest.invalid/php-8.2.30-win-x64.zip",
      checksumSha256: "demo-checksum",
      archiveKind: "zip",
      entryBinary: "php.exe",
      notes: "Preview catalog entry.",
    },
    {
      id: "php-8.5.5-win-x64",
      runtimeType: "php",
      version: "8.5.5",
      phpFamily: "8.5",
      platform: "windows",
      arch: "x64",
      displayName: "PHP 8.5.5",
      downloadUrl: "https://downloads.devnest.invalid/php-8.5.5-win-x64.zip",
      checksumSha256: "demo-checksum",
      archiveKind: "zip",
      entryBinary: "php.exe",
      notes: "Preview catalog entry.",
    },
    {
      id: "php-8.4.20-win-x64",
      runtimeType: "php",
      version: "8.4.20",
      phpFamily: "8.4",
      platform: "windows",
      arch: "x64",
      displayName: "PHP 8.4.20",
      downloadUrl: "https://downloads.devnest.invalid/php-8.4.20-win-x64.zip",
      checksumSha256: "demo-checksum",
      archiveKind: "zip",
      entryBinary: "php.exe",
      notes: "Preview catalog entry.",
    },
    {
      id: "apache-2.4.66-win-x64",
      runtimeType: "apache",
      version: "2.4.66",
      platform: "windows",
      arch: "x64",
      displayName: "Apache 2.4.66",
      downloadUrl: "https://downloads.devnest.invalid/apache-2.4.66-win-x64.zip",
      checksumSha256: "demo-checksum",
      archiveKind: "zip",
      entryBinary: "Apache24/bin/httpd.exe",
      notes: "Preview catalog entry.",
    },
    {
      id: "nginx-1.28.0-win-x64",
      runtimeType: "nginx",
      version: "1.28.0",
      platform: "windows",
      arch: "x64",
      displayName: "Nginx 1.28.0",
      downloadUrl: "https://downloads.devnest.invalid/nginx-1.28.0-win-x64.zip",
      checksumSha256: "demo-checksum",
      archiveKind: "zip",
      entryBinary: "nginx-1.28.0/nginx.exe",
      notes: "Preview catalog entry.",
    },
    {
      id: "frankenphp-1.5.3-win-x64",
      runtimeType: "frankenphp",
      version: "1.5.3",
      phpFamily: "8.4",
      platform: "windows",
      arch: "x64",
      displayName: "FrankenPHP 1.5.3",
      downloadUrl: "https://downloads.devnest.invalid/frankenphp-1.5.3-win-x64.zip",
      checksumSha256: "demo-checksum",
      archiveKind: "zip",
      entryBinary: "frankenphp.exe",
      notes: "Preview catalog entry with embedded PHP 8.4.",
    },
    {
      id: "mariadb-11.8.3-win-x64",
      runtimeType: "mysql",
      version: "11.8.3",
      platform: "windows",
      arch: "x64",
      displayName: "MariaDB 11.8.3",
      downloadUrl: "https://downloads.devnest.invalid/mariadb-11.8.3-win-x64.zip",
      checksumSha256: "demo-checksum",
      archiveKind: "zip",
      entryBinary: "mariadb-11.8.3-winx64/bin/mysqld.exe",
      notes: "Preview catalog entry.",
    },
  ];
}

function mockPhpExtensionsForRuntime(runtimeId: string, runtimeVersion: string): PhpExtensionState[] {
  const timestamp = new Date().toISOString();
  const defaults = readMockPhpAvailableExtensions()[runtimeId] ?? defaultMockPhpAvailableExtensions();
  const overrides = readMockPhpExtensionOverrides()[runtimeId] ?? {};

  return defaults.map((extensionName) => ({
    runtimeId,
    runtimeVersion,
    extensionName,
    dllFile: `php_${extensionName}.dll`,
    enabled: overrides[extensionName] ?? true,
    updatedAt: timestamp,
  }));
}

function mockPhpFunctionsForRuntime(runtimeId: string, runtimeVersion: string): PhpFunctionState[] {
  const timestamp = new Date().toISOString();
  const overrides = readMockPhpFunctionOverrides()[runtimeId] ?? {};

  return managedMockPhpFunctions().map((functionName) => ({
    runtimeId,
    runtimeVersion,
    functionName,
    enabled: overrides[functionName] ?? true,
    updatedAt: timestamp,
  }));
}

function mockPhpVersionFamily(version: string): string {
  const [major = "", minor = ""] = version.split(".");
  return major && minor ? `${major}.${minor}` : version.trim();
}

function defaultMockPhpAvailableExtensions(): string[] {
  return [
    "bcmath",
    "curl",
    "exif",
    "fileinfo",
    "gd",
    "intl",
    "mbstring",
    "mysqli",
    "opcache",
    "openssl",
    "pdo_mysql",
    "zip",
  ];
}

function mockRuntimeVersionForType(runtimeType: RuntimeInventoryItem["runtimeType"]): string {
  switch (runtimeType) {
    case "php":
      return "8.2.12";
    case "apache":
      return "2.4.54";
    case "nginx":
      return "1.22.0";
    case "frankenphp":
      return "1.5.3";
    case "mysql":
      return "8.0.30";
  }
}

function mockRuntimePhpFamilyForItem(runtime: Pick<RuntimeInventoryItem, "runtimeType" | "version" | "phpFamily">) {
  if (runtime.runtimeType === "php") {
    return runtime.phpFamily ?? mockPhpVersionFamily(runtime.version);
  }

  if (runtime.runtimeType === "frankenphp") {
    return runtime.phpFamily ?? "8.4";
  }

  return null;
}

function mockRuntimePhpFamilyForType(
  runtimeType: RuntimeInventoryItem["runtimeType"],
  version: string,
) {
  if (runtimeType === "php") {
    return mockPhpVersionFamily(version);
  }

  if (runtimeType === "frankenphp") {
    return "8.4";
  }

  return null;
}

function isMockPhpToolsRuntime(
  runtime: RuntimeInventoryItem | undefined,
): runtime is RuntimeInventoryItem & { runtimeType: "php" | "frankenphp" } {
  return Boolean(
    runtime && (runtime.runtimeType === "php" || runtime.runtimeType === "frankenphp"),
  );
}

function mockPhpToolsRuntimeLabel(
  runtime: Pick<RuntimeInventoryItem, "runtimeType" | "version" | "phpFamily">,
): string {
  if (runtime.runtimeType === "frankenphp") {
    return `FrankenPHP ${runtime.version} (PHP ${mockRuntimePhpFamilyForItem(runtime) ?? "unknown"})`;
  }

  return runtime.version;
}

function mockManagedRuntimeBinaryPath(
  runtimeType: RuntimeInventoryItem["runtimeType"],
  version: string,
  root: "downloaded" | "managed",
) {
  const baseRoot =
    root === "downloaded"
      ? `${MOCK_APP_DATA_ROOT}/runtimes/downloaded`
      : `${MOCK_APP_DATA_ROOT}/runtimes`;

  switch (runtimeType) {
    case "php":
      return `${baseRoot}/php/${version}/php.exe`;
    case "apache":
      return `${baseRoot}/apache/${version}/bin/httpd.exe`;
    case "nginx":
      return `${baseRoot}/nginx/${version}/nginx.exe`;
    case "frankenphp":
      return `${baseRoot}/frankenphp/${version}/frankenphp.exe`;
    case "mysql":
      return `${baseRoot}/mysql/${version}/bin/mysqld.exe`;
  }
}

function findMockServerRuntime(project: Project, runtimes = readMockRuntimes()) {
  return runtimes.find((runtime) => runtime.runtimeType === project.serverType && runtime.isActive) ?? null;
}

function findMockPhpRuntimeForProject(
  project: Project,
  runtimes = readMockRuntimes(),
  serverRuntime = findMockServerRuntime(project, runtimes),
) {
  const expectedPhpFamily = mockPhpVersionFamily(project.phpVersion);

  if (project.serverType === "frankenphp") {
    const resolvedPhpFamily = serverRuntime ? mockRuntimePhpFamilyForItem(serverRuntime) : null;
    return {
      runtime: serverRuntime,
      embedded: true,
      expectedPhpFamily,
      resolvedPhpFamily,
      matchesVersion: Boolean(
        serverRuntime?.status === "available" && resolvedPhpFamily === expectedPhpFamily,
      ),
    };
  }

  const runtime =
    runtimes.find(
      (item) =>
        item.runtimeType === "php" &&
        mockRuntimePhpFamilyForItem(item) === expectedPhpFamily,
    ) ?? null;

  return {
    runtime,
    embedded: false,
    expectedPhpFamily,
    resolvedPhpFamily: runtime ? mockRuntimePhpFamilyForItem(runtime) : null,
    matchesVersion: Boolean(runtime?.status === "available"),
  };
}

function mockRuntimeConfigPath(runtime: RuntimeInventoryItem): string {
  switch (runtime.runtimeType) {
    case "php":
      return `${MOCK_APP_DATA_ROOT}/service-state/php/${runtime.version}/php.ini`;
    case "apache":
      return `${MOCK_APP_DATA_ROOT}/service-state/apache/httpd.conf`;
    case "nginx":
      return `${MOCK_APP_DATA_ROOT}/service-state/nginx/nginx.conf`;
    case "frankenphp":
      return `${MOCK_APP_DATA_ROOT}/service-state/frankenphp/Caddyfile`;
    case "mysql":
      return `${MOCK_APP_DATA_ROOT}/service-state/mysql/my.ini`;
  }
}

function defaultMockRuntimeConfigValues(runtime: RuntimeInventoryItem): Record<string, string> {
  switch (runtime.runtimeType) {
    case "php":
      return {
        short_open_tag: "off",
        max_execution_time: "120",
        max_input_time: "60",
        memory_limit: "512M",
        post_max_size: "64M",
        file_uploads: "on",
        upload_max_filesize: "64M",
        max_file_uploads: "20",
        default_socket_timeout: "60",
        error_reporting: "E_ALL",
        display_errors: "on",
        date_timezone: "UTC",
      };
    case "apache":
      return {
        timeout: "60",
        keep_alive: "on",
        keep_alive_timeout: "5",
        max_keep_alive_requests: "100",
        start_servers: "3",
        max_spare_threads: "75",
        min_spare_threads: "25",
        threads_per_child: "25",
        max_request_workers: "150",
        max_connections_per_child: "0",
      };
    case "nginx":
      return {
        timeout: "60",
        keep_alive: "on",
        keep_alive_timeout: "65",
        keep_alive_requests: "100",
        worker_processes: "1",
        worker_connections: "1024",
      };
    case "frankenphp":
      return {};
    case "mysql":
      return {};
  }
}

function mockRuntimeConfigSchema(runtime: RuntimeInventoryItem): RuntimeConfigSchema {
  const toggleOptions = [
    { value: "on", label: "On" },
    { value: "off", label: "Off" },
  ];

  if (runtime.runtimeType === "mysql" || runtime.runtimeType === "frankenphp") {
    return {
      runtimeId: runtime.id,
      runtimeType: runtime.runtimeType,
      runtimeVersion: runtime.version,
      configPath: mockRuntimeConfigPath(runtime),
      supportsEditor: false,
      openFileOnly: true,
      sections: [],
    };
  }

  if (runtime.runtimeType === "php") {
    return {
      runtimeId: runtime.id,
      runtimeType: runtime.runtimeType,
      runtimeVersion: runtime.version,
      configPath: mockRuntimeConfigPath(runtime),
      supportsEditor: true,
      openFileOnly: false,
      sections: [
        {
          id: "php-core",
          title: "PHP Core",
          description: "Managed php.ini values generated by DevNest.",
          fields: [
            { key: "short_open_tag", label: "Short Open Tag", kind: "toggle", options: toggleOptions },
            { key: "max_execution_time", label: "Max Execution Time", kind: "number", options: [] },
            { key: "max_input_time", label: "Max Input Time", kind: "number", options: [] },
            { key: "memory_limit", label: "Memory Limit", kind: "size", options: [] },
            { key: "post_max_size", label: "Post Max Size", kind: "size", options: [] },
            { key: "file_uploads", label: "File Uploads", kind: "toggle", options: toggleOptions },
            { key: "upload_max_filesize", label: "Upload Max Filesize", kind: "size", options: [] },
            { key: "max_file_uploads", label: "Max File Uploads", kind: "number", options: [] },
          ],
        },
        {
          id: "php-runtime",
          title: "Errors and Runtime",
          description: "Common PHP runtime switches.",
          fields: [
            { key: "default_socket_timeout", label: "Default Socket Timeout", kind: "number", options: [] },
            {
              key: "error_reporting",
              label: "Error Reporting",
              kind: "select",
              options: [
                { value: "E_ALL", label: "E_ALL" },
                {
                  value: "E_ALL & ~E_DEPRECATED & ~E_STRICT",
                  label: "E_ALL without deprecated",
                },
                { value: "E_ALL & ~E_NOTICE", label: "E_ALL without notices" },
                {
                  value: "E_ERROR | E_WARNING | E_PARSE",
                  label: "Errors and warnings only",
                },
              ],
            },
            { key: "display_errors", label: "Display Errors", kind: "toggle", options: toggleOptions },
            { key: "date_timezone", label: "Date Timezone", kind: "text", options: [] },
          ],
        },
      ],
    };
  }

  if (runtime.runtimeType === "apache") {
    return {
      runtimeId: runtime.id,
      runtimeType: runtime.runtimeType,
      runtimeVersion: runtime.version,
      configPath: mockRuntimeConfigPath(runtime),
      supportsEditor: true,
      openFileOnly: false,
      sections: [
        {
          id: "apache-connections",
          title: "Connections",
          description: "Managed Apache connection controls.",
          fields: [
            { key: "timeout", label: "Timeout", kind: "number", options: [] },
            { key: "keep_alive", label: "KeepAlive", kind: "toggle", options: toggleOptions },
            { key: "keep_alive_timeout", label: "KeepAlive Timeout", kind: "number", options: [] },
            {
              key: "max_keep_alive_requests",
              label: "Max KeepAlive Requests",
              kind: "number",
              options: [],
            },
            {
              key: "max_connections_per_child",
              label: "Max Connections Per Child",
              kind: "number",
              options: [],
            },
          ],
        },
        {
          id: "apache-workers",
          title: "Workers",
          description: "Managed event MPM worker settings.",
          fields: [
            { key: "start_servers", label: "StartServers", kind: "number", options: [] },
            { key: "max_spare_threads", label: "MaxSpareThreads", kind: "number", options: [] },
            { key: "min_spare_threads", label: "MinSpareThreads", kind: "number", options: [] },
            { key: "threads_per_child", label: "ThreadsPerChild", kind: "number", options: [] },
            { key: "max_request_workers", label: "MaxRequestWorkers", kind: "number", options: [] },
          ],
        },
      ],
    };
  }

  return {
    runtimeId: runtime.id,
    runtimeType: runtime.runtimeType,
    runtimeVersion: runtime.version,
    configPath: mockRuntimeConfigPath(runtime),
    supportsEditor: true,
    openFileOnly: false,
    sections: [
      {
        id: "nginx-connections",
        title: "Connections",
        description: "Managed Nginx connection controls.",
        fields: [
          { key: "timeout", label: "Timeout", kind: "number", options: [] },
          { key: "keep_alive", label: "KeepAlive", kind: "toggle", options: toggleOptions },
          { key: "keep_alive_timeout", label: "KeepAlive Timeout", kind: "number", options: [] },
          { key: "keep_alive_requests", label: "KeepAlive Requests", kind: "number", options: [] },
        ],
      },
      {
        id: "nginx-workers",
        title: "Workers",
        description: "Managed Nginx worker settings.",
        fields: [
          { key: "worker_processes", label: "Worker Processes", kind: "text", options: [] },
          { key: "worker_connections", label: "Worker Connections", kind: "number", options: [] },
        ],
      },
    ],
  };
}

function mockRuntimeConfigValues(runtime: RuntimeInventoryItem): RuntimeConfigValues {
  const overrides = readMockRuntimeConfigOverrides()[runtime.id] ?? {};

  return {
    runtimeId: runtime.id,
    runtimeType: runtime.runtimeType,
    runtimeVersion: runtime.version,
    configPath: mockRuntimeConfigPath(runtime),
    values: {
      ...defaultMockRuntimeConfigValues(runtime),
      ...overrides,
    },
    updatedAt: new Date().toISOString(),
  };
}

function mockPhpCompilerFamily(phpFamily: string): "vc15" | "vs16" | "vs17" {
  if (phpFamily === "7.4") {
    return "vc15";
  }

  if (phpFamily === "8.4" || phpFamily === "8.5") {
    return "vs17";
  }

  return "vs16";
}

function mockPhpExtensionPackages(): PhpExtensionPackage[] {
  const phpFamilies = ["7.4", "8.0", "8.1", "8.2", "8.3", "8.4", "8.5"] as const;
  const imagickFamilies = ["7.4", "8.0", "8.1"] as const;

  const redisPackages = phpFamilies.map((phpFamily) => ({
    id: `php-redis-6.3.0-${phpFamily}-win-x64`,
    extensionName: "redis",
    phpFamily,
    version: "6.3.0",
    platform: "windows",
    arch: "x64",
    displayName: "Redis 6.3.0",
    downloadUrl: `https://downloads.php.net/~windows/pecl/releases/redis/6.3.0/php_redis-6.3.0-${phpFamily}-nts-${mockPhpCompilerFamily(phpFamily)}-x64.zip`,
    checksumSha256: null,
    packageKind: "zip" as const,
    dllFile: "php_redis.dll",
    notes: `PECL Redis build for PHP ${phpFamily} x64 NTS.`,
  }));
  const redisTsPackages = phpFamilies.map((phpFamily) => ({
    id: `php-redis-6.3.0-${phpFamily}-ts-win-x64`,
    extensionName: "redis",
    phpFamily,
    threadSafety: "ts" as const,
    version: "6.3.0",
    platform: "windows",
    arch: "x64",
    displayName: "Redis 6.3.0",
    downloadUrl: `https://downloads.php.net/~windows/pecl/releases/redis/6.3.0/php_redis-6.3.0-${phpFamily}-ts-${mockPhpCompilerFamily(phpFamily)}-x64.zip`,
    checksumSha256: null,
    packageKind: "zip" as const,
    dllFile: "php_redis.dll",
    notes: `PECL Redis build for PHP ${phpFamily} x64 TS.`,
  }));

  const memcachePackages = phpFamilies.map((phpFamily) => ({
    id:
      phpFamily === "7.4"
        ? `php-memcache-4.0.5.2-${phpFamily}-win-x64`
        : `php-memcache-8.2-${phpFamily}-win-x64`,
    extensionName: "memcache",
    phpFamily,
    version: phpFamily === "7.4" ? "4.0.5.2" : "8.2",
    platform: "windows",
    arch: "x64",
    displayName: phpFamily === "7.4" ? "Memcache 4.0.5.2" : "Memcache 8.2",
    downloadUrl:
      phpFamily === "7.4"
        ? `https://downloads.php.net/~windows/pecl/releases/memcache/4.0.5.2/php_memcache-4.0.5.2-${phpFamily}-nts-vc15-x64.zip`
        : `https://downloads.php.net/~windows/pecl/releases/memcache/8.2/php_memcache-8.2-${phpFamily}-nts-${mockPhpCompilerFamily(phpFamily)}-x64.zip`,
    checksumSha256: null,
    packageKind: "zip" as const,
    dllFile: "php_memcache.dll",
    notes:
      phpFamily === "7.4"
        ? "Legacy Memcache build for PHP 7.4 x64 NTS."
        : `Memcache build for PHP ${phpFamily} x64 NTS.`,
  }));

  const memcachedPackages = phpFamilies.map((phpFamily) => ({
    id: `php-memcached-3.4.0-${phpFamily}-win-x64`,
    extensionName: "memcached",
    phpFamily,
    version: "3.4.0",
    platform: "windows",
    arch: "x64",
    displayName: "Memcached 3.4.0",
    downloadUrl: `https://downloads.php.net/~windows/pecl/releases/memcached/3.4.0/php_memcached-3.4.0-${phpFamily}-nts-${mockPhpCompilerFamily(phpFamily)}-x64.zip`,
    checksumSha256: null,
    packageKind: "zip" as const,
    dllFile: "php_memcached.dll",
    notes: `Memcached build for PHP ${phpFamily} x64 NTS.`,
  }));

  const xdebugPackages = phpFamilies.map((phpFamily) => ({
    id:
      phpFamily === "7.4"
        ? `php-xdebug-3.1.6-${phpFamily}-win-x64`
        : `php-xdebug-3.5.1-${phpFamily}-win-x64`,
    extensionName: "xdebug",
    phpFamily,
    version: phpFamily === "7.4" ? "3.1.6" : "3.5.1",
    platform: "windows",
    arch: "x64",
    displayName: phpFamily === "7.4" ? "Xdebug 3.1.6" : "Xdebug 3.5.1",
    downloadUrl:
      phpFamily === "7.4"
        ? "https://xdebug.org/files/php_xdebug-3.1.6-7.4-vc15-nts-x86_64.dll"
        : `https://xdebug.org/files/php_xdebug-3.5.1-${phpFamily}-${mockPhpCompilerFamily(phpFamily)}-nts-x86_64.dll`,
    checksumSha256: null,
    packageKind: "binary" as const,
    dllFile: "php_xdebug.dll",
    notes: `Official Xdebug DLL for PHP ${phpFamily} x64 NTS.`,
  }));

  const imagickPackages = imagickFamilies.map((phpFamily) => ({
    id: `php-imagick-3.7.0-${phpFamily}-win-x64`,
    extensionName: "imagick",
    phpFamily,
    version: "3.7.0",
    platform: "windows",
    arch: "x64",
    displayName: "Imagick 3.7.0",
    downloadUrl: `https://downloads.php.net/~windows/pecl/releases/imagick/3.7.0/php_imagick-3.7.0-${phpFamily}-nts-${mockPhpCompilerFamily(phpFamily)}-x64.zip`,
    checksumSha256: null,
    packageKind: "zip" as const,
    dllFile: "php_imagick.dll",
    notes: `Imagick build for PHP ${phpFamily} x64 NTS. Package also includes required ImageMagick DLLs.`,
  }));

  return [
    ...redisPackages,
    ...redisTsPackages,
    ...memcachePackages,
    ...memcachedPackages,
    ...xdebugPackages,
    ...imagickPackages,
  ];
}

function mockPhpExtensionPackagesForRuntime(
  runtime: Pick<RuntimeInventoryItem, "runtimeType" | "version" | "phpFamily">,
): PhpExtensionPackage[] {
  const phpFamily = mockRuntimePhpFamilyForItem(runtime) ?? mockPhpVersionFamily(runtime.version);
  const requiredThreadSafety = runtime.runtimeType === "frankenphp" ? "ts" : null;
  return mockPhpExtensionPackages().filter(
    (item) =>
      item.phpFamily === phpFamily &&
      (requiredThreadSafety
        ? item.threadSafety === requiredThreadSafety
        : !item.threadSafety || item.threadSafety === "nts"),
  );
}

function readMockRuntimes(): RuntimeInventoryItem[] {
  if (typeof window === "undefined") {
    return defaultMockRuntimeInventory();
  }

  const stored = window.localStorage.getItem(MOCK_RUNTIMES_KEY);
  if (!stored) {
    const defaults = defaultMockRuntimeInventory();
    writeMockRuntimes(defaults);
    return defaults;
  }

  return JSON.parse(stored) as RuntimeInventoryItem[];
}

function writeMockRuntimes(runtimes: RuntimeInventoryItem[]) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_RUNTIMES_KEY, JSON.stringify(runtimes));
}

function readMockSslAuthorityTrusted(): boolean {
  if (typeof window === "undefined") {
    return false;
  }

  return window.localStorage.getItem(MOCK_SSL_AUTHORITY_TRUSTED_KEY) === "true";
}

function writeMockSslAuthorityTrusted(value: boolean) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_SSL_AUTHORITY_TRUSTED_KEY, value ? "true" : "false");
}

function readMockRuntimePackages(): RuntimePackage[] {
  if (typeof window === "undefined") {
    return defaultMockRuntimePackages();
  }

  const stored = window.localStorage.getItem(MOCK_RUNTIME_PACKAGES_KEY);
  if (!stored) {
    const defaults = defaultMockRuntimePackages();
    writeMockRuntimePackages(defaults);
    return defaults;
  }

  return JSON.parse(stored) as RuntimePackage[];
}

function writeMockRuntimePackages(packages: RuntimePackage[]) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_RUNTIME_PACKAGES_KEY, JSON.stringify(packages));
}

function readMockRuntimeInstallTask(): RuntimeInstallTask | null {
  if (typeof window === "undefined") {
    return null;
  }

  const stored = window.localStorage.getItem(MOCK_RUNTIME_INSTALL_TASK_KEY);
  return stored ? (JSON.parse(stored) as RuntimeInstallTask) : null;
}

function writeMockRuntimeInstallTask(task: RuntimeInstallTask | null) {
  if (typeof window === "undefined") {
    return;
  }

  if (!task) {
    window.localStorage.removeItem(MOCK_RUNTIME_INSTALL_TASK_KEY);
    return;
  }

  window.localStorage.setItem(MOCK_RUNTIME_INSTALL_TASK_KEY, JSON.stringify(task));
}

function defaultMockOptionalTools(): OptionalToolInventoryItem[] {
  return [];
}

function readMockOptionalTools(): OptionalToolInventoryItem[] {
  if (typeof window === "undefined") {
    return defaultMockOptionalTools();
  }

  const stored = window.localStorage.getItem(MOCK_OPTIONAL_TOOLS_KEY);
  if (!stored) {
    const defaults = defaultMockOptionalTools();
    writeMockOptionalTools(defaults);
    return defaults;
  }

  return JSON.parse(stored) as OptionalToolInventoryItem[];
}

function writeMockOptionalTools(tools: OptionalToolInventoryItem[]) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_OPTIONAL_TOOLS_KEY, JSON.stringify(tools));
}

function defaultMockOptionalToolPackages(): OptionalToolPackage[] {
  return [
    {
      id: "mailpit-1.29.7-win-x64",
      toolType: "mailpit",
      version: "1.29.7",
      platform: "windows",
      arch: "x64",
      displayName: "Mailpit 1.29.7",
      downloadUrl: "https://github.com/axllent/mailpit/releases/download/v1.29.7/mailpit-windows-amd64.zip",
      checksumSha256: null,
      archiveKind: "zip",
      entryBinary: "mailpit.exe",
      notes: "Preview catalog entry.",
    },
    {
      id: "phpmyadmin-5.2.3-all-languages",
      toolType: "phpmyadmin",
      version: "5.2.3",
      platform: "windows",
      arch: "x64",
      displayName: "phpMyAdmin 5.2.3",
      downloadUrl: "https://files.phpmyadmin.net/phpMyAdmin/5.2.3/phpMyAdmin-5.2.3-all-languages.zip",
      checksumSha256: null,
      archiveKind: "zip",
      entryBinary: "phpMyAdmin-5.2.3-all-languages/index.php",
      notes: "Preview catalog entry.",
    },
    {
      id: "cloudflared-2025.10.1-win-x64",
      toolType: "cloudflared",
      version: "2025.10.1",
      platform: "windows",
      arch: "x64",
      displayName: "cloudflared 2025.10.1",
      downloadUrl:
        "https://github.com/cloudflare/cloudflared/releases/download/2025.10.1/cloudflared-windows-amd64.exe",
      checksumSha256: "272c1fabc6297302cbb187f4e603d4be4330907b537354a443ee154c4e0ed8a3",
      archiveKind: "binary",
      entryBinary: "cloudflared.exe",
      notes: "Preview catalog entry.",
    },
    {
      id: "redis-8.6.2-win-x64-msys2",
      toolType: "redis",
      version: "8.6.2",
      platform: "windows",
      arch: "x64",
      displayName: "Redis 8.6.2",
      downloadUrl: "https://github.com/redis-windows/redis-windows/releases/download/8.6.2/Redis-8.6.2-Windows-x64-msys2.zip",
      checksumSha256: "c2bcaa8ce0f4b942f749c491327dcf126a98169e0bde59013251e179d6f86b8b",
      archiveKind: "zip",
      entryBinary: "Redis-8.6.2-Windows-x64-msys2/redis-server.exe",
      notes: "Preview catalog entry.",
    },
    {
      id: "restic-0.18.1-win-x64",
      toolType: "restic",
      version: "0.18.1",
      platform: "windows",
      arch: "x64",
      displayName: "Restic 0.18.1",
      downloadUrl: "https://github.com/restic/restic/releases/download/v0.18.1/restic_0.18.1_windows_amd64.zip",
      checksumSha256: "0c1a713440578cb400d2e76208feb24f1b339426b075a21f73b6b2132692515d",
      archiveKind: "zip",
      entryBinary: "restic_0.18.1_windows_amd64.exe",
      notes: "Preview catalog entry.",
    },
  ];
}

function readMockOptionalToolPackages(): OptionalToolPackage[] {
  if (typeof window === "undefined") {
    return defaultMockOptionalToolPackages();
  }

  const stored = window.localStorage.getItem(MOCK_OPTIONAL_TOOL_PACKAGES_KEY);
  if (!stored) {
    const defaults = defaultMockOptionalToolPackages();
    writeMockOptionalToolPackages(defaults);
    return defaults;
  }

  return JSON.parse(stored) as OptionalToolPackage[];
}

function writeMockOptionalToolPackages(packages: OptionalToolPackage[]) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_OPTIONAL_TOOL_PACKAGES_KEY, JSON.stringify(packages));
}

function readMockOptionalToolInstallTask(): OptionalToolInstallTask | null {
  if (typeof window === "undefined") {
    return null;
  }

  const stored = window.localStorage.getItem(MOCK_OPTIONAL_TOOL_INSTALL_TASK_KEY);
  return stored ? (JSON.parse(stored) as OptionalToolInstallTask) : null;
}

function writeMockOptionalToolInstallTask(task: OptionalToolInstallTask | null) {
  if (typeof window === "undefined") {
    return;
  }

  if (!task) {
    window.localStorage.removeItem(MOCK_OPTIONAL_TOOL_INSTALL_TASK_KEY);
    return;
  }

  window.localStorage.setItem(MOCK_OPTIONAL_TOOL_INSTALL_TASK_KEY, JSON.stringify(task));
}

type MockPhpExtensionOverrides = Record<string, Record<string, boolean>>;
type MockPhpFunctionOverrides = Record<string, Record<string, boolean>>;
type MockPhpAvailableExtensions = Record<string, string[]>;
type MockRuntimeConfigOverrides = Record<string, Record<string, string>>;

function managedMockPhpFunctions(): string[] {
  return [
    "dl",
    "exec",
    "passthru",
    "pcntl_exec",
    "popen",
    "proc_open",
    "putenv",
    "shell_exec",
    "system",
  ];
}

function readMockPhpExtensionOverrides(): MockPhpExtensionOverrides {
  if (typeof window === "undefined") {
    return {};
  }

  const stored = window.localStorage.getItem(MOCK_PHP_EXTENSION_OVERRIDES_KEY);
  return stored ? (JSON.parse(stored) as MockPhpExtensionOverrides) : {};
}

function writeMockPhpExtensionOverrides(value: MockPhpExtensionOverrides) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_PHP_EXTENSION_OVERRIDES_KEY, JSON.stringify(value));
}

function readMockPhpFunctionOverrides(): MockPhpFunctionOverrides {
  if (typeof window === "undefined") {
    return {};
  }

  const stored = window.localStorage.getItem(MOCK_PHP_FUNCTION_OVERRIDES_KEY);
  return stored ? (JSON.parse(stored) as MockPhpFunctionOverrides) : {};
}

function writeMockPhpFunctionOverrides(value: MockPhpFunctionOverrides) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_PHP_FUNCTION_OVERRIDES_KEY, JSON.stringify(value));
}

function readMockPhpAvailableExtensions(): MockPhpAvailableExtensions {
  if (typeof window === "undefined") {
    return {};
  }

  const stored = window.localStorage.getItem(MOCK_PHP_AVAILABLE_EXTENSIONS_KEY);
  return stored ? (JSON.parse(stored) as MockPhpAvailableExtensions) : {};
}

function writeMockPhpAvailableExtensions(value: MockPhpAvailableExtensions) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_PHP_AVAILABLE_EXTENSIONS_KEY, JSON.stringify(value));
}

function readMockRuntimeConfigOverrides(): MockRuntimeConfigOverrides {
  if (typeof window === "undefined") {
    return {};
  }

  const stored = window.localStorage.getItem(MOCK_RUNTIME_CONFIG_OVERRIDES_KEY);
  return stored ? (JSON.parse(stored) as MockRuntimeConfigOverrides) : {};
}

function writeMockRuntimeConfigOverrides(value: MockRuntimeConfigOverrides) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_RUNTIME_CONFIG_OVERRIDES_KEY, JSON.stringify(value));
}

function readMockLastExportedProjectProfile(): Record<string, unknown> | null {
  if (typeof window === "undefined") {
    return null;
  }

  const stored = window.localStorage.getItem(MOCK_LAST_EXPORTED_PROJECT_PROFILE_KEY);
  return stored ? (JSON.parse(stored) as Record<string, unknown>) : null;
}

function writeMockLastExportedProjectProfile(value: Record<string, unknown>) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_LAST_EXPORTED_PROJECT_PROFILE_KEY, JSON.stringify(value));
}

function readMockLastExportedTeamProjectProfile(): Record<string, unknown> | null {
  if (typeof window === "undefined") {
    return null;
  }

  const stored = window.localStorage.getItem(MOCK_LAST_EXPORTED_TEAM_PROJECT_PROFILE_KEY);
  return stored ? (JSON.parse(stored) as Record<string, unknown>) : null;
}

function writeMockLastExportedTeamProjectProfile(value: Record<string, unknown>) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(
    MOCK_LAST_EXPORTED_TEAM_PROJECT_PROFILE_KEY,
    JSON.stringify(value),
  );
}

function mockProductionExportSlug(value: string): string {
  return (
    value
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "devnest-project"
  );
}

function buildMockFrankenphpProductionExportPreview(projectId: string): FrankenphpProductionExportPreview {
  const project = getMockProjectOrThrow(projectId);
  if (project.serverType !== "frankenphp") {
    throw {
      code: "FRANKENPHP_PRODUCTION_EXPORT_UNSUPPORTED",
      message: "Production export is currently available only for FrankenPHP projects.",
    } satisfies AppError;
  }

  const settings =
    project.frankenphpMode === "classic" ? null : getMockFrankenphpOctaneSettings(projectId);
  const slug = mockProductionExportSlug(project.domain);
  const documentRoot = project.documentRoot === "." ? `/var/www/${slug}` : `/var/www/${slug}/${project.documentRoot}`;
  const warnings = [
    "This is a Linux starter recipe. DevNest does not deploy remote servers, configure DNS/firewalls, issue production TLS certificates, or manage CI/CD.",
    "Secrets and `.env` values are not exported.",
  ];
  const workerFile =
    project.frankenphpMode === "octane"
      ? "public/frankenphp-worker.php"
      : project.frankenphpMode === "symfony"
        ? "public/index.php"
        : project.frankenphpMode === "custom"
          ? settings?.customWorkerRelativePath || "worker.php"
          : null;
  const caddyfile =
    project.frankenphpMode === "classic"
      ? `{\n    auto_https off\n}\n\n:80 {\n    root * ${documentRoot}\n    php_server\n}\n`
      : `{\n    auto_https off\n}\n\n:80 {\n    root * ${documentRoot}\n    php_server {\n        worker {\n            file ${workerFile}\n            num ${settings?.workers ?? 1}\n            max_requests ${settings?.maxRequests ?? 500}\n        }\n    }\n}\n`;
  const envKeys = readMockProjectEnvVars()
    .filter((item) => item.projectId === projectId)
    .map((item) => item.envKey)
    .sort();
  const deployment = `# DevNest FrankenPHP Production Starter\n\nProject: ${project.name}\nMode: ${project.frankenphpMode}\nLinux root: /var/www/${slug}\n\n## Environment Keys\n\n${
    envKeys.length > 0 ? envKeys.map((key) => `- ${key}`).join("\n") : "- No DevNest-tracked env keys were exported."
  }\n\nDevNest does not export .env values or secrets.\n`;

  return {
    projectId,
    projectName: project.name,
    slug,
    generatedAt: new Date().toISOString(),
    assumptions: [
      "Target host is Linux with FrankenPHP available as `/usr/local/bin/frankenphp`.",
      "Project files will live under `/var/www/{project-slug}`.",
      "Generated files are starter recipes and should be reviewed before production use.",
    ],
    warnings,
    files: [
      { relativePath: "Caddyfile", kind: "caddyfile", content: caddyfile },
      {
        relativePath: "devnest-frankenphp.service",
        kind: "systemd",
        content: `[Unit]\nDescription=DevNest FrankenPHP project ${slug}\nAfter=network.target\n\n[Service]\nType=simple\nWorkingDirectory=/var/www/${slug}\nExecStart=/usr/local/bin/frankenphp run --config /etc/devnest/${slug}/Caddyfile\nRestart=always\nRestartSec=5\n\n[Install]\nWantedBy=multi-user.target\n`,
      },
      {
        relativePath: "Dockerfile",
        kind: "dockerfile",
        content: `FROM dunglas/frankenphp:php${project.phpVersion}\n\nWORKDIR /app\nCOPY . /app\nCOPY Caddyfile /etc/caddy/Caddyfile\nEXPOSE 80\nCMD [\"frankenphp\", \"run\", \"--config\", \"/etc/caddy/Caddyfile\"]\n`,
      },
      { relativePath: "DEPLOYMENT.md", kind: "markdown", content: deployment },
    ],
  };
}

function readMockProjectTunnels(): Record<string, ProjectTunnelState> {
  if (typeof window === "undefined") {
    return {};
  }

  const stored = window.localStorage.getItem(MOCK_PROJECT_TUNNELS_KEY);
  return stored ? (JSON.parse(stored) as Record<string, ProjectTunnelState>) : {};
}

function writeMockProjectTunnels(value: Record<string, ProjectTunnelState>) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_PROJECT_TUNNELS_KEY, JSON.stringify(value));
}

function readMockProjectMobilePreviews(): Record<string, ProjectMobilePreviewState> {
  if (typeof window === "undefined") {
    return {};
  }

  const stored = window.localStorage.getItem(MOCK_PROJECT_MOBILE_PREVIEWS_KEY);
  return stored ? (JSON.parse(stored) as Record<string, ProjectMobilePreviewState>) : {};
}

function writeMockProjectMobilePreviews(value: Record<string, ProjectMobilePreviewState>) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_PROJECT_MOBILE_PREVIEWS_KEY, JSON.stringify(value));
}

function readMockProjectPersistentHostnames(): Record<string, ProjectPersistentHostname> {
  if (typeof window === "undefined") {
    return {};
  }

  const stored = window.localStorage.getItem(MOCK_PROJECT_PERSISTENT_HOSTNAMES_KEY);
  return stored ? (JSON.parse(stored) as Record<string, ProjectPersistentHostname>) : {};
}

function writeMockProjectPersistentHostnames(
  value: Record<string, ProjectPersistentHostname>,
) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(
    MOCK_PROJECT_PERSISTENT_HOSTNAMES_KEY,
    JSON.stringify(value),
  );
}

function readMockProjectPersistentTunnels(): Record<string, ProjectPersistentTunnelState> {
  if (typeof window === "undefined") {
    return {};
  }

  const stored = window.localStorage.getItem(MOCK_PROJECT_PERSISTENT_TUNNELS_KEY);
  return stored ? (JSON.parse(stored) as Record<string, ProjectPersistentTunnelState>) : {};
}

function writeMockProjectPersistentTunnels(
  value: Record<string, ProjectPersistentTunnelState>,
) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(
    MOCK_PROJECT_PERSISTENT_TUNNELS_KEY,
    JSON.stringify(value),
  );
}

function defaultMockPersistentTunnelSetup(): PersistentTunnelSetupStatus {
  return {
    provider: "cloudflared",
    ready: false,
    managed: false,
    binaryPath: `${MOCK_APP_DATA_ROOT}/optional-tools/cloudflared/cloudflared.exe`,
    authCertPath: null,
    credentialsPath: null,
    tunnelId: null,
    tunnelName: null,
    defaultHostnameZone: null,
    details:
      "Connect Cloudflare once, create or select a named tunnel, then set a default public zone so DevNest can publish projects with one click.",
    guidance:
      "One-time setup: connect Cloudflare, create or select a named tunnel, and set a default public zone like previews.example.com.",
  };
}

function readMockPersistentTunnelSetup(): PersistentTunnelSetupStatus {
  if (typeof window === "undefined") {
    return defaultMockPersistentTunnelSetup();
  }

  const stored = window.localStorage.getItem(MOCK_PERSISTENT_TUNNEL_SETUP_KEY);
  if (!stored) {
    const defaults = defaultMockPersistentTunnelSetup();
    writeMockPersistentTunnelSetup(defaults);
    return defaults;
  }

  return JSON.parse(stored) as PersistentTunnelSetupStatus;
}

function writeMockPersistentTunnelSetup(value: PersistentTunnelSetupStatus) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_PERSISTENT_TUNNEL_SETUP_KEY, JSON.stringify(value));
}

function normalizeMockTunnelNameKey(value: string): string {
  return value.trim().toLowerCase();
}

function readMockPersistentNamedTunnels(): PersistentTunnelNamedTunnelSummary[] {
  if (typeof window === "undefined") {
    return [];
  }

  const stored = window.localStorage.getItem(MOCK_PERSISTENT_NAMED_TUNNELS_KEY);
  if (stored) {
    return JSON.parse(stored) as PersistentTunnelNamedTunnelSummary[];
  }

  const setup = readMockPersistentTunnelSetup();
  if (!setup.tunnelId || !setup.tunnelName) {
    return [];
  }

  const defaults = [
    {
      tunnelId: setup.tunnelId,
      tunnelName: setup.tunnelName,
      credentialsPath: setup.credentialsPath ?? null,
      selected: true,
    } satisfies PersistentTunnelNamedTunnelSummary,
  ];
  writeMockPersistentNamedTunnels(defaults);
  return defaults;
}

function writeMockPersistentNamedTunnels(value: PersistentTunnelNamedTunnelSummary[]) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_PERSISTENT_NAMED_TUNNELS_KEY, JSON.stringify(value));
}

function readMockServices(): ServiceState[] {
  if (typeof window === "undefined") {
    return defaultMockServices();
  }

  const stored = window.localStorage.getItem(MOCK_SERVICES_KEY);
  if (!stored) {
    const defaults = defaultMockServices();
    writeMockServices(defaults);
    return defaults;
  }

  return JSON.parse(stored) as ServiceState[];
}

function writeMockServices(services: ServiceState[]) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_SERVICES_KEY, JSON.stringify(services));
}

function readMockServiceLogs(): Record<string, string[]> {
  if (typeof window === "undefined") {
    return {};
  }

  const stored = window.localStorage.getItem(MOCK_SERVICE_LOGS_KEY);
  return stored ? (JSON.parse(stored) as Record<string, string[]>) : {};
}

function writeMockServiceLogs(logs: Record<string, string[]>) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(MOCK_SERVICE_LOGS_KEY, JSON.stringify(logs));
}

function appendMockServiceLog(serviceName: ServiceState["name"], entry: string) {
  const logs = readMockServiceLogs();
  const next = [...(logs[serviceName] ?? []), entry].slice(-400);
  logs[serviceName] = next;
  writeMockServiceLogs(logs);
}

function appendMockWorkerLog(workerId: string, entry: string) {
  const logs = readMockServiceLogs();
  const next = [...(logs[`worker:${workerId}`] ?? []), entry].slice(-400);
  logs[`worker:${workerId}`] = next;
  writeMockServiceLogs(logs);
}

function appendMockScheduledTaskRunLog(runId: string, entry: string) {
  const logs = readMockServiceLogs();
  const next = [...(logs[`task-run:${runId}`] ?? []), entry].slice(-400);
  logs[`task-run:${runId}`] = next;
  writeMockServiceLogs(logs);
}

function upsertMockService(nextService: ServiceState): ServiceState {
  const services = readMockServices();
  const nextServices = services.map((service) =>
    service.name === nextService.name ? nextService : service,
  );
  writeMockServices(nextServices);
  return nextService;
}

function getMockServiceOrThrow(name: string): ServiceState {
  const service = readMockServices().find((item) => item.name === name);

  if (!service) {
    throw {
      code: "SERVICE_NOT_FOUND",
      message: "Service not found.",
    } satisfies AppError;
  }

  return service;
}

function getMockWorkerOrThrow(workerId: string): ProjectWorker {
  const worker = readMockProjectWorkers().find((item) => item.id === workerId);
  if (!worker) {
    throw {
      code: "WORKER_NOT_FOUND",
      message: "Worker not found.",
    } satisfies AppError;
  }

  return worker;
}

function upsertMockWorker(nextWorker: ProjectWorker): ProjectWorker {
  const workers = readMockProjectWorkers();
  const nextWorkers = workers.some((worker) => worker.id === nextWorker.id)
    ? workers.map((worker) => (worker.id === nextWorker.id ? nextWorker : worker))
    : [nextWorker, ...workers];
  writeMockProjectWorkers(nextWorkers);
  return nextWorker;
}

function getMockScheduledTaskOrThrow(taskId: string): ProjectScheduledTask {
  const task = readMockProjectScheduledTasks().find((item) => item.id === taskId);
  if (!task) {
    throw {
      code: "SCHEDULED_TASK_NOT_FOUND",
      message: "Scheduled task not found.",
    } satisfies AppError;
  }

  return task;
}

function upsertMockScheduledTask(nextTask: ProjectScheduledTask): ProjectScheduledTask {
  const tasks = readMockProjectScheduledTasks();
  const nextTasks = tasks.some((task) => task.id === nextTask.id)
    ? tasks.map((task) => (task.id === nextTask.id ? nextTask : task))
    : [nextTask, ...tasks];
  writeMockProjectScheduledTasks(nextTasks);
  return nextTask;
}

function parseMockWorkerCommandLine(commandLine: string) {
  const parts = commandLine.trim().split(/\s+/).filter(Boolean);
  return {
    command: parts[0] ?? "",
    args: parts.slice(1),
  };
}

function mockScheduledTaskScheduleExpression(task: {
  scheduleMode: ProjectScheduledTask["scheduleMode"];
  simpleScheduleKind?: ProjectScheduledTask["simpleScheduleKind"];
  scheduleExpression?: string | null;
  intervalSeconds?: number | null;
  dailyTime?: string | null;
  weeklyDay?: number | null;
}) {
  if (task.scheduleMode === "cron") {
    return task.scheduleExpression?.trim() || "*/15 * * * *";
  }

  switch (task.simpleScheduleKind) {
    case "everySeconds":
      return `Every ${task.intervalSeconds ?? 5} seconds`;
    case "everyMinutes":
      return `Every ${Math.max(1, Math.round((task.intervalSeconds ?? 300) / 60))} minutes`;
    case "everyHours":
      return `Every ${Math.max(1, Math.round((task.intervalSeconds ?? 3600) / 3600))} hours`;
    case "daily":
      return `Daily at ${task.dailyTime ?? "08:00"}`;
    case "weekly":
      return `Weekly on ${task.weeklyDay ?? 0} at ${task.dailyTime ?? "08:00"}`;
    default:
      return "Every 5 minutes";
  }
}

function mockNextScheduledTaskRunAt(
  task: Pick<
    ProjectScheduledTask,
    | "enabled"
    | "scheduleMode"
    | "simpleScheduleKind"
    | "intervalSeconds"
    | "dailyTime"
    | "weeklyDay"
  >,
  baseIso = new Date().toISOString(),
): string | null {
  if (!task.enabled) {
    return null;
  }

  const baseDate = new Date(baseIso);
  if (task.scheduleMode === "cron") {
    baseDate.setMinutes(baseDate.getMinutes() + 15, 0, 0);
    return baseDate.toISOString();
  }

  switch (task.simpleScheduleKind) {
    case "everySeconds":
    case "everyMinutes":
    case "everyHours":
      baseDate.setTime(baseDate.getTime() + (task.intervalSeconds ?? 300) * 1000);
      return baseDate.toISOString();
    case "daily": {
      const [hours, minutes] = (task.dailyTime ?? "08:00").split(":").map(Number);
      const next = new Date(baseDate);
      next.setSeconds(0, 0);
      next.setHours(hours || 8, minutes || 0, 0, 0);
      if (next.getTime() <= baseDate.getTime()) {
        next.setDate(next.getDate() + 1);
      }
      return next.toISOString();
    }
    case "weekly": {
      const [hours, minutes] = (task.dailyTime ?? "08:00").split(":").map(Number);
      const targetDay = Number(task.weeklyDay ?? 0);
      const next = new Date(baseDate);
      next.setSeconds(0, 0);
      next.setHours(hours || 8, minutes || 0, 0, 0);
      const currentDay = (next.getDay() + 6) % 7;
      let delta = targetDay - currentDay;
      if (delta < 0 || (delta === 0 && next.getTime() <= baseDate.getTime())) {
        delta += 7;
      }
      next.setDate(next.getDate() + delta);
      return next.toISOString();
    }
    default:
      baseDate.setMinutes(baseDate.getMinutes() + 5, 0, 0);
      return baseDate.toISOString();
  }
}

function buildMockBootState(): BootState {
  return {
    appName: "DevNest",
    environment: "browser",
    dbPath: ".devnest/devnest.sqlite3",
    startedAt: new Date().toISOString(),
  };
}

function buildMockWorkspaceOverview(): WorkspaceOverviewPayload {
  const projects = readMockProjects();
  const services = readMockServices();
  const workers = readMockProjectWorkers();
  const scheduledTasks = readMockProjectScheduledTasks();
  return {
    bootState: buildMockBootState(),
    projects,
    services,
    workers,
    scheduledTasks,
    portSummary: [],
  };
}

function buildMockWorkspacePortSummary(): WorkspaceOverviewPayload["portSummary"] {
  const services = readMockServices();
  const portSummary = Array.from(
    new Set(
      services
        .map((service) => service.port)
        .filter((port): port is number => typeof port === "number"),
    ),
  ).map((port) => {
    const managedOwner =
      services.find((service) => service.status === "running" && service.port === port)?.name ?? null;
    return {
      port,
      available: managedOwner == null,
      pid: services.find((service) => service.status === "running" && service.port === port)?.pid ?? null,
      processName: managedOwner,
      managedOwner,
      expectedServices: services
        .filter((service) => service.port === port)
        .map((service) => service.name),
    };
  });

  return portSummary;
}

function inferFrameworkFromPath(path: string): ScanResult {
  const normalized = path.toLowerCase();
  const segments = normalized.split(/[\\/]/).filter(Boolean);
  const name = segments.length > 0 ? segments[segments.length - 1] : "project";

  if (normalized.includes("laravel")) {
    return {
      framework: "laravel",
      recommendedServer: "nginx",
      serverReason: "Browser mock uses folder-name heuristics and defaults Laravel to Nginx.",
      recommendedPhpVersion: "8.2",
      suggestedDomain: `${name}.test`,
      documentRoot: "public",
      documentRootReason: "Browser mock found a Laravel-like path and assumed public/ as the web root.",
      detectedFiles: ["artisan", "bootstrap/app.php", "public/index.php"],
      warnings: [],
      missingPhpExtensions: [
        "bcmath",
        "ctype",
        "fileinfo",
        "intl",
        "mbstring",
        "openssl",
        "pdo_mysql",
        "tokenizer",
        "xml",
      ],
    };
  }

  if (normalized.includes("symfony")) {
    return {
      framework: "symfony",
      recommendedServer: "nginx",
      serverReason: "Browser mock uses folder-name heuristics and defaults Symfony to Nginx.",
      recommendedPhpVersion: "8.2",
      suggestedDomain: `${name}.test`,
      documentRoot: "public",
      documentRootReason: "Browser mock found a Symfony-like path and assumed public/ as the web root.",
      detectedFiles: ["bin/console", "config/bundles.php", "public/index.php"],
      warnings: [],
      missingPhpExtensions: ["ctype", "iconv", "intl", "mbstring", "openssl", "pdo_mysql", "tokenizer", "xml"],
    };
  }

  if (normalized.includes("wordpress") || normalized.includes("wp")) {
    return {
      framework: "wordpress",
      recommendedServer: "apache",
      serverReason: "Browser mock uses folder-name heuristics and assumes WordPress prefers Apache.",
      suggestedDomain: `${name}.test`,
      documentRoot: ".",
      documentRootReason: "Browser mock assumed WordPress serves from the project root.",
      detectedFiles: ["wp-config.php", "wp-content/"],
      warnings: [],
      missingPhpExtensions: ["curl", "dom", "gd", "json", "mysqli", "openssl", "xml", "zip"],
    };
  }

  return {
    framework: "php",
    recommendedServer: "apache",
    serverReason: "Browser mock falls back to Apache for generic PHP projects.",
    suggestedDomain: `${name}.test`,
    documentRoot: normalized.includes("public") ? "public" : ".",
    documentRootReason: normalized.includes("public")
      ? "Browser mock saw a public-like path and assumed public/ as the web root."
      : "Browser mock fell back to the project root.",
    detectedFiles: normalized.includes("public") ? ["public/index.php"] : ["index.php"],
    warnings: ["Browser mock scan uses path heuristics only."],
    missingPhpExtensions: [],
  };
}

function validateMockProjectInput(input: CreateProjectInput) {
  const pathResult = projectPathSchema.safeParse(input.path);
  if (!pathResult.success) {
    throw {
      code: "INVALID_PROJECT_PATH",
      message: pathResult.error.issues[0]?.message ?? "Project path is required.",
    } satisfies AppError;
  }

  const nameResult = projectNameSchema.safeParse(input.name);
  if (!nameResult.success) {
    throw {
      code: "INVALID_PROJECT_NAME",
      message: nameResult.error.issues[0]?.message ?? "Project name is invalid.",
    } satisfies AppError;
  }

  const domainResult = domainSchema.safeParse(input.domain);
  if (!domainResult.success) {
    throw {
      code: "INVALID_PROJECT_DOMAIN",
      message: domainResult.error.issues[0]?.message ?? "Project domain is invalid.",
    } satisfies AppError;
  }

  const documentRootResult = documentRootSchema.safeParse(input.documentRoot);
  if (!documentRootResult.success) {
    throw {
      code: "INVALID_DOCUMENT_ROOT",
      message: documentRootResult.error.issues[0]?.message ?? "Document root is invalid.",
    } satisfies AppError;
  }

  if (
    input.frankenphpMode === "octane" &&
    (input.serverType !== "frankenphp" || input.framework !== "laravel")
  ) {
    throw {
      code: "INVALID_FRANKENPHP_MODE",
      message: "Laravel Octane Worker mode is only available for Laravel projects using FrankenPHP.",
    } satisfies AppError;
  }
  if (
    input.frankenphpMode === "symfony" &&
    (input.serverType !== "frankenphp" || input.framework !== "symfony")
  ) {
    throw {
      code: "INVALID_FRANKENPHP_MODE",
      message: "Symfony Worker mode is only available for Symfony projects using FrankenPHP.",
    } satisfies AppError;
  }
  if (input.frankenphpMode === "custom" && input.serverType !== "frankenphp") {
    throw {
      code: "INVALID_FRANKENPHP_MODE",
      message: "Custom FrankenPHP Worker mode is only available for FrankenPHP projects.",
    } satisfies AppError;
  }
}

function createMockRecipeProject(
  input: Omit<CreateProjectInput, "name" | "framework" | "documentRoot"> & {
    framework: CreateProjectInput["framework"];
    documentRoot: string;
  },
): Project {
  const pathResult = recipeTargetPathSchema.safeParse(input.path);
  if (!pathResult.success) {
    throw {
      code: "INVALID_RECIPE_PATH",
      message: pathResult.error.issues[0]?.message ?? "Recipe target path is required.",
    } satisfies AppError;
  }

  const domainResult = domainSchema.safeParse(input.domain);
  if (!domainResult.success) {
    throw {
      code: "INVALID_PROJECT_DOMAIN",
      message: domainResult.error.issues[0]?.message ?? "Project domain is invalid.",
    } satisfies AppError;
  }

  const existing = readMockProjects();
  if (existing.some((project) => project.domain === input.domain)) {
    throw {
      code: "DOMAIN_ALREADY_EXISTS",
      message: "A project with this domain already exists.",
    } satisfies AppError;
  }

  if (existing.some((project) => project.path === input.path)) {
    throw {
      code: "PROJECT_PATH_EXISTS",
      message: "A project with this path already exists.",
    } satisfies AppError;
  }

  const segments = input.path.split(/[\\/]/).filter(Boolean);
  const rawName = segments.length > 0 ? segments[segments.length - 1] : "project";
  const name = rawName
    .split(/[-_\s]+/)
    .filter(Boolean)
    .map((part: string) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");

  const created: Project = {
    id: crypto.randomUUID(),
    name: name || "Project",
    path: input.path.trim(),
    domain: input.domain.trim().toLowerCase(),
    serverType: input.serverType,
    phpVersion: input.phpVersion,
    framework: input.framework,
    documentRoot: input.documentRoot,
    sslEnabled: input.sslEnabled,
    databaseName: null,
    databasePort: null,
    status: "stopped",
    frankenphpMode: input.frankenphpMode ?? "classic",
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  };

  writeMockProjects([created, ...existing]);
  return created;
}

function mockDiagnostics(projectId: string): DiagnosticItem[] {
  const project = getMockProjectOrThrow(projectId);
  const services = readMockServices();
  const runtimes = readMockRuntimes();
  const timestamp = new Date().toISOString();
  const items: DiagnosticItem[] = [];
  const runtimeService = services.find((service) => service.name === project.serverType);
  const conflictingService = services.find(
    (service) =>
      service.name !== project.serverType &&
      service.status === "running" &&
      service.port === runtimeService?.port,
  );
  const mysqlService = services.find((service) => service.name === "mysql");

  if (project.framework === "laravel" && project.documentRoot !== "public") {
    items.push({
      id: "diag-laravel-root",
      projectId,
      level: "error",
      code: "LARAVEL_DOCUMENT_ROOT_MISMATCH",
      title: "Laravel document root should point to /public",
      message: `${project.name} is not serving from \`public\`, so the generated local config will be wrong.`,
      suggestion: "Change the document root to `public`, then regenerate the managed config.",
      createdAt: timestamp,
    });
  }

  if (runtimeService?.status !== "running" && conflictingService?.port) {
    items.push({
      id: "diag-port-conflict",
      projectId,
      level: "error",
      code: "PORT_IN_USE",
      title: "Web server port is already in use",
      message: `Port ${conflictingService.port} is already owned by ${conflictingService.name}.`,
      suggestion: `Stop ${conflictingService.name} or switch the selected runtime before starting ${project.serverType}.`,
      createdAt: timestamp,
    });
  }

  if (project.serverType === "apache" && project.framework === "laravel") {
    items.push({
      id: "diag-rewrite",
      projectId,
      level: "warning",
      code: "APACHE_REWRITE_UNVERIFIED",
      title: "Apache rewrite support could not be verified in preview",
      message: "Preview mode cannot inspect Apache modules directly.",
      suggestion: "Run the desktop app to verify `mod_rewrite` against the real Apache runtime.",
      createdAt: timestamp,
    });
  }

  if (project.serverType === "frankenphp") {
    const phpBinding = findMockPhpRuntimeForProject(project, runtimes);
    if (phpBinding.runtime && !phpBinding.matchesVersion) {
      items.push({
        id: "diag-frankenphp-php-family",
        projectId,
        level: "error",
        code: "FRANKENPHP_PHP_VERSION_MISMATCH",
        title: "FrankenPHP embeds a different PHP family",
        message: `The active FrankenPHP runtime embeds PHP ${phpBinding.resolvedPhpFamily ?? "unknown"}, but ${project.name} expects PHP ${phpBinding.expectedPhpFamily}.`,
        suggestion:
          "Switch to a FrankenPHP runtime with the matching embedded PHP family, or change the project's selected PHP version.",
        createdAt: timestamp,
      });
    }
  }

  if (project.framework === "laravel" && project.phpVersion === "8.1") {
    items.push({
      id: "diag-php-ext",
      projectId,
      level: "warning",
      code: "PHP_MISSING_EXTENSIONS",
      title: "PHP runtime may be missing required extensions",
      message: "Laravel usually needs extensions such as intl and mbstring before the site can run cleanly.",
      suggestion: "Verify the selected PHP runtime and enable the required extensions before starting the site.",
      createdAt: timestamp,
    });
  }

  if (project.databaseName && mysqlService?.status === "error") {
    items.push({
      id: "diag-mysql",
      projectId,
      level: "error",
      code: "MYSQL_STARTUP_FAILED",
      title: "MySQL reported a startup error",
      message: "The linked database runtime is in an error state.",
      suggestion: mysqlService.lastError ?? "Inspect the MySQL log and retry after fixing the startup issue.",
      createdAt: timestamp,
    });
  }

  if (items.length === 0) {
    items.push({
      id: "diag-workspace-ready",
      projectId,
      level: "info",
      code: "WORKSPACE_READY",
      title: "No blocking issues detected",
      message: "The current project profile looks ready for a standard local run.",
      suggestion: "Open the local site and verify the expected response.",
      createdAt: timestamp,
    });
  }

  return items;
}

function buildMockFrankenphpOctanePreflight(projectId: string): FrankenphpOctanePreflight {
  const project = getMockProjectOrThrow(projectId);
  const settings = getMockFrankenphpOctaneSettings(projectId);
  const mode = project.frankenphpMode === "symfony" || project.frankenphpMode === "custom" ? project.frankenphpMode : "octane";
  const runtime = readMockRuntimes().find((item) => item.runtimeType === "frankenphp" && item.isActive);
  const workerConflict = readMockServices().find(
    (service) => service.status === "running" && service.port === settings.workerPort,
  );
  const adminConflict = readMockServices().find(
    (service) => service.status === "running" && service.port === settings.adminPort,
  );
  const checks: FrankenphpOctanePreflight["checks"] = [
    {
      code: "PROJECT_FRAMEWORK",
      level:
        mode === "octane"
          ? project.framework === "laravel" ? "ok" : "error"
          : mode === "symfony"
            ? project.framework === "symfony" ? "ok" : "error"
            : "ok",
      title: mode === "octane" ? "Laravel project" : mode === "symfony" ? "Symfony project" : "FrankenPHP project",
      message:
        mode === "octane" && project.framework === "laravel"
          ? "Project framework is Laravel."
          : mode === "symfony" && project.framework === "symfony"
            ? "Project framework is Symfony."
            : mode === "custom"
              ? "Custom worker mode can be used by this FrankenPHP project."
              : `${mode === "octane" ? "Laravel Octane" : "Symfony"} Worker mode is not available for this framework.`,
      suggestion:
        mode === "custom" ||
        (mode === "octane" && project.framework === "laravel") ||
        (mode === "symfony" && project.framework === "symfony")
          ? null
          : "Use Classic mode or switch to a matching project framework.",
      blocking:
        (mode === "octane" && project.framework !== "laravel") ||
        (mode === "symfony" && project.framework !== "symfony"),
    },
    {
      code: "SERVER_TYPE",
      level: project.serverType === "frankenphp" ? "ok" : "error",
      title: "FrankenPHP runtime lane",
      message:
        project.serverType === "frankenphp"
          ? "Project is configured for FrankenPHP."
          : "Worker mode must stay behind FrankenPHP.",
      suggestion: project.serverType === "frankenphp" ? null : "Change the project server to FrankenPHP.",
      blocking: project.serverType !== "frankenphp",
    },
    {
      code: "OCTANE_PACKAGE",
      level:
        mode === "octane"
          ? project.name.toLowerCase().includes("octane") ? "ok" : "error"
          : mode === "symfony"
            ? project.name.toLowerCase().includes("runtime") || project.name.toLowerCase().includes("symfony") ? "ok" : "error"
            : settings.customWorkerRelativePath ? "ok" : "error",
      title:
        mode === "octane"
          ? "Laravel Octane package"
          : mode === "symfony"
            ? "Symfony Runtime"
            : "Custom worker file",
      message:
        mode === "octane"
          ? project.name.toLowerCase().includes("octane")
            ? "`laravel/octane` is present in preview metadata."
            : "`laravel/octane` is not installed yet."
          : mode === "symfony"
            ? project.name.toLowerCase().includes("runtime") || project.name.toLowerCase().includes("symfony")
              ? "Symfony Runtime support is present in preview metadata."
              : "Symfony Runtime support for FrankenPHP was not detected."
            : settings.customWorkerRelativePath
              ? `${settings.customWorkerRelativePath} is selected.`
              : "No custom worker file is selected.",
      suggestion:
        mode === "octane" && !project.name.toLowerCase().includes("octane")
          ? "Run the shown Composer command in the project terminal."
          : mode === "symfony" && !(project.name.toLowerCase().includes("runtime") || project.name.toLowerCase().includes("symfony"))
            ? "Run the shown Composer command in the project terminal."
            : mode === "custom" && !settings.customWorkerRelativePath
              ? "Choose a project-relative PHP worker file in Project Settings."
              : null,
      blocking:
        (mode === "octane" && !project.name.toLowerCase().includes("octane")) ||
        (mode === "symfony" && !(project.name.toLowerCase().includes("runtime") || project.name.toLowerCase().includes("symfony"))) ||
        (mode === "custom" && !settings.customWorkerRelativePath),
    },
    {
      code: "FRANKENPHP_RUNTIME",
      level: runtime?.status === "available" ? "ok" : "error",
      title: "Active FrankenPHP runtime",
      message: runtime
        ? `FrankenPHP ${runtime.version} is linked at ${runtime.path}.`
        : "No active FrankenPHP runtime is linked.",
      suggestion: runtime ? null : "Link or activate FrankenPHP in Settings before starting Octane.",
      blocking: runtime?.status !== "available",
    },
    {
      code: "WORKER_PORT",
      level: workerConflict ? "error" : "ok",
      title: "Worker port",
      message: workerConflict
        ? `Port ${settings.workerPort} is already used by ${workerConflict.name}.`
        : `Port ${settings.workerPort} is available.`,
      suggestion: workerConflict ? "Choose another managed worker port." : null,
      blocking: Boolean(workerConflict),
    },
    {
      code: "ADMIN_PORT",
      level: adminConflict ? "error" : "ok",
      title: "Admin port",
      message: adminConflict
        ? `Port ${settings.adminPort} is already used by ${adminConflict.name}.`
        : `Port ${settings.adminPort} is available.`,
      suggestion: adminConflict ? "Choose another managed admin port." : null,
      blocking: Boolean(adminConflict),
    },
  ];
  const ready = checks.every((item) => !item.blocking);
  return {
    projectId,
    mode,
    ready,
    summary: ready
      ? `${mode === "octane" ? "Laravel Octane" : mode === "symfony" ? "Symfony Worker" : "Custom Worker"} is ready to start behind FrankenPHP.`
      : `Fix the blocking ${mode === "octane" ? "Octane" : mode} checks before starting the worker.`,
    installCommands: checks.some((item) => item.title === "Laravel Octane package" && item.blocking)
      ? ["composer require laravel/octane"]
      : checks.some((item) => item.title === "Symfony Runtime" && item.blocking)
        ? ["composer require runtime/frankenphp-symfony"]
      : [],
    checks,
    generatedAt: new Date().toISOString(),
  };
}

function getMockProjectOrThrow(projectId: string): Project {
  const project = readMockProjects().find((item) => item.id === projectId);

  if (!project) {
    throw {
      code: "PROJECT_NOT_FOUND",
      message: "Project not found.",
    } satisfies AppError;
  }

  return project;
}

function buildMockConfigPreview(project: Project) {
  const documentRoot =
    project.documentRoot === "."
      ? `${project.path.replace(/\\/g, "/")}`
      : `${project.path.replace(/\\/g, "/")}/${project.documentRoot.replace(/\\/g, "/")}`;
  const logsBase = `.devnest/managed-configs/logs/${project.domain}-${project.serverType}`;
  const sslBase = `.devnest/ssl/${project.domain}`;

  if ((project.framework === "laravel" || project.framework === "symfony") && project.documentRoot !== "public") {
    throw {
      code: "CONFIG_INVALID_DOCUMENT_ROOT",
      message: "Laravel and Symfony projects must use `public` as the document root for generated local config.",
    } satisfies AppError;
  }

  if (project.serverType === "frankenphp") {
    const outputPath = `.devnest/managed-configs/frankenphp/sites/${project.domain}.caddy`;
    const tlsBlock = project.sslEnabled
      ? `
    tls ${sslBase}/cert.pem ${sslBase}/key.pem
`
      : "";
    const octaneSettings = getMockFrankenphpOctaneSettings(project.id);
    const forwardedProto = project.sslEnabled ? "https" : "http";
    const forwardedPort = project.sslEnabled ? "443" : "80";
    const handlerBlock =
      project.frankenphpMode !== "classic"
        ? `    reverse_proxy 127.0.0.1:${octaneSettings.workerPort} {
        header_up Host {host}
        header_up X-Forwarded-Host {host}
        header_up X-Forwarded-Proto ${forwardedProto}
        header_up X-Forwarded-Port ${forwardedPort}
    }`
        : `    php_server
    file_server`;

    return {
      serverType: "frankenphp" as const,
      outputPath,
      configText: `${project.domain} {
    root * ${documentRoot}
    encode zstd gzip
${tlsBlock}${handlerBlock}

    log {
        output file ${logsBase}-access.log
        format console
    }

    # Managed FrankenPHP site preview for PHP ${project.phpVersion}
}
`,
    };
  }

  const outputPath = `.devnest/managed-configs/${project.serverType}/sites/${project.domain}.conf`;

  if (project.serverType === "apache") {
    const httpsBlock = project.sslEnabled
      ? `
<VirtualHost *:443>
    ServerName ${project.domain}
    DocumentRoot "${documentRoot}"

    <Directory "${documentRoot}">
        AllowOverride All
        Require all granted
        Options Indexes FollowSymLinks
    </Directory>

    SSLEngine on
    SSLCertificateFile "${sslBase}/cert.pem"
    SSLCertificateKeyFile "${sslBase}/key.pem"

    ErrorLog "${logsBase}-error.log"
    CustomLog "${logsBase}-access.log" combined
</VirtualHost>
`
      : "";

    return {
      serverType: "apache" as const,
      outputPath,
      configText: `<VirtualHost *:80>
    ServerName ${project.domain}
    DocumentRoot "${documentRoot}"

    <Directory "${documentRoot}">
        AllowOverride All
        Require all granted
        Options Indexes FollowSymLinks
    </Directory>

    ErrorLog "${logsBase}-error.log"
    CustomLog "${logsBase}-access.log" combined

    # PHP runtime placeholder
    # Configure PHP ${project.phpVersion} handler mapping in the managed Apache runtime.
</VirtualHost>
${httpsBlock}
`,
    };
  }

  const httpsBlock = project.sslEnabled
    ? `
server {
    listen 443 ssl;
    server_name ${project.domain};
    root ${documentRoot};
    index index.php index.html index.htm;

    ssl_certificate ${sslBase}/cert.pem;
    ssl_certificate_key ${sslBase}/key.pem;
    access_log ${logsBase}-access.log;
    error_log ${logsBase}-error.log warn;

    location / {
        ${project.framework === "wordpress" ? "try_files $uri $uri/ /index.php?$args;" : "try_files $uri $uri/ /index.php?$query_string;"}
    }
}
`
    : "";

  return {
    serverType: "nginx" as const,
    outputPath,
    configText: `server {
    listen 80;
    server_name ${project.domain};
    root ${documentRoot};
    index index.php index.html index.htm;

    access_log ${logsBase}-access.log;
    error_log ${logsBase}-error.log warn;

    location / {
        ${project.framework === "wordpress" ? "try_files $uri $uri/ /index.php?$args;" : "try_files $uri $uri/ /index.php?$query_string;"}
    }

    location ~ \\.php$ {
        include fastcgi_params;
        fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
        # PHP runtime placeholder for version ${project.phpVersion}
        fastcgi_pass 127.0.0.1:9000;
    }
}
${httpsBlock}
`,
  };
}

function getMockResponse<T>(command: string): T {
  switch (command) {
    case "get_boot_state":
      return buildMockBootState() as T;
    case "get_workspace_overview":
      return buildMockWorkspaceOverview() as T;
    case "get_workspace_port_summary":
      return buildMockWorkspacePortSummary() as T;
    case "ping":
      return "pong:browser" as T;
    case "get_app_release_info":
      return {
        appName: "DevNest",
        currentVersion: "0.1.0",
        releaseChannel: "stable",
        updateEndpoint: BROWSER_PREVIEW_UPDATE_ENDPOINT,
        updaterConfigured: true,
      } as T;
    case "list_projects":
      return readMockProjects() as T;
    case "get_all_service_status":
      return readMockServices() as T;
    case "list_runtime_inventory":
      return readMockRuntimes() as T;
    case "list_runtime_packages":
      return readMockRuntimePackages() as T;
    case "get_runtime_install_task":
      return readMockRuntimeInstallTask() as T;
    case "list_databases":
      return [] as T;
    default:
      throw {
        code: "TAURI_RUNTIME_UNAVAILABLE",
        message: "Desktop runtime is not available in preview mode.",
      } satisfies AppError;
  }
}

function getMockResponseWithArgs<T>(command: string, args?: Record<string, unknown>): T {
  const timestamp = new Date().toISOString();

  switch (command) {
    case "check_for_app_update":
      return {
        status: "updateAvailable",
        currentVersion: "0.1.0",
        latestVersion: "0.1.1",
        releaseChannel: "stable",
        checkedAt: timestamp,
        notes:
          "Updater wiring, release metadata delivery, and the Settings update flow are ready for packaged builds.",
        pubDate: timestamp,
        updateEndpoint: BROWSER_PREVIEW_UPDATE_ENDPOINT,
      } as T;
    case "install_app_update":
      return {
        status: "restartRequired",
        targetVersion: "0.1.1",
      } as T;
    case "get_project": {
      const projectId = String(args?.projectId ?? "");
      return getMockProjectOrThrow(projectId) as T;
    }
    case "open_project_folder":
    case "open_project_terminal":
    case "open_project_vscode": {
      const projectId = String(args?.projectId ?? "");
      const project = getMockProjectOrThrow(projectId);
      const url =
        command === "open_project_folder"
          ? `file://${project.path}`
          : command === "open_project_vscode"
            ? `vscode://file/${project.path.replace(/\\/g, "/")}`
            : "";

      if (typeof window !== "undefined") {
        if (url) {
          window.open(url, "_blank", "noopener,noreferrer");
        }
      }

      return { success: true } as T;
    }
    case "pick_project_folder":
      return null as T;
    case "scan_project": {
      const path = String(args?.path ?? "");
      const pathResult = projectPathSchema.safeParse(path);
      if (!pathResult.success) {
        throw {
          code: "INVALID_PROJECT_PATH",
          message: pathResult.error.issues[0]?.message ?? "Project path is required.",
        } satisfies AppError;
      }
      return inferFrameworkFromPath(path) as T;
    }
    case "create_project": {
      const input = args?.input as CreateProjectInput | undefined;

      if (!input) {
        throw {
          code: "INVALID_INPUT",
          message: "Project input is required.",
        } satisfies AppError;
      }

      validateMockProjectInput(input);

      const existing = readMockProjects();

      if (existing.some((project) => project.domain === input.domain)) {
        throw {
          code: "DOMAIN_ALREADY_EXISTS",
          message: "A project with this domain already exists.",
        } satisfies AppError;
      }

      if (existing.some((project) => project.path === input.path)) {
        throw {
          code: "PROJECT_PATH_EXISTS",
          message: "A project with this path already exists.",
        } satisfies AppError;
      }

      const created: Project = {
        id: crypto.randomUUID(),
        name: input.name,
        path: input.path,
        domain: input.domain,
        serverType: input.serverType,
        phpVersion: input.phpVersion,
        framework: input.framework,
        documentRoot: input.documentRoot,
        sslEnabled: input.sslEnabled,
        databaseName: input.databaseName ?? null,
        databasePort: input.databasePort ?? null,
        status: "stopped",
        frankenphpMode: input.frankenphpMode ?? "classic",
        createdAt: timestamp,
        updatedAt: timestamp,
      };

      writeMockProjects([created, ...existing]);
      if (created.databaseName) {
        const nextDatabases = Array.from(new Set([...readMockDatabases(), created.databaseName])).sort(
          (left, right) => left.localeCompare(right),
        );
        writeMockDatabases(nextDatabases);
      }
      return created as T;
    }
    case "create_laravel_recipe": {
      const input = args?.input as {
        path?: string;
        domain?: string;
        phpVersion?: string;
        serverType?: CreateProjectInput["serverType"];
        sslEnabled?: boolean;
      };

      return createMockRecipeProject({
        path: String(input?.path ?? ""),
        domain: String(input?.domain ?? ""),
        phpVersion: String(input?.phpVersion ?? "8.4"),
        serverType: (input?.serverType ?? "apache") as CreateProjectInput["serverType"],
        sslEnabled: Boolean(input?.sslEnabled),
        framework: "laravel",
        documentRoot: "public",
      }) as T;
    }
    case "create_wordpress_recipe": {
      const input = args?.input as {
        path?: string;
        domain?: string;
        phpVersion?: string;
        serverType?: CreateProjectInput["serverType"];
        sslEnabled?: boolean;
      };

      return createMockRecipeProject({
        path: String(input?.path ?? ""),
        domain: String(input?.domain ?? ""),
        phpVersion: String(input?.phpVersion ?? "8.4"),
        serverType: (input?.serverType ?? "apache") as CreateProjectInput["serverType"],
        sslEnabled: Boolean(input?.sslEnabled),
        framework: "wordpress",
        documentRoot: ".",
      }) as T;
    }
    case "clone_git_recipe": {
      const input = args?.input as {
        repositoryUrl?: string;
        path?: string;
        domain?: string;
        phpVersion?: string;
        serverType?: CreateProjectInput["serverType"];
        sslEnabled?: boolean;
      };
      const repositoryCheck = gitRepositoryUrlSchema.safeParse(String(input?.repositoryUrl ?? ""));
      if (!repositoryCheck.success) {
        throw {
          code: "INVALID_REPOSITORY_URL",
          message: repositoryCheck.error.issues[0]?.message ?? "Repository URL is invalid.",
        } satisfies AppError;
      }

      const inferred = inferFrameworkFromPath(String(input?.path ?? ""));
      return createMockRecipeProject({
        path: String(input?.path ?? ""),
        domain: String(input?.domain ?? ""),
        phpVersion: String(input?.phpVersion ?? "8.4"),
        serverType: (input?.serverType ?? inferred.recommendedServer) as CreateProjectInput["serverType"],
        sslEnabled: Boolean(input?.sslEnabled),
        framework: inferred.framework,
        documentRoot: inferred.documentRoot,
      }) as T;
    }
    case "update_project": {
      const projectId = String(args?.projectId ?? "");
      const patch = (args?.patch ?? {}) as UpdateProjectPatch;
      const existing = readMockProjects();
      const project = existing.find((item) => item.id === projectId);

      if (!project) {
        throw {
          code: "PROJECT_NOT_FOUND",
          message: "Project not found.",
        } satisfies AppError;
      }

      validateMockProjectInput({
        name: patch.name ?? project.name,
        path: project.path,
        domain: patch.domain ?? project.domain,
        serverType: patch.serverType ?? project.serverType,
        phpVersion: patch.phpVersion ?? project.phpVersion,
        framework: patch.framework ?? project.framework,
        documentRoot: patch.documentRoot ?? project.documentRoot,
        sslEnabled: patch.sslEnabled ?? project.sslEnabled,
        databaseName:
          patch.databaseName === undefined ? project.databaseName ?? null : patch.databaseName,
        databasePort:
          patch.databasePort === undefined ? project.databasePort ?? null : patch.databasePort,
        frankenphpMode: patch.frankenphpMode ?? project.frankenphpMode ?? "classic",
      });

      const updated: Project = {
        ...project,
        ...patch,
        updatedAt: timestamp,
      };

      const nextDatabaseName =
        patch.databaseName === undefined ? project.databaseName ?? null : patch.databaseName;
      if ((project.databaseName ?? null) !== nextDatabaseName) {
        takeMockProjectLinkedSnapshotIfEnabled(project, `before relinking project ${project.name}`);
      }

      writeMockProjects(existing.map((item) => (item.id === projectId ? updated : item)));
      return updated as T;
    }
    case "export_project_profile": {
      const projectId = String(args?.projectId ?? "");
      const project = getMockProjectOrThrow(projectId);
      const envVars = readMockProjectEnvVars()
        .filter((item) => item.projectId === projectId)
        .map((item) => ({
          envKey: item.envKey,
          envValue: item.envValue,
        }));
      const profile = {
        formatVersion: 1,
        exportedAt: timestamp,
        source: "DevNest",
        project: {
          name: project.name,
          path: project.path,
          domain: project.domain,
          serverType: project.serverType,
          phpVersion: project.phpVersion,
          framework: project.framework,
          documentRoot: project.documentRoot,
          sslEnabled: project.sslEnabled,
          databaseName: project.databaseName ?? null,
          databasePort: project.databasePort ?? null,
          frankenphpMode: project.frankenphpMode,
          envVars,
        },
      };
      writeMockLastExportedProjectProfile(profile);
      return ({
        success: true,
        path: `.mock-app-data/exports/${project.domain.replace(/\./g, "-")}.devnest-project.json`,
        warnings: [],
      }) as T;
    }
    case "export_team_project_profile": {
      const projectId = String(args?.projectId ?? "");
      const project = getMockProjectOrThrow(projectId);
      const envVars = readMockProjectEnvVars()
        .filter((item) => item.projectId === projectId)
        .map((item) => ({
          envKey: item.envKey,
          envValue: item.envValue,
        }));
      const pathSegments = project.path.split(/[\\/]/).filter(Boolean);
      const rootNameHint =
        pathSegments[pathSegments.length - 1] ?? project.name.toLowerCase();
      const profile = {
        formatVersion: 2,
        profileKind: "team-share",
        exportedAt: timestamp,
        source: "DevNest",
        project: {
          name: project.name,
          rootNameHint,
          domain: project.domain,
          serverType: project.serverType,
          phpVersion: project.phpVersion,
          framework: project.framework,
          documentRoot: project.documentRoot,
          sslEnabled: project.sslEnabled,
          databaseName: project.databaseName ?? null,
          databasePort: project.databasePort ?? null,
          frankenphpMode: project.frankenphpMode,
          envVars: [],
          envKeys: envVars.map((item) => item.envKey),
          frankenphp:
            project.serverType === "frankenphp"
              ? {
                  mode: project.frankenphpMode,
                  workerMode: project.frankenphpMode === "classic" ? null : project.frankenphpMode,
                  workerPolicy:
                    project.frankenphpMode === "classic"
                      ? null
                      : {
                          workers: getMockFrankenphpOctaneSettings(project.id).workers,
                          maxRequests: getMockFrankenphpOctaneSettings(project.id).maxRequests,
                          preferredWorkerPort: getMockFrankenphpOctaneSettings(project.id).workerPort,
                          preferredAdminPort: getMockFrankenphpOctaneSettings(project.id).adminPort,
                          customWorkerRelativePath:
                            getMockFrankenphpOctaneSettings(project.id).customWorkerRelativePath,
                        },
                  runtimeRequirements: {
                    phpFamily: project.phpVersion,
                    requiredExtensions: ["mbstring", "openssl", "pdo"],
                  },
                  localIntent: {
                    domain: project.domain,
                    sslEnabled: project.sslEnabled,
                  },
                  localDiagnostics:
                    project.frankenphpMode === "classic"
                      ? null
                      : {
                          currentWorkerPort: getMockFrankenphpOctaneSettings(project.id).workerPort,
                          currentAdminPort: getMockFrankenphpOctaneSettings(project.id).adminPort,
                          workerLogPath: getMockFrankenphpOctaneSettings(project.id).logPath,
                        },
                }
              : null,
        },
      };
      writeMockLastExportedTeamProjectProfile(profile);
      return ({
        success: true,
        path: `.mock-app-data/exports/${project.domain.replace(/\./g, "-")}.devnest-team-project.json`,
        warnings:
          envVars.length > 0
            ? [
                {
                  code: "TEAM_PROFILE_ENV_VALUES_OMITTED",
                  title: "Environment values were not exported",
                  message: "Team profiles include env key names only.",
                  suggestion: "Share secret values through your team's secret manager.",
                },
              ]
            : [],
      }) as T;
    }
    case "import_project_profile": {
      const profile = readMockLastExportedProjectProfile();
      if (!profile) {
        throw {
          code: "PROJECT_PROFILE_READ_FAILED",
          message: "No preview project profile is available to import yet. Export one first in preview mode.",
        } satisfies AppError;
      }

      const projectProfile = profile.project as {
        name: string;
        path: string;
        domain: string;
        serverType: CreateProjectInput["serverType"];
        phpVersion: string;
        framework: CreateProjectInput["framework"];
        documentRoot: string;
        sslEnabled: boolean;
        databaseName?: string | null;
        databasePort?: number | null;
        frankenphpMode?: Project["frankenphpMode"];
        envVars?: Array<{ envKey: string; envValue: string }>;
      };

      const existing = readMockProjects();
      if (existing.some((project) => project.domain === projectProfile.domain)) {
        throw {
          code: "DOMAIN_ALREADY_EXISTS",
          message: "A project with this domain already exists.",
        } satisfies AppError;
      }

      if (existing.some((project) => project.path === projectProfile.path)) {
        throw {
          code: "PROJECT_PATH_EXISTS",
          message: "A project with this path already exists.",
        } satisfies AppError;
      }

      const created: Project = {
        id: crypto.randomUUID(),
        name: projectProfile.name,
        path: projectProfile.path,
        domain: projectProfile.domain,
        serverType: projectProfile.serverType,
        phpVersion: projectProfile.phpVersion,
        framework: projectProfile.framework,
        documentRoot: projectProfile.documentRoot,
        sslEnabled: projectProfile.sslEnabled,
        databaseName: projectProfile.databaseName ?? null,
        databasePort: projectProfile.databasePort ?? null,
        status: "stopped",
        frankenphpMode: projectProfile.frankenphpMode ?? "classic",
        createdAt: timestamp,
        updatedAt: timestamp,
      };

      writeMockProjects([created, ...existing]);
      const envVars = projectProfile.envVars ?? [];
      if (envVars.length > 0) {
        const currentEnvVars = readMockProjectEnvVars();
        const importedEnvVars = envVars.map((item) => ({
          id: crypto.randomUUID(),
          projectId: created.id,
          envKey: item.envKey.toUpperCase(),
          envValue: item.envValue,
          createdAt: timestamp,
          updatedAt: timestamp,
        }));
        writeMockProjectEnvVars([...currentEnvVars, ...importedEnvVars]);
      }

      return { project: created, warnings: [] } as T;
    }
    case "import_team_project_profile": {
      const profile = readMockLastExportedTeamProjectProfile();
      if (!profile) {
        throw {
          code: "PROJECT_PROFILE_READ_FAILED",
          message:
            "No preview team-share profile is available to import yet. Export one first in preview mode.",
        } satisfies AppError;
      }

      const projectProfile = profile.project as {
        name: string;
        rootNameHint: string;
        domain: string;
        serverType: CreateProjectInput["serverType"];
        phpVersion: string;
        framework: CreateProjectInput["framework"];
        documentRoot: string;
        sslEnabled: boolean;
        databaseName?: string | null;
        databasePort?: number | null;
        frankenphpMode?: Project["frankenphpMode"];
        envVars?: Array<{ envKey: string; envValue: string }>;
        envKeys?: string[];
        frankenphp?: {
          workerPolicy?: {
            workers?: number;
            maxRequests?: number;
            preferredWorkerPort?: number;
            preferredAdminPort?: number;
            customWorkerRelativePath?: string | null;
          } | null;
        } | null;
      };
      const nextPath = `${MOCK_SHARED_ROOT}/${projectProfile.rootNameHint}`;

      const existing = readMockProjects();
      if (existing.some((project) => project.domain === projectProfile.domain)) {
        throw {
          code: "DOMAIN_ALREADY_EXISTS",
          message: "A project with this domain already exists.",
        } satisfies AppError;
      }

      if (existing.some((project) => project.path === nextPath)) {
        throw {
          code: "PROJECT_PATH_EXISTS",
          message: "A project with this path already exists.",
        } satisfies AppError;
      }

      const created: Project = {
        id: crypto.randomUUID(),
        name: projectProfile.name,
        path: nextPath,
        domain: projectProfile.domain,
        serverType: projectProfile.serverType,
        phpVersion: projectProfile.phpVersion,
        framework: projectProfile.framework,
        documentRoot: projectProfile.documentRoot,
        sslEnabled: projectProfile.sslEnabled,
        databaseName: projectProfile.databaseName ?? null,
        databasePort: projectProfile.databasePort ?? null,
        status: "stopped",
        frankenphpMode: projectProfile.frankenphpMode ?? "classic",
        createdAt: timestamp,
        updatedAt: timestamp,
      };

      writeMockProjects([created, ...existing]);
      const envVars = projectProfile.envVars ?? [];
      if (envVars.length > 0) {
        const currentEnvVars = readMockProjectEnvVars();
        const importedEnvVars = envVars.map((item) => ({
          id: crypto.randomUUID(),
          projectId: created.id,
          envKey: item.envKey.toUpperCase(),
          envValue: item.envValue,
          createdAt: timestamp,
          updatedAt: timestamp,
        }));
        writeMockProjectEnvVars([...currentEnvVars, ...importedEnvVars]);
      }

      const warnings =
        projectProfile.serverType === "frankenphp"
          ? [
              {
                code: "FRANKENPHP_PORTS_PORTABLE_WARNING",
                title: "Worker ports are local preferences",
                message:
                  "The shared profile may include source-machine worker/admin ports for diagnostics only.",
                suggestion: "Check the Runtime tab after import if ports conflict locally.",
              },
            ]
          : [];

      return { project: created, warnings } as T;
    }
    case "preview_frankenphp_production_export": {
      return buildMockFrankenphpProductionExportPreview(String(args?.projectId ?? "")) as T;
    }
    case "write_frankenphp_production_export": {
      const preview = buildMockFrankenphpProductionExportPreview(String(args?.projectId ?? ""));
      return {
        success: true,
        path: `${MOCK_APP_DATA_ROOT}/production-exports/${preview.slug}`,
        warnings: preview.warnings,
        files: preview.files.map((file) => `${MOCK_APP_DATA_ROOT}/production-exports/${preview.slug}/${file.relativePath}`),
      } satisfies FrankenphpProductionExportWriteResult as T;
    }
    case "list_project_env_vars": {
      const projectId = String(args?.projectId ?? "");
      getMockProjectOrThrow(projectId);
      return readMockProjectEnvVars()
        .filter((item) => item.projectId === projectId)
        .sort((left, right) => left.envKey.localeCompare(right.envKey)) as T;
    }
    case "inspect_project_env": {
      const projectId = String(args?.projectId ?? "");
      return buildMockProjectEnvInspection(projectId) as T;
    }
    case "create_project_env_var": {
      const input = args?.input as
        | { projectId: string; envKey: string; envValue: string }
        | undefined;
      if (!input) {
        throw {
          code: "INVALID_INPUT",
          message: "Project env var input is required.",
        } satisfies AppError;
      }

      getMockProjectOrThrow(input.projectId);
      const envKey = String(input.envKey ?? "").trim().toUpperCase();
      const envValue = String(input.envValue ?? "");
      if (!/^[A-Z][A-Z0-9_]{0,63}$/.test(envKey)) {
        throw {
          code: "INVALID_ENV_KEY",
          message: "Use letters, numbers, and underscores, starting with a letter.",
        } satisfies AppError;
      }

      const envVars = readMockProjectEnvVars();
      if (envVars.some((item) => item.projectId === input.projectId && item.envKey === envKey)) {
        throw {
          code: "PROJECT_ENV_KEY_EXISTS",
          message: "This project already tracks an environment variable with that key.",
        } satisfies AppError;
      }

      const created: ProjectEnvVar = {
        id: crypto.randomUUID(),
        projectId: input.projectId,
        envKey,
        envValue,
        createdAt: timestamp,
        updatedAt: timestamp,
      };
      writeMockProjectEnvVars([...envVars, created]);
      return created as T;
    }
    case "update_project_env_var": {
      const input = args?.input as
        | { projectId: string; envVarId: string; envKey: string; envValue: string }
        | undefined;
      if (!input) {
        throw {
          code: "INVALID_INPUT",
          message: "Project env var input is required.",
        } satisfies AppError;
      }

      getMockProjectOrThrow(input.projectId);
      const envKey = String(input.envKey ?? "").trim().toUpperCase();
      const envValue = String(input.envValue ?? "");
      if (!/^[A-Z][A-Z0-9_]{0,63}$/.test(envKey)) {
        throw {
          code: "INVALID_ENV_KEY",
          message: "Use letters, numbers, and underscores, starting with a letter.",
        } satisfies AppError;
      }

      const envVars = readMockProjectEnvVars();
      const current = envVars.find(
        (item) => item.projectId === input.projectId && item.id === input.envVarId,
      );
      if (!current) {
        throw {
          code: "PROJECT_ENV_VAR_NOT_FOUND",
          message: "Project environment variable was not found.",
        } satisfies AppError;
      }

      if (
        envVars.some(
          (item) =>
            item.projectId === input.projectId &&
            item.envKey === envKey &&
            item.id !== input.envVarId,
        )
      ) {
        throw {
          code: "PROJECT_ENV_KEY_EXISTS",
          message: "This project already tracks an environment variable with that key.",
        } satisfies AppError;
      }

      const updated: ProjectEnvVar = {
        ...current,
        envKey,
        envValue,
        updatedAt: timestamp,
      };
      writeMockProjectEnvVars(envVars.map((item) => (item.id === updated.id ? updated : item)));
      return updated as T;
    }
    case "delete_project_env_var": {
      const projectId = String(args?.projectId ?? "");
      const envVarId = String(args?.envVarId ?? "");
      const envVars = readMockProjectEnvVars();
      const current = envVars.find((item) => item.projectId === projectId && item.id === envVarId);
      if (!current) {
        throw {
          code: "PROJECT_ENV_VAR_NOT_FOUND",
          message: "Project environment variable was not found.",
        } satisfies AppError;
      }
      writeMockProjectEnvVars(
        envVars.filter((item) => !(item.projectId === projectId && item.id === envVarId)),
      );
      return { success: true } as T;
    }
    case "list_databases": {
      const mysqlService = getMockServiceOrThrow("mysql");
      if (mysqlService.status !== "running") {
        throw {
          code: "DATABASE_SERVICE_STOPPED",
          message: "Start MySQL before managing databases.",
        } satisfies AppError;
      }

      return readMockDatabases() as T;
    }
    case "create_database": {
      const mysqlService = getMockServiceOrThrow("mysql");
      if (mysqlService.status !== "running") {
        throw {
          code: "DATABASE_SERVICE_STOPPED",
          message: "Start MySQL before managing databases.",
        } satisfies AppError;
      }

      const name = String(args?.name ?? "").trim();
      if (!/^[a-zA-Z0-9_-]{1,64}$/.test(name)) {
        throw {
          code: "INVALID_DATABASE_NAME",
          message: "Use only letters, numbers, underscores, and dashes.",
        } satisfies AppError;
      }

      const databases = readMockDatabases();
      if (databases.includes(name)) {
        throw {
          code: "DATABASE_ALREADY_EXISTS",
          message: "A database with this name already exists.",
        } satisfies AppError;
      }

      writeMockDatabases([...databases, name].sort((left, right) => left.localeCompare(right)));
      return { success: true, name } as T;
    }
    case "get_database_time_machine_status": {
      const mysqlService = getMockServiceOrThrow("mysql");
      if (mysqlService.status !== "running") {
        throw {
          code: "DATABASE_SERVICE_STOPPED",
          message: "Start MySQL before managing databases.",
        } satisfies AppError;
      }

      const name = String(args?.name ?? "").trim();
      ensureMockDatabaseExists(name);
      return mockDatabaseTimeMachineStatus(name) as T;
    }
    case "enable_database_time_machine": {
      const mysqlService = getMockServiceOrThrow("mysql");
      if (mysqlService.status !== "running") {
        throw {
          code: "DATABASE_SERVICE_STOPPED",
          message: "Start MySQL before managing databases.",
        } satisfies AppError;
      }

      const name = String(args?.name ?? "").trim();
      ensureMockDatabaseExists(name);
      const statuses = readMockDatabaseTimeMachine();
      statuses[name] = {
        ...(statuses[name] ?? defaultMockDatabaseTimeMachineState(true)),
        enabled: true,
        updatedAt: timestamp,
      };
      writeMockDatabaseTimeMachine(statuses);
      return mockDatabaseTimeMachineStatus(name) as T;
    }
    case "disable_database_time_machine": {
      const mysqlService = getMockServiceOrThrow("mysql");
      if (mysqlService.status !== "running") {
        throw {
          code: "DATABASE_SERVICE_STOPPED",
          message: "Start MySQL before managing databases.",
        } satisfies AppError;
      }

      const name = String(args?.name ?? "").trim();
      ensureMockDatabaseExists(name);
      const statuses = readMockDatabaseTimeMachine();
      statuses[name] = {
        ...(statuses[name] ?? defaultMockDatabaseTimeMachineState(false)),
        enabled: false,
        updatedAt: timestamp,
      };
      writeMockDatabaseTimeMachine(statuses);
      return mockDatabaseTimeMachineStatus(name) as T;
    }
    case "take_database_snapshot": {
      const mysqlService = getMockServiceOrThrow("mysql");
      if (mysqlService.status !== "running") {
        throw {
          code: "DATABASE_SERVICE_STOPPED",
          message: "Start MySQL before managing databases.",
        } satisfies AppError;
      }

      const name = String(args?.name ?? "").trim();
      ensureMockDatabaseExists(name);
      return createMockSnapshot(name, "manual") as T;
    }
    case "list_database_snapshots": {
      const mysqlService = getMockServiceOrThrow("mysql");
      if (mysqlService.status !== "running") {
        throw {
          code: "DATABASE_SERVICE_STOPPED",
          message: "Start MySQL before managing databases.",
        } satisfies AppError;
      }

      const name = String(args?.name ?? "").trim();
      ensureMockDatabaseExists(name);
      return ((readMockDatabaseSnapshots()[name] ?? []).sort((left, right) =>
        right.createdAt.localeCompare(left.createdAt),
      )) as T;
    }
    case "rollback_database_snapshot": {
      const mysqlService = getMockServiceOrThrow("mysql");
      if (mysqlService.status !== "running") {
        throw {
          code: "DATABASE_SERVICE_STOPPED",
          message: "Start MySQL before managing databases.",
        } satisfies AppError;
      }

      const name = String(args?.name ?? "").trim();
      const snapshotId = String(args?.snapshotId ?? "").trim();
      ensureMockDatabaseExists(name);
      const status = mockDatabaseTimeMachineStatus(name);
      if (!status.enabled) {
        throw {
          code: "DATABASE_TIME_MACHINE_DISABLED",
          message: "Enable Time Machine before rolling a database back to a managed snapshot.",
        } satisfies AppError;
      }

      const snapshots = readMockDatabaseSnapshots()[name] ?? [];
      const target = snapshots.find((snapshot) => snapshot.id === snapshotId);
      if (!target) {
        throw {
          code: "DATABASE_SNAPSHOT_NOT_FOUND",
          message: "The selected managed database snapshot does not exist anymore.",
        } satisfies AppError;
      }

      const safetySnapshot = takeMockPreActionSnapshotIfEnabled(name, "before rollback");
      return {
        success: true,
        name,
        snapshotId: target.id,
        restoredAt: new Date().toISOString(),
        restoredSnapshot: target,
        safetySnapshotId: safetySnapshot?.id ?? null,
      } satisfies DatabaseSnapshotRollbackResult as T;
    }
    case "drop_database": {
      const mysqlService = getMockServiceOrThrow("mysql");
      if (mysqlService.status !== "running") {
        throw {
          code: "DATABASE_SERVICE_STOPPED",
          message: "Start MySQL before managing databases.",
        } satisfies AppError;
      }

      const name = String(args?.name ?? "").trim();
      const dependentProjects = readMockProjects().filter((project) => project.databaseName === name);
      if (dependentProjects.length > 0) {
        throw {
          code: "DATABASE_IN_USE",
          message: "Unlink this database from tracked projects before deleting it.",
          details: dependentProjects.map((project) => `${project.name} (${project.domain})`).join(", "),
        } satisfies AppError;
      }

      const databases = readMockDatabases();
      if (!databases.includes(name)) {
        throw {
          code: "DATABASE_NOT_FOUND",
          message: "The selected database does not exist anymore.",
        } satisfies AppError;
      }

      writeMockDatabases(databases.filter((database) => database !== name));
      const snapshots = readMockDatabaseSnapshots();
      delete snapshots[name];
      writeMockDatabaseSnapshots(snapshots);
      const statuses = readMockDatabaseTimeMachine();
      delete statuses[name];
      writeMockDatabaseTimeMachine(statuses);
      return { success: true, name } as T;
    }
    case "backup_database": {
      const mysqlService = getMockServiceOrThrow("mysql");
      if (mysqlService.status !== "running") {
        throw {
          code: "DATABASE_SERVICE_STOPPED",
          message: "Start MySQL before managing databases.",
        } satisfies AppError;
      }

      const name = String(args?.name ?? "").trim();
      const databases = readMockDatabases();
      if (!databases.includes(name)) {
        throw {
          code: "DATABASE_NOT_FOUND",
          message: "The selected database does not exist anymore.",
        } satisfies AppError;
      }

      return {
        success: true,
        name,
        path: `${MOCK_APP_DATA_ROOT}/backups/${name}.sql`,
      } as T;
    }
    case "restore_database": {
      const mysqlService = getMockServiceOrThrow("mysql");
      if (mysqlService.status !== "running") {
        throw {
          code: "DATABASE_SERVICE_STOPPED",
          message: "Start MySQL before managing databases.",
        } satisfies AppError;
      }

      const name = String(args?.name ?? "").trim();
      const databases = readMockDatabases();
      if (!databases.includes(name)) {
        throw {
          code: "DATABASE_NOT_FOUND",
          message: "Create the target database before restoring a SQL backup into it.",
        } satisfies AppError;
      }

      takeMockPreActionSnapshotIfEnabled(name, "before restore");

      return {
        success: true,
        name,
        path: `${MOCK_APP_DATA_ROOT}/backups/${name}.sql`,
      } as T;
    }
    case "delete_project": {
      const projectId = String(args?.projectId ?? "");
      const project = getMockProjectOrThrow(projectId);
      takeMockProjectLinkedSnapshotIfEnabled(project, `before deleting project ${project.name}`);
      writeMockProjects(readMockProjects().filter((item) => item.id !== projectId));
      writeMockProjectEnvVars(
        readMockProjectEnvVars().filter((item) => item.projectId !== projectId),
      );
      writeMockProjectWorkers(
        readMockProjectWorkers().filter((item) => item.projectId !== projectId),
      );
      const removedTaskIds = readMockProjectScheduledTasks()
        .filter((item) => item.projectId === projectId)
        .map((item) => item.id);
      const removedRunIds = readMockProjectScheduledTaskRuns()
        .filter((item) => item.projectId === projectId || removedTaskIds.includes(item.taskId))
        .map((item) => item.id);
      writeMockProjectScheduledTasks(
        readMockProjectScheduledTasks().filter((item) => item.projectId !== projectId),
      );
      writeMockProjectScheduledTaskRuns(
        readMockProjectScheduledTaskRuns().filter(
          (item) => item.projectId !== projectId && !removedTaskIds.includes(item.taskId),
        ),
      );
      const logs = readMockServiceLogs();
      removedRunIds.forEach((runId) => {
        delete logs[`task-run:${runId}`];
      });
      writeMockServiceLogs(logs);
      return { success: true } as T;
    }
    case "list_project_scheduled_tasks": {
      const projectId = String(args?.projectId ?? "");
      return readMockProjectScheduledTasks()
        .filter((task) => task.projectId === projectId)
        .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt)) as T;
    }
    case "list_all_scheduled_tasks":
      return [...readMockProjectScheduledTasks()].sort((left, right) =>
        right.updatedAt.localeCompare(left.updatedAt),
      ) as T;
    case "create_project_scheduled_task": {
      const input = args?.input as CreateProjectScheduledTaskInput | undefined;
      if (!input) {
        throw {
          code: "INVALID_INPUT",
          message: "Scheduled task input is required.",
        } satisfies AppError;
      }

      const project = getMockProjectOrThrow(input.projectId);
      const parsedCommand = input.commandLine ? parseMockWorkerCommandLine(input.commandLine) : null;
      const scheduleExpression = mockScheduledTaskScheduleExpression({
        scheduleMode: input.scheduleMode,
        simpleScheduleKind: input.simpleScheduleKind ?? undefined,
        scheduleExpression: input.scheduleExpression ?? null,
        intervalSeconds: input.intervalSeconds ?? null,
        dailyTime: input.dailyTime ?? null,
        weeklyDay: input.weeklyDay ?? null,
      });
      const created: ProjectScheduledTask = {
        id: crypto.randomUUID(),
        projectId: project.id,
        name: input.name.trim(),
        taskType: input.taskType,
        scheduleMode: input.scheduleMode,
        simpleScheduleKind: input.scheduleMode === "simple" ? input.simpleScheduleKind ?? null : null,
        scheduleExpression,
        intervalSeconds: input.intervalSeconds ?? null,
        dailyTime: input.dailyTime ?? null,
        weeklyDay: input.weeklyDay ?? null,
        url: input.taskType === "url" ? input.url?.trim() ?? null : null,
        command: input.taskType === "command" ? parsedCommand?.command ?? null : null,
        args: input.taskType === "command" ? parsedCommand?.args ?? [] : [],
        workingDirectory:
          input.taskType === "command" ? (input.workingDirectory ?? project.path).trim() : null,
        enabled: Boolean(input.enabled),
        autoResume: Boolean(input.autoResume),
        overlapPolicy: "skip_if_running",
        status: input.enabled ? "scheduled" : "idle",
        nextRunAt: input.enabled
          ? mockNextScheduledTaskRunAt(
              {
                enabled: true,
                scheduleMode: input.scheduleMode,
                simpleScheduleKind: input.simpleScheduleKind ?? null,
                intervalSeconds: input.intervalSeconds ?? null,
                dailyTime: input.dailyTime ?? null,
                weeklyDay: input.weeklyDay ?? null,
              } satisfies Pick<
                ProjectScheduledTask,
                "enabled" | "scheduleMode" | "simpleScheduleKind" | "intervalSeconds" | "dailyTime" | "weeklyDay"
              >,
              timestamp,
            )
          : null,
        lastRunAt: null,
        lastSuccessAt: null,
        lastError: null,
        createdAt: timestamp,
        updatedAt: timestamp,
      };

      writeMockProjectScheduledTasks([created, ...readMockProjectScheduledTasks()]);
      return created as T;
    }
    case "update_project_scheduled_task": {
      const taskId = String(args?.taskId ?? "");
      const patch = (args?.patch ?? {}) as UpdateProjectScheduledTaskPatch;
      const current = getMockScheduledTaskOrThrow(taskId);
      const scheduleMode = patch.scheduleMode ?? current.scheduleMode;
      const simpleScheduleKind =
        patch.simpleScheduleKind === undefined
          ? current.simpleScheduleKind ?? null
          : patch.simpleScheduleKind;
      const intervalSeconds =
        patch.intervalSeconds === undefined ? current.intervalSeconds ?? null : patch.intervalSeconds;
      const dailyTime = patch.dailyTime === undefined ? current.dailyTime ?? null : patch.dailyTime;
      const weeklyDay = patch.weeklyDay === undefined ? current.weeklyDay ?? null : patch.weeklyDay;
      const parsedCommand =
        patch.commandLine !== undefined
          ? patch.commandLine
            ? parseMockWorkerCommandLine(patch.commandLine)
            : { command: "", args: [] }
          : { command: current.command ?? "", args: current.args };
      const nextTask: ProjectScheduledTask = {
        ...current,
        name: patch.name?.trim() || current.name,
        taskType: patch.taskType ?? current.taskType,
        scheduleMode,
        simpleScheduleKind: scheduleMode === "simple" ? simpleScheduleKind ?? null : null,
        scheduleExpression: mockScheduledTaskScheduleExpression({
          scheduleMode,
          simpleScheduleKind: scheduleMode === "simple" ? simpleScheduleKind ?? null : null,
          scheduleExpression:
            patch.scheduleExpression === undefined
              ? current.scheduleMode === "cron"
                ? current.scheduleExpression
                : null
              : patch.scheduleExpression,
          intervalSeconds,
          dailyTime,
          weeklyDay,
        }),
        intervalSeconds,
        dailyTime,
        weeklyDay,
        url:
          (patch.taskType ?? current.taskType) === "url"
            ? patch.url === undefined
              ? current.url ?? null
              : patch.url
            : null,
        command:
          (patch.taskType ?? current.taskType) === "command"
            ? parsedCommand.command || current.command
            : null,
        args:
          (patch.taskType ?? current.taskType) === "command"
            ? parsedCommand.args
            : [],
        workingDirectory:
          (patch.taskType ?? current.taskType) === "command"
            ? patch.workingDirectory === undefined
              ? current.workingDirectory ?? getMockProjectOrThrow(current.projectId).path
              : patch.workingDirectory ?? getMockProjectOrThrow(current.projectId).path
            : null,
        enabled: patch.enabled ?? current.enabled,
        autoResume: patch.autoResume ?? current.autoResume,
        status: (patch.enabled ?? current.enabled)
          ? current.status === "running"
            ? "running"
            : "scheduled"
          : "idle",
        nextRunAt: (patch.enabled ?? current.enabled)
          ? mockNextScheduledTaskRunAt(
              {
                enabled: true,
                scheduleMode,
                simpleScheduleKind: scheduleMode === "simple" ? simpleScheduleKind ?? null : null,
                intervalSeconds,
                dailyTime,
                weeklyDay,
              } satisfies Pick<
                ProjectScheduledTask,
                "enabled" | "scheduleMode" | "simpleScheduleKind" | "intervalSeconds" | "dailyTime" | "weeklyDay"
              >,
              timestamp,
            )
          : null,
        updatedAt: timestamp,
      };

      return upsertMockScheduledTask(nextTask) as T;
    }
    case "delete_project_scheduled_task": {
      const taskId = String(args?.taskId ?? "");
      const removedRunIds = readMockProjectScheduledTaskRuns()
        .filter((run) => run.taskId === taskId)
        .map((run) => run.id);
      writeMockProjectScheduledTasks(readMockProjectScheduledTasks().filter((task) => task.id !== taskId));
      writeMockProjectScheduledTaskRuns(
        readMockProjectScheduledTaskRuns().filter((run) => run.taskId !== taskId),
      );
      const logs = readMockServiceLogs();
      removedRunIds.forEach((runId) => {
        delete logs[`task-run:${runId}`];
      });
      writeMockServiceLogs(logs);
      return ({ success: true } satisfies DeleteProjectScheduledTaskResult) as T;
    }
    case "get_project_scheduled_task_status": {
      const taskId = String(args?.taskId ?? "");
      return getMockScheduledTaskOrThrow(taskId) as T;
    }
    case "enable_project_scheduled_task": {
      const taskId = String(args?.taskId ?? "");
      const task = getMockScheduledTaskOrThrow(taskId);
      return upsertMockScheduledTask({
        ...task,
        enabled: true,
        status: task.status === "running" ? "running" : "scheduled",
        nextRunAt: mockNextScheduledTaskRunAt({ ...task, enabled: true }, timestamp),
        updatedAt: timestamp,
      }) as T;
    }
    case "disable_project_scheduled_task": {
      const taskId = String(args?.taskId ?? "");
      const task = getMockScheduledTaskOrThrow(taskId);
      return upsertMockScheduledTask({
        ...task,
        enabled: false,
        status: "idle",
        nextRunAt: null,
        updatedAt: timestamp,
      }) as T;
    }
    case "run_project_scheduled_task_now": {
      const taskId = String(args?.taskId ?? "");
      const task = getMockScheduledTaskOrThrow(taskId);
      const runId = crypto.randomUUID();
      const target =
        task.taskType === "url"
          ? task.url ?? ""
          : serializeCommandLine(task.command, task.args);
      const failed = /fail|error|exit\s*1/i.test(target);
      const run: ProjectScheduledTaskRun = {
        id: runId,
        taskId: task.id,
        projectId: task.projectId,
        startedAt: timestamp,
        finishedAt: timestamp,
        durationMs: failed ? 180 : 120,
        status: failed ? "error" : "success",
        exitCode: task.taskType === "command" ? (failed ? 1 : 0) : null,
        responseStatus: task.taskType === "url" ? (failed ? 500 : 200) : null,
        errorMessage: failed ? `${task.name} failed in browser mock mode.` : null,
        logPath: `.devnest/runtime-logs/scheduled-tasks/${task.projectId}/${task.id}/${runId}.log`,
        createdAt: timestamp,
      };
      writeMockProjectScheduledTaskRuns([run, ...readMockProjectScheduledTaskRuns()]);
      appendMockScheduledTaskRunLog(runId, `[run] ${task.name}`);
      appendMockScheduledTaskRunLog(runId, `[target] ${target}`);
      appendMockScheduledTaskRunLog(
        runId,
        failed
          ? task.taskType === "url"
            ? "[response] HTTP 500"
            : "[result] exit code 1"
          : task.taskType === "url"
            ? "[response] HTTP 200"
            : "[result] exit code 0",
      );
      const updatedTask = upsertMockScheduledTask({
        ...task,
        status: failed ? "error" : "success",
        nextRunAt: task.enabled ? mockNextScheduledTaskRunAt(task, timestamp) : null,
        lastRunAt: timestamp,
        lastSuccessAt: failed ? task.lastSuccessAt ?? null : timestamp,
        lastError: failed ? `${task.name} failed in browser mock mode.` : null,
        updatedAt: timestamp,
      });
      return updatedTask as T;
    }
    case "list_project_scheduled_task_runs": {
      const taskId = String(args?.taskId ?? "");
      const limit = Number(args?.limit ?? 25);
      return readMockProjectScheduledTaskRuns()
        .filter((run) => run.taskId === taskId)
        .sort((left, right) => right.createdAt.localeCompare(left.createdAt))
        .slice(0, limit) as T;
    }
    case "read_project_scheduled_task_run_logs": {
      const runId = String(args?.runId ?? "");
      const run = readMockProjectScheduledTaskRuns().find((item) => item.id === runId);
      if (!run) {
        throw {
          code: "SCHEDULED_TASK_RUN_NOT_FOUND",
          message: "Scheduled task run not found.",
        } satisfies AppError;
      }

      const lines = Number(args?.lines ?? 200);
      const logs = readMockServiceLogs();
      const logKey = `task-run:${runId}`;
      const selected = (logs[logKey] ?? []).slice(-lines);
      return ({
        name: `Task Run ${runId}`,
        totalLines: logs[logKey]?.length ?? 0,
        truncated: (logs[logKey]?.length ?? 0) > selected.length,
        lines: selected.map((text, index) => ({
          id: `${logKey}:${Math.max((logs[logKey]?.length ?? 0) - selected.length, 0) + index}`,
          text,
          severity: /error|fatal/i.test(text) ? "error" : /warn/i.test(text) ? "warning" : "info",
          lineNumber: Math.max((logs[logKey]?.length ?? 0) - selected.length, 0) + index + 1,
        })),
        content: selected.join("\n"),
      } satisfies ProjectScheduledTaskRunLogPayload) as T;
    }
    case "clear_project_scheduled_task_logs": {
      const taskId = String(args?.taskId ?? "");
      const logs = readMockServiceLogs();
      readMockProjectScheduledTaskRuns()
        .filter((run) => run.taskId === taskId)
        .forEach((run) => {
          delete logs[`task-run:${run.id}`];
        });
      writeMockServiceLogs(logs);
      return true as T;
    }
    case "clear_project_scheduled_task_history": {
      const taskId = String(args?.taskId ?? "");
      const task = getMockScheduledTaskOrThrow(taskId);
      if (task.status === "running") {
        throw {
          code: "SCHEDULED_TASK_HISTORY_CLEAR_BLOCKED",
          message: "Stop the scheduled task before clearing its run history.",
        } satisfies AppError;
      }

      const logs = readMockServiceLogs();
      readMockProjectScheduledTaskRuns()
        .filter((run) => run.taskId === taskId)
        .forEach((run) => {
          delete logs[`task-run:${run.id}`];
        });
      writeMockServiceLogs(logs);
      writeMockProjectScheduledTaskRuns(
        readMockProjectScheduledTaskRuns().filter((run) => run.taskId !== taskId),
      );

      return upsertMockScheduledTask({
        ...task,
        status: task.enabled ? "scheduled" : "idle",
        nextRunAt: task.enabled ? mockNextScheduledTaskRunAt(task, timestamp) : null,
        lastRunAt: null,
        lastSuccessAt: null,
        lastError: null,
        updatedAt: timestamp,
      }) as T;
    }
    case "list_project_workers": {
      const projectId = String(args?.projectId ?? "");
      return readMockProjectWorkers()
        .filter((worker) => worker.projectId === projectId)
        .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt)) as T;
    }
    case "list_all_workers":
      return [...readMockProjectWorkers()].sort((left, right) =>
        right.updatedAt.localeCompare(left.updatedAt),
      ) as T;
    case "create_project_worker": {
      const input = args?.input as CreateProjectWorkerInput | undefined;
      if (!input) {
        throw {
          code: "INVALID_INPUT",
          message: "Worker input is required.",
        } satisfies AppError;
      }

      const project = getMockProjectOrThrow(input.projectId);
      const { command, args: parsedArgs } = parseMockWorkerCommandLine(input.commandLine);
      if (!command) {
        throw {
          code: "INVALID_WORKER_COMMAND",
          message: "Worker command line is required.",
        } satisfies AppError;
      }

      const created: ProjectWorker = {
        id: crypto.randomUUID(),
        projectId: project.id,
        name: input.name.trim(),
        presetType: input.presetType,
        command,
        args: parsedArgs,
        workingDirectory: (input.workingDirectory ?? project.path).trim(),
        autoStart: Boolean(input.autoStart),
        status: "stopped",
        pid: null,
        lastStartedAt: null,
        lastStoppedAt: null,
        lastExitCode: null,
        lastError: null,
        logPath: `.devnest/runtime-logs/workers/${project.id}/${timestamp.replace(/[:.]/g, "-")}.log`,
        createdAt: timestamp,
        updatedAt: timestamp,
      };

      writeMockProjectWorkers([created, ...readMockProjectWorkers()]);
      return created as T;
    }
    case "update_project_worker": {
      const workerId = String(args?.workerId ?? "");
      const patch = (args?.patch ?? {}) as UpdateProjectWorkerPatch;
      const current = getMockWorkerOrThrow(workerId);
      const parsedCommand = patch.commandLine
        ? parseMockWorkerCommandLine(patch.commandLine)
        : {
            command: current.command,
            args: current.args,
          };
      if (!parsedCommand.command) {
        throw {
          code: "INVALID_WORKER_COMMAND",
          message: "Worker command line is required.",
        } satisfies AppError;
      }

      return upsertMockWorker({
        ...current,
        name: patch.name?.trim() || current.name,
        presetType: patch.presetType ?? current.presetType,
        command: parsedCommand.command,
        args: parsedCommand.args,
        workingDirectory:
          patch.workingDirectory === undefined
            ? current.workingDirectory
            : (patch.workingDirectory ?? getMockProjectOrThrow(current.projectId).path),
        autoStart: patch.autoStart ?? current.autoStart,
        updatedAt: timestamp,
      }) as T;
    }
    case "delete_project_worker": {
      const workerId = String(args?.workerId ?? "");
      writeMockProjectWorkers(readMockProjectWorkers().filter((worker) => worker.id !== workerId));
      return ({ success: true } satisfies DeleteProjectWorkerResult) as T;
    }
    case "get_project_worker_status": {
      const workerId = String(args?.workerId ?? "");
      return getMockWorkerOrThrow(workerId) as T;
    }
    case "start_project_worker": {
      const workerId = String(args?.workerId ?? "");
      const worker = getMockWorkerOrThrow(workerId);
      appendMockWorkerLog(worker.id, `[info] ${worker.name} started from ${worker.workingDirectory}`);
      return upsertMockWorker({
        ...worker,
        status: "running",
        pid: worker.pid ?? Math.floor(Math.random() * 5000) + 1000,
        lastStartedAt: timestamp,
        lastStoppedAt: worker.lastStoppedAt ?? null,
        lastExitCode: null,
        lastError: null,
        updatedAt: timestamp,
      }) as T;
    }
    case "stop_project_worker": {
      const workerId = String(args?.workerId ?? "");
      const worker = getMockWorkerOrThrow(workerId);
      appendMockWorkerLog(worker.id, `[info] ${worker.name} stopped`);
      return upsertMockWorker({
        ...worker,
        status: "stopped",
        pid: null,
        lastStoppedAt: timestamp,
        updatedAt: timestamp,
      }) as T;
    }
    case "restart_project_worker": {
      const workerId = String(args?.workerId ?? "");
      const worker = getMockWorkerOrThrow(workerId);
      appendMockWorkerLog(worker.id, `[info] ${worker.name} restarted`);
      return upsertMockWorker({
        ...worker,
        status: "running",
        pid: worker.pid ?? Math.floor(Math.random() * 5000) + 1000,
        lastStartedAt: timestamp,
        lastStoppedAt: timestamp,
        lastExitCode: null,
        lastError: null,
        updatedAt: timestamp,
      }) as T;
    }
    case "preview_vhost_config": {
      const projectId = String(args?.projectId ?? "");
      return buildMockConfigPreview(getMockProjectOrThrow(projectId)) as T;
    }
    case "generate_vhost_config": {
      const projectId = String(args?.projectId ?? "");
      const preview = buildMockConfigPreview(getMockProjectOrThrow(projectId));
      return {
        success: true,
        outputPath: preview.outputPath,
      } as T;
    }
    case "trust_local_ssl_authority": {
      writeMockSslAuthorityTrusted(true);
      return {
        success: true,
        certPath: ".devnest/ssl/authority/devnest-local-ca.pem",
        trusted: true,
      } as T;
    }
    case "get_local_ssl_authority_status": {
      return {
        success: true,
        certPath: ".devnest/ssl/authority/devnest-local-ca.pem",
        trusted: readMockSslAuthorityTrusted(),
      } as T;
    }
    case "untrust_local_ssl_authority": {
      writeMockSslAuthorityTrusted(false);
      return {
        success: true,
        certPath: ".devnest/ssl/authority/devnest-local-ca.pem",
        trusted: false,
      } as T;
    }
    case "regenerate_project_ssl_certificate": {
      const projectId = String(args?.projectId ?? "");
      const project = getMockProjectOrThrow(projectId);
      if (!project.sslEnabled) {
        throw {
          code: "SSL_NOT_ENABLED",
          message: "Enable SSL in the project profile before regenerating a local certificate.",
        } satisfies AppError;
      }
      return {
        success: true,
        domain: project.domain,
        certPath: `.devnest/ssl/${project.domain}/cert.pem`,
        keyPath: `.devnest/ssl/${project.domain}/key.pem`,
      } as T;
    }
    case "open_project_site": {
      const projectId = String(args?.projectId ?? "");
      const project = getMockProjectOrThrow(projectId);
      const preferHttps = Boolean(args?.preferHttps);
      const url = `${preferHttps && project.sslEnabled ? "https" : "http"}://${project.domain}`;
      if (typeof window !== "undefined") {
        window.open(url, "_blank", "noopener,noreferrer");
      }
      return true as T;
    }
    case "get_project_mobile_preview_state": {
      const projectId = String(args?.projectId ?? "");
      const previews = readMockProjectMobilePreviews();
      return (previews[projectId] ?? null) as T;
    }
    case "start_project_mobile_preview": {
      const projectId = String(args?.projectId ?? "");
      const project = getMockProjectOrThrow(projectId);
      const service = getMockServiceOrThrow(project.serverType);
      if (service.status !== "running") {
        throw {
          code: "MOBILE_PREVIEW_UPSTREAM_UNAVAILABLE",
          message: `Start ${project.serverType} for this project first, then try Mobile Preview again.`,
        } satisfies AppError;
      }

      const previews = readMockProjectMobilePreviews();
      const preview: ProjectMobilePreviewState = {
        projectId,
        status: "running",
        localProjectUrl: `${project.sslEnabled ? "https" : "http"}://${project.domain}`,
        lanIp: "192.168.1.5",
        port: 50321,
        proxyUrl: "http://192.168.1.5:50321/",
        qrUrl: "http://192.168.1.5:50321/",
        updatedAt: timestamp,
        details: `LAN preview is running for ${project.domain}. Scan the QR code from a phone on the same Wi-Fi network.`,
      };
      writeMockProjectMobilePreviews({
        ...previews,
        [projectId]: preview,
      });
      return preview as T;
    }
    case "stop_project_mobile_preview": {
      const projectId = String(args?.projectId ?? "");
      const project = getMockProjectOrThrow(projectId);
      const previews = readMockProjectMobilePreviews();
      delete previews[projectId];
      writeMockProjectMobilePreviews(previews);
      return ({
        projectId,
        status: "stopped",
        localProjectUrl: `${project.sslEnabled ? "https" : "http"}://${project.domain}`,
        lanIp: null,
        port: null,
        proxyUrl: null,
        qrUrl: null,
        updatedAt: timestamp,
        details: "Mobile preview stopped. Start it again whenever you need a fresh QR session.",
      } satisfies ProjectMobilePreviewState) as T;
    }
    case "list_optional_tool_inventory": {
      return readMockOptionalTools() as T;
    }
    case "list_optional_tool_packages": {
      return readMockOptionalToolPackages() as T;
    }
    case "get_optional_tool_install_task": {
      return readMockOptionalToolInstallTask() as T;
    }
    case "install_optional_tool_package": {
      const packageId = String(args?.packageId ?? "");
      const targetPackage = readMockOptionalToolPackages().find((item) => item.id === packageId);
      if (!targetPackage) {
        throw {
          code: "OPTIONAL_TOOL_PACKAGE_NOT_FOUND",
          message: "Optional tool package was not found in the current manifest.",
        } satisfies AppError;
      }

      const path =
        targetPackage.toolType === "mailpit"
          ? `${MOCK_APP_DATA_ROOT}/optional-tools/downloaded/mailpit/${targetPackage.version}/mailpit.exe`
          : targetPackage.toolType === "phpmyadmin"
            ? `${MOCK_APP_DATA_ROOT}/optional-tools/downloaded/phpmyadmin/${targetPackage.version}/phpMyAdmin-${targetPackage.version}-all-languages/index.php`
            : targetPackage.toolType === "redis"
              ? `${MOCK_APP_DATA_ROOT}/optional-tools/downloaded/redis/${targetPackage.version}/Redis-${targetPackage.version}-Windows-x64-msys2/redis-server.exe`
              : targetPackage.toolType === "restic"
                ? `${MOCK_APP_DATA_ROOT}/optional-tools/downloaded/restic/${targetPackage.version}/restic_0.18.1_windows_amd64.exe`
                : `${MOCK_APP_DATA_ROOT}/optional-tools/downloaded/cloudflared/${targetPackage.version}/cloudflared.exe`;
      const next = {
        id: `${targetPackage.toolType}-${targetPackage.version}`,
        toolType: targetPackage.toolType,
        version: targetPackage.version,
        path,
        isActive: true,
        status: "available",
        createdAt: timestamp,
        updatedAt: timestamp,
        details: `${targetPackage.displayName} installed successfully.`,
      } satisfies OptionalToolInventoryItem;
      writeMockOptionalToolInstallTask({
        packageId: targetPackage.id,
        displayName: targetPackage.displayName,
        toolType: targetPackage.toolType,
        version: targetPackage.version,
        stage: "completed",
        message: `${targetPackage.displayName} installed successfully.`,
        updatedAt: timestamp,
        errorCode: null,
      });

      const tools = readMockOptionalTools()
        .map((item) =>
          item.toolType === targetPackage.toolType ? { ...item, isActive: false, updatedAt: timestamp } : item,
        )
        .filter((item) => item.id !== next.id);
      writeMockOptionalTools([...tools, next]);
      if (targetPackage.toolType === "phpmyadmin") {
        const hosts = readMockHosts();
        hosts["phpmyadmin.test"] = "127.0.0.1";
        writeMockHosts(hosts);
      }
      return next as T;
    }
    case "remove_optional_tool": {
      const toolId = String(args?.toolId ?? "");
      const tools = readMockOptionalTools();
      const target = tools.find((item) => item.id === toolId);
      if (!target) {
        throw {
          code: "OPTIONAL_TOOL_NOT_FOUND",
          message: "Optional tool entry was not found.",
        } satisfies AppError;
      }

      if (target.toolType === "mailpit" && getMockServiceOrThrow("mailpit").status === "running") {
        throw {
          code: "OPTIONAL_TOOL_IN_USE",
          message: "Mailpit is running right now. Stop the Mailpit service before uninstalling it.",
        } satisfies AppError;
      }

      const tunnels = readMockProjectTunnels();
      if (
        target.toolType === "cloudflared" &&
        Object.values(tunnels).some((tunnel) => tunnel.status !== "stopped")
      ) {
        throw {
          code: "OPTIONAL_TOOL_IN_USE",
          message: "A project tunnel is still active. Stop all active tunnels before uninstalling cloudflared.",
        } satisfies AppError;
      }

      if (target.toolType === "phpmyadmin") {
        const hosts = readMockHosts();
        delete hosts["phpmyadmin.test"];
        writeMockHosts(hosts);
      }

      writeMockOptionalTools(tools.filter((item) => item.id !== toolId));
      return true as T;
    }
    case "reveal_optional_tool_path": {
      const toolId = String(args?.toolId ?? "");
      const tool = readMockOptionalTools().find((item) => item.id === toolId);
      if (!tool) {
        throw {
          code: "OPTIONAL_TOOL_NOT_FOUND",
          message: "Optional tool entry was not found.",
        } satisfies AppError;
      }
      return true as T;
    }
    case "get_project_tunnel_state": {
      const projectId = String(args?.projectId ?? "");
      const tunnels = readMockProjectTunnels();
      return (tunnels[projectId] ?? null) as T;
    }
    case "start_project_tunnel": {
      const projectId = String(args?.projectId ?? "");
      const project = getMockProjectOrThrow(projectId);
      const tunnels = readMockProjectTunnels();
      const tunnel: ProjectTunnelState = {
        projectId,
        provider: "cloudflared",
        status: "running",
        localUrl: `http://${project.domain}`,
        publicUrl: `https://${project.domain.replace(/\.test$/i, "") || "project"}.trycloudflare.com`,
        publicHostAliasSynced: true,
        logPath: `${MOCK_APP_DATA_ROOT}/logs/tunnels/${projectId}.log`,
        binaryPath: "cloudflared",
        updatedAt: timestamp,
        details: "Mock tunnel is active through the optional cloudflared integration.",
      };
      writeMockProjectTunnels({
        ...tunnels,
        [projectId]: tunnel,
      });
      return tunnel as T;
    }
    case "stop_project_tunnel": {
      const projectId = String(args?.projectId ?? "");
      const project = getMockProjectOrThrow(projectId);
      const tunnels = readMockProjectTunnels();
      delete tunnels[projectId];
      writeMockProjectTunnels(tunnels);
      return ({
        projectId,
        provider: "cloudflared",
        status: "stopped",
        localUrl: `http://${project.domain}`,
        publicUrl: null,
        publicHostAliasSynced: false,
        logPath: `${MOCK_APP_DATA_ROOT}/logs/tunnels/${projectId}.log`,
        binaryPath: "cloudflared",
        updatedAt: timestamp,
        details: "The optional public tunnel is stopped.",
      } satisfies ProjectTunnelState) as T;
    }
    case "open_project_tunnel_url": {
      const projectId = String(args?.projectId ?? "");
      const tunnel = readMockProjectTunnels()[projectId];
      if (!tunnel?.publicUrl) {
        throw {
          code: "TUNNEL_URL_NOT_READY",
          message: "Start the optional tunnel first so DevNest has a public URL to open.",
        } satisfies AppError;
      }
      if (typeof window !== "undefined") {
        window.open(tunnel.publicUrl, "_blank", "noopener,noreferrer");
      }
      return true as T;
    }
    case "get_persistent_tunnel_setup_status": {
      return readMockPersistentTunnelSetup() as T;
    }
    case "connect_persistent_tunnel_provider": {
      const current = readMockPersistentTunnelSetup();
      const next = {
        ...current,
        managed: true,
        authCertPath: `${MOCK_APP_DATA_ROOT}/persistent-tunnels/cloudflared/cert.pem`,
        details:
          "Cloudflare account is connected. Create or select a named tunnel next.",
      } satisfies PersistentTunnelSetupStatus;
      writeMockPersistentTunnelSetup(next);
      return next as T;
    }
    case "import_persistent_tunnel_auth_cert": {
      const current = readMockPersistentTunnelSetup();
      const next = {
        ...current,
        managed: true,
        authCertPath: `${MOCK_APP_DATA_ROOT}/persistent-tunnels/cloudflared/cert.pem`,
        details:
          "Managed cloudflared auth cert is available. Create or select a named tunnel next.",
      } satisfies PersistentTunnelSetupStatus;
      writeMockPersistentTunnelSetup(next);
      return next as T;
    }
    case "create_persistent_named_tunnel": {
      const input = (args?.input ?? {}) as CreatePersistentNamedTunnelInput;
      const name = String(input.name ?? "").trim();
      if (name.length < 2) {
        throw {
          code: "PERSISTENT_TUNNEL_NAME_INVALID",
          message: "Named tunnel name must contain at least 2 characters.",
        } satisfies AppError;
      }
      const existingTunnels = readMockPersistentNamedTunnels();
      if (
        existingTunnels.some(
          (item) => normalizeMockTunnelNameKey(item.tunnelName) === normalizeMockTunnelNameKey(name),
        )
      ) {
        throw {
          code: "PERSISTENT_TUNNEL_NAME_EXISTS",
          message:
            "A named tunnel with this name already exists. Use Tunnel instead of creating a duplicate.",
        } satisfies AppError;
      }
      const tunnelId = `mock-${name.toLowerCase().replace(/[^a-z0-9]+/g, "-") || "devnest"}-tunnel`;
      const current = readMockPersistentTunnelSetup();
      const nextTunnels = [
        ...existingTunnels.map((item) => ({ ...item, selected: false })),
        {
          tunnelId,
          tunnelName: name,
          credentialsPath: `${MOCK_APP_DATA_ROOT}/persistent-tunnels/cloudflared/credentials/${tunnelId}.json`,
          selected: true,
        } satisfies PersistentTunnelNamedTunnelSummary,
      ];
      const next = {
        ...current,
        managed: true,
        authCertPath: current.authCertPath ?? `${MOCK_APP_DATA_ROOT}/persistent-tunnels/cloudflared/cert.pem`,
        credentialsPath: `${MOCK_APP_DATA_ROOT}/persistent-tunnels/cloudflared/credentials/${tunnelId}.json`,
        tunnelId,
        tunnelName: name,
        ready: Boolean(current.binaryPath),
        details:
          "Named tunnel is selected. Set your default public zone, then publish projects with one click.",
      } satisfies PersistentTunnelSetupStatus;
      writeMockPersistentTunnelSetup(next);
      writeMockPersistentNamedTunnels(nextTunnels);
      return next as T;
    }
    case "import_persistent_tunnel_credentials": {
      const current = readMockPersistentTunnelSetup();
      const tunnelId = current.tunnelId ?? "mock-imported-tunnel";
      const next = {
        ...current,
        managed: true,
        credentialsPath: `${MOCK_APP_DATA_ROOT}/persistent-tunnels/cloudflared/credentials/${tunnelId}.json`,
        tunnelId,
        tunnelName: current.tunnelName ?? "Imported tunnel",
        ready: Boolean(current.binaryPath && current.authCertPath),
        details:
          "Named tunnel credentials are imported. Set your default public zone, then publish projects with one click.",
      } satisfies PersistentTunnelSetupStatus;
      writeMockPersistentTunnelSetup(next);
      return next as T;
    }
    case "list_available_persistent_named_tunnels": {
      const current = readMockPersistentTunnelSetup();
      const tunnels = readMockPersistentNamedTunnels().map((item) => ({
        ...item,
        selected: item.tunnelId === current.tunnelId,
      }));
      writeMockPersistentNamedTunnels(tunnels);
      return tunnels as T;
    }
    case "select_persistent_named_tunnel": {
      const input = (args?.input ?? {}) as SelectPersistentNamedTunnelInput;
      const tunnelId = String(input.tunnelId ?? "").trim();
      if (!tunnelId) {
        throw {
          code: "PERSISTENT_TUNNEL_NOT_FOUND",
          message: "Select a named tunnel before continuing.",
        } satisfies AppError;
      }
      const tunnels = readMockPersistentNamedTunnels();
      const selectedTunnel = tunnels.find((item) => item.tunnelId === tunnelId);
      if (!selectedTunnel) {
        throw {
          code: "PERSISTENT_TUNNEL_NOT_FOUND",
          message: "Select a named tunnel before continuing.",
        } satisfies AppError;
      }
      const current = readMockPersistentTunnelSetup();
      const next = {
        ...current,
        managed: true,
        tunnelId,
        tunnelName: selectedTunnel.tunnelName,
        credentialsPath:
          selectedTunnel.credentialsPath ??
          `${MOCK_APP_DATA_ROOT}/persistent-tunnels/cloudflared/credentials/${tunnelId}.json`,
        ready: Boolean(current.binaryPath && current.authCertPath),
        details:
          "Named tunnel is selected. Set your default public zone, then publish projects with one click.",
      } satisfies PersistentTunnelSetupStatus;
      writeMockPersistentTunnelSetup(next);
      writeMockPersistentNamedTunnels(
        tunnels.map((item) => ({ ...item, selected: item.tunnelId === tunnelId })),
      );
      return next as T;
    }
    case "delete_persistent_named_tunnel": {
      const tunnelId = String(args?.tunnelId ?? "").trim();
      const current = readMockPersistentTunnelSetup();
      const tunnels = readMockPersistentNamedTunnels();
      const target = tunnels.find((item) => item.tunnelId === tunnelId);
      if (!target) {
        throw {
          code: "PERSISTENT_TUNNEL_NOT_FOUND",
          message: "DevNest could not find that named tunnel.",
        } satisfies AppError;
      }
      const projectTunnels = readMockProjectPersistentTunnels();
      const deletingSelected = current.tunnelId === tunnelId;
      if (
        deletingSelected &&
        Object.values(projectTunnels).some(
          (item) => item.tunnelId === tunnelId && item.status !== "stopped",
        )
      ) {
        throw {
          code: "PERSISTENT_TUNNEL_IN_USE",
          message:
            "Stop or delete the hostname for all projects using the selected shared tunnel before deleting it.",
        } satisfies AppError;
      }
      const remainingTunnels = tunnels.filter((item) => item.tunnelId !== tunnelId);
      writeMockPersistentNamedTunnels(
        remainingTunnels.map((item) => ({ ...item, selected: false })),
      );
      const next = deletingSelected
        ? ({
            ...current,
            credentialsPath: null,
            tunnelId: null,
            tunnelName: null,
            ready: Boolean(current.binaryPath && current.authCertPath && current.defaultHostnameZone),
            details:
              "Selected named tunnel was deleted. Create or choose another tunnel before publishing projects again.",
          } satisfies PersistentTunnelSetupStatus)
        : current;
      writeMockPersistentTunnelSetup(next);
      return next as T;
    }
    case "disconnect_persistent_tunnel_provider": {
      const projectTunnels = readMockProjectPersistentTunnels();
      if (Object.keys(projectTunnels).length > 0) {
        throw {
          code: "PERSISTENT_TUNNEL_IN_USE",
          message:
            "Stop or delete the hostname for all projects using the shared persistent tunnel before disconnecting Cloudflare from DevNest.",
        } satisfies AppError;
      }
      const current = readMockPersistentTunnelSetup();
      const next = {
        ...defaultMockPersistentTunnelSetup(),
        binaryPath: current.binaryPath,
        details:
          "Cloudflare setup was disconnected from DevNest. Connect again or import credentials before publishing stable domains.",
      } satisfies PersistentTunnelSetupStatus;
      writeMockPersistentTunnelSetup(next);
      writeMockPersistentNamedTunnels([]);
      return next as T;
    }
    case "update_persistent_tunnel_setup": {
      const input = (args?.input ?? {}) as UpdatePersistentTunnelSetupInput;
      const zone = String(input.defaultHostnameZone ?? "").trim().toLowerCase() || null;
      const current = readMockPersistentTunnelSetup();
      const next = {
        ...current,
        defaultHostnameZone: zone,
        ready: Boolean(
          current.binaryPath && current.authCertPath && current.credentialsPath && current.tunnelId,
        ),
        details: zone
          ? `Default public zone is ${zone}. Publish a project to auto-generate a stable hostname under that zone.`
          : current.details,
      } satisfies PersistentTunnelSetupStatus;
      writeMockPersistentTunnelSetup(next);
      return next as T;
    }
    case "get_project_persistent_hostname": {
      const projectId = String(args?.projectId ?? "");
      const hostnames = readMockProjectPersistentHostnames();
      return (hostnames[projectId] ?? null) as T;
    }
    case "apply_project_persistent_hostname": {
      const input = (args?.input ?? {}) as ApplyProjectPersistentHostnameInput;
      const projectId = String(input.projectId ?? "");
      const project = getMockProjectOrThrow(projectId);
      const setup = readMockPersistentTunnelSetup();
      const rawHostname = String(input.hostname ?? "").trim().toLowerCase();
      const normalizedZone = String(setup.defaultHostnameZone ?? "")
        .trim()
        .replace(/^\.+|\.+$/g, "")
        .toLowerCase();
      const resolvedHostname = rawHostname
        ? rawHostname.includes(".")
          ? rawHostname
          : normalizedZone
            ? `${rawHostname}.${normalizedZone}`
            : ""
        : normalizedZone
          ? `${project.domain.replace(/\.test$/i, "").replace(/\./g, "-") || "project"}.${normalizedZone}`
          : "";

      if (!resolvedHostname) {
        throw {
          code: "PERSISTENT_HOSTNAME_ZONE_MISSING",
          message:
            "Set a default public zone in Settings before using a bare subdomain or leaving this hostname blank.",
        } satisfies AppError;
      }

      const hostnames = readMockProjectPersistentHostnames();
      const duplicate = Object.values(hostnames).find(
        (entry) => entry.hostname === resolvedHostname && entry.projectId !== projectId,
      );
      if (duplicate) {
        throw {
          code: "PERSISTENT_HOSTNAME_EXISTS",
          message: "That persistent hostname is already assigned to another project.",
        } satisfies AppError;
      }

      const currentHostname = hostnames[projectId];
      const nextHostname = {
        id: currentHostname?.id ?? `cloudflared-${projectId}`,
        projectId,
        provider: "cloudflared",
        hostname: resolvedHostname,
        createdAt: currentHostname?.createdAt ?? timestamp,
        updatedAt: timestamp,
      } satisfies ProjectPersistentHostname;
      writeMockProjectPersistentHostnames({
        ...hostnames,
        [projectId]: nextHostname,
      });

      const tunnel: ProjectPersistentTunnelState = {
        projectId,
        provider: "cloudflared",
        status: "running",
        hostname: nextHostname.hostname,
        localUrl: `${project.sslEnabled ? "https" : "http"}://${project.domain}`,
        publicUrl: `https://${nextHostname.hostname}`,
        logPath: `${MOCK_APP_DATA_ROOT}/logs/persistent-tunnels/${projectId}.log`,
        binaryPath: `${MOCK_APP_DATA_ROOT}/optional-tools/cloudflared/cloudflared.exe`,
        tunnelId: setup.tunnelId ?? "mock-devnest-tunnel",
        credentialsPath:
          setup.credentialsPath ??
          `${MOCK_APP_DATA_ROOT}/persistent-tunnels/cloudflared/credentials/mock-devnest-tunnel.json`,
        updatedAt: timestamp,
        details: `Persistent tunnel is active for https://${nextHostname.hostname}.`,
      };
      writeMockProjectPersistentTunnels({
        ...readMockProjectPersistentTunnels(),
        [projectId]: tunnel,
      });

      return ({
        hostname: nextHostname,
        tunnel,
      } satisfies ApplyProjectPersistentHostnameResult) as T;
    }
    case "upsert_project_persistent_hostname": {
      const input = (args?.input ?? {}) as {
        projectId?: string;
        hostname?: string;
      };
      const projectId = String(input.projectId ?? "");
      const hostname = String(input.hostname ?? "").trim().toLowerCase();
      if (!projectId || !hostname) {
        throw {
          code: "INVALID_PERSISTENT_HOSTNAME",
          message: "Enter a valid persistent hostname before saving it.",
        } satisfies AppError;
      }

      const hostnames = readMockProjectPersistentHostnames();
      const duplicate = Object.values(hostnames).find(
        (entry) => entry.hostname === hostname && entry.projectId !== projectId,
      );
      if (duplicate) {
        throw {
          code: "PERSISTENT_HOSTNAME_EXISTS",
          message: "That persistent hostname is already assigned to another project.",
        } satisfies AppError;
      }

      const current = hostnames[projectId];
      const next = {
        id: current?.id ?? `cloudflared-${projectId}`,
        projectId,
        provider: "cloudflared",
        hostname,
        createdAt: current?.createdAt ?? timestamp,
        updatedAt: timestamp,
      } satisfies ProjectPersistentHostname;
      writeMockProjectPersistentHostnames({
        ...hostnames,
        [projectId]: next,
      });
      return next as T;
    }
    case "delete_project_persistent_hostname": {
      const projectId = String(args?.projectId ?? "");
      const hostnames = readMockProjectPersistentHostnames();
      const hostname = hostnames[projectId];
      if (!hostname) {
        throw {
          code: "PERSISTENT_HOSTNAME_NOT_ASSIGNED",
          message: "This project does not have a stable public hostname yet.",
        } satisfies AppError;
      }

      delete hostnames[projectId];
      writeMockProjectPersistentHostnames(hostnames);
      const tunnels = readMockProjectPersistentTunnels();
      delete tunnels[projectId];
      writeMockProjectPersistentTunnels(tunnels);

      return ({
        hostname: hostname.hostname,
      } satisfies DeleteProjectPersistentHostnameResult) as T;
    }
    case "remove_project_persistent_hostname": {
      const projectId = String(args?.projectId ?? "");
      const hostnames = readMockProjectPersistentHostnames();
      const existed = Boolean(hostnames[projectId]);
      delete hostnames[projectId];
      writeMockProjectPersistentHostnames(hostnames);
      const tunnels = readMockProjectPersistentTunnels();
      delete tunnels[projectId];
      writeMockProjectPersistentTunnels(tunnels);
      return existed as T;
    }
    case "get_project_persistent_tunnel_state": {
      const projectId = String(args?.projectId ?? "");
      const tunnels = readMockProjectPersistentTunnels();
      return (tunnels[projectId] ?? null) as T;
    }
    case "publish_project_persistent_tunnel": {
      const projectId = String(args?.projectId ?? "");
      const project = getMockProjectOrThrow(projectId);
      const setup = readMockPersistentTunnelSetup();
      if (!setup.defaultHostnameZone) {
        throw {
          code: "PERSISTENT_HOSTNAME_ZONE_MISSING",
          message:
            "Set a default public zone in Settings or save a custom hostname before publishing this project.",
        } satisfies AppError;
      }

      const hostnames = readMockProjectPersistentHostnames();
      const autoHostname =
        hostnames[projectId]?.hostname ??
        `${project.domain.replace(/\.test$/i, "").replace(/\./g, "-") || "project"}.${setup.defaultHostnameZone}`;
      const nextHostname = {
        id: hostnames[projectId]?.id ?? `cloudflared-${projectId}`,
        projectId,
        provider: "cloudflared",
        hostname: autoHostname,
        createdAt: hostnames[projectId]?.createdAt ?? timestamp,
        updatedAt: timestamp,
      } satisfies ProjectPersistentHostname;
      writeMockProjectPersistentHostnames({
        ...hostnames,
        [projectId]: nextHostname,
      });

      const tunnels = readMockProjectPersistentTunnels();
      const tunnel: ProjectPersistentTunnelState = {
        projectId,
        provider: "cloudflared",
        status: "running",
        hostname: nextHostname.hostname,
        localUrl: `${project.sslEnabled ? "https" : "http"}://${project.domain}`,
        publicUrl: `https://${nextHostname.hostname}`,
        logPath: `${MOCK_APP_DATA_ROOT}/logs/persistent-tunnels/${projectId}.log`,
        binaryPath: `${MOCK_APP_DATA_ROOT}/optional-tools/cloudflared/cloudflared.exe`,
        tunnelId: setup.tunnelId ?? "mock-devnest-tunnel",
        credentialsPath:
          setup.credentialsPath ??
          `${MOCK_APP_DATA_ROOT}/persistent-tunnels/cloudflared/credentials/mock-devnest-tunnel.json`,
        updatedAt: timestamp,
        details: `Persistent tunnel is active for https://${nextHostname.hostname}.`,
      };
      writeMockProjectPersistentTunnels({
        ...tunnels,
        [projectId]: tunnel,
      });
      return tunnel as T;
    }
    case "start_project_persistent_tunnel": {
      const projectId = String(args?.projectId ?? "");
      const project = getMockProjectOrThrow(projectId);
      const hostname = readMockProjectPersistentHostnames()[projectId];
      if (!hostname) {
        throw {
          code: "PERSISTENT_HOSTNAME_NOT_ASSIGNED",
          message:
            "Reserve a stable public hostname for this project before starting a persistent tunnel.",
        } satisfies AppError;
      }

      const tunnels = readMockProjectPersistentTunnels();
      const tunnel: ProjectPersistentTunnelState = {
        projectId,
        provider: "cloudflared",
        status: "running",
        hostname: hostname.hostname,
        localUrl: `${project.sslEnabled ? "https" : "http"}://${project.domain}`,
        publicUrl: `https://${hostname.hostname}`,
        logPath: `${MOCK_APP_DATA_ROOT}/logs/persistent-tunnels/${projectId}.log`,
        binaryPath: `${MOCK_APP_DATA_ROOT}/optional-tools/cloudflared/cloudflared.exe`,
        tunnelId: "mock-devnest-tunnel",
        credentialsPath: "C:/Users/mock/.cloudflared/devnest-tunnel.json",
        updatedAt: timestamp,
        details: `Persistent tunnel is active for https://${hostname.hostname}.`,
      };
      writeMockProjectPersistentTunnels({
        ...tunnels,
        [projectId]: tunnel,
      });
      return tunnel as T;
    }
    case "stop_project_persistent_tunnel": {
      const projectId = String(args?.projectId ?? "");
      const project = getMockProjectOrThrow(projectId);
      const hostname = readMockProjectPersistentHostnames()[projectId];
      const tunnels = readMockProjectPersistentTunnels();
      delete tunnels[projectId];
      writeMockProjectPersistentTunnels(tunnels);
      return ({
        projectId,
        provider: "cloudflared",
        status: "stopped",
        hostname: hostname?.hostname ?? "not-configured",
        localUrl: `${project.sslEnabled ? "https" : "http"}://${project.domain}`,
        publicUrl: `https://${hostname?.hostname ?? "not-configured"}`,
        logPath: `${MOCK_APP_DATA_ROOT}/logs/persistent-tunnels/${projectId}.log`,
        binaryPath: `${MOCK_APP_DATA_ROOT}/optional-tools/cloudflared/cloudflared.exe`,
        tunnelId: "mock-devnest-tunnel",
        credentialsPath: "C:/Users/mock/.cloudflared/devnest-tunnel.json",
        updatedAt: timestamp,
        details: "The persistent public tunnel is stopped.",
      } satisfies ProjectPersistentTunnelState) as T;
    }
    case "unpublish_project_persistent_tunnel": {
      const projectId = String(args?.projectId ?? "");
      const project = getMockProjectOrThrow(projectId);
      const hostnames = readMockProjectPersistentHostnames();
      const hostname = hostnames[projectId];
      if (!hostname) {
        throw {
          code: "PERSISTENT_TUNNEL_NOT_PUBLISHED",
          message: "This project does not have a published persistent hostname to remove yet.",
        } satisfies AppError;
      }
      delete hostnames[projectId];
      writeMockProjectPersistentHostnames(hostnames);
      const tunnels = readMockProjectPersistentTunnels();
      delete tunnels[projectId];
      writeMockProjectPersistentTunnels(tunnels);
      return ({
        projectId,
        provider: "cloudflared",
        status: "stopped",
        hostname: hostname.hostname,
        localUrl: `${project.sslEnabled ? "https" : "http"}://${project.domain}`,
        publicUrl: `https://${hostname.hostname}`,
        logPath: `${MOCK_APP_DATA_ROOT}/logs/persistent-tunnels/${projectId}.log`,
        binaryPath: `${MOCK_APP_DATA_ROOT}/optional-tools/cloudflared/cloudflared.exe`,
        tunnelId: readMockPersistentTunnelSetup().tunnelId ?? "mock-devnest-tunnel",
        credentialsPath: readMockPersistentTunnelSetup().credentialsPath,
        updatedAt: timestamp,
        details:
          "Project was unpublished. DevNest removed its hostname from the shared named tunnel ingress.",
      } satisfies ProjectPersistentTunnelState) as T;
    }
    case "open_project_persistent_tunnel_url": {
      const projectId = String(args?.projectId ?? "");
      const tunnel = readMockProjectPersistentTunnels()[projectId];
      if (!tunnel?.publicUrl) {
        throw {
          code: "PERSISTENT_TUNNEL_URL_NOT_READY",
          message: "Start the persistent tunnel first so DevNest has a stable public URL to open.",
        } satisfies AppError;
      }
      if (typeof window !== "undefined") {
        window.open(tunnel.publicUrl, "_blank", "noopener,noreferrer");
      }
      return true as T;
    }
    case "inspect_project_persistent_tunnel_health": {
      const projectId = String(args?.projectId ?? "");
      const project = getMockProjectOrThrow(projectId);
      const hostname = readMockProjectPersistentHostnames()[projectId];
      const tunnel = readMockProjectPersistentTunnels()[projectId];
      const report: PersistentTunnelHealthReport = {
        projectId,
        hostname: hostname?.hostname ?? null,
        overallStatus: tunnel ? "running" : hostname ? "starting" : "error",
        updatedAt: timestamp,
        checks: [
          {
            code: "setup",
            label: "Named Tunnel Setup",
            status: readMockPersistentTunnelSetup().ready ? "running" : "error",
            message: readMockPersistentTunnelSetup().ready
              ? "cloudflared, credentials, tunnel identity, and the managed setup contract are ready."
              : "Finish the one-time setup in Settings before publishing projects.",
          },
          {
            code: "hostname",
            label: "Reserved Hostname",
            status: hostname ? "running" : "error",
            message: hostname
              ? `${hostname.hostname} is reserved for this project.`
              : "No stable public hostname is reserved for this project yet.",
          },
          {
            code: "config",
            label: "Managed Config Alias",
            status: hostname ? "running" : "error",
            message: hostname
              ? "Managed Apache, Nginx, or FrankenPHP config includes the stable hostname."
              : "Managed config does not currently include the reserved hostname alias.",
          },
          {
            code: "origin",
            label: "Origin Service",
            status: "running",
            message: `${project.serverType} is running and can serve the project origin.`,
          },
          {
            code: "process",
            label: "Persistent Tunnel Process",
            status: tunnel?.status ?? "stopped",
            message:
              tunnel?.details ?? "Persistent tunnel process is not running.",
          },
          {
            code: "dns",
            label: "Public DNS",
            status: tunnel ? "running" : hostname ? "starting" : "stopped",
            message: hostname
              ? tunnel
                ? `${hostname.hostname} resolves from the current machine.`
                : `${hostname.hostname} does not resolve yet from the current machine. DNS propagation or hostname routing may still be pending.`
              : "DNS cannot be checked until a stable hostname is reserved.",
          },
        ],
      };
      return report as T;
    }
    case "open_service_dashboard": {
      const name = String(args?.name ?? "");
      if (name !== "mailpit") {
        throw {
          code: "SERVICE_DASHBOARD_UNAVAILABLE",
          message: "This service does not expose a built-in browser dashboard.",
        } satisfies AppError;
      }

      const service = getMockServiceOrThrow(name);
      if (typeof window !== "undefined") {
        window.open(`http://127.0.0.1:${service.port ?? 8025}`, "_blank", "noopener,noreferrer");
      }
      return true as T;
    }
    case "apply_hosts_entry": {
      const domain = String(args?.domain ?? "").trim().toLowerCase();
      const targetIp = String(args?.targetIp ?? "127.0.0.1").trim();
      const hosts = readMockHosts();
      hosts[domain] = targetIp;
      writeMockHosts(hosts);
      return {
        success: true,
        domain,
        targetIp,
      } as T;
    }
    case "remove_hosts_entry": {
      const domain = String(args?.domain ?? "").trim().toLowerCase();
      const hosts = readMockHosts();
      delete hosts[domain];
      writeMockHosts(hosts);
      return { success: true } as T;
    }
    case "get_service_status": {
      const name = String(args?.name ?? "");
      return getMockServiceOrThrow(name) as T;
    }
    case "start_service": {
      const service = getMockServiceOrThrow(String(args?.name ?? ""));
      const services = readMockServices();
      const conflict = services.find(
        (item) => item.name !== service.name && item.status === "running" && item.port === service.port,
      );

      if (conflict?.port) {
        throw {
          code: "PORT_IN_USE",
          message: `Port ${conflict.port} is already in use by ${conflict.name}.`,
          details: { pid: conflict.pid, processName: conflict.name },
        } satisfies AppError;
      }

      const started: ServiceState = {
        ...service,
        pid: Math.floor(Math.random() * 40000) + 1000,
        status: "running",
        lastError: null,
        updatedAt: timestamp,
      };
      appendMockServiceLog(
        started.name,
        `[${timestamp}] INFO ${started.name.toUpperCase()} service started on port ${started.port ?? "-"}.`,
      );
      return upsertMockService(started) as T;
    }
    case "stop_service": {
      const service = getMockServiceOrThrow(String(args?.name ?? ""));
      const stopped: ServiceState = {
        ...service,
        pid: null,
        status: "stopped",
        lastError: null,
        updatedAt: timestamp,
      };
      appendMockServiceLog(
        stopped.name,
        `[${timestamp}] INFO ${stopped.name.toUpperCase()} service stopped.`,
      );
      if (stopped.name === "frankenphp") {
        const workers = readMockFrankenphpOctaneWorkers();
        writeMockFrankenphpOctaneWorkers(
          Object.fromEntries(
            Object.entries(workers).map(([projectId, worker]) => [
              projectId,
              { ...worker, status: "stopped", pid: null, lastStoppedAt: timestamp, updatedAt: timestamp },
            ]),
          ),
        );
      }
      return upsertMockService(stopped) as T;
    }
    case "restart_service": {
      const service = getMockServiceOrThrow(String(args?.name ?? ""));
      const restarted: ServiceState = {
        ...service,
        pid: Math.floor(Math.random() * 40000) + 1000,
        status: "running",
        lastError: null,
        updatedAt: timestamp,
      };
      appendMockServiceLog(
        restarted.name,
        `[${timestamp}] WARN ${restarted.name.toUpperCase()} service restarted.`,
      );
      return upsertMockService(restarted) as T;
    }
    case "read_service_logs": {
      const name = String(args?.name ?? "") as ServiceState["name"];
      const lines = Number(args?.lines ?? 200);
      const logs = readMockServiceLogs();
      const selected = (logs[name] ?? []).slice(-lines);
      return ({
        name,
        totalLines: logs[name]?.length ?? 0,
        truncated: (logs[name]?.length ?? 0) > selected.length,
        lines: selected.map((text, index) => ({
          id: `${name}:${Math.max((logs[name]?.length ?? 0) - selected.length, 0) + index}`,
          text,
          severity: /error|fatal/i.test(text) ? "error" : /warn/i.test(text) ? "warning" : "info",
          lineNumber: Math.max((logs[name]?.length ?? 0) - selected.length, 0) + index + 1,
        })),
        content: selected.join("\n"),
      } satisfies ServiceLogPayload) as T;
    }
    case "clear_service_logs": {
      const name = String(args?.name ?? "") as ServiceState["name"];
      const logs = readMockServiceLogs();
      logs[name] = [];
      writeMockServiceLogs(logs);
      return true as T;
    }
    case "get_project_frankenphp_worker_settings":
    case "get_project_frankenphp_worker_status": {
      const projectId = String(args?.projectId ?? "");
      getMockProjectOrThrow(projectId);
      return getMockFrankenphpOctaneSettings(projectId) as T;
    }
    case "get_project_frankenphp_worker_health": {
      const projectId = String(args?.projectId ?? "");
      return buildMockFrankenphpOctaneHealth(projectId) as T;
    }
    case "update_project_frankenphp_worker_settings": {
      const projectId = String(args?.projectId ?? "");
      getMockProjectOrThrow(projectId);
      const input = (args?.input ?? {}) as UpdateFrankenphpOctaneWorkerSettingsInput;
      return updateMockFrankenphpOctaneSettings(projectId, {
        workerPort: input.workerPort,
        adminPort: input.adminPort,
        workers: input.workers,
        maxRequests: input.maxRequests,
        mode: input.mode,
        customWorkerRelativePath: input.customWorkerRelativePath,
        status: "stopped",
        pid: null,
      }) as T;
    }
    case "get_project_frankenphp_octane_preflight":
    case "get_project_frankenphp_worker_preflight": {
      const projectId = String(args?.projectId ?? "");
      return buildMockFrankenphpOctanePreflight(projectId) as T;
    }
    case "start_project_frankenphp_worker": {
      const projectId = String(args?.projectId ?? "");
      const project = getMockProjectOrThrow(projectId);
      const preflight = buildMockFrankenphpOctanePreflight(projectId);
      if (!preflight.ready || project.frankenphpMode === "classic") {
        throw {
          code: "FRANKENPHP_WORKER_PREFLIGHT_FAILED",
          message:
            project.frankenphpMode === "classic"
              ? "Switch this FrankenPHP project to a Worker mode before starting the worker."
              : preflight.summary,
        } satisfies AppError;
      }
      const services = readMockServices();
      writeMockServices(
        services.map((service) =>
          service.name === "frankenphp"
            ? { ...service, status: "running", pid: 4455, updatedAt: timestamp }
            : service,
        ),
      );
      const logs = readMockServiceLogs();
      const logKey = mockFrankenphpOctaneLogKey(projectId);
      logs[logKey] = [
        ...(logs[logKey] ?? []),
        `[${timestamp}] ${preflight.mode} worker started on 127.0.0.1:${getMockFrankenphpOctaneSettings(projectId).workerPort}.`,
      ];
      writeMockServiceLogs(logs);
      return updateMockFrankenphpOctaneSettings(projectId, {
        status: "running",
        pid: 8800 + Object.keys(readMockFrankenphpOctaneWorkers()).length,
        lastStartedAt: timestamp,
        lastError: null,
      }) as T;
    }
    case "stop_project_frankenphp_worker": {
      const projectId = String(args?.projectId ?? "");
      getMockProjectOrThrow(projectId);
      return updateMockFrankenphpOctaneSettings(projectId, {
        status: "stopped",
        pid: null,
        lastStoppedAt: timestamp,
      }) as T;
    }
    case "restart_project_frankenphp_worker": {
      const projectId = String(args?.projectId ?? "");
      getMockProjectOrThrow(projectId);
      return updateMockFrankenphpOctaneSettings(projectId, {
        status: "running",
        pid: 8899,
        lastStartedAt: timestamp,
        lastError: null,
      }) as T;
    }
    case "reload_project_frankenphp_worker": {
      const projectId = String(args?.projectId ?? "");
      const current = getMockFrankenphpOctaneSettings(projectId);
      if (current.status !== "running") {
        throw {
          code: "FRANKENPHP_WORKER_NOT_RUNNING",
          message: "Start the FrankenPHP worker before sending a reload signal.",
        } satisfies AppError;
      }
      const logs = readMockServiceLogs();
      const logKey = mockFrankenphpOctaneLogKey(projectId);
      logs[logKey] = [...(logs[logKey] ?? []), `[${timestamp}] FrankenPHP worker reload requested.`];
      writeMockServiceLogs(logs);
      return updateMockFrankenphpOctaneSettings(projectId, { lastError: null }) as T;
    }
    case "read_project_frankenphp_worker_logs": {
      const projectId = String(args?.projectId ?? "");
      const project = getMockProjectOrThrow(projectId);
      const lines = Number(args?.lines ?? 200);
      const logs = readMockServiceLogs();
      const logKey = mockFrankenphpOctaneLogKey(projectId);
      const selected = (logs[logKey] ?? []).slice(-lines);
      return ({
        name: `${project.name} ${getMockFrankenphpOctaneSettings(projectId).mode}`,
        totalLines: logs[logKey]?.length ?? 0,
        truncated: (logs[logKey]?.length ?? 0) > selected.length,
        lines: selected.map((text, index) => ({
          id: `${logKey}:${Math.max((logs[logKey]?.length ?? 0) - selected.length, 0) + index}`,
          text,
          severity: /error|fatal/i.test(text) ? "error" : /warn/i.test(text) ? "warning" : "info",
          lineNumber: Math.max((logs[logKey]?.length ?? 0) - selected.length, 0) + index + 1,
        })),
        content: selected.join("\n"),
      } satisfies ProjectWorkerLogPayload) as T;
    }
    case "read_project_worker_logs": {
      const workerId = String(args?.workerId ?? "");
      const worker = getMockWorkerOrThrow(workerId);
      const lines = Number(args?.lines ?? 200);
      const logs = readMockServiceLogs();
      const logKey = `worker:${workerId}`;
      const selected = (logs[logKey] ?? []).slice(-lines);
      return ({
        name: worker.name,
        totalLines: logs[logKey]?.length ?? 0,
        truncated: (logs[logKey]?.length ?? 0) > selected.length,
        lines: selected.map((text, index) => ({
          id: `${logKey}:${Math.max((logs[logKey]?.length ?? 0) - selected.length, 0) + index}`,
          text,
          severity: /error|fatal/i.test(text) ? "error" : /warn/i.test(text) ? "warning" : "info",
          lineNumber: Math.max((logs[logKey]?.length ?? 0) - selected.length, 0) + index + 1,
        })),
        content: selected.join("\n"),
      } satisfies ProjectWorkerLogPayload) as T;
    }
    case "clear_project_worker_logs": {
      const workerId = String(args?.workerId ?? "");
      const logs = readMockServiceLogs();
      logs[`worker:${workerId}`] = [];
      writeMockServiceLogs(logs);
      return true as T;
    }
    case "check_port": {
      const port = Number(args?.port ?? 0);
      const conflict = readMockServices().find(
        (service) => service.status === "running" && service.port === port,
      );
      return ({
        port,
        available: !conflict,
        pid: conflict?.pid ?? null,
        processName: conflict?.name ?? null,
      } satisfies PortCheckResult) as T;
    }
    case "run_diagnostics": {
      const projectId = String(args?.projectId ?? "unknown-project");
      return mockDiagnostics(projectId) as T;
    }
    case "list_repair_workflows": {
      return [
        {
          workflow: "project",
          title: "Repair Project",
          summary:
            "Rebuild managed config, reapply the local domain, and correct common project drift.",
          touches: ["project profile", "managed config", "hosts file"],
        },
        {
          workflow: "tunnel",
          title: "Repair Tunnel",
          summary:
            "Refresh project tunnel bindings and restart the local origin service if the public route drifted.",
          touches: ["tunnel state", "local origin", "managed aliases"],
        },
        {
          workflow: "runtimeLinks",
          title: "Repair Runtime Links",
          summary:
            "Rescan tracked runtimes and switch broken active runtime links to an available replacement.",
          touches: ["runtime inventory", "active runtime selection"],
        },
      ] satisfies RepairWorkflowInfo[] as T;
    }
    case "run_action_preflight": {
      const action = String(args?.action ?? "provisionProject") as ActionPreflightReport["action"];
      const projectId = args?.projectId ? String(args.projectId) : null;
      const project = projectId ? getMockProjectOrThrow(projectId) : null;
      const services = readMockServices();
      const runtimes = readMockRuntimes();
      const hosts = readMockHosts();
      const hostname = projectId ? readMockProjectPersistentHostnames()[projectId] : null;
      const setup = readMockPersistentTunnelSetup();
      const checks: ActionPreflightReport["checks"] = [];

      if (action === "restoreAppMetadata") {
        const runningServices = services.filter((service) => service.status === "running").length;
        checks.push({
          code: "WORKSPACE_IDLE",
          layer: "workspace",
          status: runningServices > 0 ? "error" : "ok",
          blocking: runningServices > 0,
          title: "Workspace process state",
          message:
            runningServices > 0
              ? `Stop ${runningServices} running service${runningServices === 1 ? "" : "s"} before restoring app metadata.`
              : "No managed services are currently running.",
          suggestion:
            runningServices > 0 ? "Use Services or Reliability to stop the workspace first." : null,
        });
      }

      if (project) {
        const serverRuntime = findMockServerRuntime(project, runtimes);
        const phpBinding = findMockPhpRuntimeForProject(project, runtimes, serverRuntime);
        checks.push({
          code: "SERVER_RUNTIME",
          layer: "runtime",
          status: serverRuntime?.status === "available" ? "ok" : "error",
          blocking: serverRuntime?.status !== "available",
          title: "Server runtime",
          message: serverRuntime
            ? `${project.serverType} ${serverRuntime.version} is linked at ${serverRuntime.path}.`
            : `No active ${project.serverType} runtime is linked.`,
          suggestion:
            serverRuntime?.status === "available"
              ? null
              : "Open Settings or run Repair Runtime Links before retrying.",
        });
        checks.push({
          code: "PHP_RUNTIME",
          layer: "runtime",
          status: phpBinding.matchesVersion ? "ok" : "error",
          blocking: !phpBinding.matchesVersion,
          title: "PHP runtime",
          message: phpBinding.runtime
            ? phpBinding.embedded
              ? `FrankenPHP ${phpBinding.runtime.version} embeds PHP ${phpBinding.resolvedPhpFamily ?? "unknown"} at ${phpBinding.runtime.path}.`
              : `PHP ${project.phpVersion} resolves to ${phpBinding.runtime.path}.`
            : phpBinding.embedded
              ? `No active FrankenPHP runtime is linked for the embedded PHP ${phpBinding.expectedPhpFamily} requirement.`
              : `No PHP ${project.phpVersion} runtime is linked.`,
          suggestion:
            phpBinding.matchesVersion
              ? null
              : phpBinding.embedded
                ? "Link or activate a FrankenPHP runtime with the matching embedded PHP family before retrying."
                : "Link or import the matching PHP runtime before retrying.",
        });

        if (action === "provisionProject") {
          checks.push({
            code: "HOSTS_STATE",
            layer: "dns",
            status: hosts[project.domain] ? "ok" : "warning",
            blocking: false,
            title: "Local domain mapping",
            message: hosts[project.domain]
              ? `${project.domain} already points to ${hosts[project.domain]}.`
              : `${project.domain} is not in the hosts file yet.`,
            suggestion: hosts[project.domain]
              ? null
              : "Provisioning will add the hosts entry and may trigger Windows elevation.",
          });
        }

        if (action === "publishPersistentDomain") {
          checks.push({
            code: "PERSISTENT_SETUP",
            layer: "tunnel",
            status: setup.ready ? "ok" : "error",
            blocking: !setup.ready,
            title: "Persistent tunnel setup",
            message: setup.details,
            suggestion: setup.guidance ?? null,
          });
          checks.push({
            code: "PERSISTENT_HOSTNAME",
            layer: "tunnel",
            status: hostname || setup.defaultHostnameZone ? "ok" : "error",
            blocking: !hostname && !setup.defaultHostnameZone,
            title: "Stable public hostname",
            message: hostname
              ? `${hostname.hostname} is reserved for this project.`
              : setup.defaultHostnameZone
                ? `DevNest can auto-generate a hostname under ${setup.defaultHostnameZone}.`
                : "No reserved hostname or default public zone is available yet.",
            suggestion:
              hostname || setup.defaultHostnameZone
                ? null
                : "Save a hostname on the project or set a default public zone in Settings.",
          });
        }
      }

      const ready = checks.every((check) => !check.blocking);
      return {
        action,
        projectId,
        ready,
        summary: ready
          ? "Reliability preflight passed for this action."
          : "Reliability preflight found blocking issues to fix first.",
        checks,
        generatedAt: timestamp,
      } satisfies ActionPreflightReport as T;
    }
    case "inspect_reliability_state": {
      const projectId = String(args?.projectId ?? "");
      const project = getMockProjectOrThrow(projectId);
      const diagnostics = mockDiagnostics(projectId);
      const runtimes = readMockRuntimes();
      const serverRuntime = findMockServerRuntime(project, runtimes);
      const phpBinding = findMockPhpRuntimeForProject(project, runtimes, serverRuntime);
      const mysqlRuntime =
        project.databaseName || project.databasePort
          ? runtimes.find((runtime) => runtime.runtimeType === "mysql" && runtime.isActive) ?? null
          : null;
      const configPreview = (() => {
        try {
          return buildMockConfigPreview(project);
        } catch (error) {
          return {
            serverType: project.serverType,
            outputPath: `.devnest/managed-configs/${project.serverType}/sites/${project.domain}.${project.serverType === "frankenphp" ? "caddy" : "conf"}`,
            configText:
              error instanceof Error
                ? error.message
                : "Managed config preview is unavailable until the project profile is corrected.",
          };
        }
      })();
      return {
        project,
        diagnostics,
        services: readMockServices(),
        config: {
          serverType: project.serverType,
          outputPath: configPreview.outputPath,
          preview: configPreview.configText,
          localDomainAliasPresent: Boolean(readMockHosts()[project.domain]),
          persistentAliasPresent: Boolean(readMockProjectPersistentHostnames()[projectId]),
        },
        runtime: {
          server: {
            kind: project.serverType,
            version: serverRuntime?.version ?? null,
            path: serverRuntime?.path ?? null,
            active: Boolean(serverRuntime?.isActive),
            available: serverRuntime?.status === "available",
            details: serverRuntime?.details ?? null,
          },
          php: {
            kind: phpBinding.embedded ? "frankenphp-embedded-php" : "php",
            version: phpBinding.resolvedPhpFamily ?? phpBinding.runtime?.version ?? null,
            path: phpBinding.runtime?.path ?? null,
            active: Boolean(phpBinding.runtime?.isActive),
            available: phpBinding.matchesVersion,
            details: phpBinding.runtime
              ? phpBinding.embedded
                ? `Embedded PHP ${phpBinding.resolvedPhpFamily ?? "unknown"} from FrankenPHP ${phpBinding.runtime.version}.`
                : phpBinding.runtime.details ?? null
              : null,
          },
          mysql: mysqlRuntime
            ? {
                kind: "mysql",
                version: mysqlRuntime.version,
                path: mysqlRuntime.path,
                active: Boolean(mysqlRuntime.isActive),
                available: mysqlRuntime.status === "available",
                details: mysqlRuntime.details ?? null,
              }
            : null,
          issues: phpBinding.matchesVersion
            ? []
            : [
                phpBinding.embedded
                  ? `FrankenPHP embeds PHP ${phpBinding.resolvedPhpFamily ?? "unknown"}, but the project expects PHP ${phpBinding.expectedPhpFamily}.`
                  : `No PHP runtime is linked for PHP ${phpBinding.expectedPhpFamily}.`,
              ],
        },
        quickTunnel: readMockProjectTunnels()[projectId] ?? null,
        persistentHostname: readMockProjectPersistentHostnames()[projectId] ?? null,
        persistentTunnel: readMockProjectPersistentTunnels()[projectId] ?? null,
        persistentHealth:
          readMockProjectPersistentHostnames()[projectId] || readMockProjectPersistentTunnels()[projectId]
            ? ({
                projectId,
                hostname: readMockProjectPersistentHostnames()[projectId]?.hostname ?? null,
                overallStatus: readMockProjectPersistentTunnels()[projectId]?.status ?? "starting",
                updatedAt: timestamp,
                checks: [
                  {
                    code: "setup",
                    label: "Named Tunnel Setup",
                    status: readMockPersistentTunnelSetup().ready ? "running" : "error",
                    message: readMockPersistentTunnelSetup().details,
                  },
                  {
                    code: "hostname",
                    label: "Reserved Hostname",
                    status: readMockProjectPersistentHostnames()[projectId] ? "running" : "error",
                    message: readMockProjectPersistentHostnames()[projectId]
                      ? `${readMockProjectPersistentHostnames()[projectId]?.hostname} is reserved for this project.`
                      : "No stable public hostname is reserved for this project yet.",
                  },
                ],
              } satisfies PersistentTunnelHealthReport)
            : null,
        generatedAt: timestamp,
      } satisfies ReliabilityInspectorSnapshot as T;
    }
    case "export_diagnostics_bundle": {
      const projectId = String(args?.projectId ?? "");
      const project = getMockProjectOrThrow(projectId);
      return {
        success: true,
        path: `${MOCK_APP_DATA_ROOT}/exports/${project.domain}-diagnostics.json`,
      } satisfies ReliabilityTransferResult as T;
    }
    case "backup_app_metadata": {
      return {
        success: true,
        path: `${MOCK_APP_DATA_ROOT}/exports/devnest-app-metadata-backup.json`,
      } satisfies ReliabilityTransferResult as T;
    }
    case "restore_app_metadata": {
      return {
        success: true,
        path: `${MOCK_APP_DATA_ROOT}/exports/devnest-app-metadata-backup.json`,
      } satisfies ReliabilityTransferResult as T;
    }
    case "run_repair_workflow": {
      const projectId = String(args?.projectId ?? "");
      const workflow = String(args?.workflow ?? "project") as RepairExecutionResult["workflow"];
      const project = getMockProjectOrThrow(projectId);
      if (workflow === "project") {
        writeMockProjects(
          readMockProjects().map((item) =>
            item.id === projectId
              ? { ...item, documentRoot: "public", updatedAt: timestamp }
              : item,
          ),
        );
        const hosts = readMockHosts();
        hosts[project.domain] = "127.0.0.1";
        writeMockHosts(hosts);
      }
      if (workflow === "runtimeLinks") {
        const expectedPhpFamily = mockPhpVersionFamily(project.phpVersion);
        const runtimes = readMockRuntimes().map((runtime) => {
          if (
            (
              runtime.runtimeType === project.serverType &&
              (project.serverType !== "frankenphp" ||
                mockRuntimePhpFamilyForItem(runtime) === expectedPhpFamily)
            ) ||
            (project.serverType !== "frankenphp" &&
              runtime.runtimeType === "php" &&
              mockRuntimePhpFamilyForItem(runtime) === expectedPhpFamily)
          ) {
            return {
              ...runtime,
              isActive: true,
              status: "available" as const,
              updatedAt: timestamp,
            };
          }
          if (
            runtime.runtimeType === project.serverType ||
            (project.serverType !== "frankenphp" && runtime.runtimeType === "php")
          ) {
            return { ...runtime, isActive: false, updatedAt: timestamp };
          }
          return runtime;
        });
        writeMockRuntimes(runtimes);
      }
      if (workflow === "tunnel") {
        const tunnels = readMockProjectTunnels();
        tunnels[projectId] = {
          projectId,
          provider: "cloudflared",
          status: "running",
          localUrl: `http://${project.domain}`,
          publicUrl: `https://${project.domain.replace(/\.test$/i, "")}.trycloudflare.com`,
          publicHostAliasSynced: true,
          logPath: `${MOCK_APP_DATA_ROOT}/logs/tunnels/${projectId}.log`,
          binaryPath: "cloudflared",
          updatedAt: timestamp,
          details: "Quick tunnel restarted in mock mode.",
        };
        writeMockProjectTunnels(tunnels);
      }

      return {
        workflow,
        success: true,
        message:
          workflow === "project"
            ? "Project profile, managed config, and hosts entry were refreshed."
            : workflow === "runtimeLinks"
              ? "Runtime inventory was rescanned and active links were refreshed."
              : "Tunnel state and local origin binding were refreshed.",
        touchedLayers:
          workflow === "project"
            ? ["project", "config", "dns"]
            : workflow === "runtimeLinks"
              ? ["runtime"]
              : ["tunnel", "service"],
        generatedAt: timestamp,
      } satisfies RepairExecutionResult as T;
    }
    case "apply_diagnostic_fix": {
      const projectId = String(args?.projectId ?? "");
      const code = String(args?.code ?? "");
      const project = getMockProjectOrThrow(projectId);

      if (code === "LARAVEL_DOCUMENT_ROOT_MISMATCH") {
        const updated: Project = {
          ...project,
          documentRoot: "public",
          updatedAt: timestamp,
        };
        writeMockProjects(
          readMockProjects().map((item) => (item.id === projectId ? updated : item)),
        );
        return {
          success: true,
          code,
          message: "Document root was updated to `public`.",
        } as T;
      }

      if (code === "SSL_AUTHORITY_MISSING" || code === "SSL_TRUST_MISSING") {
        writeMockSslAuthorityTrusted(true);
        return {
          success: true,
          code,
          message: "DevNest Local CA is now trusted for the current user.",
        } as T;
      }

      if (code === "SSL_CERTIFICATE_MISSING") {
        if (!project.sslEnabled) {
          throw {
            code: "SSL_NOT_ENABLED",
            message: "Enable SSL in the project profile before regenerating a certificate.",
          } satisfies AppError;
        }

        return {
          success: true,
          code,
          message: "Project SSL certificate was regenerated.",
        } as T;
      }

      throw {
        code: "DIAGNOSTIC_FIX_UNSUPPORTED",
        message: "This diagnostic does not support an automatic fix yet.",
      } satisfies AppError;
    }
    case "list_php_extensions": {
      const runtimeId = String(args?.runtimeId ?? "");
      const runtime = readMockRuntimes().find((item) => item.id === runtimeId);
      if (!isMockPhpToolsRuntime(runtime)) {
        throw {
          code: "RUNTIME_NOT_FOUND",
          message: "PHP tools runtime entry was not found.",
        } satisfies AppError;
      }

      return mockPhpExtensionsForRuntime(
        runtime.id,
        mockPhpToolsRuntimeLabel(runtime),
      ) as T;
    }
    case "list_php_extension_packages": {
      const runtimeId = String(args?.runtimeId ?? "");
      const runtime = readMockRuntimes().find((item) => item.id === runtimeId);
      if (!isMockPhpToolsRuntime(runtime)) {
        throw {
          code: "RUNTIME_NOT_FOUND",
          message: "PHP tools runtime entry was not found.",
        } satisfies AppError;
      }

      return mockPhpExtensionPackagesForRuntime(runtime) as T;
    }
    case "install_php_extension": {
      const runtimeId = String(args?.runtimeId ?? "");
      const runtime = readMockRuntimes().find((item) => item.id === runtimeId);
      if (!isMockPhpToolsRuntime(runtime)) {
        throw {
          code: "RUNTIME_NOT_FOUND",
          message: "PHP tools runtime entry was not found.",
        } satisfies AppError;
      }

      const availableExtensions = readMockPhpAvailableExtensions();
      const currentExtensions = availableExtensions[runtime.id] ?? defaultMockPhpAvailableExtensions();
      const nextExtensionName = `custom_${currentExtensions.length + 1}`;
      availableExtensions[runtime.id] = Array.from(new Set([...currentExtensions, nextExtensionName])).sort();
      writeMockPhpAvailableExtensions(availableExtensions);

      return ({
        runtimeId: runtime.id,
        runtimeVersion: mockPhpToolsRuntimeLabel(runtime),
        installedExtensions: [nextExtensionName],
        sourcePath: `${MOCK_APP_DATA_ROOT}/php-extensions/${nextExtensionName}.dll`,
      } satisfies PhpExtensionInstallResult) as T;
    }
    case "install_php_extension_package": {
      const runtimeId = String(args?.runtimeId ?? "");
      const packageId = String(args?.packageId ?? "");
      const runtime = readMockRuntimes().find((item) => item.id === runtimeId);
      if (!isMockPhpToolsRuntime(runtime)) {
        throw {
          code: "RUNTIME_NOT_FOUND",
          message: "PHP tools runtime entry was not found.",
        } satisfies AppError;
      }

      const extensionPackage = mockPhpExtensionPackagesForRuntime(runtime).find(
        (item) => item.id === packageId,
      );
      if (!extensionPackage) {
        throw {
          code: "PHP_EXTENSION_PACKAGE_NOT_FOUND",
          message: "PHP extension package was not found in browser mock mode.",
        } satisfies AppError;
      }

      const availableExtensions = readMockPhpAvailableExtensions();
      const currentExtensions = availableExtensions[runtime.id] ?? defaultMockPhpAvailableExtensions();
      availableExtensions[runtime.id] = Array.from(
        new Set([...currentExtensions, extensionPackage.extensionName]),
      ).sort();
      writeMockPhpAvailableExtensions(availableExtensions);

      const overrides = readMockPhpExtensionOverrides();
      overrides[runtime.id] = {
        ...(overrides[runtime.id] ?? {}),
        [extensionPackage.extensionName]: true,
      };
      writeMockPhpExtensionOverrides(overrides);

      return ({
        runtimeId: runtime.id,
        runtimeVersion: mockPhpToolsRuntimeLabel(runtime),
        installedExtensions: [extensionPackage.extensionName],
        sourcePath: extensionPackage.downloadUrl,
      } satisfies PhpExtensionInstallResult) as T;
    }
    case "remove_php_extension": {
      const runtimeId = String(args?.runtimeId ?? "");
      const extensionName = String(args?.extensionName ?? "").trim().toLowerCase();
      const runtime = readMockRuntimes().find((item) => item.id === runtimeId);
      if (!isMockPhpToolsRuntime(runtime)) {
        throw {
          code: "RUNTIME_NOT_FOUND",
          message: "PHP tools runtime entry was not found.",
        } satisfies AppError;
      }

      const availableExtensions = readMockPhpAvailableExtensions();
      const currentExtensions = availableExtensions[runtime.id] ?? defaultMockPhpAvailableExtensions();
      if (!currentExtensions.includes(extensionName)) {
        throw {
          code: "PHP_EXTENSION_NOT_AVAILABLE",
          message: `${mockPhpToolsRuntimeLabel(runtime)} does not expose the \`${extensionName}\` extension in browser mock mode.`,
        } satisfies AppError;
      }

      availableExtensions[runtime.id] = currentExtensions.filter(
        (item) => item !== extensionName,
      );
      writeMockPhpAvailableExtensions(availableExtensions);

      const overrides = readMockPhpExtensionOverrides();
      if (overrides[runtime.id]) {
        delete overrides[runtime.id]![extensionName];
        writeMockPhpExtensionOverrides(overrides);
      }

      return true as T;
    }
    case "set_php_extension_enabled": {
      const runtimeId = String(args?.runtimeId ?? "");
      const extensionName = String(args?.extensionName ?? "").trim().toLowerCase();
      const enabled = Boolean(args?.enabled);
      const runtime = readMockRuntimes().find((item) => item.id === runtimeId);
      if (!isMockPhpToolsRuntime(runtime)) {
        throw {
          code: "RUNTIME_NOT_FOUND",
          message: "PHP tools runtime entry was not found.",
        } satisfies AppError;
      }

      const states = mockPhpExtensionsForRuntime(runtime.id, mockPhpToolsRuntimeLabel(runtime));
      if (!states.some((item) => item.extensionName === extensionName)) {
        throw {
          code: "PHP_EXTENSION_NOT_AVAILABLE",
          message: `${mockPhpToolsRuntimeLabel(runtime)} does not expose the \`${extensionName}\` extension in browser mock mode.`,
        } satisfies AppError;
      }

      const overrides = readMockPhpExtensionOverrides();
      overrides[runtime.id] = {
        ...(overrides[runtime.id] ?? {}),
        [extensionName]: enabled,
      };
      writeMockPhpExtensionOverrides(overrides);

      return ({
        runtimeId: runtime.id,
        runtimeVersion: mockPhpToolsRuntimeLabel(runtime),
        extensionName,
        dllFile: `php_${extensionName}.dll`,
        enabled,
        updatedAt: timestamp,
      } satisfies PhpExtensionState) as T;
    }
    case "list_php_functions": {
      const runtimeId = String(args?.runtimeId ?? "");
      const runtime = readMockRuntimes().find((item) => item.id === runtimeId);
      if (!isMockPhpToolsRuntime(runtime)) {
        throw {
          code: "RUNTIME_NOT_FOUND",
          message: "PHP tools runtime entry was not found.",
        } satisfies AppError;
      }

      return mockPhpFunctionsForRuntime(
        runtime.id,
        mockPhpToolsRuntimeLabel(runtime),
      ) as T;
    }
    case "set_php_function_enabled": {
      const runtimeId = String(args?.runtimeId ?? "");
      const functionName = String(args?.functionName ?? "").trim().toLowerCase();
      const enabled = Boolean(args?.enabled);
      const runtime = readMockRuntimes().find((item) => item.id === runtimeId);
      if (!isMockPhpToolsRuntime(runtime)) {
        throw {
          code: "RUNTIME_NOT_FOUND",
          message: "PHP tools runtime entry was not found.",
        } satisfies AppError;
      }

      if (!managedMockPhpFunctions().includes(functionName)) {
        throw {
          code: "PHP_FUNCTION_NOT_MANAGED",
          message: `\`${functionName}\` is outside the managed browser mock function list.`,
        } satisfies AppError;
      }

      const overrides = readMockPhpFunctionOverrides();
      overrides[runtime.id] = {
        ...(overrides[runtime.id] ?? {}),
        [functionName]: enabled,
      };
      writeMockPhpFunctionOverrides(overrides);

      return ({
        runtimeId: runtime.id,
        runtimeVersion: mockPhpToolsRuntimeLabel(runtime),
        functionName,
        enabled,
        updatedAt: timestamp,
      } satisfies PhpFunctionState) as T;
    }
    case "get_runtime_config_schema": {
      const runtimeId = String(args?.runtimeId ?? "");
      const runtime = readMockRuntimes().find((item) => item.id === runtimeId);
      if (!runtime) {
        throw {
          code: "RUNTIME_NOT_FOUND",
          message: "Runtime entry was not found.",
        } satisfies AppError;
      }

      if (runtime.runtimeType !== "php" && !runtime.isActive) {
        throw {
          code: "RUNTIME_CONFIG_NOT_SUPPORTED",
          message:
            "Only the active Apache, Nginx, FrankenPHP, or MySQL runtime exposes the managed config file.",
        } satisfies AppError;
      }

      return mockRuntimeConfigSchema(runtime) as T;
    }
    case "get_runtime_config_values": {
      const runtimeId = String(args?.runtimeId ?? "");
      const runtime = readMockRuntimes().find((item) => item.id === runtimeId);
      if (!runtime) {
        throw {
          code: "RUNTIME_NOT_FOUND",
          message: "Runtime entry was not found.",
        } satisfies AppError;
      }

      if (runtime.runtimeType !== "php" && !runtime.isActive) {
        throw {
          code: "RUNTIME_CONFIG_NOT_SUPPORTED",
          message:
            "Only the active Apache, Nginx, FrankenPHP, or MySQL runtime exposes the managed config file.",
        } satisfies AppError;
      }

      return mockRuntimeConfigValues(runtime) as T;
    }
    case "update_runtime_config": {
      const runtimeId = String(args?.runtimeId ?? "");
      const patch = (args?.patch ?? {}) as Record<string, string>;
      const runtime = readMockRuntimes().find((item) => item.id === runtimeId);
      if (!runtime) {
        throw {
          code: "RUNTIME_NOT_FOUND",
          message: "Runtime entry was not found.",
        } satisfies AppError;
      }

      if (runtime.runtimeType === "mysql" || runtime.runtimeType === "frankenphp") {
        throw {
          code: "RUNTIME_CONFIG_NOT_SUPPORTED",
          message:
            runtime.runtimeType === "frankenphp"
              ? "FrankenPHP currently supports opening the managed Caddyfile only."
              : "MySQL currently supports opening the managed config file only.",
        } satisfies AppError;
      }

      if (runtime.runtimeType !== "php" && !runtime.isActive) {
        throw {
          code: "RUNTIME_CONFIG_NOT_SUPPORTED",
          message: "Only the active Apache or Nginx runtime exposes the managed config editor.",
        } satisfies AppError;
      }

      const overrides = readMockRuntimeConfigOverrides();
      overrides[runtime.id] = {
        ...(overrides[runtime.id] ?? {}),
        ...Object.fromEntries(
          Object.entries(patch).map(([key, value]) => [key, String(value).trim()]),
        ),
      };
      writeMockRuntimeConfigOverrides(overrides);

      return {
        ...mockRuntimeConfigValues(runtime),
        updatedAt: timestamp,
      } satisfies RuntimeConfigValues as T;
    }
    case "open_runtime_config_file": {
      const runtimeId = String(args?.runtimeId ?? "");
      const runtime = readMockRuntimes().find((item) => item.id === runtimeId);
      if (!runtime) {
        throw {
          code: "RUNTIME_NOT_FOUND",
          message: "Runtime entry was not found.",
        } satisfies AppError;
      }

      if (runtime.runtimeType !== "php" && !runtime.isActive) {
        throw {
          code: "RUNTIME_CONFIG_NOT_SUPPORTED",
          message:
            "Only the active Apache, Nginx, FrankenPHP, or MySQL runtime exposes the managed config file.",
        } satisfies AppError;
      }

      return true as T;
    }
    case "verify_runtime_path": {
      const runtimeType = String(args?.runtimeType ?? "php") as RuntimeInventoryItem["runtimeType"];
      const path = String(args?.path ?? "");
      if (!path.trim()) {
        throw {
          code: "RUNTIME_BINARY_NOT_FOUND",
          message: "Runtime binary path is required.",
        } satisfies AppError;
      }

      return ({
        id: `${runtimeType}-verified`,
        runtimeType,
        version: mockRuntimeVersionForType(runtimeType),
        phpFamily: mockRuntimePhpFamilyForType(runtimeType, mockRuntimeVersionForType(runtimeType)),
        path,
        isActive: false,
        source: "external",
        status: "available",
        createdAt: timestamp,
        updatedAt: timestamp,
        details: "Runtime binary verified successfully.",
      } satisfies RuntimeInventoryItem) as T;
    }
    case "install_runtime_package": {
      const packageId = String(args?.packageId ?? "");
      const setActive = Boolean(args?.setActive ?? true);
      const targetPackage = readMockRuntimePackages().find((item) => item.id === packageId);
      if (!targetPackage) {
        throw {
          code: "RUNTIME_PACKAGE_NOT_FOUND",
          message: "Runtime package was not found in the current manifest.",
        } satisfies AppError;
      }

      const path = mockManagedRuntimeBinaryPath(
        targetPackage.runtimeType,
        targetPackage.version,
        "downloaded",
      );

      const next = {
        id: `${targetPackage.runtimeType}-${targetPackage.version}`,
        runtimeType: targetPackage.runtimeType,
        version: targetPackage.version,
        phpFamily:
          targetPackage.phpFamily ??
          mockRuntimePhpFamilyForType(targetPackage.runtimeType, targetPackage.version),
        path,
        isActive: setActive,
        source: "downloaded",
        status: "available",
        createdAt: timestamp,
        updatedAt: timestamp,
        details: `Runtime package installed from ${targetPackage.displayName}.`,
      } satisfies RuntimeInventoryItem;
      writeMockRuntimeInstallTask({
        packageId: targetPackage.id,
        displayName: targetPackage.displayName,
        runtimeType: targetPackage.runtimeType,
        version: targetPackage.version,
        stage: "completed",
        message: `${targetPackage.displayName} installed successfully.`,
        updatedAt: timestamp,
        errorCode: null,
      });

      const runtimes = readMockRuntimes();
      const nextRuntimes = runtimes
        .map((item) =>
          item.runtimeType === targetPackage.runtimeType && setActive
            ? { ...item, isActive: false, updatedAt: timestamp }
            : item,
        )
        .filter((item) => item.id !== next.id);
      writeMockRuntimes([...nextRuntimes, next]);
      return next as T;
    }
    case "link_runtime_path": {
      const runtimeType = String(args?.runtimeType ?? "php") as RuntimeInventoryItem["runtimeType"];
      const path = String(args?.path ?? "");
      const setActive = Boolean(args?.setActive ?? true);
      if (!path.trim()) {
        throw {
          code: "RUNTIME_BINARY_NOT_FOUND",
          message: "Runtime binary path is required.",
        } satisfies AppError;
      }

      const version = mockRuntimeVersionForType(runtimeType);
      const next = {
        id: `${runtimeType}-${version}`,
        runtimeType,
        version,
        phpFamily: mockRuntimePhpFamilyForType(runtimeType, version),
        path,
        isActive: setActive,
        source: "external",
        status: "available",
        createdAt: timestamp,
        updatedAt: timestamp,
        details: null,
      } satisfies RuntimeInventoryItem;
      const runtimes = readMockRuntimes();
      const nextRuntimes = runtimes
        .map((item) =>
          item.runtimeType === runtimeType && setActive
            ? { ...item, isActive: false, updatedAt: timestamp }
            : item,
        )
        .filter((item) => item.id !== next.id);
      writeMockRuntimes([...nextRuntimes, next]);
      return next as T;
    }
    case "import_runtime_path": {
      const runtimeType = String(args?.runtimeType ?? "php") as RuntimeInventoryItem["runtimeType"];
      const path = String(args?.path ?? "");
      const setActive = Boolean(args?.setActive ?? true);
      if (!path.trim()) {
        throw {
          code: "RUNTIME_BINARY_NOT_FOUND",
          message: "Runtime binary path is required.",
        } satisfies AppError;
      }

      const version = mockRuntimeVersionForType(runtimeType);
      const managedPath = mockManagedRuntimeBinaryPath(runtimeType, version, "managed");
      const next = {
        id: `${runtimeType}-${version}`,
        runtimeType,
        version,
        phpFamily: mockRuntimePhpFamilyForType(runtimeType, version),
        path: managedPath,
        isActive: setActive,
        source: "imported",
        status: "available",
        createdAt: timestamp,
        updatedAt: timestamp,
        details: `Runtime copied into the managed DevNest runtime root from ${path}.`,
      } satisfies RuntimeInventoryItem;
      const runtimes = readMockRuntimes();
      const nextRuntimes = runtimes
        .map((item) =>
          item.runtimeType === runtimeType && setActive
            ? { ...item, isActive: false, updatedAt: timestamp }
            : item,
        )
        .filter((item) => item.id !== next.id);
      writeMockRuntimes([...nextRuntimes, next]);
      return next as T;
    }
    case "set_active_runtime": {
      const runtimeId = String(args?.runtimeId ?? "");
      const runtimes = readMockRuntimes();
      const target = runtimes.find((item) => item.id === runtimeId);
      if (!target) {
        throw {
          code: "RUNTIME_NOT_FOUND",
          message: "Runtime entry was not found.",
        } satisfies AppError;
      }

      const nextRuntimes = runtimes.map((item) => {
        if (item.runtimeType !== target.runtimeType) {
          return item;
        }

        return {
          ...item,
          isActive: item.id === target.id,
          updatedAt: timestamp,
        };
      });
      writeMockRuntimes(nextRuntimes);
      return ({ ...target, isActive: true, updatedAt: timestamp } satisfies RuntimeInventoryItem) as T;
    }
    case "remove_runtime_reference": {
      const runtimeId = String(args?.runtimeId ?? "");
      const runtimes = readMockRuntimes();
      const target = runtimes.find((item) => item.id === runtimeId);
      if (!target) {
        throw {
          code: "RUNTIME_NOT_FOUND",
          message: "Runtime entry was not found.",
        } satisfies AppError;
      }

      const dependentProjects = readMockProjects().filter((project) => {
        if (target.runtimeType === "php") {
          return (
            project.serverType !== "frankenphp" &&
            mockPhpVersionFamily(project.phpVersion) === mockRuntimePhpFamilyForItem(target)
          );
        }

        if (
          (target.runtimeType === "apache" ||
            target.runtimeType === "nginx" ||
            target.runtimeType === "frankenphp") &&
          target.isActive
        ) {
          return project.serverType === target.runtimeType;
        }

        if (target.runtimeType === "mysql" && target.isActive) {
          return Boolean(project.databaseName || project.databasePort);
        }

        return false;
      });

      if (dependentProjects.length > 0) {
        throw {
          code: "RUNTIME_IN_USE",
          message:
            target.runtimeType === "php"
              ? `PHP ${target.version} is still referenced by tracked projects. Move those projects to another PHP version before removing this runtime.`
              : target.runtimeType === "mysql"
                ? "The active MySQL runtime is still needed by tracked database projects. Set another MySQL runtime active before removing this one."
                : `The active ${target.runtimeType} runtime is still needed by tracked projects. Set another ${target.runtimeType} runtime active before removing this one.`,
          details: `Dependent projects: ${dependentProjects
            .map((project) => `${project.name} (${project.domain})`)
            .join(", ")}`,
        } satisfies AppError;
      }

      writeMockRuntimes(runtimes.filter((item) => item.id !== runtimeId));
      return true as T;
    }
    case "reveal_runtime_path": {
      const runtimeId = String(args?.runtimeId ?? "");
      const runtime = readMockRuntimes().find((item) => item.id === runtimeId);
      if (!runtime) {
        throw {
          code: "RUNTIME_NOT_FOUND",
          message: "Runtime entry was not found.",
        } satisfies AppError;
      }

      return true as T;
    }
    default:
      return getMockResponse<T>(command);
  }
}

export async function tauriInvoke<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauriRuntime()) {
    return getMockResponseWithArgs<T>(command, args);
  }

  try {
    return await invoke<T>(command, args);
  } catch (error) {
    const normalized =
      typeof error === "object" && error !== null && "code" in error && "message" in error
        ? (error as AppError)
        : {
            code: "UNKNOWN_TAURI_ERROR",
            message: "An unexpected error occurred while invoking a native command.",
            details: error,
          };

    throw normalized;
  }
}
