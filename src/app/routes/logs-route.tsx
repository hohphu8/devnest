import { lazy, Suspense, useEffect, useMemo, useRef, useState } from "react";
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

export default function LogsRoute() {
  const [searchParams, setSearchParams] = useSearchParams();
  const pushToast = useToastStore((state) => state.push);
  const workspaceLoaded = useWorkspaceStore((state) => state.loaded);
  const services = useServiceStore((state) => state.services);
  const workers = useProjectWorkerStore((state) => state.workers);
  const loadWorkers = useProjectWorkerStore((state) => state.loadWorkers);
  const scheduledTasks = useProjectScheduledTaskStore((state) => state.tasks);
  const runsByTaskId = useProjectScheduledTaskStore(
    (state) => state.runsByTaskId,
  );
  const loadScheduledTasks = useProjectScheduledTaskStore(
    (state) => state.loadTasks,
  );
  const loadTaskRuns = useProjectScheduledTaskStore(
    (state) => state.loadTaskRuns,
  );
  const clearTaskHistory = useProjectScheduledTaskStore(
    (state) => state.clearTaskHistory,
  );
  const [loading, setLoading] = useState(false);
  const [clearing, setClearing] = useState(false);
  const [error, setError] = useState<string>();
  const [payload, setPayload] = useState<
    | ServiceLogPayload
    | ProjectWorkerLogPayload
    | ProjectScheduledTaskRunLogPayload
    | null
  >(null);
  const [search, setSearch] = useState("");
  const [severityFilter, setSeverityFilter] = useState<
    "all" | "error" | "warning" | "info"
  >("all");
  const [wrap, setWrap] = useState(true);
  const [autoScroll, setAutoScroll] = useState(true);

  const selectedType =
    searchParams.get("type") === "worker"
      ? "worker"
      : searchParams.get("type") === "scheduled-task-run"
        ? "scheduled-task-run"
        : "service";
  const selectedService = searchParams.get("source") as ServiceName | null;
  const selectedWorkerId = searchParams.get("workerId");
  const selectedTaskId = searchParams.get("taskId");
  const selectedRunId = searchParams.get("runId");
  const selectedWorker = workers.find(
    (worker) => worker.id === selectedWorkerId,
  );
  const selectedTask = scheduledTasks.find(
    (task) => task.id === selectedTaskId,
  );
  const selectedTaskRuns = selectedTaskId
    ? (runsByTaskId[selectedTaskId] ?? [])
    : [];
  const selectedRun = selectedTaskRuns.find((run) => run.id === selectedRunId);

  useEffect(() => {
    if (!workspaceLoaded && workers.length === 0) {
      void loadWorkers().catch(() => undefined);
    }
  }, [loadWorkers, workers.length, workspaceLoaded]);

  useEffect(() => {
    if (!workspaceLoaded && scheduledTasks.length === 0) {
      void loadScheduledTasks().catch(() => undefined);
    }
  }, [loadScheduledTasks, scheduledTasks.length, workspaceLoaded]);

  useEffect(() => {
    if (selectedType === "worker" && selectedWorkerId) {
      return;
    }

    if (selectedType === "scheduled-task-run" && selectedRunId) {
      return;
    }

    if (selectedService || services.length === 0) {
      return;
    }

    setSearchParams({ type: "service", source: services[0].name });
  }, [
    selectedRunId,
    selectedService,
    selectedType,
    selectedWorkerId,
    services,
    setSearchParams,
  ]);

  async function loadLogs() {
    setLoading(true);
    setError(undefined);
    try {
      if (selectedType === "worker" && selectedWorkerId) {
        setPayload(await projectWorkerApi.readLogs(selectedWorkerId, 300));
        return;
      }

      if (selectedType === "scheduled-task-run" && selectedRunId) {
        setPayload(
          await projectScheduledTaskApi.readRunLogs(selectedRunId, 300),
        );
        return;
      }

      if (selectedService) {
        setPayload(await serviceApi.readLogs(selectedService, 300));
      }
    } catch (invokeError) {
      setError(
        getAppErrorMessage(
          invokeError,
          selectedType === "worker"
            ? "Failed to read worker logs."
            : selectedType === "scheduled-task-run"
              ? "Failed to read scheduled task run logs."
              : "Failed to read service logs.",
        ),
      );
    } finally {
      setLoading(false);
    }
  }

  async function refreshLogs() {
    if (selectedType === "scheduled-task-run" && selectedTaskId) {
      setLoading(true);
      setError(undefined);
      try {
        const runs = await loadTaskRuns(selectedTaskId, 1);
        const latestRun = runs[0];
        if (!latestRun) {
          setPayload(null);
          return;
        }

        if (latestRun.id !== selectedRunId) {
          setSearchParams({
            type: "scheduled-task-run",
            taskId: selectedTaskId,
            runId: latestRun.id,
          });
          return;
        }

        setPayload(
          await projectScheduledTaskApi.readRunLogs(latestRun.id, 300),
        );
        return;
      } catch (invokeError) {
        setError(
          getAppErrorMessage(
            invokeError,
            "Failed to refresh scheduled task run logs.",
          ),
        );
        return;
      } finally {
        setLoading(false);
      }
    }

    await loadLogs();
  }

  async function handleClearLogs() {
    setClearing(true);
    setError(undefined);
    try {
      if (selectedType === "worker" && selectedWorkerId) {
        await projectWorkerApi.clearLogs(selectedWorkerId);
        setPayload(await projectWorkerApi.readLogs(selectedWorkerId, 300));
      } else if (
        selectedType === "scheduled-task-run" &&
        selectedTaskId &&
        selectedRunId
      ) {
        await clearTaskHistory(selectedTaskId);
        setPayload(null);
        setSearchParams({
          type: "scheduled-task-run",
          taskId: selectedTaskId,
        });
      } else if (selectedService) {
        await serviceApi.clearLogs(selectedService);
        setPayload(await serviceApi.readLogs(selectedService, 300));
      }
      pushToast({
        tone: "success",
        title: "Logs cleared",
        message:
          selectedType === "worker"
            ? `${selectedWorker?.name ?? "Selected worker"} logs were cleared.`
            : selectedType === "scheduled-task-run"
              ? `${selectedTask?.name ?? "Selected task"} history and logs were cleared.`
              : `${selectedService} logs were cleared.`,
      });
    } catch (invokeError) {
      setError(
        getAppErrorMessage(
          invokeError,
          selectedType === "worker"
            ? "Failed to clear worker logs."
            : selectedType === "scheduled-task-run"
              ? "Failed to clear scheduled task history."
              : "Failed to clear service logs.",
        ),
      );
    } finally {
      setClearing(false);
    }
  }

  useEffect(() => {
    if (selectedType === "worker" && !selectedWorkerId) {
      if (workers.length > 0) {
        setSearchParams({ type: "worker", workerId: workers[0].id });
      }
      return;
    }

    if (selectedType === "scheduled-task-run" && !selectedRunId) {
      if (!selectedTaskId) {
        return;
      }

      void loadTaskRuns(selectedTaskId, 1)
        .then((runs) => {
          const latestRun = runs[0];
          if (!latestRun) {
            return;
          }

          setSearchParams({
            type: "scheduled-task-run",
            taskId: selectedTaskId,
            runId: latestRun.id,
          });
        })
        .catch(() => undefined);
      return;
    }

    if (selectedType === "service" && !selectedService) {
      return;
    }

    void loadLogs();
  }, [
    loadTaskRuns,
    selectedRunId,
    selectedService,
    selectedTaskId,
    selectedType,
    selectedWorkerId,
    setSearchParams,
    workers,
  ]);

  async function openTaskLogs(taskId: string) {
    try {
      const runs = await loadTaskRuns(taskId, 1);
      const latestRun = runs[0];
      if (!latestRun) {
        pushToast({
          tone: "warning",
          title: "No task logs yet",
          message: "That scheduled task has not produced a run log yet.",
        });
        return;
      }

      setSearchParams({
        type: "scheduled-task-run",
        taskId,
        runId: latestRun.id,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Task logs unavailable",
        message: getAppErrorMessage(
          invokeError,
          "Could not open the selected task logs.",
        ),
      });
    }
  }

  return (
    <PageLayout
      actions={
        <>
          <Button
            disabled={
              (!selectedService && !selectedWorkerId && !selectedRunId) ||
              loading ||
              clearing
            }
            onClick={() => void refreshLogs()}
          >
            {loading ? "Refreshing..." : "Refresh"}
          </Button>
          <Button
            disabled={
              (!selectedService && !selectedWorkerId && !selectedRunId) ||
              loading ||
              clearing
            }
            onClick={() => void handleClearLogs()}
          >
            {clearing
              ? "Clearing..."
              : selectedType === "scheduled-task-run"
                ? "Clear History"
                : "Clear Logs"}
          </Button>
          <Button onClick={() => setWrap((value) => !value)}>
            {wrap ? "Wrap On" : "Wrap Off"}
          </Button>
          <Button onClick={() => setAutoScroll((value) => !value)}>
            {autoScroll ? "Auto-scroll On" : "Auto-scroll Off"}
          </Button>
        </>
      }
      subtitle="Tail logs by service, worker, or scheduled task run source with search, severity filtering, wrap, and auto-scroll."
      title="Logs"
    >
      {services.length > 0 ||
      workers.length > 0 ||
      scheduledTasks.length > 0 ? (
        <>
          <div className="logs-toolbar">
            <div className="logs-tabs">
              {services.length > 0 ? (
                <div className="stack" style={{ gap: 8 }}>
                  <span className="helper-text">Services</span>
                  <div
                    className="page-toolbar"
                    style={{ justifyContent: "flex-start" }}
                  >
                    {services.map((service) => (
                      <button
                        className="logs-tab"
                        data-active={
                          selectedType === "service" &&
                          selectedService === service.name
                        }
                        key={service.name}
                        onClick={() =>
                          setSearchParams({
                            type: "service",
                            source: service.name,
                          })
                        }
                        type="button"
                      >
                        {service.name}
                      </button>
                    ))}
                  </div>
                </div>
              ) : null}

              {workers.length > 0 ? (
                <div className="stack" style={{ gap: 8 }}>
                  <span className="helper-text">Workers</span>
                  <div
                    className="page-toolbar"
                    style={{ justifyContent: "flex-start" }}
                  >
                    {workers.map((worker) => (
                      <button
                        className="logs-tab"
                        data-active={
                          selectedType === "worker" &&
                          selectedWorkerId === worker.id
                        }
                        key={worker.id}
                        onClick={() =>
                          setSearchParams({
                            type: "worker",
                            workerId: worker.id,
                          })
                        }
                        type="button"
                      >
                        {worker.name}
                      </button>
                    ))}
                  </div>
                </div>
              ) : null}

              {scheduledTasks.length > 0 ? (
                <div className="stack" style={{ gap: 8 }}>
                  <span className="helper-text">Scheduled Tasks</span>
                  <div
                    className="page-toolbar"
                    style={{ justifyContent: "flex-start" }}
                  >
                    {scheduledTasks.map((task) => (
                      <button
                        className="logs-tab"
                        data-active={
                          selectedType === "scheduled-task-run" &&
                          selectedTaskId === task.id
                        }
                        key={task.id}
                        onClick={() => void openTaskLogs(task.id)}
                        type="button"
                      >
                        {task.name}
                      </button>
                    ))}
                  </div>
                </div>
              ) : null}
            </div>

            <div className="logs-filters">
              <input
                className="input"
                onChange={(event) => setSearch(event.target.value)}
                placeholder="Search log text"
                value={search}
              />
              <select
                className="select"
                onChange={(event) =>
                  setSeverityFilter(
                    event.target.value as "all" | "error" | "warning" | "info",
                  )
                }
                value={severityFilter}
              >
                <option value="all">All severities</option>
                <option value="error">Errors only</option>
                <option value="warning">Warnings only</option>
                <option value="info">Info only</option>
              </select>
            </div>
          </div>
          {error ? <span className="error-text">{error}</span> : null}
        </>
      ) : (
        <EmptyState
          title="No log sources loaded"
          description="Service, worker, and scheduled task logs become available after the workspace registry is loaded."
        />
      )}

      {(selectedService && selectedType === "service") ||
      (selectedWorkerId && selectedType === "worker") ||
      (selectedRunId && selectedType === "scheduled-task-run") ? (
        <LogViewer
          autoScroll={autoScroll}
          loading={loading}
          payload={payload}
          search={search}
          serviceName={
            selectedType === "worker"
              ? (selectedWorker?.name ?? "Selected worker")
              : selectedType === "scheduled-task-run"
                ? (selectedTask?.name ?? selectedRun?.id ?? "Selected task run")
                : (selectedService ?? "Selected service")
          }
          severityFilter={severityFilter}
          wrap={wrap}
        />
      ) : null}
    </PageLayout>
  );
}
