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

export default function DiagnosticsRoute() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const projects = useProjectStore((state) => state.projects);
  const itemsByProject = useDiagnosticsStore((state) => state.itemsByProject);
  const lastRunAtByProject = useDiagnosticsStore(
    (state) => state.lastRunAtByProject,
  );
  const loadingProjectId = useDiagnosticsStore(
    (state) => state.loadingProjectId,
  );
  const diagnosticsError = useDiagnosticsStore((state) => state.error);
  const runDiagnostics = useDiagnosticsStore((state) => state.runDiagnostics);
  const updateProject = useProjectStore((state) => state.updateProject);
  const pushToast = useToastStore((state) => state.push);
  const selectedProjectId = searchParams.get("projectId");

  useEffect(() => {
    if (selectedProjectId || projects.length === 0) {
      return;
    }

    setSearchParams({ projectId: projects[0].id });
  }, [projects, selectedProjectId, setSearchParams]);

  const selectedProject =
    projects.find((project) => project.id === selectedProjectId) ?? projects[0];
  const diagnosticsItems = selectedProject
    ? (itemsByProject[selectedProject.id] ?? [])
    : [];
  const diagnosticsSummary = summarizeDiagnostics(diagnosticsItems);
  const isLoading = selectedProject
    ? loadingProjectId === selectedProject.id
    : false;
  const lastRunAt = selectedProject
    ? lastRunAtByProject[selectedProject.id]
    : undefined;
  const initialDiagnosticsLoading = Boolean(
    selectedProject && diagnosticsItems.length === 0 && isLoading,
  );
  const showDiagnosticsScrim = useDelayedBusy(initialDiagnosticsLoading);

  useEffect(() => {
    if (!selectedProject || itemsByProject[selectedProject.id]) {
      return;
    }

    const timeoutId = window.setTimeout(() => {
      void runDiagnostics(selectedProject.id).catch(() => undefined);
    }, 60);

    return () => window.clearTimeout(timeoutId);
  }, [itemsByProject, runDiagnostics, selectedProject]);

  async function openDiagnosticAction(code: string) {
    if (!selectedProject) {
      return;
    }

    if (diagnosticCanAutoFix(code)) {
      try {
        const result = await diagnosticsApi.fix(selectedProject.id, code);
        if (code === "LARAVEL_DOCUMENT_ROOT_MISMATCH") {
          await updateProject(selectedProject.id, { documentRoot: "public" });
        }
        await runDiagnostics(selectedProject.id);
        pushToast({
          tone: "success",
          title: "Quick Fix Applied",
          message: result.message,
        });
      } catch (error) {
        pushToast({
          tone: "error",
          title: "Quick Fix Failed",
          message: getAppErrorMessage(
            error,
            "DevNest could not apply that quick fix.",
          ),
        });
      }
      return;
    }

    switch (code) {
      case "PORT_IN_USE":
      case "WSL_PORT_CONFLICT":
      case "MYSQL_STARTUP_FAILED":
      case "SERVICE_RUNTIME_ERROR":
        navigate("/services");
        return;
      case "PHP_MISSING_EXTENSIONS":
      case "PHP_EXTENSION_CHECK_UNAVAILABLE":
      case "APACHE_REWRITE_DISABLED":
      case "APACHE_REWRITE_UNVERIFIED":
        navigate(`/logs?source=${selectedProject.serverType}`);
        return;
      default:
        navigate(`/projects?projectId=${selectedProject.id}`);
    }
  }

  async function handleRunDiagnostics() {
    if (!selectedProject) {
      return;
    }

    try {
      await runDiagnostics(selectedProject.id);
    } catch {
      return;
    }
  }

  return (
    <PageLayout
      actions={
        selectedProject ? (
          <>
            <Button
              onClick={() =>
                navigate(`/projects?projectId=${selectedProject.id}`)
              }
            >
              Open Project
            </Button>
            <Button
              disabled={isLoading}
              onClick={() => void handleRunDiagnostics()}
              variant="primary"
            >
              {isLoading ? "Running..." : "Run Diagnostics"}
            </Button>
          </>
        ) : undefined
      }
      subtitle="Readable project health checks, runtime conflicts, and common local setup issues."
      title="Diagnostics"
    >
      <div className="route-loading-shell">
        {projects.length === 0 ? (
          <EmptyState
            title="No projects available"
            description="Import a project first so DevNest can run diagnostics against a real project profile."
          />
        ) : selectedProject ? (
          <>
            <Card>
              <div className="page-header">
                <div>
                  <h2>Selected Project</h2>
                  <p>
                    Choose a persisted project profile and run diagnostics
                    against its current runtime setup.
                  </p>
                </div>
              </div>
              <div
                className="logs-filters"
                style={{ gridTemplateColumns: "minmax(0, 1fr) 220px" }}
              >
                <select
                  className="select"
                  onChange={(event) =>
                    setSearchParams(
                      mergeSearchParams(searchParams, {
                        projectId: event.target.value,
                      }),
                    )
                  }
                  value={selectedProject.id}
                >
                  {projects.map((project) => (
                    <option key={project.id} value={project.id}>
                      {project.name} ({project.domain})
                    </option>
                  ))}
                </select>
                <div className="detail-item">
                  <strong>
                    {selectedProject.serverType} / PHP{" "}
                    {selectedProject.phpVersion}
                  </strong>
                </div>
              </div>
            </Card>

            <div
              className="route-grid"
              data-columns="4"
              style={{ marginBottom: 16 }}
            >
              <MetricCard
                label="Errors"
                tone={diagnosticsSummary.errors > 0 ? "error" : "success"}
                value={String(diagnosticsSummary.errors)}
              />
              <MetricCard
                label="Warnings"
                tone={diagnosticsSummary.warnings > 0 ? "warning" : "success"}
                value={String(diagnosticsSummary.warnings)}
              />
              <MetricCard
                label="Suggestions"
                tone={
                  diagnosticsSummary.suggestions > 0 ? "warning" : "success"
                }
                value={String(diagnosticsSummary.suggestions)}
              />
              <MetricCard
                label="Last Run"
                tone={lastRunAt ? "success" : "warning"}
                value={lastRunAt ? formatUpdatedAt(lastRunAt) : "Not run"}
              />
            </div>

            <Card>
              <div className="page-header">
                <div>
                  <h2>Issue List</h2>
                  <p>Review issues, then jump straight to the next fix.</p>
                </div>
              </div>

              {diagnosticsError ? (
                <span className="error-text">{diagnosticsError}</span>
              ) : null}

              {isLoading && diagnosticsItems.length === 0 ? (
                <div className="log-viewer-empty">Running diagnostics...</div>
              ) : diagnosticsItems.length > 0 ? (
                <div className="stack" style={{ gap: 12 }}>
                  {diagnosticsItems.map((item: DiagnosticItem) => (
                    <div className="detail-item" key={item.id}>
                      <div
                        className="page-toolbar"
                        style={{ alignItems: "flex-start" }}
                      >
                        <div>
                          <strong>{item.title}</strong>
                          <p style={{ marginTop: 6 }}>{item.message}</p>
                        </div>
                        <span
                          className="status-chip"
                          data-tone={
                            item.level === "error"
                              ? "error"
                              : item.level === "warning"
                                ? "warning"
                                : "success"
                          }
                        >
                          {item.level}
                        </span>
                      </div>
                      {item.suggestion ? (
                        <span className="helper-text">{item.suggestion}</span>
                      ) : null}
                      <div
                        className="page-toolbar"
                        style={{ justifyContent: "flex-start" }}
                      >
                        <Button
                          onClick={() => void openDiagnosticAction(item.code)}
                        >
                          {diagnosticActionLabel(item.code)}
                        </Button>
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                <EmptyState
                  title="No diagnostics yet"
                  description="Run diagnostics for the selected project to generate health checks and quick suggestions."
                />
              )}
            </Card>
          </>
        ) : null}
        {showDiagnosticsScrim ? (
          <LoadingScrim
            message="Running the first diagnostics pass for the selected project."
            title="Preparing Diagnostics"
          />
        ) : null}
      </div>
    </PageLayout>
  );
}
