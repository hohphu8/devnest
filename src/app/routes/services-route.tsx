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

export default function ServicesRoute() {
  const navigate = useNavigate();
  const {
    actionName,
    activeService,
    error,
    fetchService,
    loadServices,
    loading,
    restartService,
    selectedServiceName,
    selectService,
    services,
    startService,
    stopService,
  } = useServiceStore();
  const [portCheck, setPortCheck] = useState<PortCheckResult>();
  const [wslRecoveryName, setWslRecoveryName] = useState<ServiceName>();
  const [optionalToolInventory, setOptionalToolInventory] = useState<
    OptionalToolInventoryItem[]
  >([]);
  const [runtimeInventory, setRuntimeInventory] = useState<
    RuntimeInventoryItem[]
  >([]);
  const pushToast = useToastStore((state) => state.push);

  useEffect(() => {
    void refreshRuntimeInventory();
    void refreshOptionalToolInventory();
  }, []);

  useEffect(() => {
    if (!services.length || selectedServiceName) {
      return;
    }

    selectService(services[0]?.name);
  }, [selectedServiceName, selectService, services]);

  useEffect(() => {
    if (!selectedServiceName || activeService?.name === selectedServiceName) {
      return;
    }

    void fetchService(selectedServiceName);
  }, [activeService?.name, fetchService, selectedServiceName]);

  useEffect(() => {
    if (!activeService?.port) {
      setPortCheck(undefined);
      return;
    }

    serviceApi
      .checkPort(activeService.port)
      .then(setPortCheck)
      .catch((invokeError) =>
        pushToast({
          tone: "error",
          title: "Port check failed",
          message: getAppErrorMessage(
            invokeError,
            "Failed to inspect the service port.",
          ),
        }),
      );
  }, [activeService]);

  const activeRuntime = useMemo(
    () =>
      activeService
        ? runtimeInventory.find(
            (runtime) =>
              runtime.runtimeType === activeService.name && runtime.isActive,
          )
        : undefined,
    [activeService, runtimeInventory],
  );
  const activeOptionalTool = useMemo(() => {
    const toolType = optionalToolTypeForService(activeService?.name);
    if (!toolType) {
      return undefined;
    }

    return optionalToolInventory.find(
      (tool) =>
        tool.toolType === toolType &&
        tool.isActive &&
        tool.status === "available",
    );
  }, [activeService?.name, optionalToolInventory]);

  async function refreshRuntimeInventory() {
    try {
      setRuntimeInventory(await runtimeApi.list());
    } catch {
      setRuntimeInventory([]);
    }
  }

  async function refreshOptionalToolInventory() {
    try {
      setOptionalToolInventory(await optionalToolApi.list());
    } catch {
      setOptionalToolInventory([]);
    }
  }

  async function refreshSelectedService() {
    if (!selectedServiceName) {
      await loadServices();
      setPortCheck(undefined);
      return;
    }

    const service = await fetchService(selectedServiceName);
    if (service.port) {
      setPortCheck(await serviceApi.checkPort(service.port));
    } else {
      setPortCheck(undefined);
    }
  }

  async function runServiceAction(
    name: ServiceName,
    action: "start" | "stop" | "restart",
  ) {
    try {
      let service: ServiceState;
      if (action === "start") {
        service = await startService(name);
        pushToast({
          tone: "success",
          title: "Service started",
          message: `${serviceLabel(name)} started.`,
        });
      } else if (action === "stop") {
        service = await stopService(name);
        pushToast({
          tone: "success",
          title: "Service stopped",
          message: `${serviceLabel(name)} stopped.`,
        });
      } else {
        service = await restartService(name);
        pushToast({
          tone: "success",
          title: "Service restarted",
          message: `${serviceLabel(name)} restarted.`,
        });
      }

      setPortCheck(
        service.port ? await serviceApi.checkPort(service.port) : undefined,
      );
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Service action failed",
        message: getAppErrorMessage(
          invokeError,
          `Failed to ${action} ${name}.`,
        ),
      });
      await refreshSelectedService().catch(() => undefined);
    }
  }

  async function handleRecoverWslPort(name: ServiceName) {
    setWslRecoveryName(name);
    try {
      await serviceApi.recoverWebPortFromWsl(name);
      pushToast({
        tone: "success",
        title: "Web server recovered",
        message: `WSL was shut down and ${serviceLabel(name)} is running.`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "WSL recovery failed",
        message: getAppErrorMessage(
          invokeError,
          "DevNest could not release the web server port.",
        ),
      });
    } finally {
      setWslRecoveryName(undefined);
      await refreshSelectedService().catch(() => undefined);
    }
  }

  async function handleOpenServiceDashboard(name: ServiceName) {
    try {
      await serviceApi.openDashboard(name);
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Open dashboard failed",
        message: getAppErrorMessage(
          invokeError,
          `Failed to open the ${serviceLabel(name)} dashboard.`,
        ),
      });
    }
  }

  async function handleStartAll() {
    await runAsyncAction(
      "workspace:start-all",
      async () => {
        const { startable, skipped } = getStartAllPlan(services);
        const started: string[] = [];

        for (const service of startable) {
          if (service.status === "running") {
            continue;
          }

          try {
            await startService(service.name);
            started.push(service.name);
          } catch (invokeError) {
            pushToast({
              tone: "error",
              title: "Start all failed",
              message: getAppErrorMessage(
                invokeError,
                `Failed to start ${service.name}.`,
              ),
            });
            return;
          }
        }

        await refreshSelectedService();
        const messageParts = [];
        if (started.length > 0) {
          messageParts.push(`Started ${started.join(", ")}.`);
        }
        if (skipped.length > 0) {
          messageParts.push(
            `Skipped ${skipped.join(", ")} because they share the same default port.`,
          );
        }
        if (messageParts.length > 0) {
          pushToast({
            tone: skipped.length > 0 ? "warning" : "success",
            title: "Service startup complete",
            message: messageParts.join(" "),
          });
        }
      },
      "Starting workspace services...",
    );
  }

  async function handleStopAll() {
    await runAsyncAction(
      "workspace:stop-all",
      async () => {
        const running = services.filter(
          (service) => service.status === "running",
        );

        for (const service of running) {
          try {
            await stopService(service.name);
          } catch (invokeError) {
            pushToast({
              tone: "error",
              title: "Stop all failed",
              message: getAppErrorMessage(
                invokeError,
                `Failed to stop ${service.name}.`,
              ),
            });
            return;
          }
        }

        await refreshSelectedService();
        pushToast({
          tone: running.length > 0 ? "success" : "info",
          title: "Service stop complete",
          message:
            running.length > 0
              ? "Stopped all running services."
              : "No services were running.",
        });
      },
      "Stopping workspace services...",
    );
  }

  const startAllBusy = useAsyncActionPending("workspace:start-all");
  const stopAllBusy = useAsyncActionPending("workspace:stop-all");
  const globalServiceBusy = startAllBusy || stopAllBusy;

  return (
    <PageLayout
      actions={
        <>
          <Button
            busy={loading}
            busyLabel="Refreshing service status..."
            onClick={() => void refreshSelectedService()}
          >
            Refresh Status
          </Button>
          <Button
            busy={stopAllBusy}
            busyLabel="Stopping services..."
            disabled={globalServiceBusy && !stopAllBusy}
            onClick={() => void handleStopAll()}
          >
            Stop All
          </Button>
          <Button
            busy={startAllBusy}
            busyLabel="Starting services..."
            disabled={globalServiceBusy && !startAllBusy}
            onClick={() => void handleStartAll()}
            variant="primary"
          >
            Start All
          </Button>
        </>
      }
      subtitle="Live runtime control, PID tracking, port checks, logs,.. for the local PHP stack."
      title="Services"
    >
      <div className="stack">
        <div className="split-layout">
          <div className="stack">
            <ServiceTable
              actionName={actionName}
              onInspect={(name) => {
                selectService(name);
              }}
              onRestart={(name) => runServiceAction(name, "restart")}
              onStart={(name) => runServiceAction(name, "start")}
              onStop={(name) => runServiceAction(name, "stop")}
              selectedServiceName={selectedServiceName}
              services={services}
            />
          </div>

          <ServiceInspector
            actionName={actionName}
            activeOptionalTool={activeOptionalTool}
            activeRuntime={activeRuntime}
            onOpenDashboard={() =>
              activeService
                ? handleOpenServiceDashboard(activeService.name)
                : Promise.resolve()
            }
            onOpenLogs={() =>
              navigate(
                `/logs?source=${activeService?.name ?? selectedServiceName ?? "apache"}`,
              )
            }
            onRecoverWsl={() =>
              activeService
                ? handleRecoverWslPort(activeService.name)
                : Promise.resolve()
            }
            onRefresh={refreshSelectedService}
            onRestart={() =>
              activeService
                ? runServiceAction(activeService.name, "restart")
                : Promise.resolve()
            }
            onStart={() =>
              activeService
                ? runServiceAction(activeService.name, "start")
                : Promise.resolve()
            }
            onStop={() =>
              activeService
                ? runServiceAction(activeService.name, "stop")
                : Promise.resolve()
            }
            portCheck={portCheck}
            service={activeService}
            wslRecoveryBusy={wslRecoveryName === activeService?.name}
          />
        </div>
      </div>
    </PageLayout>
  );
}
