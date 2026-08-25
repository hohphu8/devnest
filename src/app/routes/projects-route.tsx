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

export default function ProjectsRoute() {
  const [searchParams, setSearchParams] = useSearchParams();
  const [search, setSearch] = useState("");
  const [frameworkFilter, setFrameworkFilter] = useState<
    "all" | "laravel" | "symfony" | "wordpress" | "php" | "unknown"
  >("all");
  const [serverFilter, setServerFilter] = useState<
    "all" | "apache" | "nginx" | "frankenphp"
  >("all");
  const [statusFilter, setStatusFilter] = useState<
    "all" | "running" | "stopped" | "error"
  >("all");
  const [sortBy, setSortBy] = useState<
    "updated-desc" | "name-asc" | "domain-asc"
  >("updated-desc");
  const [viewMode, setViewMode] = useState<"list" | "grid">(() => {
    const fromQuery =
      typeof window !== "undefined"
        ? parseProjectsViewMode(
            new URLSearchParams(window.location.search).get("view"),
          )
        : null;
    if (fromQuery) {
      return fromQuery;
    }

    if (typeof window !== "undefined") {
      const stored = parseProjectsViewMode(
        window.localStorage.getItem(PROJECTS_VIEW_STORAGE_KEY),
      );
      if (stored) {
        return stored;
      }
    }

    return "list";
  });
  const diagnosticsByProject = useDiagnosticsStore(
    (state) => state.itemsByProject,
  );
  const services = useServiceStore((state) => state.services);
  const loadServices = useServiceStore((state) => state.loadServices);
  const pushToast = useToastStore((state) => state.push);
  const {
    activeProject,
    deleteProject,
    error,
    fetchProject,
    loading,
    loadProjects,
    projects,
    selectedProjectId,
    selectProject,
    updateProject,
  } = useProjectStore();
  const wizardOpen = searchParams.get("wizard") === "1";
  const requestedProjectId = searchParams.get("projectId");
  const requestedProjectExists = Boolean(
    requestedProjectId &&
    projects.some((project) => project.id === requestedProjectId),
  );
  const activeModalProject =
    requestedProjectId && activeProject?.id === requestedProjectId
      ? activeProject
      : undefined;
  const modalProjectSummary =
    requestedProjectId && requestedProjectExists
      ? projects.find((project) => project.id === requestedProjectId)
      : undefined;
  const projectModalOpen = requestedProjectExists;
  const projectModalLoading = Boolean(
    requestedProjectId &&
    requestedProjectExists &&
    (loading || !activeModalProject),
  );
  const showProjectModalScrim = useDelayedBusy(projectModalLoading);
  const requestedViewMode = parseProjectsViewMode(searchParams.get("view"));

  useEffect(() => {
    if (!projects.length) {
      return;
    }

    if (
      requestedProjectId &&
      projects.some((project) => project.id === requestedProjectId)
    ) {
      if (selectedProjectId !== requestedProjectId) {
        selectProject(requestedProjectId);
      }
      return;
    }

    if (!selectedProjectId) {
      selectProject(projects[0]?.id);
    }
  }, [
    fetchProject,
    projects,
    requestedProjectId,
    selectProject,
    selectedProjectId,
  ]);

  useEffect(() => {
    if (requestedViewMode) {
      setViewMode((current) =>
        current === requestedViewMode ? current : requestedViewMode,
      );
      return;
    }

    if (typeof window !== "undefined") {
      const stored = parseProjectsViewMode(
        window.localStorage.getItem(PROJECTS_VIEW_STORAGE_KEY),
      );
      if (stored) {
        setViewMode((current) => (current === stored ? current : stored));
      }
    }
  }, [requestedViewMode]);

  useEffect(() => {
    if (
      !requestedProjectId ||
      !selectedProjectId ||
      selectedProjectId !== requestedProjectId ||
      activeProject?.id === requestedProjectId
    ) {
      return;
    }

    void fetchProject(requestedProjectId);
  }, [activeProject?.id, fetchProject, requestedProjectId, selectedProjectId]);

  useEffect(() => {
    if (!projectModalOpen) {
      return;
    }

    function handleKeydown(event: KeyboardEvent) {
      if (event.key !== "Escape") {
        return;
      }

      if (
        document.querySelector(
          ".project-detail-dialog [data-nested-modal='true']",
        )
      ) {
        return;
      }

      event.preventDefault();
      setSearchParams(
        mergeSearchParams(searchParams, { projectId: undefined }),
      );
    }

    document.addEventListener("keydown", handleKeydown);
    return () => document.removeEventListener("keydown", handleKeydown);
  }, [projectModalOpen, searchParams, setSearchParams]);

  async function handleProjectUpdate(
    projectId: string,
    patch: UpdateProjectPatch,
  ) {
    const previousDomain =
      activeProject?.id === projectId ? activeProject.domain : undefined;
    const updatedProject = await updateProject(projectId, patch);

    if (previousDomain && patch.domain && patch.domain !== previousDomain) {
      try {
        await configApi.removeHosts(previousDomain);
        pushToast({
          tone: "success",
          title: "Project updated",
          message: `Removed old hosts entry ${previousDomain}.`,
        });
      } catch (invokeError) {
        pushToast({
          tone: "warning",
          title: "Project updated with cleanup warning",
          message: `Old hosts entry ${previousDomain} could not be removed: ${getAppErrorMessage(invokeError, "Hosts cleanup failed.")}`,
        });
      }
    }

    return updatedProject;
  }

  async function handleProjectDelete(projectId: string) {
    const existingProject = projects.find((item) => item.id === projectId);
    await deleteProject(projectId);

    if (!existingProject) {
      return;
    }

    try {
      await configApi.removeHosts(existingProject.domain);
      pushToast({
        tone: "success",
        title: "Project deleted",
        message: `Removed hosts entry ${existingProject.domain}.`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "warning",
        title: "Project deleted with cleanup warning",
        message: `Hosts cleanup for ${existingProject.domain} failed: ${getAppErrorMessage(invokeError, "Hosts cleanup failed.")}`,
      });
    }
  }

  async function handleImportProjectProfile() {
    try {
      const importedProject = await projectProfileApi.importProject();
      if (!importedProject) {
        return;
      }

      await loadProjects();
      openProject(importedProject.project.id);
      pushToast({
        tone: "success",
        title: "Project profile imported",
        message: `${importedProject.project.name} is now tracked in DevNest.`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Project profile import failed",
        message: getAppErrorMessage(
          invokeError,
          "Failed to import the selected project profile.",
        ),
      });
    }
  }

  async function handleImportTeamProjectProfile() {
    try {
      const importedProject = await projectProfileApi.importTeamProject();
      if (!importedProject) {
        return;
      }

      await loadProjects();
      openProject(importedProject.project.id);
      const warningSuffix =
        importedProject.warnings.length > 0
          ? ` ${importedProject.warnings.length} compatibility warning(s) need review.`
          : "";
      pushToast({
        tone: importedProject.warnings.length > 0 ? "warning" : "success",
        title: "Team profile imported",
        message: `${importedProject.project.name} is now tracked in DevNest from a shared project profile.${warningSuffix}`,
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Team profile import failed",
        message: getAppErrorMessage(
          invokeError,
          "Failed to import the selected shared project profile.",
        ),
      });
    }
  }

  function openProject(projectId: string) {
    selectProject(projectId);
    setSearchParams(mergeSearchParams(searchParams, { projectId }));
  }

  function closeProjectModal() {
    setSearchParams(mergeSearchParams(searchParams, { projectId: undefined }));
  }

  function handleSetViewMode(nextViewMode: "list" | "grid") {
    setViewMode(nextViewMode);
    if (typeof window !== "undefined") {
      window.localStorage.setItem(PROJECTS_VIEW_STORAGE_KEY, nextViewMode);
    }
    setSearchParams(
      mergeSearchParams(searchParams, {
        view: nextViewMode === "grid" ? "grid" : undefined,
      }),
    );
  }

  const visibleProjects = useMemo(() => {
    const filtered = projects.filter((project) => {
      const liveStatus = getLiveProjectStatus(project, services);
      const textMatches =
        search.trim().length === 0 ||
        [project.name, project.domain, project.path]
          .join(" ")
          .toLowerCase()
          .includes(search.trim().toLowerCase());

      const frameworkMatches =
        frameworkFilter === "all" || project.framework === frameworkFilter;
      const serverMatches =
        serverFilter === "all" || project.serverType === serverFilter;
      const statusMatches =
        statusFilter === "all" || liveStatus === statusFilter;

      return textMatches && frameworkMatches && serverMatches && statusMatches;
    });

    return filtered.sort((left, right) => {
      if (sortBy === "name-asc") {
        return left.name.localeCompare(right.name);
      }

      if (sortBy === "domain-asc") {
        return left.domain.localeCompare(right.domain);
      }

      return right.updatedAt.localeCompare(left.updatedAt);
    });
  }, [
    frameworkFilter,
    projects,
    search,
    serverFilter,
    services,
    sortBy,
    statusFilter,
  ]);

  return (
    <PageLayout
      actions={
        <>
          <Button onClick={() => void handleImportTeamProjectProfile()}>
            Import Team Profile
          </Button>
          <Button onClick={() => void handleImportProjectProfile()}>
            Import Profile
          </Button>
          <Button
            onClick={() => {
              void loadProjects();
              void loadServices();
            }}
          >
            Refresh
          </Button>
          <Button
            onClick={() =>
              setSearchParams(mergeSearchParams(searchParams, { wizard: "1" }))
            }
            variant="primary"
          >
            Add Project
          </Button>
        </>
      }
      subtitle="Search, filter, inspect, and provision PHP projects from one project-first workspace."
      title="Projects"
    >
      <AddProjectWizard
        onClose={() => {
          setSearchParams(
            mergeSearchParams(searchParams, { wizard: undefined }),
          );
          void loadProjects();
        }}
        onCreated={(project) => {
          void loadProjects();
          openProject(project.id);
        }}
        open={wizardOpen}
        recentPaths={projects.map((project) => project.path)}
      />

      <Card>
        <div className="page-header">
          <div>
            <h2>Registry Controls</h2>
          </div>
          <div className="page-toolbar">
            <Button
              onClick={() => handleSetViewMode("list")}
              variant={viewMode === "list" ? "primary" : "secondary"}
            >
              List
            </Button>
            <Button
              onClick={() => handleSetViewMode("grid")}
              variant={viewMode === "grid" ? "primary" : "secondary"}
            >
              Grid
            </Button>
          </div>
        </div>

        <div className="stack" style={{ gap: 12 }}>
          <div
            className="logs-filters"
            style={{
              gridTemplateColumns:
                "minmax(0, 1.4fr) repeat(4, minmax(0, 180px))",
            }}
          >
            <input
              className="input"
              onChange={(event) => setSearch(event.target.value)}
              placeholder="Search by project name, domain, or path"
              value={search}
            />
            <select
              className="select"
              onChange={(event) =>
                setFrameworkFilter(event.target.value as typeof frameworkFilter)
              }
              value={frameworkFilter}
            >
              <option value="all">All frameworks</option>
              <option value="laravel">Laravel</option>
              <option value="symfony">Symfony</option>
              <option value="wordpress">WordPress</option>
              <option value="php">PHP</option>
              <option value="unknown">Unknown</option>
            </select>
            <select
              className="select"
              onChange={(event) =>
                setServerFilter(event.target.value as typeof serverFilter)
              }
              value={serverFilter}
            >
              <option value="all">All servers</option>
              <option value="apache">Apache</option>
              <option value="nginx">Nginx</option>
              <option value="frankenphp">FrankenPHP</option>
            </select>
            <select
              className="select"
              onChange={(event) =>
                setStatusFilter(event.target.value as typeof statusFilter)
              }
              value={statusFilter}
            >
              <option value="all">All statuses</option>
              <option value="running">Running</option>
              <option value="stopped">Stopped</option>
              <option value="error">Error</option>
            </select>
            <select
              className="select"
              onChange={(event) =>
                setSortBy(event.target.value as typeof sortBy)
              }
              value={sortBy}
            >
              <option value="updated-desc">Recently updated</option>
              <option value="name-asc">Name A-Z</option>
              <option value="domain-asc">Domain A-Z</option>
            </select>
          </div>
          <span className="helper-text">
            Showing {visibleProjects.length} of {projects.length} tracked
            projects.
          </span>
        </div>

        <div className="page-header">
          <p>
            Dense project browsing with a quick path into provisioning,
            diagnostics, and runtime control.
          </p>
        </div>

        {error ? <span className="error-text">{error}</span> : null}

        {visibleProjects.length > 0 ? (
          viewMode === "list" ? (
            <div className="list-stack">
              {visibleProjects.map((project) => {
                const liveStatus = getLiveProjectStatus(project, services);

                return (
                  <button
                    className="list-row"
                    data-active={selectedProjectId === project.id}
                    key={project.id}
                    onClick={() => openProject(project.id)}
                    style={{ textAlign: "left" }}
                    type="button"
                  >
                    <div className="list-row-head">
                      <div>
                        <strong>{project.name}</strong>
                        <div className="helper-text">{project.domain}</div>
                      </div>
                      <span
                        className="status-chip"
                        data-tone={getStatusTone(liveStatus)}
                      >
                        {liveStatus}
                      </span>
                    </div>
                    <div className="list-row-meta">
                      <span className="status-chip">{project.framework}</span>
                      <span className="status-chip">{project.serverType}</span>
                      <span className="status-chip">
                        PHP {project.phpVersion}
                      </span>
                      <span className="status-chip">
                        {project.documentRoot}
                      </span>
                      <span className="status-chip">
                        {
                          summarizeDiagnostics(
                            diagnosticsByProject[project.id] ?? [],
                          ).actionable
                        }{" "}
                        issues
                      </span>
                    </div>
                  </button>
                );
              })}
            </div>
          ) : (
            <div className="route-grid" data-columns="3">
              {visibleProjects.map((project) => (
                <ProjectCard
                  issueCount={
                    summarizeDiagnostics(diagnosticsByProject[project.id] ?? [])
                      .actionable
                  }
                  key={project.id}
                  onInspect={openProject}
                  project={project}
                />
              ))}
            </div>
          )
        ) : (
          <EmptyState
            title="No projects match the current filters"
            description="Adjust the search or filters, or import another project."
          />
        )}
      </Card>

      {projectModalOpen ? (
        <div
          className="wizard-overlay"
          onClick={closeProjectModal}
          role="dialog"
          aria-modal="true"
        >
          <div
            className="project-detail-dialog"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="project-detail-header">
              <div>
                <h2>
                  {activeModalProject?.name ??
                    modalProjectSummary?.name ??
                    "Project Detail"}
                </h2>
                <p>
                  Project detail, provisioning, diagnostics, and runtime
                  controls in one modal surface.
                </p>
              </div>
              <Button onClick={closeProjectModal}>Close</Button>
            </div>

            <div className="project-detail-stage">
              <div className="project-detail-content">
                <ProjectInspector
                  loading={loading}
                  onDelete={handleProjectDelete}
                  onUpdate={handleProjectUpdate}
                  project={activeModalProject}
                />
              </div>
              {showProjectModalScrim ? (
                <LoadingScrim
                  message="Fetching project profile, runtime metadata, and diagnostics context."
                  title="Opening Project"
                />
              ) : null}
            </div>
          </div>
        </div>
      ) : null}
    </PageLayout>
  );
}
