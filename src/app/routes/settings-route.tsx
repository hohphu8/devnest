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
import { formatFileSize } from "@/app/routes/database-route-utils";

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
import type { PhpToolsTab } from "./route-shared";
import { waitForNextPaint } from "./route-shared";

export default function SettingsRoute() {
  const [searchParams, setSearchParams] = useSearchParams();
  const [releaseInfo, setReleaseInfo] = useState<AppReleaseInfo | null>(null);
  const [appUpdateState, setAppUpdateState] = useState<AppUpdateState>("idle");
  const [updateResult, setUpdateResult] = useState<AppUpdateCheckResult | null>(
    null,
  );
  const [lastUpdateCheckAt, setLastUpdateCheckAt] = useState<string | null>(
    null,
  );
  const [runtimes, setRuntimes] = useState<RuntimeInventoryItem[]>([]);
  const [runtimePackages, setRuntimePackages] = useState<RuntimePackage[]>([]);
  const [optionalTools, setOptionalTools] = useState<
    OptionalToolInventoryItem[]
  >([]);
  const [optionalToolPackages, setOptionalToolPackages] = useState<
    OptionalToolPackage[]
  >([]);
  const [downloadCacheSummary, setDownloadCacheSummary] =
    useState<DownloadCacheSummary | null>(null);
  const [downloadCacheLoading, setDownloadCacheLoading] = useState(false);
  const [persistentTunnelSetup, setPersistentTunnelSetup] =
    useState<PersistentTunnelSetupStatus | null>(null);
  const [namedTunnels, setNamedTunnels] = useState<
    PersistentTunnelNamedTunnelSummary[]
  >([]);
  const [namedTunnelsLoading, setNamedTunnelsLoading] = useState(false);
  const [persistentTunnelSetupLoading, setPersistentTunnelSetupLoading] =
    useState(false);
  const [createTunnelName, setCreateTunnelName] = useState("devnest-main");
  const [defaultHostnameZone, setDefaultHostnameZone] = useState("");
  const [phpExtensions, setPhpExtensions] = useState<PhpExtensionState[]>([]);
  const [phpExtensionPackages, setPhpExtensionPackages] = useState<
    PhpExtensionPackage[]
  >([]);
  const [installTask, setInstallTask] = useState<RuntimeInstallTask | null>(
    null,
  );
  const [optionalToolInstallTask, setOptionalToolInstallTask] =
    useState<OptionalToolInstallTask | null>(null);
  const [loading, setLoading] = useState(false);
  const [packagesLoading, setPackagesLoading] = useState(false);
  const [phpExtensionsLoading, setPhpExtensionsLoading] = useState(false);
  const [phpExtensionPackagesLoading, setPhpExtensionPackagesLoading] =
    useState(false);
  const [phpFunctionsLoading, setPhpFunctionsLoading] = useState(false);
  const [error, setError] = useState<string>();
  const [packageError, setPackageError] = useState<string>();
  const [optionalToolError, setOptionalToolError] = useState<string>();
  const [optionalToolPackageError, setOptionalToolPackageError] =
    useState<string>();
  const [downloadCacheError, setDownloadCacheError] = useState<string>();
  const [persistentTunnelError, setPersistentTunnelError] = useState<string>();
  const [updateError, setUpdateError] = useState<string>();
  const [phpExtensionsError, setPhpExtensionsError] = useState<string>();
  const [phpExtensionPackagesError, setPhpExtensionPackagesError] =
    useState<string>();
  const [phpFunctionsError, setPhpFunctionsError] = useState<string>();
  const [actionLoading, setActionLoading] = useState<string | null>(null);
  const [pendingRuntimeRemoval, setPendingRuntimeRemoval] =
    useState<RuntimeInventoryItem | null>(null);
  const [pendingOptionalToolRemoval, setPendingOptionalToolRemoval] =
    useState<OptionalToolInventoryItem | null>(null);
  const [redisManagerOpen, setRedisManagerOpen] = useState(false);
  const [pendingPhpExtensionRemoval, setPendingPhpExtensionRemoval] =
    useState<PhpExtensionState | null>(null);
  const [pendingPersistentTunnelDeletion, setPendingPersistentTunnelDeletion] =
    useState<PersistentTunnelNamedTunnelSummary | null>(null);
  const [
    disconnectPersistentTunnelConfirm,
    setDisconnectPersistentTunnelConfirm,
  ] = useState(false);
  const [selectedPhpRuntimeId, setSelectedPhpRuntimeId] = useState<string>("");
  const [phpToolsRuntimeId, setPhpToolsRuntimeId] = useState<string | null>(
    null,
  );
  const [runtimeConfigRuntimeId, setRuntimeConfigRuntimeId] = useState<
    string | null
  >(null);
  const [runtimeConfigSchema, setRuntimeConfigSchema] =
    useState<RuntimeConfigSchema | null>(null);
  const [runtimeConfigValues, setRuntimeConfigValues] =
    useState<RuntimeConfigValues | null>(null);
  const [runtimeConfigLoading, setRuntimeConfigLoading] = useState(false);
  const [runtimeConfigSaving, setRuntimeConfigSaving] = useState(false);
  const [runtimeConfigOpenFileLoading, setRuntimeConfigOpenFileLoading] =
    useState(false);
  const [runtimeConfigOpenFileRuntimeId, setRuntimeConfigOpenFileRuntimeId] =
    useState<string | null>(null);
  const [runtimeConfigError, setRuntimeConfigError] = useState<string>();
  const [phpToolsTab, setPhpToolsTab] = useState<PhpToolsTab>("extensions");
  const [phpToolsSearch, setPhpToolsSearch] = useState("");
  const [phpFunctions, setPhpFunctions] = useState<PhpFunctionState[]>([]);
  const [workspaceBootLoading, setWorkspaceBootLoading] = useState(true);
  const [persistentTunnelLoaded, setPersistentTunnelLoaded] = useState(false);
  const phpExtensionsRequestRef = useRef(0);
  const phpExtensionPackagesRequestRef = useRef(0);
  const phpFunctionsRequestRef = useRef(0);
  const pushToast = useToastStore((state) => state.push);
  const redisService = useServiceStore((state) =>
    state.services.find((service) => service.name === "redis"),
  );
  const loadServicesForRedisManager = useServiceStore(
    (state) => state.loadServices,
  );
  const showSettingsScrim = useDelayedBusy(workspaceBootLoading);
  const activeTab = (() => {
    const tab = searchParams.get("tab");
    if (tab === "runtimes") {
      return "php";
    }
    if (
      tab === "general" ||
      tab === "php" ||
      tab === "web" ||
      tab === "database" ||
      tab === "tools"
    ) {
      return tab;
    }
    return tab === "tunnel" ? "tunnel" : "general";
  })();
  const showPersistentTunnelScrim = useDelayedBusy(
    activeTab === "tunnel" &&
      persistentTunnelSetupLoading &&
      !workspaceBootLoading,
  );
  const persistentTunnelProjectReady = Boolean(
    persistentTunnelSetup?.ready &&
    persistentTunnelSetup?.defaultHostnameZone &&
    persistentTunnelSetup.defaultHostnameZone.trim(),
  );
  const persistentTunnelSharedTunnelReady = Boolean(
    persistentTunnelSetup?.tunnelId && persistentTunnelSetup?.credentialsPath,
  );

  async function loadReleaseInfo() {
    try {
      setReleaseInfo(await appApi.getReleaseInfo());
    } catch (invokeError) {
      setReleaseInfo(null);
      const details =
        typeof invokeError === "object" &&
        invokeError !== null &&
        "details" in invokeError
          ? String((invokeError as AppError).details ?? "")
          : "";
      const baseMessage = getAppErrorMessage(
        invokeError,
        "Failed to load app release info.",
      );
      setUpdateError(details ? `${baseMessage} ${details}` : baseMessage);
    }
  }

  function persistLastUpdateCheck(value: string) {
    setLastUpdateCheckAt(value);
    if (typeof window !== "undefined") {
      window.localStorage.setItem(SETTINGS_UPDATE_LAST_CHECKED_KEY, value);
    }
  }

  async function handleCheckForUpdates() {
    setUpdateError(undefined);
    setAppUpdateState("checking");

    try {
      const result = await appApi.checkForUpdate();
      setUpdateResult(result);
      persistLastUpdateCheck(result.checkedAt);

      if (result.status === "updateAvailable") {
        setAppUpdateState("updateAvailable");
        pushToast({
          tone: "success",
          message: `DevNest ${result.latestVersion ?? "update"} is ready to install.`,
        });
        return;
      }

      setAppUpdateState("noUpdate");
      pushToast({
        tone: "info",
        message: `DevNest is already on ${result.currentVersion}.`,
      });
    } catch (invokeError) {
      setAppUpdateState("failed");
      const details =
        typeof invokeError === "object" &&
        invokeError !== null &&
        "details" in invokeError
          ? String((invokeError as AppError).details ?? "")
          : "";
      const baseMessage = getAppErrorMessage(
        invokeError,
        "Failed to check for updates.",
      );
      setUpdateError(details ? `${baseMessage} ${details}` : baseMessage);
    }
  }

  async function handleInstallUpdate() {
    if (!updateResult?.latestVersion) {
      return;
    }

    setUpdateError(undefined);
    setAppUpdateState("downloading");

    try {
      if (typeof window !== "undefined") {
        await new Promise((resolve) => window.setTimeout(resolve, 120));
      }

      setAppUpdateState("installing");
      await appApi.installUpdate();
      setAppUpdateState("restartRequired");
      pushToast({
        tone: "success",
        message: `DevNest ${updateResult.latestVersion} is ready. Finish the installer flow and reopen the app if Windows does not relaunch it automatically.`,
      });
    } catch (invokeError) {
      setAppUpdateState("failed");
      const details =
        typeof invokeError === "object" &&
        invokeError !== null &&
        "details" in invokeError
          ? String((invokeError as AppError).details ?? "")
          : "";
      const baseMessage = getAppErrorMessage(
        invokeError,
        "Failed to download and install the update.",
      );
      setUpdateError(details ? `${baseMessage} ${details}` : baseMessage);
    }
  }

  function handleDismissAvailableUpdate() {
    setUpdateResult(null);
    setUpdateError(undefined);
    setAppUpdateState(lastUpdateCheckAt ? "noUpdate" : "idle");
  }

  async function loadRuntimeInventory(refresh = false) {
    setLoading(true);
    setError(undefined);

    try {
      setRuntimes(await (refresh ? runtimeApi.refresh() : runtimeApi.list()));
    } catch (invokeError) {
      setError(
        getAppErrorMessage(invokeError, "Failed to load runtime inventory."),
      );
    } finally {
      setLoading(false);
    }
  }

  function applyActiveRuntimeLocally(nextRuntime: RuntimeInventoryItem) {
    setRuntimes((current) =>
      current.map((runtime) => {
        if (runtime.runtimeType !== nextRuntime.runtimeType) {
          return runtime;
        }

        if (runtime.id === nextRuntime.id) {
          return nextRuntime;
        }

        return {
          ...runtime,
          isActive: false,
        };
      }),
    );
  }

  async function loadRuntimePackages() {
    setPackagesLoading(true);
    setPackageError(undefined);

    try {
      setRuntimePackages(await runtimeApi.listPackages());
    } catch (invokeError) {
      setRuntimePackages([]);
      setPackageError(
        getAppErrorMessage(
          invokeError,
          "Failed to load the runtime package catalog.",
        ),
      );
    } finally {
      setPackagesLoading(false);
    }
  }

  async function loadInstallTask() {
    try {
      setInstallTask(await runtimeApi.getInstallTask());
    } catch {
      setInstallTask(null);
    }
  }

  async function loadOptionalToolInventory() {
    setOptionalToolError(undefined);

    try {
      setOptionalTools(await optionalToolApi.list());
    } catch (invokeError) {
      setOptionalTools([]);
      setOptionalToolError(
        getAppErrorMessage(
          invokeError,
          "Failed to load installed optional tools.",
        ),
      );
    }
  }

  async function loadOptionalToolPackages() {
    setOptionalToolPackageError(undefined);

    try {
      setOptionalToolPackages(await optionalToolApi.listPackages());
    } catch (invokeError) {
      setOptionalToolPackages([]);
      setOptionalToolPackageError(
        getAppErrorMessage(
          invokeError,
          "Failed to load the optional tool catalog.",
        ),
      );
    }
  }

  async function loadOptionalToolInstallTask() {
    try {
      setOptionalToolInstallTask(await optionalToolApi.getInstallTask());
    } catch {
      setOptionalToolInstallTask(null);
    }
  }

  async function loadDownloadCacheSummary() {
    setDownloadCacheLoading(true);
    setDownloadCacheError(undefined);

    try {
      setDownloadCacheSummary(await downloadCacheApi.summary());
    } catch (invokeError) {
      setDownloadCacheSummary(null);
      setDownloadCacheError(
        getAppErrorMessage(
          invokeError,
          "Failed to inspect managed download cache.",
        ),
      );
    } finally {
      setDownloadCacheLoading(false);
    }
  }

  async function loadPersistentTunnelSetup() {
    setPersistentTunnelSetupLoading(true);
    setNamedTunnelsLoading(true);
    setPersistentTunnelError(undefined);

    try {
      const [setup, tunnels] = await Promise.all([
        persistentTunnelApi.getSetupStatus(),
        persistentTunnelApi.listNamedTunnels().catch(() => []),
      ]);
      setPersistentTunnelSetup(setup);
      setNamedTunnels(tunnels);
      setDefaultHostnameZone(setup.defaultHostnameZone ?? "");
    } catch (invokeError) {
      setPersistentTunnelSetup(null);
      setNamedTunnels([]);
      setPersistentTunnelError(
        getAppErrorMessage(
          invokeError,
          "Failed to load the persistent tunnel setup status.",
        ),
      );
    } finally {
      setPersistentTunnelLoaded(true);
      setPersistentTunnelSetupLoading(false);
      setNamedTunnelsLoading(false);
    }
  }

  async function loadNamedTunnels() {
    setNamedTunnelsLoading(true);
    try {
      setNamedTunnels(await persistentTunnelApi.listNamedTunnels());
    } catch {
      setNamedTunnels([]);
    } finally {
      setNamedTunnelsLoading(false);
    }
  }

  async function loadPhpExtensions(runtimeId: string) {
    const requestId = ++phpExtensionsRequestRef.current;

    if (!runtimeId) {
      setPhpExtensions([]);
      setPhpExtensionsError(undefined);
      return;
    }

    setPhpExtensionsLoading(true);
    setPhpExtensionsError(undefined);

    try {
      const nextExtensions = await runtimeApi.listPhpExtensions(runtimeId);
      if (phpExtensionsRequestRef.current !== requestId) {
        return;
      }

      setPhpExtensions(nextExtensions);
    } catch (invokeError) {
      if (phpExtensionsRequestRef.current !== requestId) {
        return;
      }

      setPhpExtensions([]);
      setPhpExtensionsError(
        getAppErrorMessage(invokeError, "Failed to load PHP extension state."),
      );
    } finally {
      if (phpExtensionsRequestRef.current === requestId) {
        setPhpExtensionsLoading(false);
      }
    }
  }

  async function loadPhpExtensionPackages(runtimeId: string) {
    const requestId = ++phpExtensionPackagesRequestRef.current;

    if (!runtimeId) {
      setPhpExtensionPackages([]);
      setPhpExtensionPackagesError(undefined);
      return;
    }

    setPhpExtensionPackagesLoading(true);
    setPhpExtensionPackagesError(undefined);

    try {
      const nextPackages = await runtimeApi.listPhpExtensionPackages(runtimeId);
      if (phpExtensionPackagesRequestRef.current !== requestId) {
        return;
      }

      setPhpExtensionPackages(nextPackages);
    } catch (invokeError) {
      if (phpExtensionPackagesRequestRef.current !== requestId) {
        return;
      }

      setPhpExtensionPackages([]);
      setPhpExtensionPackagesError(
        getAppErrorMessage(
          invokeError,
          "Failed to load the PHP extension catalog.",
        ),
      );
    } finally {
      if (phpExtensionPackagesRequestRef.current === requestId) {
        setPhpExtensionPackagesLoading(false);
      }
    }
  }

  async function loadPhpFunctions(runtimeId: string) {
    const requestId = ++phpFunctionsRequestRef.current;

    if (!runtimeId) {
      setPhpFunctions([]);
      setPhpFunctionsError(undefined);
      return;
    }

    setPhpFunctionsLoading(true);
    setPhpFunctionsError(undefined);

    try {
      const nextFunctions = await runtimeApi.listPhpFunctions(runtimeId);
      if (phpFunctionsRequestRef.current !== requestId) {
        return;
      }

      setPhpFunctions(nextFunctions);
    } catch (invokeError) {
      if (phpFunctionsRequestRef.current !== requestId) {
        return;
      }

      setPhpFunctions([]);
      setPhpFunctionsError(
        getAppErrorMessage(invokeError, "Failed to load PHP function state."),
      );
    } finally {
      if (phpFunctionsRequestRef.current === requestId) {
        setPhpFunctionsLoading(false);
      }
    }
  }

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }

    const stored = window.localStorage.getItem(
      SETTINGS_UPDATE_LAST_CHECKED_KEY,
    );
    if (stored) {
      setLastUpdateCheckAt(stored);
    }
  }, []);

  useEffect(() => {
    async function hydrateSettingsWorkspace() {
      setWorkspaceBootLoading(true);

      await Promise.allSettled([
        loadReleaseInfo(),
        loadRuntimeInventory(),
        loadRuntimePackages(),
        loadInstallTask(),
        loadOptionalToolInventory(),
        loadOptionalToolPackages(),
        loadOptionalToolInstallTask(),
        loadDownloadCacheSummary(),
      ]);

      setWorkspaceBootLoading(false);
    }

    void hydrateSettingsWorkspace();
  }, []);

  useEffect(() => {
    if (activeTab !== "tunnel" || persistentTunnelLoaded) {
      return;
    }

    void loadPersistentTunnelSetup();
  }, [activeTab, persistentTunnelLoaded]);

  useEffect(() => {
    if (!actionLoading?.startsWith("install:")) {
      return;
    }

    void loadInstallTask();
    const intervalId = window.setInterval(() => {
      void loadInstallTask();
    }, 450);

    return () => window.clearInterval(intervalId);
  }, [actionLoading]);

  useEffect(() => {
    if (!actionLoading?.startsWith("optional-install:")) {
      return;
    }

    void loadOptionalToolInstallTask();
    const intervalId = window.setInterval(() => {
      void loadOptionalToolInstallTask();
    }, 450);

    return () => window.clearInterval(intervalId);
  }, [actionLoading]);

  useEffect(() => {
    const phpRuntimes = runtimes.filter(
      (runtime) =>
        runtime.runtimeType === "php" || runtime.runtimeType === "frankenphp",
    );
    if (phpRuntimes.length === 0) {
      if (selectedPhpRuntimeId) {
        setSelectedPhpRuntimeId("");
      }
      setPhpExtensions([]);
      setPhpExtensionsError(undefined);
      setPhpExtensionPackages([]);
      setPhpExtensionPackagesError(undefined);
      setPhpFunctions([]);
      setPhpFunctionsError(undefined);
      return;
    }

    if (
      !selectedPhpRuntimeId ||
      !phpRuntimes.some((runtime) => runtime.id === selectedPhpRuntimeId)
    ) {
      const nextSelectedRuntime =
        phpRuntimes.find((runtime) => runtime.isActive) ?? phpRuntimes[0];
      if (nextSelectedRuntime) {
        setSelectedPhpRuntimeId(nextSelectedRuntime.id);
      }
    }
  }, [runtimes, selectedPhpRuntimeId]);

  useEffect(() => {
    if (!selectedPhpRuntimeId) {
      return;
    }

    void loadPhpExtensions(selectedPhpRuntimeId);
    void loadPhpExtensionPackages(selectedPhpRuntimeId);
    void loadPhpFunctions(selectedPhpRuntimeId);
  }, [selectedPhpRuntimeId]);

  useEffect(() => {
    if (!phpToolsRuntimeId) {
      return;
    }

    function handleKeydown(event: KeyboardEvent) {
      if (event.key !== "Escape") {
        return;
      }

      if (
        document.querySelector(
          ".runtime-tools-dialog [data-nested-modal='true']",
        )
      ) {
        return;
      }

      event.preventDefault();
      setPhpToolsRuntimeId(null);
    }

    document.addEventListener("keydown", handleKeydown);
    return () => document.removeEventListener("keydown", handleKeydown);
  }, [phpToolsRuntimeId]);

  useEffect(() => {
    if (!redisManagerOpen) {
      return;
    }

    function handleKeydown(event: KeyboardEvent) {
      if (event.key !== "Escape") {
        return;
      }

      if (
        document.querySelector(
          ".redis-manager-dialog [data-nested-modal='true']",
        )
      ) {
        return;
      }

      event.preventDefault();
      setRedisManagerOpen(false);
    }

    document.addEventListener("keydown", handleKeydown);
    return () => document.removeEventListener("keydown", handleKeydown);
  }, [redisManagerOpen]);

  useEffect(() => {
    if (!runtimeConfigRuntimeId) {
      return;
    }

    function handleKeydown(event: KeyboardEvent) {
      if (
        event.key !== "Escape" ||
        runtimeConfigSaving ||
        runtimeConfigOpenFileLoading
      ) {
        return;
      }

      event.preventDefault();
      setRuntimeConfigRuntimeId(null);
      setRuntimeConfigError(undefined);
    }

    document.addEventListener("keydown", handleKeydown);
    return () => document.removeEventListener("keydown", handleKeydown);
  }, [
    runtimeConfigOpenFileLoading,
    runtimeConfigRuntimeId,
    runtimeConfigSaving,
  ]);

  useEffect(() => {
    if (!pendingPhpExtensionRemoval) {
      return;
    }

    const extensionName = pendingPhpExtensionRemoval.extensionName;

    function handleKeydown(event: KeyboardEvent) {
      if (
        event.key !== "Escape" ||
        actionLoading === `php-extension-remove:${extensionName}`
      ) {
        return;
      }

      event.preventDefault();
      setPendingPhpExtensionRemoval(null);
    }

    document.addEventListener("keydown", handleKeydown);
    return () => document.removeEventListener("keydown", handleKeydown);
  }, [actionLoading, pendingPhpExtensionRemoval]);

  async function handleConnectPersistentTunnelProvider() {
    setActionLoading("persistent-connect");

    try {
      const setup = await persistentTunnelApi.connectProvider();
      setPersistentTunnelSetup(setup);
      setDefaultHostnameZone(setup.defaultHostnameZone ?? "");
      await loadNamedTunnels();
      pushToast({
        tone: "success",
        title: "Cloudflare connected",
        message: "Named tunnel auth is ready. Create or select a tunnel next.",
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Cloudflare connect failed",
        message: getAppErrorMessage(
          invokeError,
          "DevNest could not finish the cloudflared login flow.",
        ),
      });
    } finally {
      setActionLoading(null);
      await loadPersistentTunnelSetup();
    }
  }

  async function handleImportPersistentTunnelAuthCert() {
    setActionLoading("persistent-import-cert");

    try {
      const setup = await persistentTunnelApi.importAuthCert();
      if (setup) {
        setPersistentTunnelSetup(setup);
        setDefaultHostnameZone(setup.defaultHostnameZone ?? "");
        pushToast({
          tone: "success",
          title: "Auth cert imported",
          message:
            "Managed cloudflared auth cert is ready. Create or select a tunnel next.",
        });
      }
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Auth cert import failed",
        message: getAppErrorMessage(
          invokeError,
          "DevNest could not import the cloudflared auth cert.",
        ),
      });
    } finally {
      setActionLoading(null);
      await loadPersistentTunnelSetup();
    }
  }

  async function handleCreatePersistentNamedTunnel() {
    setActionLoading("persistent-create-tunnel");

    try {
      const setup = await persistentTunnelApi.createNamedTunnel({
        name: createTunnelName,
      });
      setPersistentTunnelSetup(setup);
      setDefaultHostnameZone(setup.defaultHostnameZone ?? "");
      pushToast({
        tone: "success",
        title: "Named tunnel created",
        message: `${setup.tunnelName ?? createTunnelName} is now selected for stable project domains.`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Tunnel create failed",
        message: getAppErrorMessage(
          invokeError,
          "DevNest could not create the named tunnel.",
        ),
      });
    } finally {
      setActionLoading(null);
      await loadPersistentTunnelSetup();
    }
  }

  async function handleImportPersistentTunnelCredentials() {
    setActionLoading("persistent-import-credentials");

    try {
      const setup = await persistentTunnelApi.importCredentials();
      if (setup) {
        setPersistentTunnelSetup(setup);
        setDefaultHostnameZone(setup.defaultHostnameZone ?? "");
        pushToast({
          tone: "success",
          title: "Credentials imported",
          message: "Named tunnel credentials are ready to publish projects.",
        });
      }
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Credentials import failed",
        message: getAppErrorMessage(
          invokeError,
          "DevNest could not import the named tunnel credentials.",
        ),
      });
    } finally {
      setActionLoading(null);
      await loadPersistentTunnelSetup();
    }
  }

  async function handleSelectPersistentNamedTunnel(
    tunnel: PersistentTunnelNamedTunnelSummary,
  ) {
    setActionLoading(`persistent-select:${tunnel.tunnelId}`);

    try {
      const setup = await persistentTunnelApi.selectNamedTunnel({
        tunnelId: tunnel.tunnelId,
      });
      setPersistentTunnelSetup(setup);
      setDefaultHostnameZone(setup.defaultHostnameZone ?? "");
      pushToast({
        tone: "success",
        title: "Named tunnel selected",
        message: `${tunnel.tunnelName} is now the active persistent tunnel for project publishing.`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Tunnel selection failed",
        message: getAppErrorMessage(
          invokeError,
          "DevNest could not select the named tunnel.",
        ),
      });
    } finally {
      setActionLoading(null);
      await loadPersistentTunnelSetup();
    }
  }

  async function handleDeletePersistentNamedTunnel(
    tunnel: PersistentTunnelNamedTunnelSummary,
  ) {
    setActionLoading(`persistent-delete:${tunnel.tunnelId}`);

    try {
      const setup = await persistentTunnelApi.deleteNamedTunnel(
        tunnel.tunnelId,
      );
      setPersistentTunnelSetup(setup);
      setDefaultHostnameZone(setup.defaultHostnameZone ?? "");
      setPendingPersistentTunnelDeletion(null);
      pushToast({
        tone: "success",
        title: "Named tunnel deleted",
        message: `${tunnel.tunnelName} was removed from Cloudflare and DevNest.`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Tunnel delete failed",
        message: getAppErrorMessage(
          invokeError,
          "DevNest could not delete the named tunnel.",
        ),
      });
    } finally {
      setActionLoading(null);
      await loadPersistentTunnelSetup();
    }
  }

  async function handleDisconnectPersistentTunnelProvider() {
    setActionLoading("persistent-disconnect");

    try {
      const setup = await persistentTunnelApi.disconnectProvider();
      setPersistentTunnelSetup(setup);
      setDefaultHostnameZone(setup.defaultHostnameZone ?? "");
      setDisconnectPersistentTunnelConfirm(false);
      pushToast({
        tone: "success",
        title: "Cloudflare setup disconnected",
        message:
          "DevNest cleared the managed Cloudflare setup for this app. Remote tunnels in your Cloudflare account were left untouched.",
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Disconnect failed",
        message: getAppErrorMessage(
          invokeError,
          "DevNest could not disconnect the Cloudflare setup.",
        ),
      });
    } finally {
      setActionLoading(null);
      await loadPersistentTunnelSetup();
    }
  }

  async function handleSavePersistentTunnelZone() {
    setActionLoading("persistent-save-zone");

    try {
      const setup = await persistentTunnelApi.updateSetup({
        defaultHostnameZone: defaultHostnameZone.trim() || null,
      });
      setPersistentTunnelSetup(setup);
      setDefaultHostnameZone(setup.defaultHostnameZone ?? "");
      pushToast({
        tone: "success",
        title: "Default zone saved",
        message: setup.defaultHostnameZone
          ? `Projects can now auto-publish under ${setup.defaultHostnameZone}.`
          : "Default public zone was cleared.",
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Zone save failed",
        message: getAppErrorMessage(
          invokeError,
          "DevNest could not save the default public zone.",
        ),
      });
    } finally {
      setActionLoading(null);
      await loadPersistentTunnelSetup();
    }
  }

  async function handleSetActiveRuntime(runtime: RuntimeInventoryItem) {
    setActionLoading(`activate:${runtime.id}`);

    try {
      const nextRuntime = await runtimeApi.setActive(runtime.id);
      applyActiveRuntimeLocally(nextRuntime);
      pushToast({
        tone:
          nextRuntime.runtimeType === "php" && nextRuntime.details
            ? "warning"
            : "success",
        title: "Active runtime updated",
        message: withRuntimeDetails(
          nextRuntime.runtimeType === "php"
            ? phpCliActivationMessage(nextRuntime.version)
            : `${runtimeTypeLabel(nextRuntime.runtimeType)} ${nextRuntime.version} is now active.`,
          nextRuntime,
        ),
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Set active failed",
        message: getAppErrorMessage(
          invokeError,
          "Failed to set the active runtime.",
        ),
      });
    } finally {
      setActionLoading(null);
    }
  }

  async function handleRestartPhpRuntime(runtime: RuntimeInventoryItem) {
    setActionLoading(`php-runtime-restart:${runtime.id}`);

    try {
      const services = await serviceApi.list();
      const runningWebServices = services.filter(
        (service) =>
          (service.name === "apache" || service.name === "nginx") &&
          service.status === "running",
      );

      if (runningWebServices.length === 0) {
        pushToast({
          tone: "info",
          title: "No web server is running",
          message: `PHP ${runtime.version} is ready. Start Apache or Nginx when you want DevNest to reload its FastCGI worker.`,
        });
        return;
      }

      for (const service of runningWebServices) {
        await serviceApi.restart(service.name);
      }

      pushToast({
        tone: "success",
        title: "Web stack restarted",
        message: `${runningWebServices.map((service) => serviceLabel(service.name)).join(" + ")} reloaded to pick up PHP ${runtime.version} changes.`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Web restart failed",
        message: getAppErrorMessage(
          invokeError,
          "DevNest could not restart the running web service for this PHP runtime.",
        ),
      });
    } finally {
      setActionLoading(null);
    }
  }

  async function handleRemoveRuntime(runtime: RuntimeInventoryItem) {
    setActionLoading(`remove:${runtime.id}`);

    try {
      await runtimeApi.remove(runtime.id);
      await loadRuntimeInventory();
      setPendingRuntimeRemoval(null);
      pushToast({
        tone: "success",
        title:
          runtime.source === "external"
            ? "Runtime reference removed"
            : "Runtime uninstalled",
        message: `${runtimeTypeLabel(runtime.runtimeType)} ${runtime.version} was removed from DevNest.`,
      });
    } catch (invokeError) {
      const details =
        typeof invokeError === "object" &&
        invokeError !== null &&
        "details" in invokeError
          ? String((invokeError as AppError).details ?? "")
          : "";
      const baseMessage = getAppErrorMessage(
        invokeError,
        "Failed to remove the runtime reference.",
      );
      pushToast({
        tone: "error",
        title: "Runtime removal failed",
        message: details ? `${baseMessage} ${details}` : baseMessage,
      });
    } finally {
      setActionLoading(null);
    }
  }

  async function handleRevealRuntime(runtime: RuntimeInventoryItem) {
    setActionLoading(`reveal:${runtime.id}`);

    try {
      await runtimeApi.reveal(runtime.id);
      pushToast({
        tone: "info",
        title: "Opened in Explorer",
        message: `${runtimeTypeLabel(runtime.runtimeType)} ${runtime.version} path opened in Explorer.`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Open path failed",
        message: getAppErrorMessage(
          invokeError,
          "Failed to open the runtime path in Explorer.",
        ),
      });
    } finally {
      setActionLoading(null);
    }
  }

  async function loadRuntimeConfig(runtimeId: string) {
    setRuntimeConfigLoading(true);
    setRuntimeConfigError(undefined);

    try {
      const [schema, values] = await Promise.all([
        runtimeApi.getConfigSchema(runtimeId),
        runtimeApi.getConfigValues(runtimeId),
      ]);
      setRuntimeConfigSchema(schema);
      setRuntimeConfigValues(values);
    } catch (invokeError) {
      setRuntimeConfigSchema(null);
      setRuntimeConfigValues(null);
      setRuntimeConfigError(
        getAppErrorMessage(
          invokeError,
          "Failed to load the managed runtime config.",
        ),
      );
    } finally {
      setRuntimeConfigLoading(false);
    }
  }

  function openRuntimeConfig(runtime: RuntimeInventoryItem) {
    setRuntimeConfigRuntimeId(runtime.id);
    setRuntimeConfigSchema(null);
    setRuntimeConfigValues(null);
    setRuntimeConfigError(undefined);
    void loadRuntimeConfig(runtime.id);
  }

  async function handleSaveRuntimeConfig(patch: Record<string, string>) {
    if (!runtimeConfigRuntimeId) {
      return;
    }

    setRuntimeConfigSaving(true);
    setRuntimeConfigError(undefined);

    try {
      const nextValues = await runtimeApi.updateConfig(
        runtimeConfigRuntimeId,
        patch,
      );
      setRuntimeConfigValues(nextValues);
      pushToast({
        tone: "success",
        title: "Runtime config saved",
        message:
          nextValues.runtimeType === "php"
            ? `PHP ${nextValues.runtimeVersion} config was updated. Restart the running web server to pick up the new php.ini.`
            : `${runtimeTypeLabel(nextValues.runtimeType)} ${nextValues.runtimeVersion} config was updated.`,
      });
    } catch (invokeError) {
      const message = getAppErrorMessage(
        invokeError,
        "Failed to save the managed runtime config.",
      );
      setRuntimeConfigError(message);
      pushToast({
        tone: "error",
        title: "Runtime config save failed",
        message,
      });
    } finally {
      setRuntimeConfigSaving(false);
    }
  }

  async function handleOpenRuntimeConfigFile(runtime: RuntimeInventoryItem) {
    setRuntimeConfigOpenFileLoading(true);
    setRuntimeConfigOpenFileRuntimeId(runtime.id);

    try {
      await runtimeApi.openConfigFile(runtime.id);
      pushToast({
        tone: "info",
        title: "Config file opened",
        message: `${runtimeTypeLabel(runtime.runtimeType)} ${runtime.version} config opened in your default Windows editor.`,
      });
    } catch (invokeError) {
      const message = getAppErrorMessage(
        invokeError,
        "Failed to open the managed runtime config file.",
      );
      setRuntimeConfigError(message);
      pushToast({
        tone: "error",
        title: "Open config failed",
        message,
      });
    } finally {
      setRuntimeConfigOpenFileLoading(false);
      setRuntimeConfigOpenFileRuntimeId(null);
    }
  }

  async function handleInstallPackage(
    runtimePackage: RuntimePackage,
    preferredSetActive?: boolean,
  ) {
    setActionLoading(`install:${runtimePackage.id}`);
    const shouldSetActive =
      preferredSetActive ??
      !runtimes.some(
        (runtime) =>
          runtime.runtimeType === runtimePackage.runtimeType &&
          runtime.isActive,
      );
    setInstallTask({
      packageId: runtimePackage.id,
      displayName: runtimePackage.displayName,
      runtimeType: runtimePackage.runtimeType,
      version: runtimePackage.version,
      stage: "queued",
      message: `Preparing ${runtimePackage.displayName} for download...`,
      updatedAt: new Date().toISOString(),
      errorCode: null,
    });

    try {
      await waitForNextPaint();
      const runtime = await runtimeApi.installPackage(
        runtimePackage.id,
        shouldSetActive,
      );
      await loadRuntimeInventory();
      await loadInstallTask();
      pushToast({
        tone:
          runtime.runtimeType === "php" && runtime.details
            ? "warning"
            : "success",
        title: "Runtime installed",
        message: withRuntimeDetails(
          runtime.runtimeType === "php" && runtime.isActive
            ? `${runtimePackage.displayName} installed successfully and is now active.`
            : `${runtimePackage.displayName} installed successfully${runtime.isActive ? " and is now active" : ""}.`,
          runtime,
        ),
      });
    } catch (invokeError) {
      await loadInstallTask();
      const details =
        typeof invokeError === "object" &&
        invokeError !== null &&
        "details" in invokeError
          ? String((invokeError as AppError).details ?? "")
          : "";
      const baseMessage = getAppErrorMessage(
        invokeError,
        "Failed to download and install the selected runtime package.",
      );
      pushToast({
        tone: "error",
        title: "Runtime install failed",
        message: details ? `${baseMessage} ${details}` : baseMessage,
      });
    } finally {
      setActionLoading(null);
    }
  }

  async function handleTogglePhpExtension(extension: PhpExtensionState) {
    const runtimeId = phpToolsRuntimeId ?? selectedPhpRuntimeId;
    if (!runtimeId) {
      return;
    }

    setActionLoading(`php-extension:${extension.extensionName}`);

    try {
      const updated = await runtimeApi.setPhpExtensionEnabled(
        runtimeId,
        extension.extensionName,
        !extension.enabled,
      );
      await loadPhpExtensions(runtimeId);
      pushToast({
        tone: "success",
        title: "PHP extension updated",
        message: `${phpExtensionLabel(updated.extensionName)} was ${updated.enabled ? "enabled" : "disabled"}. Restart the linked web server to apply the new php.ini.`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "PHP extension update failed",
        message: getAppErrorMessage(
          invokeError,
          "Failed to update the PHP extension state.",
        ),
      });
    } finally {
      setActionLoading(null);
    }
  }

  function openPhpTools(runtime: RuntimeInventoryItem) {
    setSelectedPhpRuntimeId(runtime.id);
    setPhpToolsRuntimeId(runtime.id);
    setPhpToolsTab("extensions");
    setPhpToolsSearch("");
  }

  async function handleInstallPhpExtension() {
    const runtimeId = phpToolsRuntimeId ?? selectedPhpRuntimeId;
    if (!runtimeId) {
      return;
    }

    setActionLoading(`php-extension-install:${runtimeId}`);

    try {
      const result = await runtimeApi.installPhpExtension(runtimeId);
      if (!result) {
        return;
      }

      await loadPhpExtensions(runtimeId);
      pushToast({
        tone: "success",
        title: "PHP extension installed",
        message: `${result.installedExtensions.map(phpExtensionLabel).join(", ")} installed into ${result.runtimeVersion}. Restart the linked web server to apply the updated php.ini.`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "PHP extension install failed",
        message: getAppErrorMessage(
          invokeError,
          "Failed to install the selected PHP extension into the runtime.",
        ),
      });
    } finally {
      setActionLoading(null);
    }
  }

  async function handleRemovePhpExtension(extension: PhpExtensionState) {
    const runtimeId = phpToolsRuntimeId ?? selectedPhpRuntimeId;
    if (!runtimeId) {
      return;
    }

    setActionLoading(`php-extension-remove:${extension.extensionName}`);

    try {
      await runtimeApi.removePhpExtension(runtimeId, extension.extensionName);
      await loadPhpExtensions(runtimeId);
      setPendingPhpExtensionRemoval(null);
      pushToast({
        tone: "success",
        title: "PHP extension uninstalled",
        message: `${phpExtensionLabel(extension.extensionName)} DLL was removed from ${extension.runtimeVersion}. Restart the linked web server to apply the updated php.ini.`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "PHP extension uninstall failed",
        message: getAppErrorMessage(
          invokeError,
          "Failed to remove the PHP extension from this runtime.",
        ),
      });
    } finally {
      setActionLoading(null);
    }
  }

  async function handleInstallPhpExtensionPackage(
    extensionPackage: PhpExtensionPackage,
  ) {
    const runtimeId = phpToolsRuntimeId ?? selectedPhpRuntimeId;
    if (!runtimeId) {
      return;
    }

    setActionLoading(`php-extension-package:${extensionPackage.id}`);

    try {
      const result = await runtimeApi.installPhpExtensionPackage(
        runtimeId,
        extensionPackage.id,
      );
      await loadPhpExtensions(runtimeId);
      await loadPhpExtensionPackages(runtimeId);
      pushToast({
        tone: "success",
        title: "PHP extension installed",
        message: `${result.installedExtensions.map(phpExtensionLabel).join(", ")} installed into ${result.runtimeVersion}. Restart the linked web server to apply the updated php.ini.`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "PHP extension install failed",
        message: getAppErrorMessage(
          invokeError,
          "Failed to download and install the selected PHP extension package.",
        ),
      });
    } finally {
      setActionLoading(null);
    }
  }

  async function handleTogglePhpFunction(functionState: PhpFunctionState) {
    const runtimeId = phpToolsRuntimeId ?? selectedPhpRuntimeId;
    if (!runtimeId) {
      return;
    }

    setActionLoading(`php-function:${functionState.functionName}`);

    try {
      const updated = await runtimeApi.setPhpFunctionEnabled(
        runtimeId,
        functionState.functionName,
        !functionState.enabled,
      );
      await loadPhpFunctions(runtimeId);
      pushToast({
        tone: "success",
        title: "PHP function updated",
        message: `${phpExtensionLabel(updated.functionName)} was ${updated.enabled ? "enabled" : "disabled"} in the managed disable_functions list. Restart the linked web server to apply the new php.ini.`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "PHP function update failed",
        message: getAppErrorMessage(
          invokeError,
          "Failed to update the PHP function state.",
        ),
      });
    } finally {
      setActionLoading(null);
    }
  }

  async function handleInstallOptionalToolPackage(
    optionalToolPackage: OptionalToolPackage,
  ) {
    setActionLoading(`optional-install:${optionalToolPackage.id}`);
    setOptionalToolInstallTask({
      packageId: optionalToolPackage.id,
      displayName: optionalToolPackage.displayName,
      toolType: optionalToolPackage.toolType,
      version: optionalToolPackage.version,
      stage: "queued",
      message: `Preparing ${optionalToolPackage.displayName} for download...`,
      updatedAt: new Date().toISOString(),
      errorCode: null,
    });

    try {
      await waitForNextPaint();
      const installedTool = await optionalToolApi.installPackage(
        optionalToolPackage.id,
      );
      await loadOptionalToolInventory();
      await loadOptionalToolInstallTask();
      pushToast({
        tone: "success",
        title: "Optional tool installed",
        message: `${optionalToolPackage.displayName} installed successfully${installedTool.isActive ? " and is now active" : ""}.`,
      });
    } catch (invokeError) {
      await loadOptionalToolInstallTask();
      const details =
        typeof invokeError === "object" &&
        invokeError !== null &&
        "details" in invokeError
          ? String((invokeError as AppError).details ?? "")
          : "";
      const baseMessage = getAppErrorMessage(
        invokeError,
        "Failed to download and install the selected optional tool.",
      );
      pushToast({
        tone: "error",
        title: "Optional tool install failed",
        message: details ? `${baseMessage} ${details}` : baseMessage,
      });
    } finally {
      setActionLoading(null);
    }
  }

  async function handleClearDownloadCache() {
    setActionLoading("download-cache-clear");
    setDownloadCacheError(undefined);

    try {
      const result = await downloadCacheApi.clear();
      setDownloadCacheSummary(result.summary);
      pushToast({
        tone: "success",
        title: "Download cache cleared",
        message: `Removed ${formatFileSize(result.deletedBytes)} across ${result.deletedFiles} cached file${result.deletedFiles === 1 ? "" : "s"}.`,
      });
    } catch (invokeError) {
      const message = getAppErrorMessage(
        invokeError,
        "Failed to clear the managed download cache.",
      );
      setDownloadCacheError(message);
      pushToast({
        tone: "error",
        title: "Download cache cleanup failed",
        message,
      });
    } finally {
      setActionLoading(null);
    }
  }

  async function handleRevealOptionalTool(tool: OptionalToolInventoryItem) {
    setActionLoading(`optional-reveal:${tool.id}`);

    try {
      await optionalToolApi.reveal(tool.id);
      pushToast({
        tone: "info",
        title: "Folder opened",
        message: `${optionalToolLabel(tool.toolType)} ${displayCatalogVersion(tool.version)} folder opened in Explorer.`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Open path failed",
        message: getAppErrorMessage(
          invokeError,
          "Failed to open the optional tool path in Explorer.",
        ),
      });
    } finally {
      setActionLoading(null);
    }
  }

  async function handleRemoveOptionalTool(tool: OptionalToolInventoryItem) {
    setActionLoading(`optional-remove:${tool.id}`);

    try {
      await optionalToolApi.remove(tool.id);
      await loadOptionalToolInventory();
      setPendingOptionalToolRemoval(null);
      pushToast({
        tone: "success",
        title: "Optional tool uninstalled",
        message: `${optionalToolLabel(tool.toolType)} ${displayCatalogVersion(tool.version)} was removed from DevNest.`,
      });
    } catch (invokeError) {
      const details =
        typeof invokeError === "object" &&
        invokeError !== null &&
        "details" in invokeError
          ? String((invokeError as AppError).details ?? "")
          : "";
      const baseMessage = getAppErrorMessage(
        invokeError,
        "Failed to uninstall the optional tool.",
      );
      pushToast({
        tone: "error",
        title: "Optional tool removal failed",
        message: details ? `${baseMessage} ${details}` : baseMessage,
      });
    } finally {
      setActionLoading(null);
    }
  }

  async function handleOpenRedisManager() {
    setRedisManagerOpen(true);
    try {
      await loadServicesForRedisManager();
    } catch {
      // The manager can still show its own Redis connection state without service metadata.
    }
  }

  const runtimeTypeOrder: RuntimeType[] = [
    "php",
    "apache",
    "nginx",
    "frankenphp",
    "mysql",
  ];
  const optionalToolTypeOrder: OptionalToolType[] = [
    "mailpit",
    "redis",
    "phpmyadmin",
    "restic",
    "cloudflared",
  ];
  const sortedRuntimes = useMemo(
    () =>
      [...runtimes].sort((left, right) => {
        const typeDelta =
          runtimeTypeOrder.indexOf(left.runtimeType) -
          runtimeTypeOrder.indexOf(right.runtimeType);
        if (typeDelta !== 0) {
          return typeDelta;
        }

        if (left.isActive !== right.isActive) {
          return left.isActive ? -1 : 1;
        }

        return right.version.localeCompare(left.version, undefined, {
          numeric: true,
          sensitivity: "base",
        });
      }),
    [runtimes],
  );
  const sortedPackages = useMemo(
    () =>
      [...runtimePackages].sort((left, right) => {
        const typeDelta =
          runtimeTypeOrder.indexOf(left.runtimeType) -
          runtimeTypeOrder.indexOf(right.runtimeType);
        if (typeDelta !== 0) {
          return typeDelta;
        }

        return right.version.localeCompare(left.version, undefined, {
          numeric: true,
          sensitivity: "base",
        });
      }),
    [runtimePackages],
  );
  const sortedOptionalTools = useMemo(
    () =>
      [...optionalTools].sort((left, right) => {
        const typeDelta =
          optionalToolTypeOrder.indexOf(left.toolType) -
          optionalToolTypeOrder.indexOf(right.toolType);
        if (typeDelta !== 0) {
          return typeDelta;
        }

        if (left.isActive !== right.isActive) {
          return left.isActive ? -1 : 1;
        }

        return right.version.localeCompare(left.version, undefined, {
          numeric: true,
          sensitivity: "base",
        });
      }),
    [optionalTools],
  );
  const sortedOptionalToolPackages = useMemo(
    () =>
      [...optionalToolPackages].sort((left, right) => {
        const typeDelta =
          optionalToolTypeOrder.indexOf(left.toolType) -
          optionalToolTypeOrder.indexOf(right.toolType);
        if (typeDelta !== 0) {
          return typeDelta;
        }

        return right.version.localeCompare(left.version, undefined, {
          numeric: true,
          sensitivity: "base",
        });
      }),
    [optionalToolPackages],
  );
  const installedPackageRuntimeIds = useMemo(
    () =>
      new Map(
        runtimes.map(
          (runtime) =>
            [
              runtimeCatalogKey(
                runtime.runtimeType,
                runtime.version,
                runtime.phpFamily,
              ),
              runtime,
            ] as const,
        ),
      ),
    [runtimes],
  );
  const runtimeUpdatePackages = useMemo(
    () =>
      new Map(
        sortedRuntimes
          .map((runtime) => {
            const nextPackage = findRuntimeUpdatePackage(
              runtime,
              runtimePackages,
            );
            if (
              nextPackage &&
              installedPackageRuntimeIds.has(
                runtimeCatalogKey(
                  nextPackage.runtimeType,
                  nextPackage.version,
                  nextPackage.phpFamily,
                ),
              )
            ) {
              return [runtime.id, null] as const;
            }

            return [runtime.id, nextPackage] as const;
          })
          .filter(
            (entry): entry is readonly [string, RuntimePackage] =>
              entry[1] !== null,
          ),
      ),
    [installedPackageRuntimeIds, runtimePackages, sortedRuntimes],
  );
  const installedOptionalToolIds = useMemo(
    () =>
      new Map(
        optionalTools.map(
          (tool) =>
            [
              `${tool.toolType}:${normalizeCatalogVersion(tool.version)}`,
              tool,
            ] as const,
        ),
      ),
    [optionalTools],
  );
  const optionalToolUpdatePackages = useMemo(
    () =>
      new Map(
        sortedOptionalTools
          .map(
            (tool) =>
              [
                tool.id,
                findOptionalToolUpdatePackage(tool, optionalToolPackages),
              ] as const,
          )
          .filter(
            (entry): entry is readonly [string, OptionalToolPackage] =>
              entry[1] !== null,
          ),
      ),
    [optionalToolPackages, sortedOptionalTools],
  );
  const phpRuntimes = useMemo(
    () =>
      sortedRuntimes.filter(
        (runtime) =>
          runtime.runtimeType === "php" || runtime.runtimeType === "frankenphp",
      ),
    [sortedRuntimes],
  );
  const activePhpToolsRuntimeId = phpToolsRuntimeId ?? selectedPhpRuntimeId;
  const selectedPhpRuntime =
    phpRuntimes.find((runtime) => runtime.id === activePhpToolsRuntimeId) ??
    null;
  const selectedRuntimeConfigRuntime =
    sortedRuntimes.find((runtime) => runtime.id === runtimeConfigRuntimeId) ??
    null;
  const phpToolsSearchQuery = phpToolsSearch.trim().toLowerCase();
  const recommendedPhpExtensions = useMemo(() => {
    const installedExtensions = new Map(
      phpExtensions.map(
        (extension) => [extension.extensionName, extension] as const,
      ),
    );
    const packagesByExtension = new Map(
      phpExtensionPackages.map(
        (extensionPackage) =>
          [extensionPackage.extensionName, extensionPackage] as const,
      ),
    );

    return RECOMMENDED_PHP_EXTENSIONS.filter((spec) =>
      matchesPhpToolsSearch(phpToolsSearchQuery, [
        spec.extensionName,
        phpExtensionLabel(spec.extensionName),
        spec.summary,
        spec.keywords.join(" "),
        packagesByExtension.get(spec.extensionName)?.displayName,
        packagesByExtension.get(spec.extensionName)?.notes ?? undefined,
      ]),
    ).map((spec) => ({
      spec,
      installedState: installedExtensions.get(spec.extensionName) ?? null,
      extensionPackage: packagesByExtension.get(spec.extensionName) ?? null,
    }));
  }, [phpExtensionPackages, phpExtensions, phpToolsSearchQuery]);
  const filteredPhpExtensions = useMemo(
    () =>
      phpExtensions.filter((extension) =>
        matchesPhpToolsSearch(phpToolsSearchQuery, [
          extension.extensionName,
          extension.dllFile,
          phpExtensionLabel(extension.extensionName),
          extension.enabled ? "enabled" : "disabled",
        ]),
      ),
    [phpExtensions, phpToolsSearchQuery],
  );
  const filteredPhpFunctions = useMemo(
    () =>
      phpFunctions.filter((functionState) =>
        matchesPhpToolsSearch(phpToolsSearchQuery, [
          functionState.functionName,
          phpExtensionLabel(functionState.functionName),
          functionState.enabled ? "enabled" : "disabled",
        ]),
      ),
    [phpFunctions, phpToolsSearchQuery],
  );
  const phpExtensionPackagesByName = useMemo(
    () =>
      new Map(
        phpExtensionPackages.map(
          (extensionPackage) =>
            [extensionPackage.extensionName, extensionPackage] as const,
        ),
      ),
    [phpExtensionPackages],
  );
  const enabledPhpExtensions = useMemo(
    () => filteredPhpExtensions.filter((extension) => extension.enabled),
    [filteredPhpExtensions],
  );
  const disabledPhpExtensions = useMemo(
    () => filteredPhpExtensions.filter((extension) => !extension.enabled),
    [filteredPhpExtensions],
  );
  const installablePhpExtensionRecommendations = useMemo(
    () =>
      recommendedPhpExtensions.filter(
        ({ installedState, extensionPackage }) =>
          installedState === null && extensionPackage !== null,
      ),
    [recommendedPhpExtensions],
  );
  const missingBundledPhpExtensionRecommendations = useMemo(
    () =>
      recommendedPhpExtensions.filter(
        ({ installedState, extensionPackage, spec }) =>
          installedState === null &&
          extensionPackage === null &&
          spec.source === "bundled",
      ),
    [recommendedPhpExtensions],
  );
  const enabledPhpFunctions = useMemo(
    () => filteredPhpFunctions.filter((functionState) => functionState.enabled),
    [filteredPhpFunctions],
  );
  const disabledPhpFunctions = useMemo(
    () =>
      filteredPhpFunctions.filter((functionState) => !functionState.enabled),
    [filteredPhpFunctions],
  );
  const phpToolsTabs = [
    {
      id: "extensions",
      label: "Extensions",
      meta: `${enabledPhpExtensions.length} enabled now`,
    },
    {
      id: "policy",
      label: "Runtime Policy",
      meta: `${disabledPhpFunctions.length} restricted`,
    },
  ] as const;
  const removalDialogTitle =
    pendingRuntimeRemoval?.source === "external"
      ? "Remove runtime reference?"
      : "Uninstall runtime?";
  const removalDialogAction =
    pendingRuntimeRemoval?.source === "external"
      ? "Remove Runtime"
      : "Uninstall Runtime";
  const webRuntimes = useMemo(
    () =>
      sortedRuntimes.filter(
        (runtime) =>
          runtime.runtimeType === "apache" ||
          runtime.runtimeType === "nginx" ||
          runtime.runtimeType === "frankenphp",
      ),
    [sortedRuntimes],
  );
  const databaseRuntimes = useMemo(
    () => sortedRuntimes.filter((runtime) => runtime.runtimeType === "mysql"),
    [sortedRuntimes],
  );
  const phpPackages = useMemo(
    () =>
      sortedPackages.filter(
        (runtimePackage) => runtimePackage.runtimeType === "php",
      ),
    [sortedPackages],
  );
  const webPackages = useMemo(
    () =>
      sortedPackages.filter(
        (runtimePackage) =>
          runtimePackage.runtimeType === "apache" ||
          runtimePackage.runtimeType === "nginx" ||
          runtimePackage.runtimeType === "frankenphp",
      ),
    [sortedPackages],
  );
  const databasePackages = useMemo(
    () =>
      sortedPackages.filter(
        (runtimePackage) => runtimePackage.runtimeType === "mysql",
      ),
    [sortedPackages],
  );

  function runtimeStatusPresentation(runtime: RuntimeInventoryItem): {
    tone: "success" | "warning" | "error";
    label: string;
  } {
    if (runtime.status === "missing") {
      return { tone: "error", label: "missing" };
    }

    if (runtime.runtimeType === "php") {
      return runtime.isActive
        ? { tone: "success", label: "preferred" }
        : { tone: "success", label: "installed" };
    }

    return runtime.isActive
      ? { tone: "success", label: "active" }
      : { tone: "warning", label: runtime.status };
  }

  function canEditRuntimeConfig(runtime: RuntimeInventoryItem): boolean {
    if (runtime.runtimeType === "php") {
      return true;
    }

    return (
      (runtime.runtimeType === "apache" || runtime.runtimeType === "nginx") &&
      runtime.isActive
    );
  }

  function canOpenRuntimeConfigFile(runtime: RuntimeInventoryItem): boolean {
    if (runtime.runtimeType === "php") {
      return true;
    }

    return runtime.isActive;
  }

  function renderRuntimeFamilyPanel({
    emptyDownloadsDescription,
    emptyDownloadsTitle,
    emptyInstalledDescription,
    emptyInstalledTitle,
    family,
    installedDescription,
    installedTitle,
    packages,
    runtimes: familyRuntimes,
  }: {
    emptyDownloadsDescription: string;
    emptyDownloadsTitle: string;
    emptyInstalledDescription: string;
    emptyInstalledTitle: string;
    family: "php" | "web" | "database";
    installedDescription: string;
    installedTitle: string;
    packages: RuntimePackage[];
    runtimes: RuntimeInventoryItem[];
  }) {
    const downloadsTitle =
      family === "php"
        ? "PHP Downloads"
        : family === "web"
          ? "Web Server Downloads"
          : "Database Downloads";
    const downloadsDescription =
      family === "php"
        ? "Install additional PHP versions into the managed catalog, then open PHP Tools or restart the running web stack when needed."
        : family === "web"
          ? "Install Apache, Nginx, or FrankenPHP builds into the managed runtime root and switch the active runtime when needed."
          : "Install managed database engines into the workspace catalog and keep the active MySQL runtime ready for local use.";

    function renderInstalledRuntimeActions(
      runtime: RuntimeInventoryItem,
      actionableUpdatePackage: RuntimePackage | null,
    ) {
      if (
        runtime.runtimeType === "php" ||
        runtime.runtimeType === "frankenphp"
      ) {
        return (
          <div className="runtime-table-actions runtime-table-actions-compact">
            <Button
              disabled={loading || actionLoading !== null}
              onClick={() => openPhpTools(runtime)}
              variant="primary"
            >
              Tools
            </Button>
            <ActionMenu disabled={loading || actionLoading !== null}>
              {canEditRuntimeConfig(runtime) ? (
                <ActionMenuItem onClick={() => openRuntimeConfig(runtime)}>
                  Config
                </ActionMenuItem>
              ) : null}
              {canOpenRuntimeConfigFile(runtime) ? (
                <ActionMenuItem
                  onClick={() => void handleOpenRuntimeConfigFile(runtime)}
                >
                  {runtimeConfigOpenFileLoading &&
                  runtimeConfigOpenFileRuntimeId === runtime.id
                    ? "Opening file..."
                    : "Open file"}
                </ActionMenuItem>
              ) : null}
              <ActionMenuItem onClick={() => void handleRevealRuntime(runtime)}>
                {actionLoading === `reveal:${runtime.id}`
                  ? "Opening folder..."
                  : "Open folder"}
              </ActionMenuItem>
              {actionableUpdatePackage ? (
                <ActionMenuItem
                  onClick={() =>
                    void handleInstallPackage(
                      actionableUpdatePackage,
                      runtime.isActive,
                    )
                  }
                >
                  {actionLoading === `install:${actionableUpdatePackage.id}`
                    ? "Updating..."
                    : "Update"}
                </ActionMenuItem>
              ) : null}
              <ActionMenuItem
                disabled={runtime.isActive || runtime.status === "missing"}
                onClick={() => void handleSetActiveRuntime(runtime)}
              >
                {actionLoading === `activate:${runtime.id}`
                  ? "Setting..."
                  : "Set active"}
              </ActionMenuItem>
              {runtime.runtimeType === "php" ? (
                <ActionMenuItem
                  disabled={runtime.status === "missing"}
                  onClick={() => void handleRestartPhpRuntime(runtime)}
                >
                  {actionLoading === `php-runtime-restart:${runtime.id}`
                    ? "Restarting..."
                    : "Restart"}
                </ActionMenuItem>
              ) : null}
              <ActionMenuItem
                onClick={() => setPendingRuntimeRemoval(runtime)}
                tone="danger"
              >
                Uninstall
              </ActionMenuItem>
            </ActionMenu>
          </div>
        );
      }

      return (
        <div className="runtime-table-actions runtime-table-actions-compact">
          <ActionMenu disabled={loading || actionLoading !== null}>
            {canEditRuntimeConfig(runtime) ? (
              <ActionMenuItem onClick={() => openRuntimeConfig(runtime)}>
                Config
              </ActionMenuItem>
            ) : null}
            {canOpenRuntimeConfigFile(runtime) ? (
              <ActionMenuItem
                onClick={() => void handleOpenRuntimeConfigFile(runtime)}
              >
                {runtimeConfigOpenFileLoading &&
                runtimeConfigOpenFileRuntimeId === runtime.id
                  ? "Opening file..."
                  : "Open file"}
              </ActionMenuItem>
            ) : null}
            <ActionMenuItem onClick={() => void handleRevealRuntime(runtime)}>
              {actionLoading === `reveal:${runtime.id}`
                ? "Opening folder..."
                : "Open folder"}
            </ActionMenuItem>
            {actionableUpdatePackage ? (
              <ActionMenuItem
                onClick={() =>
                  void handleInstallPackage(
                    actionableUpdatePackage,
                    runtime.isActive,
                  )
                }
              >
                {actionLoading === `install:${actionableUpdatePackage.id}`
                  ? "Updating..."
                  : "Update"}
              </ActionMenuItem>
            ) : null}
            <ActionMenuItem
              disabled={runtime.isActive || runtime.status === "missing"}
              onClick={() => void handleSetActiveRuntime(runtime)}
            >
              {actionLoading === `activate:${runtime.id}`
                ? "Setting..."
                : "Set active"}
            </ActionMenuItem>
            <ActionMenuItem
              onClick={() => setPendingRuntimeRemoval(runtime)}
              tone="danger"
            >
              Uninstall
            </ActionMenuItem>
          </ActionMenu>
        </div>
      );
    }

    return (
      <>
        <Card>
          <div className="page-header">
            <div>
              <h2>{installedTitle}</h2>
              <p>{installedDescription}</p>
            </div>
          </div>

          {family === "database" ? (
            <div className="inline-note-card" data-tone="warning">
              <strong>Shared database state</strong>
              <span>
                DevNest currently reuses one managed data directory for all
                installed MariaDB/MySQL runtimes. Version switches keep the same
                databases and recovery files, so older builds can fail to start
                until the previous data state is shut down cleanly, backed up,
                or moved aside.
              </span>
            </div>
          ) : null}

          {familyRuntimes.length > 0 ? (
            <div className="runtime-table-shell">
              <table className="runtime-table">
                <thead>
                  <tr>
                    <th>Runtime</th>
                    <th>Version</th>
                    {family === "web" ? <th>PHP Family</th> : null}
                    <th>Status</th>
                    <th>Update</th>
                    <th>Updated</th>
                    <th>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {familyRuntimes.map((runtime) => {
                    const updatePackage =
                      runtimeUpdatePackages.get(runtime.id) ?? null;
                    const higherInstalledRuntime =
                      [...familyRuntimes]
                        .filter((candidate) =>
                          runtimeCanOfferUpdateTo(runtime, candidate),
                        )
                        .sort((left, right) =>
                          compareRuntimeVersions(right.version, left.version),
                        )[0] ?? null;
                    const actionableUpdatePackage =
                      higherInstalledRuntime === null ? updatePackage : null;
                    const status = runtimeStatusPresentation(runtime);

                    return (
                      <tr key={runtime.id}>
                        <td>
                          <div className="runtime-table-type">
                            <strong>
                              {runtimeTypeLabel(runtime.runtimeType)}
                            </strong>
                            <span className="runtime-table-note">
                              {runtime.runtimeType === "php"
                                ? runtime.isActive
                                  ? "Preferred CLI version and fallback for optional PHP web tools."
                                  : "Tracked per project via project PHP version."
                                : runtime.runtimeType === "frankenphp"
                                  ? `Embedded PHP ${runtime.phpFamily ?? "unknown"} · experimental web server`
                                  : runtimeFamilyLabel(runtime.runtimeType)}
                            </span>
                          </div>
                        </td>
                        <td>{runtime.version}</td>
                        {family === "web" ? (
                          <td>
                            {runtime.runtimeType === "frankenphp" ? (
                              <div className="runtime-status-copy">
                                <span
                                  className="status-chip"
                                  data-tone={
                                    runtime.phpFamily ? "success" : "warning"
                                  }
                                >
                                  {runtime.phpFamily
                                    ? `PHP ${runtime.phpFamily}`
                                    : "Unknown"}
                                </span>
                                <span className="runtime-table-note">
                                  Embedded in the selected FrankenPHP binary.
                                </span>
                              </div>
                            ) : (
                              <span className="runtime-table-note">
                                Uses linked PHP runtime
                              </span>
                            )}
                          </td>
                        ) : null}
                        <td>
                          <div className="runtime-status-copy">
                            <span
                              className="status-chip"
                              data-tone={status.tone}
                            >
                              {status.label}
                            </span>
                            {runtime.details ? (
                              <span className="helper-text">
                                {runtime.details}
                              </span>
                            ) : null}
                          </div>
                        </td>
                        <td>
                          {actionableUpdatePackage ? (
                            <div className="runtime-status-copy">
                              <span className="status-chip" data-tone="warning">
                                Available
                              </span>
                              <span className="runtime-table-note">
                                {actionableUpdatePackage.displayName} is ready
                                to install.
                              </span>
                            </div>
                          ) : higherInstalledRuntime ? (
                            <div className="runtime-status-copy">
                              <span className="status-chip" data-tone="success">
                                Installed
                              </span>
                              <span className="runtime-table-note">
                                {runtimeTypeLabel(runtime.runtimeType)}{" "}
                                {higherInstalledRuntime.version} is already
                                installed.
                              </span>
                            </div>
                          ) : (
                            <span className="runtime-table-note">
                              Current catalog is up to date.
                            </span>
                          )}
                        </td>
                        <td>{formatUpdatedAt(runtime.updatedAt)}</td>
                        <td>
                          {renderInstalledRuntimeActions(
                            runtime,
                            actionableUpdatePackage,
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          ) : (
            <EmptyState
              title={
                loading
                  ? `Loading ${installedTitle.toLowerCase()}`
                  : emptyInstalledTitle
              }
              description={emptyInstalledDescription}
            />
          )}
        </Card>

        <Card>
          <div className="page-header">
            <div>
              <h2>{downloadsTitle}</h2>
              <p>{downloadsDescription}</p>
            </div>
          </div>

          {packages.length > 0 ? (
            <div className="runtime-table-shell">
              <table className="runtime-table">
                <thead>
                  <tr>
                    <th>Package</th>
                    <th>Version</th>
                    {family === "web" ? <th>PHP Family</th> : null}
                    <th>Platform</th>
                    <th>Status</th>
                    <th>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {packages.map((runtimePackage) => {
                    const installedRuntime =
                      installedPackageRuntimeIds.get(
                        runtimeCatalogKey(
                          runtimePackage.runtimeType,
                          runtimePackage.version,
                          runtimePackage.phpFamily,
                        ),
                      ) ?? null;
                    const currentTask =
                      installTask?.packageId === runtimePackage.id
                        ? installTask
                        : null;

                    return (
                      <tr key={runtimePackage.id}>
                        <td>
                          <div className="runtime-table-type">
                            <strong>{runtimePackage.displayName}</strong>
                            <span className="runtime-table-note">
                              {runtimeTypeLabel(runtimePackage.runtimeType)}
                            </span>
                            <span className="mono">
                              {runtimePackage.entryBinary}
                            </span>
                          </div>
                        </td>
                        <td>{runtimePackage.version}</td>
                        {family === "web" ? (
                          <td>
                            {runtimePackage.runtimeType === "frankenphp" ? (
                              <div className="runtime-status-copy">
                                <span
                                  className="status-chip"
                                  data-tone={
                                    runtimePackage.phpFamily
                                      ? "success"
                                      : "warning"
                                  }
                                >
                                  {runtimePackage.phpFamily
                                    ? `PHP ${runtimePackage.phpFamily}`
                                    : "Unknown"}
                                </span>
                                <span className="runtime-table-note">
                                  Select a build matching the projects that will
                                  use FrankenPHP.
                                </span>
                              </div>
                            ) : (
                              <span className="runtime-table-note">
                                External PHP runtime
                              </span>
                            )}
                          </td>
                        ) : null}
                        <td>
                          {runtimePackage.platform} {runtimePackage.arch}
                        </td>
                        <td>
                          <div className="runtime-status-copy">
                            <span
                              className="status-chip"
                              data-tone={
                                currentTask?.stage === "failed"
                                  ? "error"
                                  : currentTask?.stage === "completed" ||
                                      installedRuntime
                                    ? "success"
                                    : "warning"
                              }
                            >
                              {currentTask
                                ? runtimeInstallStageLabel(currentTask.stage)
                                : installedRuntime
                                  ? "Installed"
                                  : "Ready"}
                            </span>
                            <span className="runtime-table-note">
                              {currentTask?.message ??
                                runtimePackage.notes ??
                                "Managed package download."}
                            </span>
                          </div>
                        </td>
                        <td>
                          <div className="runtime-table-actions">
                            <Button
                              disabled={
                                loading ||
                                packagesLoading ||
                                actionLoading !== null
                              }
                              onClick={() =>
                                void handleInstallPackage(runtimePackage)
                              }
                              variant="primary"
                            >
                              {actionLoading === `install:${runtimePackage.id}`
                                ? "Installing..."
                                : installedRuntime
                                  ? "Reinstall"
                                  : "Install"}
                            </Button>
                            {installedRuntime ? (
                              <Button
                                className="button-danger"
                                disabled={
                                  loading ||
                                  packagesLoading ||
                                  actionLoading !== null
                                }
                                onClick={() =>
                                  setPendingRuntimeRemoval(installedRuntime)
                                }
                              >
                                Uninstall
                              </Button>
                            ) : null}
                          </div>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          ) : (
            <EmptyState
              title={
                packagesLoading
                  ? `Loading ${downloadsTitle.toLowerCase()}`
                  : emptyDownloadsTitle
              }
              description={emptyDownloadsDescription}
            />
          )}
        </Card>
      </>
    );
  }

  const settingsTabs = [
    {
      id: "general",
      label: "General",
      meta: releaseInfo?.currentVersion ?? "app info",
    },
    {
      id: "php",
      label: "PHP",
      meta: `${phpRuntimes.length} installed`,
    },
    {
      id: "web",
      label: "Web Servers",
      meta: `${webRuntimes.length} installed`,
    },
    {
      id: "database",
      label: "Databases",
      meta: `${databaseRuntimes.length} installed`,
    },
    {
      id: "tunnel",
      label: "Persistent Tunnel",
      meta: persistentTunnelProjectReady ? "ready" : "needs setup",
    },
    {
      id: "tools",
      label: "Optional Tools",
      meta: `${sortedOptionalTools.length} installed`,
    },
  ] as const;
  function handleSelectTab(
    tab: "general" | "php" | "web" | "database" | "tunnel" | "tools",
  ) {
    setSearchParams(
      mergeSearchParams(searchParams, {
        tab: tab === "general" ? undefined : tab,
      }),
    );
  }

  const appUpdateTone =
    appUpdateState === "failed"
      ? "error"
      : appUpdateState === "updateAvailable" ||
          appUpdateState === "restartRequired"
        ? "success"
        : "warning";

  const appUpdateLabel = (() => {
    switch (appUpdateState) {
      case "checking":
        return "Checking";
      case "noUpdate":
        return "Up to date";
      case "updateAvailable":
        return "Update ready";
      case "downloading":
        return "Downloading";
      case "installing":
        return "Installing";
      case "restartRequired":
        return "Restart required";
      case "failed":
        return "Failed";
      default:
        return releaseInfo?.updaterConfigured ? "Ready" : "Needs config";
    }
  })();

  const updateSummary = (() => {
    switch (appUpdateState) {
      case "checking":
        return "DevNest is reading the signed metadata endpoint for a newer packaged Windows build.";
      case "noUpdate":
        return "No newer packaged build was announced for this release channel.";
      case "updateAvailable":
        return (
          updateResult?.notes ??
          `DevNest ${updateResult?.latestVersion ?? "update"} is available and ready to install.`
        );
      case "downloading":
        return `Downloading DevNest ${updateResult?.latestVersion ?? "update"} from the hosted release artifact.`;
      case "installing":
        return "Verifying the signature and handing off to the Windows installer.";
      case "restartRequired":
        return "Finish the installer flow, then reopen DevNest on the newer version if Windows does not bring it back automatically.";
      case "failed":
        return updateError ?? "DevNest could not complete the update flow.";
      default:
        return releaseInfo?.updaterConfigured
          ? "Use Check Updates to query the hosted metadata feed for the stable channel."
          : "This build still needs an updater public key injected at release build time before it can check for signed updates.";
    }
  })();

  return (
    <PageLayout
      actions={
        <Button
          onClick={() => {
            void loadReleaseInfo();
            void loadRuntimeInventory(true);
            void loadRuntimePackages();
            void loadOptionalToolInventory();
            void loadOptionalToolPackages();
            void loadDownloadCacheSummary();
            if (activeTab === "tunnel" || persistentTunnelLoaded) {
              void loadPersistentTunnelSetup();
            }
          }}
          variant="primary"
        >
          {loading || packagesLoading ? "Refreshing..." : "Refresh Catalog"}
        </Button>
      }
      subtitle="Manage app updates, runtimes, defaults, databases, tunnels, and optional tools from one place."
      title="Settings"
    >
      {error ? <span className="error-text">{error}</span> : null}
      {packageError ? <span className="error-text">{packageError}</span> : null}
      {optionalToolError ? (
        <span className="error-text">{optionalToolError}</span>
      ) : null}
      {optionalToolPackageError ? (
        <span className="error-text">{optionalToolPackageError}</span>
      ) : null}
      {downloadCacheError ? (
        <span className="error-text">{downloadCacheError}</span>
      ) : null}
      {persistentTunnelError ? (
        <span className="error-text">{persistentTunnelError}</span>
      ) : null}
      {updateError && appUpdateState !== "failed" ? (
        <span className="error-text">{updateError}</span>
      ) : null}

      {installTask ? (
        <Card className="runtime-install-card">
          <div className="runtime-install-status">
            <div className="runtime-install-copy">
              <strong>{installTask.displayName}</strong>
              <span className="helper-text">{installTask.message}</span>
            </div>
            <span
              className="status-chip"
              data-tone={
                installTask.stage === "failed"
                  ? "error"
                  : installTask.stage === "completed"
                    ? "success"
                    : "warning"
              }
            >
              {runtimeInstallStageLabel(installTask.stage)}
            </span>
          </div>
        </Card>
      ) : null}

      {optionalToolInstallTask ? (
        <Card className="runtime-install-card">
          <div className="runtime-install-status">
            <div className="runtime-install-copy">
              <strong>{optionalToolInstallTask.displayName}</strong>
              <span className="helper-text">
                {optionalToolInstallTask.message}
              </span>
            </div>
            <span
              className="status-chip"
              data-tone={
                optionalToolInstallTask.stage === "failed"
                  ? "error"
                  : optionalToolInstallTask.stage === "completed"
                    ? "success"
                    : "warning"
              }
            >
              {optionalToolInstallStageLabel(optionalToolInstallTask.stage)}
            </span>
          </div>
        </Card>
      ) : null}

      <div className="route-loading-shell">
        <div className="stack workspace-shell">
          <StickyTabs
            activeTab={activeTab}
            ariaLabel="Settings sections"
            items={settingsTabs}
            onSelect={handleSelectTab}
          />

          <div
            aria-labelledby="workspace-tab-general"
            className="workspace-panel"
            hidden={activeTab !== "general"}
            id="workspace-panel-general"
            role="tabpanel"
          >
            <Card className="runtime-toolbar-card app-update-card">
              <div className="page-header">
                <div>
                  <h2>App Updates</h2>
                  <p>
                    Check the signed Windows release feed, then install packaged
                    updates without leaving DevNest.
                  </p>
                </div>
                <div className="page-toolbar">
                  <Button
                    disabled={
                      !releaseInfo?.updaterConfigured ||
                      appUpdateState === "checking" ||
                      appUpdateState === "downloading" ||
                      appUpdateState === "installing"
                    }
                    onClick={() => void handleCheckForUpdates()}
                    variant="primary"
                  >
                    {appUpdateState === "checking"
                      ? "Checking..."
                      : "Check Updates"}
                  </Button>
                  {appUpdateState === "updateAvailable" ||
                  appUpdateState === "downloading" ||
                  appUpdateState === "installing" ? (
                    <>
                      <Button
                        disabled={
                          appUpdateState === "downloading" ||
                          appUpdateState === "installing"
                        }
                        onClick={() => void handleInstallUpdate()}
                        variant="primary"
                      >
                        {appUpdateState === "downloading"
                          ? "Downloading..."
                          : appUpdateState === "installing"
                            ? "Installing..."
                            : "Download and Install"}
                      </Button>
                      <Button onClick={handleDismissAvailableUpdate}>
                        Later
                      </Button>
                    </>
                  ) : null}
                </div>
              </div>

              <div className="detail-grid">
                <div className="detail-item">
                  <span className="detail-label">Current Version</span>
                  <strong className="mono detail-value">
                    {releaseInfo?.currentVersion ?? "Loading..."}
                  </strong>
                </div>
                <div className="detail-item">
                  <span className="detail-label">Release Channel</span>
                  <strong>{releaseInfo?.releaseChannel ?? "stable"}</strong>
                </div>
                <div className="detail-item">
                  <span className="detail-label">Update Status</span>
                  <strong>
                    <span className="status-chip" data-tone={appUpdateTone}>
                      {appUpdateLabel}
                    </span>
                  </strong>
                </div>
                <div className="detail-item">
                  <span className="detail-label">Signing Key</span>
                  <strong>
                    <span
                      className="status-chip"
                      data-tone={
                        releaseInfo?.updaterConfigured ? "success" : "warning"
                      }
                    >
                      {releaseInfo?.updaterConfigured ? "embedded" : "missing"}
                    </span>
                  </strong>
                </div>
                <div className="detail-item">
                  <span className="detail-label">Last Checked</span>
                  <strong>
                    {lastUpdateCheckAt
                      ? formatUpdatedAt(lastUpdateCheckAt)
                      : "Not checked yet"}
                  </strong>
                </div>
                <div className="detail-item">
                  <span className="detail-label">Metadata Endpoint</span>
                  <strong className="mono detail-value">
                    {releaseInfo?.updateEndpoint ?? "Not configured"}
                  </strong>
                </div>
              </div>

              <div className="app-update-callout">
                <div className="app-update-copy">
                  <strong>
                    {appUpdateState === "updateAvailable" &&
                    updateResult?.latestVersion
                      ? `DevNest ${updateResult.latestVersion} is available`
                      : "Manual update flow"}
                  </strong>
                  <span>{updateSummary}</span>
                </div>
                {updateResult?.pubDate ? (
                  <span className="helper-text">
                    Published {formatUpdatedAt(updateResult.pubDate)}
                  </span>
                ) : null}
              </div>

              {appUpdateState === "failed" && updateError ? (
                <span className="error-text">{updateError}</span>
              ) : null}
            </Card>
          </div>

          <div
            aria-labelledby="workspace-tab-tunnel"
            className="workspace-panel"
            hidden={activeTab !== "tunnel"}
            id="workspace-panel-tunnel"
            role="tabpanel"
          >
            <Card>
              <div className="page-header">
                <div>
                  <h2>Persistent Tunnel Setup</h2>
                  <p>
                    Connect Cloudflare once, choose the one shared named tunnel
                    DevNest should use, then set the default zone projects will
                    publish under when you leave the hostname blank.
                  </p>
                </div>
                <div className="page-toolbar">
                  <Button
                    disabled={actionLoading !== null}
                    onClick={() => void loadPersistentTunnelSetup()}
                  >
                    {loading || persistentTunnelSetupLoading
                      ? "Refreshing..."
                      : "Refresh Setup"}
                  </Button>
                  <Button
                    disabled={actionLoading !== null}
                    onClick={() => void handleConnectPersistentTunnelProvider()}
                  >
                    {actionLoading === "persistent-connect"
                      ? persistentTunnelSetup?.authCertPath
                        ? "Reconnecting..."
                        : "Connecting..."
                      : persistentTunnelSetup?.authCertPath
                        ? "Reconnect Cloudflare"
                        : "Connect Cloudflare"}
                  </Button>
                  <Button
                    className={
                      persistentTunnelSetup?.tunnelId
                        ? "button-danger"
                        : undefined
                    }
                    disabled={
                      actionLoading !== null || !persistentTunnelSetup?.managed
                    }
                    onClick={() => setDisconnectPersistentTunnelConfirm(true)}
                  >
                    {actionLoading === "persistent-disconnect"
                      ? "Disconnecting..."
                      : "Disconnect"}
                  </Button>
                </div>
              </div>

              {persistentTunnelSetup ? (
                <>
                  <div className="detail-grid">
                    <div className="detail-item">
                      <span className="detail-label">Project Publish</span>
                      <strong>
                        <span
                          className="status-chip"
                          data-tone={
                            persistentTunnelProjectReady ? "success" : "warning"
                          }
                        >
                          {persistentTunnelProjectReady
                            ? "ready"
                            : "needs setup"}
                        </span>
                      </strong>
                    </div>
                    <div className="detail-item">
                      <span className="detail-label">Connection</span>
                      <strong>
                        <span
                          className="status-chip"
                          data-tone={
                            persistentTunnelSetup.authCertPath
                              ? "success"
                              : "warning"
                          }
                        >
                          {persistentTunnelSetup.authCertPath
                            ? "connected"
                            : "not connected"}
                        </span>
                      </strong>
                    </div>
                    <div className="detail-item">
                      <span className="detail-label">Shared Tunnel</span>
                      <strong>
                        <span
                          className="status-chip"
                          data-tone={
                            persistentTunnelSharedTunnelReady
                              ? "success"
                              : "warning"
                          }
                        >
                          {persistentTunnelSharedTunnelReady
                            ? "selected"
                            : persistentTunnelSetup.tunnelId
                              ? "needs credentials"
                              : "not selected"}
                        </span>
                      </strong>
                    </div>
                    <div className="detail-item">
                      <span className="detail-label">Default Zone</span>
                      <strong className="mono detail-value">
                        {persistentTunnelSetup.defaultHostnameZone ??
                          "Not configured"}
                      </strong>
                    </div>
                  </div>
                  <span className="helper-text">
                    {persistentTunnelProjectReady
                      ? `Projects can now leave the hostname blank and publish as project-name.${persistentTunnelSetup.defaultHostnameZone}.`
                      : persistentTunnelSetup.details}
                  </span>

                  <div className="detail-grid" style={{ marginTop: 16 }}>
                    <div className="detail-item">
                      <span className="detail-label">Shared Tunnel Name</span>
                      <input
                        className="input"
                        onChange={(event) =>
                          setCreateTunnelName(event.target.value)
                        }
                        placeholder="devnest-main"
                        type="text"
                        value={createTunnelName}
                      />
                      <span className="helper-text">
                        Create one DevNest-owned shared tunnel, then reuse it
                        across published projects through managed ingress rules.
                      </span>
                      <div className="page-toolbar" style={{ marginTop: 12 }}>
                        <Button
                          disabled={
                            actionLoading !== null ||
                            !persistentTunnelSetup.authCertPath
                          }
                          onClick={() =>
                            void handleCreatePersistentNamedTunnel()
                          }
                          variant="primary"
                        >
                          {actionLoading === "persistent-create-tunnel"
                            ? "Creating..."
                            : "Create Tunnel"}
                        </Button>
                      </div>
                      {persistentTunnelSetup.tunnelName ? (
                        <span className="helper-text" style={{ marginTop: 8 }}>
                          Current shared tunnel:{" "}
                          <span className="mono">
                            {persistentTunnelSetup.tunnelName}
                          </span>
                        </span>
                      ) : null}
                    </div>
                    <div className="detail-item">
                      <span className="detail-label">Default Public Zone</span>
                      <input
                        className="input mono"
                        onChange={(event) =>
                          setDefaultHostnameZone(event.target.value)
                        }
                        placeholder="preview.example.com"
                        type="text"
                        value={defaultHostnameZone}
                      />
                      <span className="helper-text">
                        Projects that leave the hostname blank publish as
                        `project-name.{defaultHostnameZone || "your-zone"}`.
                      </span>
                      <div className="page-toolbar" style={{ marginTop: 12 }}>
                        <Button
                          disabled={
                            actionLoading !== null ||
                            !persistentTunnelSetup.authCertPath
                          }
                          onClick={() => void handleSavePersistentTunnelZone()}
                        >
                          {actionLoading === "persistent-save-zone"
                            ? "Saving..."
                            : "Set Default Zone"}
                        </Button>
                      </div>
                    </div>
                  </div>

                  <div className="detail-grid" style={{ marginTop: 16 }}>
                    <div className="detail-item">
                      <span className="detail-label">Advanced Recovery</span>
                      <span className="helper-text">
                        Only use these if you already have a Cloudflare auth
                        cert or a named tunnel credentials JSON that DevNest
                        should adopt.
                      </span>
                      <div className="page-toolbar" style={{ marginTop: 12 }}>
                        <Button
                          disabled={actionLoading !== null}
                          onClick={() =>
                            void handleImportPersistentTunnelAuthCert()
                          }
                        >
                          {actionLoading === "persistent-import-cert"
                            ? "Importing..."
                            : "Import Cert"}
                        </Button>
                        <Button
                          disabled={actionLoading !== null}
                          onClick={() =>
                            void handleImportPersistentTunnelCredentials()
                          }
                        >
                          {actionLoading === "persistent-import-credentials"
                            ? "Importing..."
                            : "Import Credentials"}
                        </Button>
                      </div>
                    </div>
                    <div className="detail-item">
                      <span className="detail-label">Managed Paths</span>
                      <span className="helper-text">Auth Cert</span>
                      <strong className="mono detail-value">
                        {persistentTunnelSetup.authCertPath ?? "Not found"}
                      </strong>
                      <span className="helper-text" style={{ marginTop: 10 }}>
                        Credentials
                      </span>
                      <strong className="mono detail-value">
                        {persistentTunnelSetup.credentialsPath ?? "Not found"}
                      </strong>
                    </div>
                  </div>

                  {namedTunnels.length > 0 ? (
                    <div
                      className="runtime-table-shell"
                      style={{ marginTop: 16 }}
                    >
                      <table className="runtime-table">
                        <thead>
                          <tr>
                            <th>Name</th>
                            <th>Source</th>
                            <th>Status</th>
                            <th>Actions</th>
                          </tr>
                        </thead>
                        <tbody>
                          {namedTunnels.map((tunnel) => (
                            <tr key={tunnel.tunnelId}>
                              <td>
                                <div className="stack" style={{ gap: 4 }}>
                                  <strong>{tunnel.tunnelName}</strong>
                                  <span className="helper-text mono">
                                    {tunnel.tunnelId}
                                  </span>
                                </div>
                              </td>
                              <td>
                                {tunnel.credentialsPath
                                  ? "Managed"
                                  : "Cloudflare account"}
                              </td>
                              <td>
                                <span
                                  className="status-chip"
                                  data-tone={
                                    tunnel.selected
                                      ? "success"
                                      : tunnel.credentialsPath
                                        ? "warning"
                                        : "error"
                                  }
                                >
                                  {tunnel.selected
                                    ? "Selected"
                                    : tunnel.credentialsPath
                                      ? "Ready"
                                      : "Needs Credentials"}
                                </span>
                              </td>
                              <td>
                                <div className="runtime-table-actions">
                                  {tunnel.credentialsPath ? (
                                    <Button
                                      disabled={
                                        actionLoading !== null ||
                                        tunnel.selected
                                      }
                                      onClick={() =>
                                        void handleSelectPersistentNamedTunnel(
                                          tunnel,
                                        )
                                      }
                                      variant={
                                        tunnel.selected ? undefined : "primary"
                                      }
                                    >
                                      {actionLoading ===
                                      `persistent-select:${tunnel.tunnelId}`
                                        ? "Selecting..."
                                        : tunnel.selected
                                          ? "Selected"
                                          : "Use"}
                                    </Button>
                                  ) : (
                                    <Button
                                      disabled={actionLoading !== null}
                                      onClick={() =>
                                        void handleImportPersistentTunnelCredentials()
                                      }
                                    >
                                      {actionLoading ===
                                      "persistent-import-credentials"
                                        ? "Importing..."
                                        : "Import Credentials"}
                                    </Button>
                                  )}
                                  <Button
                                    className="button-danger"
                                    disabled={actionLoading !== null}
                                    onClick={() =>
                                      setPendingPersistentTunnelDeletion(tunnel)
                                    }
                                  >
                                    {actionLoading ===
                                    `persistent-delete:${tunnel.tunnelId}`
                                      ? "Deleting..."
                                      : "Delete"}
                                  </Button>
                                </div>
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  ) : (
                    <EmptyState
                      title="No named tunnels yet"
                      description="Connect Cloudflare, then create one shared tunnel or import its credentials so DevNest can publish projects on stable domains."
                    />
                  )}

                  {persistentTunnelSetup.guidance ? (
                    <span className="helper-text" style={{ marginTop: 12 }}>
                      {persistentTunnelSetup.guidance}
                    </span>
                  ) : null}
                  <span className="helper-text" style={{ marginTop: 12 }}>
                    Delete removes a tunnel from Cloudflare. Disconnect clears
                    DevNest's managed setup only and does not delete remote
                    tunnels from your Cloudflare account.
                  </span>
                  <span className="helper-text" style={{ marginTop: 6 }}>
                    If a tunnel shows `Needs Credentials`, DevNest can see it in
                    your Cloudflare account but still needs that tunnel's JSON
                    imported into this workspace before it can be selected.
                  </span>
                </>
              ) : (
                <EmptyState
                  title="Persistent tunnel setup not loaded yet"
                  description="Refresh setup after installing cloudflared and preparing your named tunnel credentials."
                />
              )}
            </Card>
          </div>

          <div
            aria-labelledby="workspace-tab-php"
            className="workspace-panel"
            hidden={activeTab !== "php"}
            id="workspace-panel-php"
            role="tabpanel"
          >
            {renderRuntimeFamilyPanel({
              family: "php",
              installedTitle: "Installed PHP",
              installedDescription:
                "Track multiple PHP versions side by side. Projects choose their own version, while this panel focuses on PHP Tools and web reload workflows.",
              runtimes: phpRuntimes,
              packages: phpPackages,
              emptyInstalledTitle: "No PHP runtimes installed yet",
              emptyInstalledDescription:
                "Install one or more PHP versions from the catalog below. DevNest will keep them available for per-project routing.",
              emptyDownloadsTitle: "No PHP packages available",
              emptyDownloadsDescription:
                "Configure the runtime manifest to expose downloadable PHP builds in this workspace.",
            })}
          </div>

          <div
            aria-labelledby="workspace-tab-web"
            className="workspace-panel"
            hidden={activeTab !== "web"}
            id="workspace-panel-web"
            role="tabpanel"
          >
            {renderRuntimeFamilyPanel({
              family: "web",
              installedTitle: "Installed Web Servers",
              installedDescription:
                "Manage Apache, Nginx, and FrankenPHP builds here. FrankenPHP is tracked as an experimental web server with embedded PHP family compatibility.",
              runtimes: webRuntimes,
              packages: webPackages,
              emptyInstalledTitle: "No web servers installed yet",
              emptyInstalledDescription:
                "Install Apache, Nginx, or FrankenPHP from the managed catalog so projects can attach to a DevNest-controlled web server.",
              emptyDownloadsTitle: "No web server packages available",
              emptyDownloadsDescription:
                "Configure the runtime manifest to expose downloadable Apache, Nginx, and FrankenPHP builds in this workspace.",
            })}
          </div>

          <div
            aria-labelledby="workspace-tab-database"
            className="workspace-panel"
            hidden={activeTab !== "database"}
            id="workspace-panel-database"
            role="tabpanel"
          >
            {renderRuntimeFamilyPanel({
              family: "database",
              installedTitle: "Installed Databases",
              installedDescription:
                "Database runtimes stay separate from PHP and web server tools, so MySQL state and actions remain easy to scan.",
              runtimes: databaseRuntimes,
              packages: databasePackages,
              emptyInstalledTitle: "No database runtimes installed yet",
              emptyInstalledDescription:
                "Install and activate a managed MySQL runtime here before creating or linking local databases.",
              emptyDownloadsTitle: "No database packages available",
              emptyDownloadsDescription:
                "Configure the runtime manifest to expose downloadable database engine builds in this workspace.",
            })}
          </div>

          <div
            aria-labelledby="workspace-tab-tools"
            className="workspace-panel"
            hidden={activeTab !== "tools"}
            id="workspace-panel-tools"
            role="tabpanel"
          >
            <Card className="download-cache-card">
              <div className="page-header">
                <div>
                  <h2>Download Cache</h2>
                  <p>
                    Cached package archives are temporary; installed runtimes
                    and tools stay in their managed folders.
                  </p>
                </div>
                <div className="runtime-table-actions">
                  <Button
                    disabled={downloadCacheLoading || actionLoading !== null}
                    onClick={() => void loadDownloadCacheSummary()}
                  >
                    {downloadCacheLoading ? "Refreshing..." : "Refresh"}
                  </Button>
                  <Button
                    className="button-danger"
                    disabled={
                      downloadCacheLoading ||
                      actionLoading !== null ||
                      !downloadCacheSummary ||
                      downloadCacheSummary.fileCount === 0
                    }
                    onClick={() => void handleClearDownloadCache()}
                  >
                    {actionLoading === "download-cache-clear"
                      ? "Clearing..."
                      : "Clear Cache"}
                  </Button>
                </div>
              </div>

              <div className="download-cache-summary">
                <div className="download-cache-total">
                  <span className="runtime-table-note">Cached archives</span>
                  <strong>
                    {downloadCacheSummary
                      ? formatFileSize(downloadCacheSummary.totalSizeBytes)
                      : downloadCacheLoading
                        ? "Scanning..."
                        : "Unknown"}
                  </strong>
                  <span className="helper-text">
                    {downloadCacheSummary
                      ? `${downloadCacheSummary.fileCount} file${downloadCacheSummary.fileCount === 1 ? "" : "s"} across managed download folders.`
                      : "DevNest has not inspected the download folders yet."}
                  </span>
                </div>
                <div className="download-cache-buckets">
                  {(downloadCacheSummary?.buckets ?? []).map((bucket) => (
                    <div className="download-cache-bucket" key={bucket.id}>
                      <div>
                        <strong>{bucket.displayName}</strong>
                        <span className="mono runtime-table-note">
                          {bucket.path}
                        </span>
                      </div>
                      <span
                        className="status-chip"
                        data-tone={bucket.fileCount > 0 ? "warning" : "success"}
                      >
                        {formatFileSize(bucket.sizeBytes)} / {bucket.fileCount}{" "}
                        file
                        {bucket.fileCount === 1 ? "" : "s"}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            </Card>

            <Card>
              <div className="page-header">
                <div>
                  <h2>Installed Optional Tools</h2>
                  <p>
                    Manage additional tools that enhance your development
                    environment.
                  </p>
                </div>
              </div>

              {sortedOptionalTools.length > 0 ? (
                <div className="runtime-table-shell">
                  <table className="runtime-table">
                    <thead>
                      <tr>
                        <th>Tool</th>
                        <th>Version</th>
                        <th>Status</th>
                        <th>Update</th>
                        <th>Updated</th>
                        <th>Actions</th>
                      </tr>
                    </thead>
                    <tbody>
                      {sortedOptionalTools.map((tool) => {
                        const updatePackage =
                          optionalToolUpdatePackages.get(tool.id) ?? null;

                        return (
                          <tr key={tool.id}>
                            <td>
                              <div className="runtime-table-type">
                                <strong>
                                  {optionalToolLabel(tool.toolType)}
                                </strong>
                                <span className="runtime-table-note">
                                  {optionalToolFamilyLabel(tool.toolType)}
                                </span>
                              </div>
                            </td>
                            <td>{displayCatalogVersion(tool.version)}</td>
                            <td>
                              <div className="runtime-status-copy">
                                <span
                                  className="status-chip"
                                  data-tone={
                                    tool.status === "missing"
                                      ? "error"
                                      : tool.isActive
                                        ? "success"
                                        : "warning"
                                  }
                                >
                                  {optionalToolHealthLabel(tool)}
                                </span>
                                {tool.details ? (
                                  <span className="helper-text">
                                    {tool.details}
                                  </span>
                                ) : null}
                              </div>
                            </td>
                            <td>
                              {updatePackage ? (
                                <div className="runtime-status-copy">
                                  <span
                                    className="status-chip"
                                    data-tone="warning"
                                  >
                                    Update
                                  </span>
                                  <span className="runtime-table-note">
                                    {updatePackage.displayName} is available.
                                  </span>
                                </div>
                              ) : (
                                <div className="runtime-status-copy">
                                  <span
                                    className="status-chip"
                                    data-tone="success"
                                  >
                                    Current
                                  </span>
                                  <span className="runtime-table-note">
                                    Installed package is current.
                                  </span>
                                </div>
                              )}
                            </td>
                            <td>{formatUpdatedAt(tool.updatedAt)}</td>
                            <td>
                              <div className="runtime-table-actions">
                                <Button
                                  disabled={actionLoading !== null}
                                  onClick={() =>
                                    void handleRevealOptionalTool(tool)
                                  }
                                >
                                  {actionLoading ===
                                  `optional-reveal:${tool.id}`
                                    ? "Opening..."
                                    : "Open"}
                                </Button>
                                {tool.toolType === "redis" &&
                                tool.status === "available" ? (
                                  <Button
                                    disabled={actionLoading !== null}
                                    onClick={() =>
                                      void handleOpenRedisManager()
                                    }
                                    variant="primary"
                                  >
                                    Open Manager
                                  </Button>
                                ) : null}
                                {updatePackage ? (
                                  <Button
                                    disabled={
                                      packagesLoading || actionLoading !== null
                                    }
                                    onClick={() =>
                                      void handleInstallOptionalToolPackage(
                                        updatePackage,
                                      )
                                    }
                                    variant="primary"
                                  >
                                    {actionLoading ===
                                    `optional-install:${updatePackage.id}`
                                      ? "Updating..."
                                      : "Update"}
                                  </Button>
                                ) : null}
                                <Button
                                  className="button-danger"
                                  disabled={actionLoading !== null}
                                  onClick={() =>
                                    setPendingOptionalToolRemoval(tool)
                                  }
                                >
                                  Uninstall
                                </Button>
                              </div>
                            </td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>
              ) : (
                <EmptyState
                  title="No optional tools installed yet"
                  description="Install Mailpit, phpMyAdmin, or cloudflared from the compact catalog below when your workspace needs them."
                />
              )}
            </Card>

            <Card>
              <div className="page-header">
                <div>
                  <h2>Available Optional Tool Downloads</h2>
                  <p>
                    Download and install additional tools to enhance your
                    development environment.
                  </p>
                </div>
              </div>

              {sortedOptionalToolPackages.length > 0 ? (
                <div className="runtime-table-shell">
                  <table className="runtime-table">
                    <thead>
                      <tr>
                        <th>Tool</th>
                        <th>Package</th>
                        <th>Version</th>
                        <th>Platform</th>
                        <th>Status</th>
                        <th>Actions</th>
                      </tr>
                    </thead>
                    <tbody>
                      {sortedOptionalToolPackages.map((toolPackage) => {
                        const installedTool =
                          installedOptionalToolIds.get(
                            `${toolPackage.toolType}:${normalizeCatalogVersion(toolPackage.version)}`,
                          ) ?? null;
                        const currentTask =
                          optionalToolInstallTask?.packageId === toolPackage.id
                            ? optionalToolInstallTask
                            : null;

                        return (
                          <tr key={toolPackage.id}>
                            <td>
                              <div className="runtime-table-type">
                                <strong>
                                  {optionalToolLabel(toolPackage.toolType)}
                                </strong>
                                <span className="runtime-table-note">
                                  {optionalToolFamilyLabel(
                                    toolPackage.toolType,
                                  )}
                                </span>
                              </div>
                            </td>
                            <td>
                              <div className="runtime-table-type">
                                <strong>{toolPackage.displayName}</strong>
                                <span className="mono">
                                  {toolPackage.entryBinary}
                                </span>
                              </div>
                            </td>
                            <td>{toolPackage.version}</td>
                            <td>
                              {toolPackage.platform} {toolPackage.arch}
                            </td>
                            <td>
                              <div className="runtime-status-copy">
                                <span
                                  className="status-chip"
                                  data-tone={
                                    currentTask?.stage === "failed"
                                      ? "error"
                                      : currentTask?.stage === "completed" ||
                                          installedTool
                                        ? "success"
                                        : "warning"
                                  }
                                >
                                  {currentTask
                                    ? optionalToolInstallStageLabel(
                                        currentTask.stage,
                                      )
                                    : installedTool
                                      ? "Completed"
                                      : "Ready"}
                                </span>
                                <span className="runtime-table-note">
                                  {currentTask?.message ??
                                    toolPackage.notes ??
                                    "Managed optional tool download."}
                                </span>
                              </div>
                            </td>
                            <td>
                              <div className="runtime-table-actions">
                                <Button
                                  disabled={
                                    packagesLoading || actionLoading !== null
                                  }
                                  onClick={() =>
                                    void handleInstallOptionalToolPackage(
                                      toolPackage,
                                    )
                                  }
                                  variant="primary"
                                >
                                  {actionLoading ===
                                  `optional-install:${toolPackage.id}`
                                    ? "Installing..."
                                    : installedTool
                                      ? "Reinstall"
                                      : "Install"}
                                </Button>
                                {installedTool ? (
                                  <>
                                    {toolPackage.toolType === "redis" &&
                                    installedTool.status === "available" ? (
                                      <Button
                                        disabled={actionLoading !== null}
                                        onClick={() =>
                                          void handleOpenRedisManager()
                                        }
                                        variant="primary"
                                      >
                                        Open Manager
                                      </Button>
                                    ) : null}
                                    <Button
                                      className="button-danger"
                                      disabled={actionLoading !== null}
                                      onClick={() =>
                                        setPendingOptionalToolRemoval(
                                          installedTool,
                                        )
                                      }
                                    >
                                      Uninstall
                                    </Button>
                                  </>
                                ) : null}
                              </div>
                            </td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>
              ) : (
                <EmptyState
                  title="No optional tool packages available"
                  description="Configure the optional tool manifest to expose Mailpit, phpMyAdmin, and cloudflared downloads in this table."
                />
              )}
            </Card>
          </div>
        </div>
        {showSettingsScrim ? (
          <LoadingScrim
            message="Preparing runtimes, package catalogs, and optional tool inventory."
            title="Loading Settings"
          />
        ) : null}
        {showPersistentTunnelScrim ? (
          <LoadingScrim
            message="Checking Cloudflare auth, managed credentials, and named tunnel inventory."
            title="Preparing Persistent Tunnel"
          />
        ) : null}
      </div>

      {redisManagerOpen ? (
        <div
          className="wizard-overlay"
          onClick={() => setRedisManagerOpen(false)}
          role="dialog"
          aria-modal="true"
        >
          <div
            className="redis-manager-dialog"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="runtime-tools-header">
              <div>
                <h2>Redis Manager</h2>
                <p>
                  Browse local Redis keys and edit string values from the
                  installed Redis optional tool.
                </p>
              </div>
              <Button onClick={() => setRedisManagerOpen(false)}>Close</Button>
            </div>
            <RedisManager service={redisService} />
          </div>
        </div>
      ) : null}

      {phpToolsRuntimeId && selectedPhpRuntime ? (
        <div
          className="wizard-overlay"
          onClick={() => setPhpToolsRuntimeId(null)}
          role="dialog"
          aria-modal="true"
        >
          <div
            className="runtime-tools-dialog"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="runtime-tools-header">
              <div>
                <h2>PHP Tools</h2>
                <p>
                  Manage extensions and guarded functions for one tracked PHP or
                  FrankenPHP runtime without leaving the Installed Runtimes
                  table.
                </p>
              </div>
              <div className="page-toolbar">
                <select
                  className="select"
                  onChange={(event) => {
                    setSelectedPhpRuntimeId(event.target.value);
                    setPhpToolsRuntimeId(event.target.value);
                  }}
                  value={activePhpToolsRuntimeId}
                >
                  {phpRuntimes.map((runtime) => (
                    <option key={runtime.id} value={runtime.id}>
                      {runtime.runtimeType === "frankenphp"
                        ? `FrankenPHP ${runtime.version} (PHP ${runtime.phpFamily ?? "unknown"})${runtime.isActive ? " (active)" : ""}`
                        : `PHP ${runtime.version}${runtime.isActive ? " (active)" : ""}`}
                    </option>
                  ))}
                </select>
              </div>
            </div>

            <StickyTabs
              activeTab={phpToolsTab}
              ariaLabel="PHP runtime tools"
              items={phpToolsTabs}
              namespace="php-tools"
              onSelect={(tab) => setPhpToolsTab(tab as PhpToolsTab)}
            />

            <div className="runtime-tools-toolbar">
              <input
                className="input runtime-tools-search"
                onChange={(event) => setPhpToolsSearch(event.target.value)}
                placeholder={
                  phpToolsTab === "extensions"
                    ? "Search extension names, DLLs, and install options"
                    : "Search restricted PHP functions"
                }
                type="search"
                value={phpToolsSearch}
              />
              <div className="runtime-table-actions">
                {phpToolsTab === "extensions" ? (
                  <>
                    <Button
                      disabled={actionLoading !== null}
                      onClick={() => void handleInstallPhpExtension()}
                      variant="primary"
                    >
                      {actionLoading ===
                      `php-extension-install:${activePhpToolsRuntimeId}`
                        ? "Importing..."
                        : "Import Local"}
                    </Button>
                    <Button
                      disabled={
                        phpExtensionsLoading ||
                        phpExtensionPackagesLoading ||
                        actionLoading !== null
                      }
                      onClick={() => {
                        void loadPhpExtensions(activePhpToolsRuntimeId);
                        void loadPhpExtensionPackages(activePhpToolsRuntimeId);
                      }}
                    >
                      {phpExtensionsLoading || phpExtensionPackagesLoading
                        ? "Refreshing..."
                        : "Refresh"}
                    </Button>
                  </>
                ) : (
                  <Button
                    disabled={phpFunctionsLoading || actionLoading !== null}
                    onClick={() =>
                      void loadPhpFunctions(activePhpToolsRuntimeId)
                    }
                  >
                    {phpFunctionsLoading ? "Refreshing..." : "Refresh"}
                  </Button>
                )}
              </div>
            </div>

            {phpToolsTab === "extensions" ? (
              <div
                aria-labelledby="php-tools-tab-extensions"
                className="workspace-panel runtime-tools-panel"
                id="php-tools-panel-extensions"
                role="tabpanel"
              >
                {phpExtensionsError ? (
                  <span className="error-text">{phpExtensionsError}</span>
                ) : null}
                {phpExtensionPackagesError ? (
                  <span className="error-text">
                    {phpExtensionPackagesError}
                  </span>
                ) : null}
                <section className="runtime-tools-overview">
                  <article className="runtime-tools-stat">
                    <span className="runtime-tools-stat-label">
                      Enabled Now
                    </span>
                    <strong>{enabledPhpExtensions.length}</strong>
                    <p>
                      Extensions currently active in DevNest-managed `php.ini`
                      for this runtime.
                    </p>
                  </article>
                  <article className="runtime-tools-stat">
                    <span className="runtime-tools-stat-label">
                      Available to Review
                    </span>
                    <strong>{disabledPhpExtensions.length}</strong>
                    <p>
                      Tracked DLLs already in this runtime but still disabled or
                      intentionally held back.
                    </p>
                  </article>
                  <article className="runtime-tools-stat">
                    <span className="runtime-tools-stat-label">
                      Install More
                    </span>
                    <strong>
                      {installablePhpExtensionRecommendations.length}
                    </strong>
                    <p>
                      Curated packages DevNest can install for PHP{" "}
                      {selectedPhpRuntime.phpFamily ??
                        runtimeVersionFamily(selectedPhpRuntime.version)}
                      .
                    </p>
                  </article>
                </section>
                <section className="runtime-tools-section">
                  <div className="runtime-tools-section-head">
                    <div className="runtime-tools-section-copy">
                      <h3>Enabled Now</h3>
                      <span className="helper-text">
                        Keep the currently active capabilities visible first so
                        this screen answers "what is loaded right now?" before
                        anything else.
                      </span>
                    </div>
                  </div>
                  <div className="runtime-table-shell">
                    <table className="runtime-table">
                      <thead>
                        <tr>
                          <th>Extension</th>
                          <th>Source</th>
                          <th>Why It Matters</th>
                          <th>Actions</th>
                        </tr>
                      </thead>
                      <tbody>
                        {enabledPhpExtensions.length > 0 ? (
                          enabledPhpExtensions.map((extension) => {
                            const recommendedSpec =
                              RECOMMENDED_PHP_EXTENSION_BY_NAME.get(
                                extension.extensionName,
                              ) ?? null;
                            const extensionPackage =
                              phpExtensionPackagesByName.get(
                                extension.extensionName,
                              ) ?? null;

                            return (
                              <tr
                                key={`${extension.runtimeId}:${extension.extensionName}`}
                              >
                                <td>
                                  <div className="runtime-table-type">
                                    <strong>
                                      {phpExtensionLabel(
                                        extension.extensionName,
                                      )}
                                    </strong>
                                    <span className="runtime-table-note">
                                      {extension.extensionName}
                                    </span>
                                  </div>
                                </td>
                                <td>
                                  <div className="runtime-tools-pill-row">
                                    <span className="runtime-tools-pill">
                                      {phpExtensionAvailabilityLabel(
                                        recommendedSpec,
                                        extensionPackage,
                                      )}
                                    </span>
                                    <span className="runtime-tools-pill runtime-tools-pill-success">
                                      Enabled
                                    </span>
                                  </div>
                                </td>
                                <td>
                                  <div className="runtime-table-type">
                                    <strong>
                                      {recommendedSpec?.summary ??
                                        "Tracked extension already active."}
                                    </strong>
                                    <span className="runtime-table-note">
                                      {phpExtensionAvailabilityNote(
                                        extension.extensionName,
                                        recommendedSpec,
                                        extensionPackage,
                                      )}
                                    </span>
                                  </div>
                                </td>
                                <td>
                                  <div className="runtime-table-actions">
                                    <Button
                                      disabled={
                                        phpExtensionsLoading ||
                                        actionLoading !== null
                                      }
                                      onClick={() =>
                                        void handleTogglePhpExtension(extension)
                                      }
                                    >
                                      {actionLoading ===
                                      `php-extension:${extension.extensionName}`
                                        ? "Saving..."
                                        : "Disable"}
                                    </Button>
                                    <Button
                                      className="button-danger"
                                      disabled={
                                        phpExtensionsLoading ||
                                        actionLoading !== null
                                      }
                                      onClick={() =>
                                        setPendingPhpExtensionRemoval(extension)
                                      }
                                    >
                                      Uninstall
                                    </Button>
                                  </div>
                                </td>
                              </tr>
                            );
                          })
                        ) : (
                          <tr>
                            <td colSpan={4}>
                              <span className="helper-text">
                                {phpExtensions.length > 0
                                  ? "No enabled extensions matched this search."
                                  : "This runtime does not expose any tracked extension DLLs yet."}
                              </span>
                            </td>
                          </tr>
                        )}
                      </tbody>
                    </table>
                  </div>
                </section>
                <section className="runtime-tools-section">
                  <div className="runtime-tools-section-head">
                    <div className="runtime-tools-section-copy">
                      <h3>Available in This Runtime</h3>
                      <span className="helper-text">
                        These DLLs are already present in the runtime. Turn them
                        on here or leave them off if the runtime policy should
                        stay tighter.
                      </span>
                    </div>
                  </div>
                  <div className="runtime-table-shell">
                    <table className="runtime-table">
                      <thead>
                        <tr>
                          <th>Extension</th>
                          <th>DLL / Source</th>
                          <th>State</th>
                          <th>Context</th>
                          <th>Actions</th>
                        </tr>
                      </thead>
                      <tbody>
                        {disabledPhpExtensions.length > 0 ? (
                          disabledPhpExtensions.map((extension) => {
                            const recommendedSpec =
                              RECOMMENDED_PHP_EXTENSION_BY_NAME.get(
                                extension.extensionName,
                              ) ?? null;
                            const extensionPackage =
                              phpExtensionPackagesByName.get(
                                extension.extensionName,
                              ) ?? null;
                            const disabledByDefault =
                              isPhpExtensionDisabledByDefault(
                                extension.extensionName,
                              );

                            return (
                              <tr
                                key={`${extension.runtimeId}:${extension.extensionName}`}
                              >
                                <td>
                                  <div className="runtime-table-type">
                                    <strong>
                                      {phpExtensionLabel(
                                        extension.extensionName,
                                      )}
                                    </strong>
                                    <span className="runtime-table-note">
                                      {recommendedSpec?.summary ??
                                        extension.extensionName}
                                    </span>
                                  </div>
                                </td>
                                <td>
                                  <div className="runtime-table-type">
                                    <strong className="mono">
                                      {extension.dllFile}
                                    </strong>
                                    <span className="runtime-table-note">
                                      {phpExtensionAvailabilityLabel(
                                        recommendedSpec,
                                        extensionPackage,
                                      )}
                                    </span>
                                  </div>
                                </td>
                                <td>
                                  <div className="runtime-tools-pill-row">
                                    <span className="runtime-tools-pill runtime-tools-pill-warning">
                                      Disabled
                                    </span>
                                    {disabledByDefault ? (
                                      <span className="runtime-tools-pill runtime-tools-pill-muted">
                                        Disabled by default
                                      </span>
                                    ) : null}
                                  </div>
                                </td>
                                <td>
                                  <div className="runtime-table-type">
                                    <strong>
                                      {formatUpdatedAt(extension.updatedAt)}
                                    </strong>
                                    <span className="runtime-table-note">
                                      {phpExtensionAvailabilityNote(
                                        extension.extensionName,
                                        recommendedSpec,
                                        extensionPackage,
                                      )}
                                    </span>
                                  </div>
                                </td>
                                <td>
                                  <div className="runtime-table-actions">
                                    <Button
                                      disabled={
                                        phpExtensionsLoading ||
                                        actionLoading !== null
                                      }
                                      onClick={() =>
                                        void handleTogglePhpExtension(extension)
                                      }
                                    >
                                      {actionLoading ===
                                      `php-extension:${extension.extensionName}`
                                        ? "Saving..."
                                        : "Enable"}
                                    </Button>
                                    <Button
                                      className="button-danger"
                                      disabled={
                                        phpExtensionsLoading ||
                                        actionLoading !== null
                                      }
                                      onClick={() =>
                                        setPendingPhpExtensionRemoval(extension)
                                      }
                                    >
                                      Uninstall
                                    </Button>
                                  </div>
                                </td>
                              </tr>
                            );
                          })
                        ) : (
                          <tr>
                            <td colSpan={5}>
                              <span className="helper-text">
                                {phpExtensions.length > 0
                                  ? "No disabled runtime DLLs matched this search."
                                  : "This runtime does not expose any tracked extension DLLs yet. Use Import Local or Install More to add one."}
                              </span>
                            </td>
                          </tr>
                        )}
                      </tbody>
                    </table>
                  </div>
                </section>
                <section className="runtime-tools-section">
                  <div className="runtime-tools-section-head">
                    <div className="runtime-tools-section-copy">
                      <h3>Install More</h3>
                      <span className="helper-text">
                        Curated packages and expected bundled DLLs for the{" "}
                        {selectedPhpRuntime.phpFamily ??
                          runtimeVersionFamily(selectedPhpRuntime.version)}{" "}
                        family.
                      </span>
                    </div>
                  </div>
                  <div className="runtime-table-shell">
                    <table className="runtime-table">
                      <thead>
                        <tr>
                          <th>Extension</th>
                          <th>Availability</th>
                          <th>Package / Guidance</th>
                          <th>Actions</th>
                        </tr>
                      </thead>
                      <tbody>
                        {installablePhpExtensionRecommendations.length > 0 ||
                        missingBundledPhpExtensionRecommendations.length > 0 ? (
                          <>
                            {installablePhpExtensionRecommendations.map(
                              ({ spec, extensionPackage }) =>
                                extensionPackage ? (
                                  <tr
                                    key={`php-extension-package:${extensionPackage.id}`}
                                  >
                                    <td>
                                      <div className="runtime-table-type">
                                        <strong>
                                          {phpExtensionLabel(
                                            spec.extensionName,
                                          )}
                                        </strong>
                                        <span className="runtime-table-note">
                                          {spec.summary}
                                        </span>
                                      </div>
                                    </td>
                                    <td>
                                      <div className="runtime-tools-pill-row">
                                        <span className="runtime-tools-pill runtime-tools-pill-success">
                                          Ready to install
                                        </span>
                                        <span className="runtime-tools-pill">
                                          {extensionPackage.packageKind ===
                                          "zip"
                                            ? "ZIP package"
                                            : "Binary package"}
                                        </span>
                                      </div>
                                    </td>
                                    <td>
                                      <div className="runtime-table-type">
                                        <strong>
                                          {extensionPackage.displayName}
                                        </strong>
                                        <span className="runtime-table-note">
                                          {extensionPackage.notes ??
                                            "DevNest can download this package into the runtime ext directory."}
                                        </span>
                                      </div>
                                    </td>
                                    <td>
                                      <div className="runtime-table-actions">
                                        <Button
                                          disabled={
                                            phpExtensionPackagesLoading ||
                                            actionLoading !== null
                                          }
                                          onClick={() =>
                                            void handleInstallPhpExtensionPackage(
                                              extensionPackage,
                                            )
                                          }
                                          variant="primary"
                                        >
                                          {actionLoading ===
                                          `php-extension-package:${extensionPackage.id}`
                                            ? "Installing..."
                                            : "Install"}
                                        </Button>
                                      </div>
                                    </td>
                                  </tr>
                                ) : null,
                            )}
                            {missingBundledPhpExtensionRecommendations.map(
                              ({ spec }) => (
                                <tr
                                  key={`recommended-missing:${spec.extensionName}`}
                                >
                                  <td>
                                    <div className="runtime-table-type">
                                      <strong>
                                        {phpExtensionLabel(spec.extensionName)}
                                      </strong>
                                      <span className="runtime-table-note">
                                        {spec.summary}
                                      </span>
                                    </div>
                                  </td>
                                  <td>
                                    <div className="runtime-tools-pill-row">
                                      <span className="runtime-tools-pill runtime-tools-pill-warning">
                                        Not in runtime
                                      </span>
                                      <span className="runtime-tools-pill">
                                        Bundled DLL
                                      </span>
                                    </div>
                                  </td>
                                  <td>
                                    <div className="runtime-table-type">
                                      <strong>
                                        Bring in a compatible local DLL
                                      </strong>
                                      <span className="runtime-table-note">
                                        DevNest expects this extension to come
                                        from the PHP runtime bundle. If your
                                        build does not ship it, use Import
                                        Local.
                                      </span>
                                    </div>
                                  </td>
                                  <td>
                                    <div className="runtime-table-actions">
                                      <span className="helper-text">
                                        Use Import Local
                                      </span>
                                    </div>
                                  </td>
                                </tr>
                              ),
                            )}
                          </>
                        ) : (
                          <tr>
                            <td colSpan={4}>
                              <span className="helper-text">
                                No install candidates matched this search. Try
                                another keyword or use Import Local for a custom
                                DLL.
                              </span>
                            </td>
                          </tr>
                        )}
                      </tbody>
                    </table>
                  </div>
                </section>
                <span className="helper-text runtime-tools-note">
                  Extension changes land in DevNest-managed `php.ini`. Restart
                  the linked Apache, Nginx, FrankenPHP, or long-running PHP
                  worker after enabling, disabling, or installing one.
                </span>
              </div>
            ) : (
              <div
                aria-labelledby="php-tools-tab-policy"
                className="workspace-panel runtime-tools-panel"
                id="php-tools-panel-policy"
                role="tabpanel"
              >
                {phpFunctionsError ? (
                  <span className="error-text">{phpFunctionsError}</span>
                ) : null}
                <section className="runtime-tools-overview">
                  <article className="runtime-tools-stat">
                    <span className="runtime-tools-stat-label">Allowed</span>
                    <strong>{enabledPhpFunctions.length}</strong>
                    <p>
                      Functions currently allowed to execute in this runtime.
                    </p>
                  </article>
                  <article className="runtime-tools-stat">
                    <span className="runtime-tools-stat-label">Restricted</span>
                    <strong>{disabledPhpFunctions.length}</strong>
                    <p>
                      Functions written into `disable_functions` for safer or
                      cleaner project defaults.
                    </p>
                  </article>
                  <article className="runtime-tools-stat">
                    <span className="runtime-tools-stat-label">Scope</span>
                    <strong>Per Runtime</strong>
                    <p>
                      These guards apply to the selected PHP runtime, not to a
                      single project only.
                    </p>
                  </article>
                </section>
                <section className="runtime-tools-section">
                  <div className="runtime-tools-section-head">
                    <div className="runtime-tools-section-copy">
                      <h3>Restricted Functions</h3>
                      <span className="helper-text">
                        This is runtime policy, not extension inventory. Use it
                        to control what PHP functions stay callable in
                        DevNest-managed environments.
                      </span>
                    </div>
                  </div>
                  <div className="runtime-table-shell">
                    <table className="runtime-table">
                      <thead>
                        <tr>
                          <th>Function</th>
                          <th>Mode</th>
                          <th>Context</th>
                          <th>Actions</th>
                        </tr>
                      </thead>
                      <tbody>
                        {filteredPhpFunctions.length > 0 ? (
                          filteredPhpFunctions.map((functionState) => (
                            <tr
                              key={`${functionState.runtimeId}:${functionState.functionName}`}
                            >
                              <td>
                                <div className="runtime-table-type">
                                  <strong>
                                    {phpExtensionLabel(
                                      functionState.functionName,
                                    )}
                                  </strong>
                                  <span className="runtime-table-note">
                                    {functionState.functionName}
                                  </span>
                                </div>
                              </td>
                              <td>
                                <div className="runtime-tools-pill-row">
                                  <span
                                    className={`runtime-tools-pill ${
                                      functionState.enabled
                                        ? "runtime-tools-pill-success"
                                        : "runtime-tools-pill-warning"
                                    }`}
                                  >
                                    {functionState.enabled
                                      ? "Allowed"
                                      : "Restricted"}
                                  </span>
                                </div>
                              </td>
                              <td>
                                <div className="runtime-table-type">
                                  <strong>
                                    {formatUpdatedAt(functionState.updatedAt)}
                                  </strong>
                                  <span className="runtime-table-note">
                                    {functionState.enabled
                                      ? "Available to project code running on this runtime."
                                      : "Written into DevNest-managed `disable_functions`."}
                                  </span>
                                </div>
                              </td>
                              <td>
                                <div className="runtime-table-actions">
                                  <Button
                                    disabled={
                                      phpFunctionsLoading ||
                                      actionLoading !== null
                                    }
                                    onClick={() =>
                                      void handleTogglePhpFunction(
                                        functionState,
                                      )
                                    }
                                  >
                                    {actionLoading ===
                                    `php-function:${functionState.functionName}`
                                      ? "Saving..."
                                      : functionState.enabled
                                        ? "Restrict"
                                        : "Allow"}
                                  </Button>
                                </div>
                              </td>
                            </tr>
                          ))
                        ) : (
                          <tr>
                            <td colSpan={4}>
                              <span className="helper-text">
                                {phpFunctions.length > 0
                                  ? "No runtime policy rows matched this search."
                                  : "DevNest did not load the managed `disable_functions` list for this runtime yet."}
                              </span>
                            </td>
                          </tr>
                        )}
                      </tbody>
                    </table>
                  </div>
                </section>
                <span className="helper-text runtime-tools-note">
                  Runtime policy changes write into `disable_functions`. Restart
                  the linked Apache, Nginx, FrankenPHP, or long-running PHP
                  worker after changing these guards.
                </span>
              </div>
            )}

            {pendingPhpExtensionRemoval ? (
              <div
                data-nested-modal="true"
                className="wizard-overlay"
                onClick={() => {
                  if (
                    actionLoading !==
                    `php-extension-remove:${pendingPhpExtensionRemoval.extensionName}`
                  ) {
                    setPendingPhpExtensionRemoval(null);
                  }
                }}
                role="dialog"
                aria-modal="true"
              >
                <div
                  className="confirm-dialog"
                  onClick={(event) => event.stopPropagation()}
                >
                  <div className="confirm-dialog-copy">
                    <h3>Uninstall PHP extension?</h3>
                    <p>
                      This will remove{" "}
                      <strong>
                        {phpExtensionLabel(
                          pendingPhpExtensionRemoval.extensionName,
                        )}
                      </strong>{" "}
                      from {pendingPhpExtensionRemoval.runtimeVersion}.
                    </p>
                    <div className="detail-item">
                      <span className="detail-label">DLL</span>
                      <strong className="mono detail-value">
                        {pendingPhpExtensionRemoval.dllFile}
                      </strong>
                    </div>
                    <span className="helper-text">
                      DevNest removes the managed DLL from this runtime's `ext`
                      directory and clears its saved override. Restart the
                      linked web server after uninstalling.
                    </span>
                  </div>
                  <div className="confirm-dialog-actions">
                    <Button
                      disabled={
                        actionLoading ===
                        `php-extension-remove:${pendingPhpExtensionRemoval.extensionName}`
                      }
                      onClick={() => setPendingPhpExtensionRemoval(null)}
                    >
                      Cancel
                    </Button>
                    <Button
                      className="button-danger"
                      disabled={
                        actionLoading ===
                        `php-extension-remove:${pendingPhpExtensionRemoval.extensionName}`
                      }
                      onClick={() =>
                        void handleRemovePhpExtension(
                          pendingPhpExtensionRemoval,
                        )
                      }
                    >
                      {actionLoading ===
                      `php-extension-remove:${pendingPhpExtensionRemoval.extensionName}`
                        ? "Removing..."
                        : "Uninstall"}
                    </Button>
                  </div>
                </div>
              </div>
            ) : null}
          </div>
        </div>
      ) : null}

      {runtimeConfigRuntimeId && selectedRuntimeConfigRuntime ? (
        <RuntimeConfigDialog
          error={runtimeConfigError}
          loading={runtimeConfigLoading}
          onClose={() => {
            if (runtimeConfigSaving || runtimeConfigOpenFileLoading) {
              return;
            }

            setRuntimeConfigRuntimeId(null);
            setRuntimeConfigError(undefined);
          }}
          onOpenFile={() =>
            handleOpenRuntimeConfigFile(selectedRuntimeConfigRuntime)
          }
          onSave={handleSaveRuntimeConfig}
          openFileLoading={
            runtimeConfigOpenFileLoading &&
            runtimeConfigOpenFileRuntimeId === selectedRuntimeConfigRuntime.id
          }
          runtime={selectedRuntimeConfigRuntime}
          saving={runtimeConfigSaving}
          schema={runtimeConfigSchema}
          values={runtimeConfigValues}
        />
      ) : null}

      {pendingRuntimeRemoval ? (
        <div
          className="wizard-overlay"
          onClick={() => {
            if (actionLoading !== `remove:${pendingRuntimeRemoval.id}`) {
              setPendingRuntimeRemoval(null);
            }
          }}
          role="dialog"
          aria-modal="true"
        >
          <div
            className="confirm-dialog"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="confirm-dialog-copy">
              <h3>{removalDialogTitle}</h3>
              <p>
                This will remove{" "}
                <strong>
                  {runtimeTypeLabel(pendingRuntimeRemoval.runtimeType)}{" "}
                  {pendingRuntimeRemoval.version}
                </strong>{" "}
                from the DevNest runtime registry.
              </p>
              <div className="detail-item">
                <span className="detail-label">Source</span>
                <strong>
                  {runtimeSourceLabel(pendingRuntimeRemoval.source)}
                </strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">Path</span>
                <strong className="mono detail-value">
                  {pendingRuntimeRemoval.path}
                </strong>
              </div>
              <span className="helper-text">
                Imported runtimes inside the managed DevNest folder will also
                have their copied runtime files removed. External runtimes on
                your machine are not deleted.
              </span>
            </div>
            <div className="confirm-dialog-actions">
              <Button
                disabled={
                  actionLoading === `remove:${pendingRuntimeRemoval.id}`
                }
                onClick={() => setPendingRuntimeRemoval(null)}
              >
                Cancel
              </Button>
              <Button
                className="button-danger"
                disabled={
                  actionLoading === `remove:${pendingRuntimeRemoval.id}`
                }
                onClick={() => void handleRemoveRuntime(pendingRuntimeRemoval)}
              >
                {actionLoading === `remove:${pendingRuntimeRemoval.id}`
                  ? "Removing..."
                  : removalDialogAction}
              </Button>
            </div>
          </div>
        </div>
      ) : null}

      {pendingOptionalToolRemoval ? (
        <div
          className="wizard-overlay"
          onClick={() => {
            if (
              actionLoading !==
              `optional-remove:${pendingOptionalToolRemoval.id}`
            ) {
              setPendingOptionalToolRemoval(null);
            }
          }}
          role="dialog"
          aria-modal="true"
        >
          <div
            className="confirm-dialog"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="confirm-dialog-copy">
              <h3>Uninstall optional tool?</h3>
              <p>
                This will remove{" "}
                <strong>
                  {optionalToolLabel(pendingOptionalToolRemoval.toolType)}{" "}
                  {displayCatalogVersion(pendingOptionalToolRemoval.version)}
                </strong>{" "}
                from the DevNest optional tools inventory.
              </p>
              <div className="detail-item">
                <span className="detail-label">Path</span>
                <strong className="mono detail-value">
                  {pendingOptionalToolRemoval.path}
                </strong>
              </div>
              <span className="helper-text">
                DevNest only removes the managed install it owns. If the tool is
                currently in use, uninstall stays blocked until you stop the
                related service or tunnel first.
              </span>
            </div>
            <div className="confirm-dialog-actions">
              <Button
                disabled={
                  actionLoading ===
                  `optional-remove:${pendingOptionalToolRemoval.id}`
                }
                onClick={() => setPendingOptionalToolRemoval(null)}
              >
                Cancel
              </Button>
              <Button
                className="button-danger"
                disabled={
                  actionLoading ===
                  `optional-remove:${pendingOptionalToolRemoval.id}`
                }
                onClick={() =>
                  void handleRemoveOptionalTool(pendingOptionalToolRemoval)
                }
              >
                {actionLoading ===
                `optional-remove:${pendingOptionalToolRemoval.id}`
                  ? "Removing..."
                  : "Uninstall"}
              </Button>
            </div>
          </div>
        </div>
      ) : null}

      {pendingPersistentTunnelDeletion ? (
        <div
          className="wizard-overlay"
          onClick={() => {
            if (
              actionLoading !==
              `persistent-delete:${pendingPersistentTunnelDeletion.tunnelId}`
            ) {
              setPendingPersistentTunnelDeletion(null);
            }
          }}
          role="dialog"
          aria-modal="true"
        >
          <div
            className="confirm-dialog"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="confirm-dialog-copy">
              <h3>Delete Named Tunnel?</h3>
              <p>
                DevNest will delete{" "}
                <strong>{pendingPersistentTunnelDeletion.tunnelName}</strong>{" "}
                from Cloudflare and remove its managed credentials from this
                app.
              </p>
              <div className="detail-item">
                <span className="detail-label">Tunnel ID</span>
                <strong className="mono detail-value">
                  {pendingPersistentTunnelDeletion.tunnelId}
                </strong>
              </div>
              <span className="helper-text">
                If projects are still using the selected shared tunnel, stop
                them or delete their hostname first.
              </span>
            </div>
            <div className="confirm-dialog-actions">
              <Button
                disabled={
                  actionLoading ===
                  `persistent-delete:${pendingPersistentTunnelDeletion.tunnelId}`
                }
                onClick={() => setPendingPersistentTunnelDeletion(null)}
              >
                Cancel
              </Button>
              <Button
                className="button-danger"
                disabled={
                  actionLoading ===
                  `persistent-delete:${pendingPersistentTunnelDeletion.tunnelId}`
                }
                onClick={() =>
                  void handleDeletePersistentNamedTunnel(
                    pendingPersistentTunnelDeletion,
                  )
                }
              >
                {actionLoading ===
                `persistent-delete:${pendingPersistentTunnelDeletion.tunnelId}`
                  ? "Deleting..."
                  : "Delete Tunnel"}
              </Button>
            </div>
          </div>
        </div>
      ) : null}

      {disconnectPersistentTunnelConfirm ? (
        <div
          className="wizard-overlay"
          onClick={() => {
            if (actionLoading !== "persistent-disconnect") {
              setDisconnectPersistentTunnelConfirm(false);
            }
          }}
          role="dialog"
          aria-modal="true"
        >
          <div
            className="confirm-dialog"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="confirm-dialog-copy">
              <h3>Disconnect Cloudflare Setup?</h3>
              <p>
                DevNest will remove the managed auth cert, named tunnel
                credentials, and selected tunnel identity from this app.
              </p>
              <span className="helper-text">
                This does not delete projects or managed tunnel credentials. It
                only disconnects Cloudflare auth until you connect again.
              </span>
            </div>
            <div className="confirm-dialog-actions">
              <Button
                disabled={actionLoading === "persistent-disconnect"}
                onClick={() => setDisconnectPersistentTunnelConfirm(false)}
              >
                Cancel
              </Button>
              <Button
                className="button-danger"
                disabled={actionLoading === "persistent-disconnect"}
                onClick={() => void handleDisconnectPersistentTunnelProvider()}
              >
                {actionLoading === "persistent-disconnect"
                  ? "Disconnecting..."
                  : "Disconnect"}
              </Button>
            </div>
          </div>
        </div>
      ) : null}
    </PageLayout>
  );
}
