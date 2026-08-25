import { useEffect, useMemo, useRef, useState } from "react";
import {
  createBrowserRouter,
  useNavigate,
  useSearchParams,
} from "react-router-dom";
import { AppShell } from "@/components/layout/app-shell";
import {
  runAsyncAction,
  useAsyncActionPending,
} from "@/app/store/async-action-store";
import { useToastStore } from "@/app/store/toast-store";
import { AddProjectWizard } from "@/components/projects/add-project-wizard";
import { MetricCard } from "@/components/dashboard/metric-card";
import { LogViewer } from "@/components/logs/log-viewer";
import { ProjectCard } from "@/components/projects/project-card";
import { ProjectInspector } from "@/components/projects/project-inspector";
import { ProjectScheduledTaskPanel } from "@/components/tasks/project-scheduled-task-panel";
import { ProjectWorkerPanel } from "@/components/workers/project-worker-panel";
import { ReliabilityWorkbench } from "@/components/reliability/reliability-workbench";
import { RecipeStudio } from "@/components/recipes/recipe-studio";
import { RuntimeConfigDialog } from "@/components/settings/runtime-config-dialog";
import { RedisManager } from "@/components/settings/redis-manager";
import { ServiceInspector } from "@/components/services/service-inspector";
import { ServiceTable } from "@/components/services/service-table";
import { ActionMenu, ActionMenuItem } from "@/components/ui/action-menu";
import { useDiagnosticsStore } from "@/app/store/diagnostics-store";
import { useProjectStore } from "@/app/store/project-store";
import { useProjectScheduledTaskStore } from "@/app/store/project-scheduled-task-store";
import { useProjectWorkerStore } from "@/app/store/project-worker-store";
import { useServiceStore } from "@/app/store/service-store";
import { useWorkspaceStore } from "@/app/store/workspace-store";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { StickyTabs } from "@/components/ui/sticky-tabs";
import { appApi } from "@/lib/api/app-api";
import { databaseApi } from "@/lib/api/database-api";
import { configApi } from "@/lib/api/config-api";
import { diagnosticsApi } from "@/lib/api/diagnostics-api";
import { downloadCacheApi } from "@/lib/api/download-cache-api";
import { optionalToolApi } from "@/lib/api/optional-tool-api";
import { persistentTunnelApi } from "@/lib/api/persistent-tunnel-api";
import { projectProfileApi } from "@/lib/api/project-profile-api";
import { projectScheduledTaskApi } from "@/lib/api/project-scheduled-task-api";
import { projectWorkerApi } from "@/lib/api/project-worker-api";
import { serviceApi } from "@/lib/api/service-api";
import { runtimeApi } from "@/lib/api/runtime-api";
import {
  getLiveProjectStatus,
  getStatusTone,
  summarizeDiagnostics,
} from "@/lib/project-health";
import { getAppErrorMessage, tauriInvoke, type AppError } from "@/lib/tauri";
import { runtimeVersionFamily } from "@/lib/runtime-version";
import { databaseNameSchema } from "@/lib/validators";
import { formatUpdatedAt } from "@/lib/utils";
import type {
  DatabaseSnapshotSummary,
  DatabaseTimeMachineStatus,
} from "@/types/database";
import type { DiagnosticItem } from "@/types/diagnostics";
import type { DownloadCacheSummary } from "@/types/download-cache";
import type {
  OptionalToolInstallStage,
  OptionalToolInstallTask,
  OptionalToolInventoryItem,
  OptionalToolPackage,
  OptionalToolType,
} from "@/types/optional-tool";
import type {
  PersistentTunnelNamedTunnelSummary,
  PersistentTunnelSetupStatus,
} from "@/types/persistent-tunnel";
import type { UpdateProjectPatch } from "@/types/project";
import type { ProjectScheduledTaskRunLogPayload } from "@/types/project-scheduled-task";
import type { ProjectWorkerLogPayload } from "@/types/project-worker";
import type {
  PhpExtensionPackage,
  PhpExtensionState,
  PhpFunctionState,
  RuntimeInstallStage,
  RuntimeInstallTask,
  RuntimeInventoryItem,
  RuntimePackage,
  RuntimeType,
} from "@/types/runtime";
import type {
  RuntimeConfigSchema,
  RuntimeConfigValues,
} from "@/types/runtime-config";
import type {
  PortCheckResult,
  ServiceLogPayload,
  ServiceName,
  ServiceState,
} from "@/types/service";
import type {
  AppReleaseInfo,
  AppUpdateCheckResult,
  AppUpdateState,
} from "@/types/update";
import {
  databaseSnapshotBackendLabel,
  databaseSnapshotTriggerLabel,
  formatDatabaseScheduleLabel,
  formatFileSize,
  getDatabaseTimeMachinePresentation,
} from "@/app/routes/database-route-utils";

import {
  SERVICE_START_ORDER,
  PROJECTS_VIEW_STORAGE_KEY,
  SETTINGS_UPDATE_LAST_CHECKED_KEY,
  PageLayout,
  LoadingScrim,
  useDelayedBusy,
  getStartAllPlan,
  mergeSearchParams,
  parseProjectsViewMode,
  diagnosticActionLabel,
  diagnosticCanAutoFix,
  runtimeTypeLabel,
  phpCliActivationMessage,
  withRuntimeDetails,
  runtimeSourceLabel,
  runtimeFamilyLabel,
  runtimeInstallStageLabel,
  optionalToolLabel,
  optionalToolFamilyLabel,
  optionalToolInstallStageLabel,
  findOptionalToolUpdatePackage,
  compareRuntimeVersions,
  normalizeCatalogVersion,
  displayCatalogVersion,
  optionalToolHealthLabel,
  findRuntimeUpdatePackage,
  runtimeCanOfferUpdateTo,
  runtimeCatalogKey,
  serviceLabel,
  optionalToolTypeForService,
  phpExtensionLabel,
  RECOMMENDED_PHP_EXTENSIONS,
  RECOMMENDED_PHP_EXTENSION_BY_NAME,
  isPhpExtensionDisabledByDefault,
  phpExtensionAvailabilityLabel,
  phpExtensionAvailabilityNote,
  matchesPhpToolsSearch,
} from "./route-shared";

export default function DatabasesRoute() {
  const [searchParams, setSearchParams] = useSearchParams();
  const pushToast = useToastStore((state) => state.push);
  const projects = useProjectStore((state) => state.projects);
  const updateProject = useProjectStore((state) => state.updateProject);
  const actionName = useServiceStore((state) => state.actionName);
  const servicesLoaded = useServiceStore((state) => state.loaded);
  const services = useServiceStore((state) => state.services);
  const startService = useServiceStore((state) => state.startService);
  const [databases, setDatabases] = useState<string[]>([]);
  const [timeMachineStatusByDatabase, setTimeMachineStatusByDatabase] =
    useState<Record<string, DatabaseTimeMachineStatus>>({});
  const [timeMachineStatusLoaded, setTimeMachineStatusLoaded] = useState(false);
  const [timeMachineStatusLoading, setTimeMachineStatusLoading] =
    useState(false);
  const timeMachineStatusRequestRef = useRef(0);
  const [runtimeInventory, setRuntimeInventory] = useState<
    RuntimeInventoryItem[]
  >([]);
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState<string>();
  const [createName, setCreateName] = useState("");
  const [createBusy, setCreateBusy] = useState(false);
  const [databaseSearch, setDatabaseSearch] = useState("");
  const [linkDrafts, setLinkDrafts] = useState<Record<string, string>>({});
  const [linkingProjectId, setLinkingProjectId] = useState<string>();
  const [dropTarget, setDropTarget] = useState<string>();
  const [dropBusy, setDropBusy] = useState(false);
  const [transferActionKey, setTransferActionKey] = useState<string>();
  const [timeMachineActionKey, setTimeMachineActionKey] = useState<string>();
  const [snapshotDialogDatabase, setSnapshotDialogDatabase] = useState<
    string | null
  >(null);
  const [snapshotDialogMode, setSnapshotDialogMode] = useState<
    "history" | "rollback"
  >("history");
  const [snapshotDialogLoading, setSnapshotDialogLoading] = useState(false);
  const [snapshotDialogError, setSnapshotDialogError] = useState<string>();
  const [snapshotDialogSnapshots, setSnapshotDialogSnapshots] = useState<
    DatabaseSnapshotSummary[]
  >([]);
  const [selectedRollbackSnapshotId, setSelectedRollbackSnapshotId] =
    useState("");
  const [rollbackConfirmationInput, setRollbackConfirmationInput] =
    useState("");

  const mysqlService = services.find((service) => service.name === "mysql");
  const activeMysqlRuntime =
    runtimeInventory.find(
      (runtime) => runtime.runtimeType === "mysql" && runtime.isActive,
    ) ?? null;
  const mysqlRunning = mysqlService?.status === "running";
  const linkedProjects = projects.filter((project) => project.databaseName);
  const linkedProjectCount = linkedProjects.length;
  const mysqlPort = mysqlService?.port ?? 3306;
  const databaseTabs = [
    {
      id: "overview",
      label: "Overview",
      meta: mysqlRunning ? `MySQL on ${mysqlPort}` : "MySQL stopped",
    },
    { id: "databases", label: "Databases", meta: `${databases.length} local` },
    {
      id: "links",
      label: "Project Links",
      meta: `${linkedProjectCount} linked`,
    },
  ] as const;
  const activeTab = (() => {
    const tab = searchParams.get("tab");
    if (tab === "databases" || tab === "links") {
      return tab;
    }
    return "overview";
  })();

  function handleSelectTab(tab: "overview" | "databases" | "links") {
    setSearchParams(
      mergeSearchParams(searchParams, {
        tab: tab === "overview" ? undefined : tab,
      }),
    );
  }

  function isTimeMachineBusy(databaseName: string) {
    return Boolean(timeMachineActionKey?.endsWith(`:${databaseName}`));
  }

  function buildTimeMachineStatusFallback(
    databaseName: string,
    error: unknown,
  ): DatabaseTimeMachineStatus {
    return {
      name: databaseName,
      enabled: false,
      status: "error",
      snapshotCount: 0,
      scheduleEnabled: true,
      scheduleIntervalMinutes: 5,
      linkedProjectActionSnapshotsEnabled: true,
      latestSnapshotAt: null,
      nextScheduledSnapshotAt: null,
      lastError: getAppErrorMessage(
        error,
        "Time Machine status could not be loaded.",
      ),
    };
  }

  useEffect(() => {
    setLinkDrafts(
      Object.fromEntries(
        projects.map((project) => [project.id, project.databaseName ?? ""]),
      ),
    );
  }, [projects]);

  useEffect(() => {
    if (!dropTarget) {
      return;
    }

    function handleKeydown(event: KeyboardEvent) {
      if (event.key !== "Escape" || dropBusy) {
        return;
      }

      event.preventDefault();
      setDropTarget(undefined);
    }

    document.addEventListener("keydown", handleKeydown);
    return () => document.removeEventListener("keydown", handleKeydown);
  }, [dropBusy, dropTarget]);

  async function loadDatabaseWorkspace() {
    setLoading(true);
    setLoadError(undefined);

    try {
      const [runtimes, nextDatabases] = await Promise.all([
        runtimeApi.list(),
        mysqlRunning ? databaseApi.list() : Promise.resolve([]),
      ]);
      setRuntimeInventory(runtimes);
      setDatabases(nextDatabases);

      if (!mysqlRunning || nextDatabases.length === 0) {
        timeMachineStatusRequestRef.current += 1;
        setTimeMachineStatusByDatabase({});
        setTimeMachineStatusLoaded(false);
        setTimeMachineStatusLoading(false);
        return;
      }
      timeMachineStatusRequestRef.current += 1;
      setTimeMachineStatusByDatabase({});
      setTimeMachineStatusLoaded(false);
    } catch (error) {
      timeMachineStatusRequestRef.current += 1;
      setDatabases([]);
      setTimeMachineStatusByDatabase({});
      setTimeMachineStatusLoaded(false);
      setTimeMachineStatusLoading(false);
      setLoadError(
        getAppErrorMessage(error, "Could not load the database workspace."),
      );
    } finally {
      setLoading(false);
    }
  }

  async function loadTimeMachineStatuses(databaseNames: string[]) {
    if (databaseNames.length === 0) {
      timeMachineStatusRequestRef.current += 1;
      setTimeMachineStatusByDatabase({});
      setTimeMachineStatusLoaded(false);
      setTimeMachineStatusLoading(false);
      return;
    }

    const requestId = timeMachineStatusRequestRef.current + 1;
    timeMachineStatusRequestRef.current = requestId;
    setTimeMachineStatusLoading(true);

    try {
      const statuses = await Promise.all(
        databaseNames.map(async (databaseName) => {
          try {
            return [
              databaseName,
              await databaseApi.getTimeMachineStatus(databaseName),
            ] as const;
          } catch (error) {
            return [
              databaseName,
              buildTimeMachineStatusFallback(databaseName, error),
            ] as const;
          }
        }),
      );
      if (requestId !== timeMachineStatusRequestRef.current) {
        return;
      }
      setTimeMachineStatusByDatabase(Object.fromEntries(statuses));
      setTimeMachineStatusLoaded(true);
    } catch {
      if (requestId !== timeMachineStatusRequestRef.current) {
        return;
      }
      setTimeMachineStatusByDatabase({});
      setTimeMachineStatusLoaded(false);
    } finally {
      if (requestId === timeMachineStatusRequestRef.current) {
        setTimeMachineStatusLoading(false);
      }
    }
  }

  useEffect(() => {
    if (!servicesLoaded) {
      return;
    }

    void loadDatabaseWorkspace();
  }, [actionName, mysqlRunning, servicesLoaded]);

  useEffect(() => {
    if (
      activeTab !== "databases" ||
      !mysqlRunning ||
      databases.length === 0 ||
      timeMachineStatusLoaded ||
      timeMachineStatusLoading
    ) {
      return;
    }

    void loadTimeMachineStatuses(databases);
  }, [
    activeTab,
    databases,
    mysqlRunning,
    timeMachineStatusLoaded,
    timeMachineStatusLoading,
  ]);

  useEffect(() => {
    if (activeTab !== "databases" || !mysqlRunning || databases.length === 0) {
      return;
    }

    const intervalId = window.setInterval(() => {
      void loadTimeMachineStatuses(databases);
    }, 30000);

    return () => window.clearInterval(intervalId);
  }, [activeTab, databases, mysqlRunning]);

  async function refreshSnapshotDialogData(
    databaseName: string,
    resetSelection = false,
  ) {
    const [snapshots, status] = await Promise.all([
      databaseApi.listSnapshots(databaseName),
      databaseApi
        .getTimeMachineStatus(databaseName)
        .catch((error) => buildTimeMachineStatusFallback(databaseName, error)),
    ]);

    setSnapshotDialogSnapshots(snapshots);
    setSelectedRollbackSnapshotId((current) => {
      if (resetSelection || !current) {
        return snapshots[0]?.id ?? "";
      }

      return snapshots.some((snapshot) => snapshot.id === current)
        ? current
        : (snapshots[0]?.id ?? "");
    });
    setTimeMachineStatusByDatabase((current) => ({
      ...current,
      [databaseName]: status,
    }));
    setSnapshotDialogError(undefined);
  }

  async function loadSnapshotHistory(
    databaseName: string,
    mode: "history" | "rollback",
  ) {
    setSnapshotDialogDatabase(databaseName);
    setSnapshotDialogMode(mode);
    setSnapshotDialogLoading(true);
    setSnapshotDialogError(undefined);
    setSnapshotDialogSnapshots([]);
    setSelectedRollbackSnapshotId("");
    setRollbackConfirmationInput("");
    setTimeMachineActionKey(`history:${databaseName}`);

    try {
      await refreshSnapshotDialogData(databaseName, true);
    } catch (error) {
      setSnapshotDialogError(
        getAppErrorMessage(error, "Could not load managed snapshots."),
      );
    } finally {
      setSnapshotDialogLoading(false);
      setTimeMachineActionKey(undefined);
    }
  }

  useEffect(() => {
    if (
      !snapshotDialogDatabase ||
      !mysqlRunning ||
      snapshotDialogLoading ||
      isTimeMachineBusy(snapshotDialogDatabase)
    ) {
      return;
    }

    const intervalId = window.setInterval(() => {
      void refreshSnapshotDialogData(snapshotDialogDatabase).catch((error) => {
        setSnapshotDialogError(
          getAppErrorMessage(error, "Could not refresh managed snapshots."),
        );
      });
    }, 30000);

    return () => window.clearInterval(intervalId);
  }, [
    mysqlRunning,
    snapshotDialogDatabase,
    snapshotDialogLoading,
    timeMachineActionKey,
  ]);

  async function handleCreateDatabase() {
    const parsed = databaseNameSchema.safeParse(createName);
    if (!parsed.success) {
      pushToast({
        tone: "error",
        title: "Create Database Failed",
        message: parsed.error.issues[0]?.message ?? "Database name is invalid.",
      });
      return;
    }

    setCreateBusy(true);
    try {
      const result = await databaseApi.create(parsed.data);
      setCreateName("");
      await loadDatabaseWorkspace();
      pushToast({
        tone: "success",
        title: "Database Created",
        message: `${result.name} is ready to link with a project.`,
      });
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Create Database Failed",
        message: getAppErrorMessage(
          error,
          "Could not create the requested database.",
        ),
      });
    } finally {
      setCreateBusy(false);
    }
  }

  async function handleStartMysql() {
    try {
      await startService("mysql");
      pushToast({
        tone: "success",
        title: "MySQL Running",
        message: "Database tools are ready.",
      });
      await loadDatabaseWorkspace();
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Start MySQL Failed",
        message: getAppErrorMessage(error, "Could not start MySQL."),
      });
    }
  }

  async function handleSaveProjectLink(projectId: string) {
    const nextDatabaseName = (linkDrafts[projectId] ?? "").trim();
    setLinkingProjectId(projectId);

    try {
      const project = projects.find((item) => item.id === projectId);
      await updateProject(projectId, {
        databaseName: nextDatabaseName || null,
        databasePort: nextDatabaseName ? mysqlPort : null,
      });
      pushToast({
        tone: "success",
        title: nextDatabaseName ? "Project Linked" : "Database Unlinked",
        message: project
          ? nextDatabaseName
            ? `${project.name} now points to ${nextDatabaseName}.`
            : `${project.name} no longer points to a local database.`
          : "Project database metadata was updated.",
      });
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Link Update Failed",
        message: getAppErrorMessage(
          error,
          "Could not update the project database metadata.",
        ),
      });
    } finally {
      setLinkingProjectId(undefined);
    }
  }

  async function handleDropDatabase() {
    if (!dropTarget) {
      return;
    }

    setDropBusy(true);
    try {
      const result = await databaseApi.drop(dropTarget);
      setDropTarget(undefined);
      await loadDatabaseWorkspace();
      pushToast({
        tone: "success",
        title: "Database Removed",
        message: `${result.name} was removed from the local MySQL runtime.`,
      });
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Delete Database Failed",
        message: getAppErrorMessage(
          error,
          "Could not delete the selected database.",
        ),
      });
    } finally {
      setDropBusy(false);
    }
  }

  async function handleBackupDatabase(databaseName: string) {
    setTransferActionKey(`backup:${databaseName}`);

    try {
      const result = await databaseApi.backup(databaseName);
      if (!result) {
        return;
      }

      pushToast({
        tone: "success",
        title: "Database Backup Ready",
        message: `${result.name} was exported to ${result.path}.`,
      });
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Backup Failed",
        message: getAppErrorMessage(
          error,
          "Could not export the selected database.",
        ),
      });
    } finally {
      setTransferActionKey(undefined);
    }
  }

  async function handleRestoreDatabase(databaseName: string) {
    setTransferActionKey(`restore:${databaseName}`);

    try {
      const result = await databaseApi.restore(databaseName);
      if (!result) {
        return;
      }

      await loadDatabaseWorkspace();
      pushToast({
        tone: "success",
        title: "Database Restored",
        message: `${result.name} was restored from ${result.path}.`,
      });
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Restore Failed",
        message: getAppErrorMessage(
          error,
          "Could not restore the selected SQL backup.",
        ),
      });
    } finally {
      setTransferActionKey(undefined);
    }
  }

  async function handleTakeSnapshot(databaseName: string) {
    setTimeMachineActionKey(`snapshot:${databaseName}`);

    try {
      const result = await databaseApi.takeSnapshot(databaseName);
      setTimeMachineStatusByDatabase((current) => ({
        ...current,
        [databaseName]: result.status,
      }));
      setSnapshotDialogSnapshots((current) => {
        if (snapshotDialogDatabase !== databaseName) {
          return current;
        }

        return [
          result.snapshot,
          ...current.filter((snapshot) => snapshot.id !== result.snapshot.id),
        ].slice(0, 3);
      });
      setSelectedRollbackSnapshotId((current) =>
        snapshotDialogDatabase === databaseName
          ? current || result.snapshot.id
          : current,
      );
      pushToast({
        tone: "success",
        title: "Snapshot Captured",
        message:
          result.status.snapshotCount === 1
            ? `${databaseName} is now protected with its first managed snapshot.`
            : `${databaseName} snapshot ${formatUpdatedAt(result.snapshot.createdAt)} is ready.`,
      });
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Snapshot Failed",
        message: getAppErrorMessage(
          error,
          "Could not create a managed database snapshot.",
        ),
      });
    } finally {
      setTimeMachineActionKey(undefined);
    }
  }

  async function handleToggleTimeMachine(
    databaseName: string,
    enabled: boolean,
  ) {
    setTimeMachineActionKey(`toggle:${databaseName}`);

    try {
      const nextStatus = enabled
        ? await databaseApi.enableTimeMachine(databaseName)
        : await databaseApi.disableTimeMachine(databaseName);
      setTimeMachineStatusByDatabase((current) => ({
        ...current,
        [databaseName]: nextStatus,
      }));
      pushToast({
        tone: "success",
        title: enabled ? "Time Machine Enabled" : "Time Machine Disabled",
        message: enabled
          ? `${databaseName} will keep a managed ring of local snapshots.`
          : `${databaseName} will stop taking managed pre-action snapshots until you enable protection again.`,
      });
    } catch (error) {
      pushToast({
        tone: "error",
        title: enabled
          ? "Enable Protection Failed"
          : "Disable Protection Failed",
        message: getAppErrorMessage(
          error,
          "Could not update the Time Machine state.",
        ),
      });
    } finally {
      setTimeMachineActionKey(undefined);
    }
  }

  async function handleRollbackSnapshot() {
    if (
      !snapshotDialogDatabase ||
      !selectedRollbackSnapshotId ||
      rollbackConfirmationInput.trim() !== snapshotDialogDatabase
    ) {
      return;
    }

    setTimeMachineActionKey(`rollback:${snapshotDialogDatabase}`);

    try {
      const result = await databaseApi.rollbackSnapshot(
        snapshotDialogDatabase,
        selectedRollbackSnapshotId,
      );
      setSnapshotDialogDatabase(null);
      setRollbackConfirmationInput("");
      await loadDatabaseWorkspace();
      pushToast({
        tone: "success",
        title: "Database Rolled Back",
        message: result.safetySnapshotId
          ? `${result.name} was restored from ${formatUpdatedAt(result.restoredSnapshot.createdAt)}. DevNest kept a safety snapshot first.`
          : `${result.name} was restored from ${formatUpdatedAt(result.restoredSnapshot.createdAt)}.`,
      });
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Rollback Failed",
        message: getAppErrorMessage(
          error,
          "Could not roll back the selected database snapshot.",
        ),
      });
    } finally {
      setTimeMachineActionKey(undefined);
    }
  }

  const snapshotDialogStatus = snapshotDialogDatabase
    ? timeMachineStatusByDatabase[snapshotDialogDatabase]
    : undefined;
  const snapshotDialogBusy = snapshotDialogDatabase
    ? isTimeMachineBusy(snapshotDialogDatabase)
    : false;
  useEffect(() => {
    if (!snapshotDialogDatabase) {
      return;
    }

    function handleKeydown(event: KeyboardEvent) {
      if (
        event.key !== "Escape" ||
        snapshotDialogBusy ||
        snapshotDialogLoading
      ) {
        return;
      }

      event.preventDefault();
      setSnapshotDialogDatabase(null);
      setRollbackConfirmationInput("");
    }

    document.addEventListener("keydown", handleKeydown);
    return () => document.removeEventListener("keydown", handleKeydown);
  }, [snapshotDialogBusy, snapshotDialogDatabase, snapshotDialogLoading]);

  const protectedDatabaseCount = Object.values(
    timeMachineStatusByDatabase,
  ).filter((status) => status.enabled).length;
  const protectedDatabaseCountPending =
    mysqlRunning && databases.length > 0 && !timeMachineStatusLoaded;
  const normalizedDatabaseSearch = databaseSearch.trim().toLowerCase();
  const filteredDatabases = useMemo(
    () =>
      normalizedDatabaseSearch.length === 0
        ? databases
        : databases.filter((databaseName) =>
            databaseName.toLowerCase().includes(normalizedDatabaseSearch),
          ),
    [databases, normalizedDatabaseSearch],
  );
  const selectedRollbackSnapshot = snapshotDialogSnapshots.find(
    (snapshot) => snapshot.id === selectedRollbackSnapshotId,
  );
  const rollbackConfirmationMatches =
    snapshotDialogDatabase !== null &&
    rollbackConfirmationInput.trim() === snapshotDialogDatabase;

  return (
    <PageLayout
      actions={
        <>
          <Button onClick={() => void loadDatabaseWorkspace()}>
            {loading ? "Refreshing..." : "Refresh"}
          </Button>
          {!mysqlRunning ? (
            <Button
              disabled={actionName === "mysql"}
              onClick={() => void handleStartMysql()}
              variant="primary"
            >
              {actionName === "mysql" ? "Starting..." : "Start MySQL"}
            </Button>
          ) : null}
        </>
      }
      subtitle="Create, restore, and recover local MySQL databases without leaving the workspace."
      title="Databases"
    >
      <div className="route-grid" data-columns="4">
        <MetricCard
          label="Databases"
          tone={databases.length > 0 ? "success" : "warning"}
          value={loading ? "..." : String(databases.length)}
        />
        <MetricCard
          label="Linked Projects"
          tone={linkedProjectCount > 0 ? "success" : "warning"}
          value={String(linkedProjectCount)}
        />
        <MetricCard
          label="MySQL Service"
          tone={
            mysqlRunning
              ? "success"
              : mysqlService?.status === "error"
                ? "error"
                : "warning"
          }
          value={
            mysqlRunning
              ? "Running"
              : mysqlService?.status === "error"
                ? "Error"
                : "Stopped"
          }
        />
        <MetricCard
          label="Protected DBs"
          tone={
            !protectedDatabaseCountPending && protectedDatabaseCount > 0
              ? "success"
              : "warning"
          }
          value={
            protectedDatabaseCountPending
              ? "..."
              : String(protectedDatabaseCount)
          }
        />
      </div>

      <div className="stack workspace-shell">
        <StickyTabs
          activeTab={activeTab}
          ariaLabel="Database workspace sections"
          items={databaseTabs}
          onSelect={handleSelectTab}
        />

        <div
          aria-labelledby="workspace-tab-overview"
          className="workspace-panel"
          hidden={activeTab !== "overview"}
          id="workspace-panel-overview"
          role="tabpanel"
        >
          <Card className="runtime-toolbar-card">
            <div className="page-header">
              <div>
                <h2>Create Database</h2>
                <p>
                  Provision a new UTF-8 database on the active MySQL runtime,
                  then link it to a project below.
                </p>
              </div>
              <span
                className="status-chip"
                data-tone={
                  mysqlRunning
                    ? "success"
                    : mysqlService?.status === "error"
                      ? "error"
                      : "warning"
                }
              >
                {mysqlRunning ? `MySQL on ${mysqlPort}` : "MySQL not ready"}
              </span>
            </div>
            <div className="runtime-inline-form database-inline-form">
              <div className="field">
                <label htmlFor="database-name">Database Name</label>
                <input
                  className="input"
                  id="database-name"
                  onChange={(event) => setCreateName(event.target.value)}
                  placeholder="vietruyen_app"
                  value={createName}
                />
              </div>
              <div className="field">
                <label>Runtime</label>
                <div className="database-inline-status">
                  <strong>
                    {activeMysqlRuntime
                      ? `MySQL ${activeMysqlRuntime.version}`
                      : "No active MySQL runtime"}
                  </strong>
                  <span>
                    {activeMysqlRuntime?.path ??
                      "Install or activate a MySQL runtime from Settings first."}
                  </span>
                </div>
              </div>
              <div className="field">
                <label>Action</label>
                <Button
                  disabled={!mysqlRunning || !activeMysqlRuntime || createBusy}
                  onClick={() => void handleCreateDatabase()}
                  variant="primary"
                >
                  {createBusy ? "Creating..." : "Create Database"}
                </Button>
              </div>
            </div>
          </Card>
        </div>

        <div
          aria-labelledby="workspace-tab-databases"
          className="workspace-panel"
          hidden={activeTab !== "databases"}
          id="workspace-panel-databases"
          role="tabpanel"
        >
          <Card>
            <div className="page-header">
              <div>
                <h2>Local Databases</h2>
                <p>
                  Managed list from the active MySQL runtime. System schemas
                  stay hidden.
                </p>
              </div>
              <div className="page-toolbar">
                <input
                  aria-label="Search databases"
                  className="input"
                  onChange={(event) => setDatabaseSearch(event.target.value)}
                  placeholder="Search database name"
                  type="search"
                  value={databaseSearch}
                />
              </div>
            </div>
            {loadError ? (
              <EmptyState
                description={loadError}
                title="Database workspace is not ready"
              />
            ) : !mysqlRunning ? (
              <EmptyState
                description="Start MySQL to inspect or create databases. Project links below still remain editable."
                title="MySQL is stopped"
              />
            ) : databases.length === 0 ? (
              <EmptyState
                description="Create the first database, then link it to a tracked project."
                title="No local databases yet"
              />
            ) : filteredDatabases.length === 0 ? (
              <EmptyState
                description={`No local databases match "${databaseSearch.trim()}" on the active MySQL runtime.`}
                title="No databases found"
              />
            ) : (
              <div className="runtime-table-shell">
                <table className="runtime-table">
                  <thead>
                    <tr>
                      <th>Name</th>
                      <th>Linked Projects</th>
                      <th>Port</th>
                      <th>Time Machine</th>
                      <th>Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {filteredDatabases.map((databaseName) => {
                      const linked = projects.filter(
                        (project) => project.databaseName === databaseName,
                      );
                      const status = timeMachineStatusByDatabase[databaseName];
                      const busy = isTimeMachineBusy(databaseName);
                      const timeMachine = getDatabaseTimeMachinePresentation(
                        status,
                        busy,
                        timeMachineStatusLoading && !status,
                      );

                      return (
                        <tr key={databaseName}>
                          <td>
                            <div className="runtime-table-type">
                              <strong className="mono">{databaseName}</strong>
                              <span>
                                {linked.length > 0
                                  ? "Tracked by project metadata."
                                  : "Available to link."}
                              </span>
                            </div>
                          </td>
                          <td>
                            {linked.length > 0 ? (
                              <div className="project-card-badges">
                                {linked.map((project) => (
                                  <span className="badge" key={project.id}>
                                    {project.name}
                                  </span>
                                ))}
                              </div>
                            ) : (
                              <span className="runtime-table-note">
                                No tracked projects linked.
                              </span>
                            )}
                          </td>
                          <td>
                            <span className="mono runtime-table-note">
                              {mysqlPort}
                            </span>
                          </td>
                          <td>
                            <div className="database-time-machine-cell">
                              <span
                                className="status-chip"
                                data-tone={timeMachine.tone}
                              >
                                {timeMachine.label}
                              </span>
                              <span className="runtime-table-note">
                                {timeMachine.message}
                              </span>
                            </div>
                          </td>
                          <td>
                            <div className="runtime-table-actions">
                              <ActionMenu
                                disabled={
                                  busy || dropBusy || Boolean(transferActionKey)
                                }
                                label="Time Machine"
                              >
                                <ActionMenuItem
                                  onClick={() =>
                                    void handleTakeSnapshot(databaseName)
                                  }
                                >
                                  Take Snapshot
                                </ActionMenuItem>
                                <ActionMenuItem
                                  onClick={() =>
                                    void loadSnapshotHistory(
                                      databaseName,
                                      "rollback",
                                    )
                                  }
                                >
                                  Rollback
                                </ActionMenuItem>
                                <ActionMenuItem
                                  onClick={() =>
                                    void loadSnapshotHistory(
                                      databaseName,
                                      "history",
                                    )
                                  }
                                >
                                  History
                                </ActionMenuItem>
                              </ActionMenu>
                              <Button
                                disabled={
                                  dropBusy || Boolean(transferActionKey) || busy
                                }
                                onClick={() =>
                                  void handleBackupDatabase(databaseName)
                                }
                              >
                                {transferActionKey === `backup:${databaseName}`
                                  ? "Backing Up..."
                                  : "Backup"}
                              </Button>
                              <Button
                                disabled={
                                  dropBusy || Boolean(transferActionKey) || busy
                                }
                                onClick={() =>
                                  void handleRestoreDatabase(databaseName)
                                }
                              >
                                {transferActionKey === `restore:${databaseName}`
                                  ? "Restoring..."
                                  : "Restore"}
                              </Button>
                              <Button
                                disabled={
                                  linked.length > 0 ||
                                  dropBusy ||
                                  Boolean(transferActionKey) ||
                                  busy
                                }
                                onClick={() => setDropTarget(databaseName)}
                              >
                                {linked.length > 0 ? "In Use" : "Delete"}
                              </Button>
                            </div>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </Card>
        </div>

        <div
          aria-labelledby="workspace-tab-links"
          className="workspace-panel"
          hidden={activeTab !== "links"}
          id="workspace-panel-links"
          role="tabpanel"
        >
          <Card>
            <div className="page-header">
              <div>
                <h2>Project Links</h2>
                <p>
                  Keep project metadata aligned with the local database that
                  project expects.
                </p>
              </div>
            </div>
            {projects.length === 0 ? (
              <EmptyState
                description="Add a project first, then attach a database from this page."
                title="No tracked projects"
              />
            ) : (
              <div className="runtime-table-shell">
                <table className="runtime-table">
                  <thead>
                    <tr>
                      <th>Project</th>
                      <th>Current Database</th>
                      <th>Assign</th>
                      <th>Action</th>
                    </tr>
                  </thead>
                  <tbody>
                    {projects.map((project) => {
                      const currentDatabase = project.databaseName ?? "";
                      const selectedDatabase =
                        linkDrafts[project.id] ?? currentDatabase;
                      const hasMissingDatabase =
                        Boolean(currentDatabase) &&
                        !databases.includes(currentDatabase);
                      const isDirty = selectedDatabase !== currentDatabase;

                      return (
                        <tr key={project.id}>
                          <td>
                            <div className="runtime-table-type">
                              <strong>{project.name}</strong>
                              <span>{project.domain}</span>
                            </div>
                          </td>
                          <td>
                            {currentDatabase ? (
                              <div className="runtime-table-type">
                                <strong className="mono">
                                  {currentDatabase}
                                </strong>
                                <span>
                                  {hasMissingDatabase
                                    ? "Tracked, but not found in MySQL."
                                    : `Port ${project.databasePort ?? mysqlPort}`}
                                </span>
                              </div>
                            ) : (
                              <span className="runtime-table-note">
                                No database linked.
                              </span>
                            )}
                          </td>
                          <td>
                            <select
                              className="select database-link-select"
                              onChange={(event) =>
                                setLinkDrafts((current) => ({
                                  ...current,
                                  [project.id]: event.target.value,
                                }))
                              }
                              value={selectedDatabase}
                            >
                              <option value="">No database</option>
                              {databases.map((databaseName) => (
                                <option key={databaseName} value={databaseName}>
                                  {databaseName}
                                </option>
                              ))}
                              {hasMissingDatabase ? (
                                <option value={currentDatabase}>
                                  {currentDatabase} (missing)
                                </option>
                              ) : null}
                            </select>
                          </td>
                          <td>
                            <div className="runtime-table-actions">
                              <Button
                                disabled={
                                  !isDirty || linkingProjectId === project.id
                                }
                                onClick={() =>
                                  void handleSaveProjectLink(project.id)
                                }
                                variant="primary"
                              >
                                {linkingProjectId === project.id
                                  ? "Saving..."
                                  : "Save"}
                              </Button>
                            </div>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </Card>
        </div>
      </div>

      {dropTarget ? (
        <div
          aria-modal="true"
          className="wizard-overlay"
          onClick={() => {
            if (!dropBusy) {
              setDropTarget(undefined);
            }
          }}
          role="dialog"
        >
          <div
            className="confirm-dialog"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="confirm-dialog-copy">
              <h3>Delete Database</h3>
              <p>
                DevNest will drop <strong className="mono">{dropTarget}</strong>{" "}
                from the active MySQL runtime. This cannot be undone.
              </p>
            </div>
            <div className="confirm-dialog-actions">
              <Button
                disabled={dropBusy}
                onClick={() => setDropTarget(undefined)}
              >
                Cancel
              </Button>
              <Button
                disabled={dropBusy}
                onClick={() => void handleDropDatabase()}
                variant="primary"
              >
                {dropBusy ? "Deleting..." : "Delete Database"}
              </Button>
            </div>
          </div>
        </div>
      ) : null}

      {snapshotDialogDatabase ? (
        <div
          aria-modal="true"
          className="wizard-overlay"
          onClick={() => {
            if (!snapshotDialogBusy && !snapshotDialogLoading) {
              setSnapshotDialogDatabase(null);
              setRollbackConfirmationInput("");
            }
          }}
          role="dialog"
        >
          <div
            className="confirm-dialog database-snapshot-dialog"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="confirm-dialog-copy">
              <div className="database-snapshot-header">
                <div>
                  <h3>
                    {snapshotDialogMode === "rollback"
                      ? "Rollback Database"
                      : "Time Machine History"}
                  </h3>
                  <p>
                    Managed snapshots for{" "}
                    <strong className="mono">{snapshotDialogDatabase}</strong>.
                  </p>
                </div>
                <span
                  className="status-chip"
                  data-tone={
                    getDatabaseTimeMachinePresentation(
                      snapshotDialogStatus,
                      snapshotDialogBusy,
                      timeMachineStatusLoading && !snapshotDialogStatus,
                    ).tone
                  }
                >
                  {
                    getDatabaseTimeMachinePresentation(
                      snapshotDialogStatus,
                      snapshotDialogBusy,
                      timeMachineStatusLoading && !snapshotDialogStatus,
                    ).label
                  }
                </span>
              </div>

              {snapshotDialogError ? (
                <span className="error-text">{snapshotDialogError}</span>
              ) : null}

              {snapshotDialogStatus ? (
                <div className="database-snapshot-summary">
                  <div className="database-snapshot-summary-item">
                    <span className="detail-label">Schedule</span>
                    <strong>
                      {formatDatabaseScheduleLabel(snapshotDialogStatus)}
                    </strong>
                  </div>
                  <div className="database-snapshot-summary-item">
                    <span className="detail-label">Project Actions</span>
                    <strong>
                      {snapshotDialogStatus.linkedProjectActionSnapshotsEnabled
                        ? "Linked project snapshots enabled"
                        : "Linked project snapshots disabled"}
                    </strong>
                  </div>
                </div>
              ) : null}

              {snapshotDialogLoading ? (
                <span className="helper-text">
                  Loading the last managed snapshots...
                </span>
              ) : snapshotDialogSnapshots.length === 0 ? (
                <div className="database-snapshot-empty">
                  <strong>No managed snapshots yet.</strong>
                  <span>
                    Use Take Snapshot to start a rolling ring of local recovery
                    points for this database.
                  </span>
                </div>
              ) : (
                <div className="database-snapshot-list" role="list">
                  {snapshotDialogSnapshots.map((snapshot) => (
                    <label
                      className="database-snapshot-item"
                      data-selected={selectedRollbackSnapshotId === snapshot.id}
                      key={snapshot.id}
                    >
                      <input
                        checked={selectedRollbackSnapshotId === snapshot.id}
                        name="database-snapshot-selection"
                        onChange={() =>
                          setSelectedRollbackSnapshotId(snapshot.id)
                        }
                        type="radio"
                      />
                      <div className="database-snapshot-item-copy">
                        <strong>{formatUpdatedAt(snapshot.createdAt)}</strong>
                        <span>
                          {databaseSnapshotTriggerLabel(snapshot.triggerSource)}{" "}
                          •{" "}
                          {databaseSnapshotBackendLabel(
                            snapshot.storageBackend,
                          )}{" "}
                          • {formatFileSize(snapshot.sizeBytes)}
                        </span>
                        {snapshot.linkedProjectNames.length > 0 ? (
                          <span>
                            Projects: {snapshot.linkedProjectNames.join(", ")}
                          </span>
                        ) : null}
                        {snapshot.scheduledIntervalMinutes ? (
                          <span>
                            Scheduled every {snapshot.scheduledIntervalMinutes}{" "}
                            minutes
                          </span>
                        ) : null}
                        {snapshot.note ? <span>{snapshot.note}</span> : null}
                      </div>
                    </label>
                  ))}
                </div>
              )}

              {snapshotDialogMode === "rollback" && selectedRollbackSnapshot ? (
                <div className="database-snapshot-rollback-guard">
                  <strong>
                    Roll back to{" "}
                    {formatUpdatedAt(selectedRollbackSnapshot.createdAt)}?
                  </strong>
                  <span>
                    DevNest will replace the current contents of{" "}
                    <strong className="mono">{snapshotDialogDatabase}</strong>{" "}
                    with this managed snapshot and capture one more safety
                    snapshot first when possible.
                  </span>
                  <div className="field">
                    <label htmlFor="database-rollback-confirm">
                      Type{" "}
                      <strong className="mono">{snapshotDialogDatabase}</strong>{" "}
                      to confirm
                    </label>
                    <input
                      className="input"
                      id="database-rollback-confirm"
                      onChange={(event) =>
                        setRollbackConfirmationInput(event.target.value)
                      }
                      placeholder={snapshotDialogDatabase}
                      value={rollbackConfirmationInput}
                    />
                  </div>
                </div>
              ) : null}

              <span className="helper-text">
                DevNest keeps the latest 3 managed snapshots per database.
                Protection is still scoped per database, not the whole MySQL
                datadir.
              </span>
            </div>

            <div className="confirm-dialog-actions">
              <Button
                disabled={snapshotDialogBusy}
                onClick={() =>
                  void handleToggleTimeMachine(
                    snapshotDialogDatabase,
                    !(snapshotDialogStatus?.enabled ?? false),
                  )
                }
              >
                {snapshotDialogStatus?.enabled
                  ? "Disable Protection"
                  : "Enable Protection"}
              </Button>
              <Button
                disabled={snapshotDialogBusy}
                onClick={() => void handleTakeSnapshot(snapshotDialogDatabase)}
              >
                {timeMachineActionKey === `snapshot:${snapshotDialogDatabase}`
                  ? "Capturing..."
                  : "Take Snapshot"}
              </Button>
              <Button
                disabled={snapshotDialogBusy || snapshotDialogLoading}
                onClick={() => {
                  setSnapshotDialogDatabase(null);
                  setRollbackConfirmationInput("");
                }}
              >
                Close
              </Button>
              <Button
                disabled={
                  snapshotDialogMode === "history"
                    ? snapshotDialogBusy ||
                      snapshotDialogLoading ||
                      snapshotDialogSnapshots.length === 0
                    : snapshotDialogBusy ||
                      snapshotDialogLoading ||
                      snapshotDialogSnapshots.length === 0 ||
                      selectedRollbackSnapshotId.length === 0 ||
                      !rollbackConfirmationMatches
                }
                onClick={() => {
                  if (snapshotDialogMode === "history") {
                    setSnapshotDialogMode("rollback");
                    return;
                  }

                  void handleRollbackSnapshot();
                }}
                variant="primary"
              >
                {snapshotDialogMode === "history"
                  ? "Review Rollback"
                  : timeMachineActionKey ===
                      `rollback:${snapshotDialogDatabase}`
                    ? "Rolling Back..."
                    : "Rollback Selected"}
              </Button>
            </div>
          </div>
        </div>
      ) : null}
    </PageLayout>
  );
}
