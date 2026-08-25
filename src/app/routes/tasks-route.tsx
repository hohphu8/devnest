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

export default function TasksRoute() {
  const tasks = useProjectScheduledTaskStore((state) => state.tasks);
  const enabledTasks = tasks.filter((task) => task.enabled).length;
  const runningTasks = tasks.filter((task) => task.status === "running").length;
  const taskErrors = tasks.filter((task) => task.status === "error").length;

  return (
    <PageLayout
      subtitle="Per-project scheduled commands and URL tasks managed with recurring timing, run history, and log access."
      title="Tasks"
    >
      <div className="route-grid" data-columns="4">
        <MetricCard
          label="Scheduled Tasks"
          tone={tasks.length > 0 ? "success" : "warning"}
          value={String(tasks.length)}
        />
        <MetricCard
          label="Enabled Now"
          tone={enabledTasks > 0 ? "success" : "warning"}
          value={String(enabledTasks)}
        />
        <MetricCard
          label="Running"
          tone={runningTasks > 0 ? "success" : "warning"}
          value={String(runningTasks)}
        />
        <MetricCard
          label="Needs Review"
          tone={taskErrors > 0 ? "error" : "success"}
          value={String(taskErrors)}
        />
      </div>

      <ProjectScheduledTaskPanel mode="workspace" />
    </PageLayout>
  );
}
