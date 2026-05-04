import { useEffect, useMemo, useRef, useState } from "react";
import { createBrowserRouter, useNavigate, useSearchParams } from "react-router-dom";
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
import type { RuntimeConfigSchema, RuntimeConfigValues } from "@/types/runtime-config";
import type { PortCheckResult, ServiceLogPayload, ServiceName, ServiceState } from "@/types/service";
import type {
  AppReleaseInfo,
  AppUpdateCheckResult,
  AppUpdateState,
} from "@/types/update";

const SERVICE_START_ORDER: ServiceName[] = [
  "mysql",
  "redis",
  "mailpit",
  "apache",
  "nginx",
  "frankenphp",
];
const PROJECTS_VIEW_STORAGE_KEY = "devnest.projects.view-mode";
const SETTINGS_UPDATE_LAST_CHECKED_KEY = "devnest.settings.updates.last-checked-at";

function PageLayout({
  title,
  subtitle,
  actions,
  children,
}: {
  title: string;
  subtitle: string;
  actions?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="page">
      <div className="page-header">
        <div>
          <h1>{title}</h1>
          <p>{subtitle}</p>
        </div>
        <div className="page-toolbar">{actions}</div>
      </div>
      {children}
    </section>
  );
}

function LoadingScrim({
  message,
  title,
}: {
  message: string;
  title: string;
}) {
  return (
    <div aria-live="polite" className="loading-scrim" role="status">
      <div className="loading-scrim-card">
        <span aria-hidden="true" className="loading-spinner" />
        <div className="loading-scrim-copy">
          <strong>{title}</strong>
          <span>{message}</span>
        </div>
      </div>
    </div>
  );
}

function useDelayedBusy(active: boolean, delayMs = 160) {
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    if (!active) {
      setVisible(false);
      return;
    }

    const timeoutId = window.setTimeout(() => {
      setVisible(true);
    }, delayMs);

    return () => window.clearTimeout(timeoutId);
  }, [active, delayMs]);

  return visible;
}

function getStartAllPlan(services: ServiceState[]) {
  const enabled = [...services]
    .filter((service) => service.enabled)
    .sort(
      (left, right) =>
        SERVICE_START_ORDER.indexOf(left.name) - SERVICE_START_ORDER.indexOf(right.name),
    );
  const reservedPorts = new Set<number>();
  const startable: ServiceState[] = [];
  const skipped: string[] = [];

  enabled.forEach((service) => {
    const port = service.port ?? undefined;
    if (port && reservedPorts.has(port)) {
      skipped.push(`${service.name} (port ${port})`);
      return;
    }

    if (port) {
      reservedPorts.add(port);
    }
    startable.push(service);
  });

  return { startable, skipped };
}

function mergeSearchParams(
  searchParams: URLSearchParams,
  patch: Record<string, string | undefined>,
) {
  const next = new URLSearchParams(searchParams);

  Object.entries(patch).forEach(([key, value]) => {
    if (value === undefined || value.length === 0) {
      next.delete(key);
      return;
    }

    next.set(key, value);
  });

  return next;
}

function parseProjectsViewMode(value: string | null): "list" | "grid" | null {
  return value === "grid" || value === "list" ? value : null;
}

function diagnosticActionLabel(code: string): string {
  switch (code) {
    case "LARAVEL_DOCUMENT_ROOT_MISMATCH":
    case "SSL_AUTHORITY_MISSING":
    case "SSL_TRUST_MISSING":
    case "SSL_CERTIFICATE_MISSING":
      return "Fix Now";
  }

  switch (code) {
    case "PORT_IN_USE":
    case "MYSQL_STARTUP_FAILED":
    case "SERVICE_RUNTIME_ERROR":
      return "Open Services";
    case "PHP_MISSING_EXTENSIONS":
    case "PHP_EXTENSION_CHECK_UNAVAILABLE":
    case "APACHE_REWRITE_DISABLED":
    case "APACHE_REWRITE_UNVERIFIED":
      return "Open Logs";
    case "LARAVEL_DOCUMENT_ROOT_MISMATCH":
      return "Open Project";
    default:
      return "Open Project";
  }
}

function diagnosticCanAutoFix(code: string): boolean {
  return (
    code === "LARAVEL_DOCUMENT_ROOT_MISMATCH" ||
    code === "SSL_AUTHORITY_MISSING" ||
    code === "SSL_TRUST_MISSING" ||
    code === "SSL_CERTIFICATE_MISSING"
  );
}

function runtimeTypeLabel(runtimeType: RuntimeType): string {
  switch (runtimeType) {
    case "php":
      return "PHP";
    case "apache":
      return "Apache";
    case "nginx":
      return "Nginx";
    case "frankenphp":
      return "FrankenPHP";
    case "mysql":
      return "MySQL";
  }
}

function phpCliActivationMessage(version: string): string {
  return `PHP ${version} is now active.`;
}

function withRuntimeDetails(message: string, runtime: Pick<RuntimeInventoryItem, "details">): string {
  return runtime.details ? `${message} ${runtime.details}` : message;
}

function runtimeSourceLabel(source: RuntimeInventoryItem["source"]): string {
  switch (source) {
    case "downloaded":
      return "Downloaded";
    case "imported":
      return "Imported";
    case "bundled":
      return "Bundled";
    case "external":
      return "External";
  }
}

function runtimeFamilyLabel(runtimeType: RuntimeType): string {
  switch (runtimeType) {
    case "php":
      return "PHP";
    case "mysql":
      return "Database";
    case "apache":
    case "nginx":
    case "frankenphp":
      return "Web Server";
  }
}

function runtimeInstallStageLabel(stage: RuntimeInstallStage): string {
  switch (stage) {
    case "queued":
      return "Queued";
    case "downloading":
      return "Downloading";
    case "verifying":
      return "Verifying";
    case "extracting":
      return "Extracting";
    case "registering":
      return "Registering";
    case "completed":
      return "Completed";
    case "failed":
      return "Failed";
  }
}

function optionalToolLabel(toolType: OptionalToolType): string {
  switch (toolType) {
    case "mailpit":
      return "Mailpit";
    case "cloudflared":
      return "Cloudflared";
    case "phpmyadmin":
      return "phpMyAdmin";
    case "redis":
      return "Redis";
    case "restic":
      return "Restic";
  }
}

function optionalToolFamilyLabel(toolType: OptionalToolType): string {
  switch (toolType) {
    case "mailpit":
      return "Mail Sandbox";
    case "cloudflared":
      return "Tunnel Client";
    case "phpmyadmin":
      return "Database UI";
    case "redis":
      return "Cache Service";
    case "restic":
      return "Dedup Backup";
  }
}

function optionalToolInstallStageLabel(stage: OptionalToolInstallStage): string {
  switch (stage) {
    case "queued":
      return "Queued";
    case "downloading":
      return "Downloading";
    case "verifying":
      return "Verifying";
    case "extracting":
      return "Extracting";
    case "registering":
      return "Registering";
    case "completed":
      return "Completed";
    case "failed":
      return "Failed";
  }
}

function findOptionalToolUpdatePackage(
  tool: OptionalToolInventoryItem,
  packages: OptionalToolPackage[],
): OptionalToolPackage | null {
  const installedVersion = normalizeCatalogVersion(tool.version);
  const candidates = packages.filter((toolPackage) => {
    if (toolPackage.toolType !== tool.toolType) {
      return false;
    }

    return compareRuntimeVersions(normalizeCatalogVersion(toolPackage.version), installedVersion) > 0;
  });

  if (candidates.length === 0) {
    return null;
  }

  return [...candidates].sort((left, right) =>
    compareRuntimeVersions(normalizeCatalogVersion(right.version), normalizeCatalogVersion(left.version)),
  )[0];
}

function compareRuntimeVersions(left: string, right: string): number {
  return left.localeCompare(right, undefined, {
    numeric: true,
    sensitivity: "base",
  });
}

function normalizeCatalogVersion(value: string): string {
  return value
    .trim()
    .replace(/^[^0-9a-z]+/i, "")
    .replace(/[^0-9a-z._-]+$/i, "")
    .replace(/^v/i, "")
    .toLowerCase();
}

function displayCatalogVersion(value: string): string {
  return value
    .trim()
    .replace(/^[^0-9a-z]+/i, "")
    .replace(/[^0-9a-z._-]+$/i, "")
    .replace(/^v/i, "");
}

function optionalToolHealthLabel(tool: OptionalToolInventoryItem): string {
  if (tool.status === "missing") {
    return "Missing";
  }

  return tool.isActive ? "Active install" : "Installed";
}

function findRuntimeUpdatePackage(
  runtime: RuntimeInventoryItem,
  packages: RuntimePackage[],
): RuntimePackage | null {
  const candidates = packages.filter((runtimePackage) =>
    runtimeCanOfferUpdateTo(runtime, runtimePackage),
  );

  if (candidates.length === 0) {
    return null;
  }

  return [...candidates].sort((left, right) => compareRuntimeVersions(right.version, left.version))[0];
}

function runtimeCanOfferUpdateTo(
  runtime: Pick<RuntimeInventoryItem, "runtimeType" | "version" | "phpFamily">,
  candidate: Pick<RuntimePackage, "runtimeType" | "version" | "phpFamily">,
): boolean {
  if (candidate.runtimeType !== runtime.runtimeType) {
    return false;
  }

  if (compareRuntimeVersions(candidate.version, runtime.version) <= 0) {
    return false;
  }

  if (runtime.runtimeType === "php") {
    return runtimeVersionFamily(candidate.version) === runtimeVersionFamily(runtime.version);
  }

  if (runtime.runtimeType === "frankenphp" && runtime.phpFamily && candidate.phpFamily) {
    return runtime.phpFamily.toLowerCase() === candidate.phpFamily.toLowerCase();
  }

  return true;
}

function runtimeCatalogKey(
  runtimeType: RuntimeType,
  version: string,
  phpFamily?: string | null,
): string {
  const normalizedVersion = normalizeCatalogVersion(version);
  if (runtimeType === "frankenphp" && phpFamily) {
    return `${runtimeType}:${normalizedVersion}:php-${phpFamily.toLowerCase()}`;
  }

  return `${runtimeType}:${normalizedVersion}`;
}

function serviceLabel(name: ServiceName): string {
  switch (name) {
    case "apache":
      return "Apache";
    case "nginx":
      return "Nginx";
    case "frankenphp":
      return "FrankenPHP";
    case "mysql":
      return "MySQL";
    case "mailpit":
      return "Mailpit";
    case "redis":
      return "Redis";
  }
}

function optionalToolTypeForService(name?: ServiceName | null): OptionalToolType | null {
  if (name === "mailpit" || name === "redis") {
    return name;
  }

  return null;
}

function phpExtensionLabel(extensionName: string): string {
  return extensionName
    .split("_")
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

type RecommendedPhpExtensionSource = "bundled" | "download";

interface RecommendedPhpExtensionSpec {
  extensionName: string;
  source: RecommendedPhpExtensionSource;
  summary: string;
  keywords: string[];
}

type PhpToolsTab = "extensions" | "policy";

const RECOMMENDED_PHP_EXTENSIONS: RecommendedPhpExtensionSpec[] = [
  {
    extensionName: "fileinfo",
    source: "bundled",
    summary: "Mime detection and upload inspection used by many PHP apps.",
    keywords: ["uploads", "mime", "filesystem"],
  },
  {
    extensionName: "opcache",
    source: "bundled",
    summary: "Bytecode cache for faster local PHP request handling.",
    keywords: ["cache", "performance", "zend"],
  },
  {
    extensionName: "bcmath",
    source: "bundled",
    summary: "Required by common billing, crypto, and precision math packages.",
    keywords: ["math", "precision", "finance"],
  },
  {
    extensionName: "curl",
    source: "bundled",
    summary: "HTTP client support for API calls and remote downloads.",
    keywords: ["http", "api", "network"],
  },
  {
    extensionName: "exif",
    source: "bundled",
    summary: "Image metadata parsing for uploads and media libraries.",
    keywords: ["images", "metadata", "uploads"],
  },
  {
    extensionName: "gd",
    source: "bundled",
    summary: "Common image resize and thumbnail support for CMS stacks.",
    keywords: ["images", "thumbnails", "cms"],
  },
  {
    extensionName: "intl",
    source: "bundled",
    summary: "ICU locale, transliteration, and formatter support.",
    keywords: ["locale", "icu", "unicode"],
  },
  {
    extensionName: "mbstring",
    source: "bundled",
    summary: "Multibyte string support required by most modern frameworks.",
    keywords: ["unicode", "framework", "strings"],
  },
  {
    extensionName: "mysqli",
    source: "bundled",
    summary: "Native MySQL client extension for classic PHP apps.",
    keywords: ["mysql", "database", "legacy"],
  },
  {
    extensionName: "openssl",
    source: "bundled",
    summary: "TLS, certificates, signed tokens, and encrypted transport.",
    keywords: ["ssl", "tls", "crypto"],
  },
  {
    extensionName: "pdo_mysql",
    source: "bundled",
    summary: "PDO MySQL driver used by Laravel, Symfony, and WordPress plugins.",
    keywords: ["pdo", "mysql", "database"],
  },
  {
    extensionName: "zip",
    source: "bundled",
    summary: "Archive support for composer plugins, exports, and installers.",
    keywords: ["archives", "composer", "exports"],
  },
  {
    extensionName: "redis",
    source: "download",
    summary: "Redis cache and queue client packaged for one-click install.",
    keywords: ["cache", "queue", "sessions"],
  },
  {
    extensionName: "memcache",
    source: "download",
    summary: "Legacy Memcache client for older CMS and PHP apps.",
    keywords: ["cache", "legacy", "sessions"],
  },
  {
    extensionName: "memcached",
    source: "download",
    summary: "Memcached client for distributed local cache testing.",
    keywords: ["cache", "memcached", "sessions"],
  },
  {
    extensionName: "imagick",
    source: "download",
    summary: "ImageMagick bindings for media pipelines and advanced transforms.",
    keywords: ["images", "media", "imagemagick"],
  },
  {
    extensionName: "xdebug",
    source: "download",
    summary: "Step debugger and profiling hooks for local PHP debugging.",
    keywords: ["debug", "profiling", "zend"],
  },
];

const RECOMMENDED_PHP_EXTENSION_BY_NAME = new Map(
  RECOMMENDED_PHP_EXTENSIONS.map((spec) => [spec.extensionName, spec] as const),
);

function isPhpExtensionDisabledByDefault(extensionName: string): boolean {
  return (
    extensionName === "snmp" ||
    extensionName === "pdo_firebird" ||
    extensionName === "pdo_oci" ||
    extensionName.startsWith("oci8")
  );
}

function phpExtensionAvailabilityLabel(
  spec: RecommendedPhpExtensionSpec | null,
  extensionPackage: PhpExtensionPackage | null,
): string {
  if (extensionPackage) {
    return "Download available";
  }

  if (spec?.source === "bundled") {
    return "Bundled DLL";
  }

  return "Imported locally";
}

function phpExtensionAvailabilityNote(
  extensionName: string,
  spec: RecommendedPhpExtensionSpec | null,
  extensionPackage: PhpExtensionPackage | null,
): string {
  if (extensionPackage) {
    return extensionPackage.notes ?? extensionPackage.displayName;
  }

  if (spec?.source === "bundled") {
    return "Shipped with this PHP family when the runtime bundle includes the DLL.";
  }

  if (isPhpExtensionDisabledByDefault(extensionName)) {
    return "Kept off by default because it often needs external client libraries or extra data files.";
  }

  return "Tracked from the local runtime folder rather than DevNest's download catalog.";
}

function matchesPhpToolsSearch(query: string, values: Array<string | null | undefined>): boolean {
  const normalizedQuery = query.trim().toLowerCase();
  if (normalizedQuery.length === 0) {
    return true;
  }

  return values
    .filter((value): value is string => Boolean(value))
    .some((value) => value.toLowerCase().includes(normalizedQuery));
}

async function waitForNextPaint() {
  await new Promise<void>((resolve) => {
    if (typeof window === "undefined") {
      resolve();
      return;
    }

    window.requestAnimationFrame(() => resolve());
  });
}

function DashboardRoute() {
  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useNavigate();
  const pushToast = useToastStore((state) => state.push);
  const overview = useWorkspaceStore((state) => state.overview);
  const workspaceLoading = useWorkspaceStore((state) => state.loading);
  const portSummaryLoading = useWorkspaceStore((state) => state.portSummaryLoading);
  const workspaceError = useWorkspaceStore((state) => state.error);
  const refreshOverview = useWorkspaceStore((state) => state.refreshOverview);
  const projects = useProjectStore((state) => state.projects);
  const services = useServiceStore((state) => state.services);
  const startService = useServiceStore((state) => state.startService);
  const stopService = useServiceStore((state) => state.stopService);
  const diagnosticsByProject = useDiagnosticsStore((state) => state.itemsByProject);
  const lastRunAtByProject = useDiagnosticsStore((state) => state.lastRunAtByProject);
  const bootState = overview?.bootState ?? null;
  const error =
    !bootState && workspaceError
      ? ({
          code: "WORKSPACE_OVERVIEW_FAILED",
          message: workspaceError,
        } satisfies AppError)
      : null;
  const portConflictCount = useMemo(
    () =>
      overview?.portSummary.filter((port) => !port.available && !port.managedOwner).length ?? 0,
    [overview],
  );
  const portHealthLoading = (workspaceLoading && !overview) || portSummaryLoading;

  const projectIssueCounts = useMemo(
    () =>
      projects.reduce<Record<string, number>>((counts, project) => {
        counts[project.id] = summarizeDiagnostics(diagnosticsByProject[project.id] ?? []).actionable;
        return counts;
      }, {}),
    [diagnosticsByProject, projects],
  );

  const diagnosticIssueCount = useMemo(
    () => Object.values(projectIssueCounts).reduce((total, count) => total + count, 0),
    [projectIssueCounts],
  );

  const diagnosticsCoverage = useMemo(
    () => projects.filter((project) => Boolean(lastRunAtByProject[project.id])).length,
    [lastRunAtByProject, projects],
  );
  const startAllBusy = useAsyncActionPending("workspace:start-all");
  const stopAllBusy = useAsyncActionPending("workspace:stop-all");
  const globalServiceBusy = startAllBusy || stopAllBusy;

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
          } catch (startError) {
            pushToast({
              tone: "error",
              title: "Start all failed",
              message: getAppErrorMessage(startError, `Failed to start ${service.name}.`),
            });
            return;
          }
        }

        await refreshOverview().catch(() => undefined);
        const segments = [];
        if (started.length > 0) {
          segments.push(`Started ${started.join(", ")}.`);
        }
        if (skipped.length > 0) {
          segments.push(`Skipped ${skipped.join(", ")} due to shared default ports.`);
        }
        if (segments.length > 0) {
          pushToast({
            tone: skipped.length > 0 ? "warning" : "success",
            title: "Service startup complete",
            message: segments.join(" "),
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
        const running = services.filter((service) => service.status === "running");

        for (const service of running) {
          try {
            await stopService(service.name);
          } catch (invokeError) {
            pushToast({
              tone: "error",
              title: "Stop all failed",
              message: getAppErrorMessage(invokeError, `Failed to stop ${service.name}.`),
            });
            return;
          }
        }

        await refreshOverview().catch(() => undefined);
        pushToast({
          tone: running.length > 0 ? "success" : "info",
          title: "Service stop complete",
          message: running.length > 0 ? "Stopped all running services." : "No services were running.",
        });
      },
      "Stopping workspace services...",
    );
  }

  const runningProjects = projects.filter(
    (project) => getLiveProjectStatus(project, services) === "running",
  ).length;
  const runningServices = services.filter((service) => service.status === "running").length;
  const orderedProjects = useMemo(
    () =>
      [...projects].sort(
        (left, right) => (projectIssueCounts[right.id] ?? 0) - (projectIssueCounts[left.id] ?? 0),
      ),
    [projectIssueCounts, projects],
  );
  const dashboardTabs = [
    { id: "workspace", label: "Workspace", meta: `${projects.length} projects ready` },
    { id: "projects", label: "Projects", meta: `${orderedProjects.length} tracked` },
  ] as const;
  const activeTab =
    searchParams.get("tab") === "projects" ? "projects" : "workspace";

  function handleSelectTab(tab: "workspace" | "projects") {
    setSearchParams(mergeSearchParams(searchParams, { tab: tab === "workspace" ? undefined : tab }));
  }

  return (
    <PageLayout
      actions={
        <>
          <Button onClick={() => navigate("/projects?wizard=1")}>Add Project</Button>
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
      subtitle="Project status, service health, and recent workspace activity at a glance."
      title="Dashboard"
    >
      <div className="route-grid" data-columns="4">
        <MetricCard label="Running Projects" tone={runningProjects > 0 ? "success" : "warning"} value={String(runningProjects)} />
        <MetricCard label="Active Services" tone={runningServices > 0 ? "success" : "warning"} value={String(runningServices)} />
        <MetricCard label="Port Conflicts" tone={portConflictCount > 0 ? "error" : "success"} value={portHealthLoading ? "..." : String(portConflictCount)} />
        <MetricCard
          label="Diagnostics Issues"
          tone={
            diagnosticsCoverage === 0
              ? "warning"
              : diagnosticIssueCount > 0
                ? "warning"
                : "success"
          }
          value={diagnosticsCoverage === 0 ? "Not run" : String(diagnosticIssueCount)}
        />
      </div>

      <div className="stack workspace-shell">
        <StickyTabs
          activeTab={activeTab}
          ariaLabel="Dashboard sections"
          items={dashboardTabs}
          onSelect={handleSelectTab}
        />

        <div
          aria-labelledby="workspace-tab-workspace"
          className="workspace-panel"
          hidden={activeTab !== "workspace"}
          id="workspace-panel-workspace"
          role="tabpanel"
        >
          <Card>
            <div className="page-header">
              <div>
                <h2>Workspace Health</h2>
                <p>See what is running, what needs attention, and which projects need the next action.</p>
              </div>
            </div>
            {bootState ? (
              <div className="detail-grid">
                <div className="detail-item">
                  <span className="detail-label">Environment</span>
                  <strong>{bootState.environment}</strong>
                </div>
                <div className="detail-item">
                  <span className="detail-label">Database</span>
                  <strong className="mono detail-value">{bootState.dbPath}</strong>
                </div>
                <div className="detail-item">
                  <span className="detail-label">Started</span>
                  <strong>{formatUpdatedAt(bootState.startedAt)}</strong>
                </div>
                <div className="detail-item">
                  <span className="detail-label">Projects Ready</span>
                  <strong>{projects.length}</strong>
                </div>
              </div>
            ) : (
              <EmptyState
                title={error ? error.code : "Loading boot state"}
                description={error ? error.message : "Waiting for the native foundation to respond."}
              />
            )}
            <span className="helper-text">
              Diagnostics snapshot coverage: {diagnosticsCoverage}/{projects.length} projects. Open Diagnostics or a project detail to run a fresh scan.
            </span>
          </Card>
        </div>

        <div
          aria-labelledby="workspace-tab-projects"
          className="workspace-panel"
          hidden={activeTab !== "projects"}
          id="workspace-panel-projects"
          role="tabpanel"
        >
          {orderedProjects.length > 0 ? (
            <div className="route-grid" data-columns="2">
              {orderedProjects.slice(0, 4).map((project) => (
                <ProjectCard
                  issueCount={projectIssueCounts[project.id] ?? 0}
                  key={project.id}
                  project={project}
                  onInspect={(projectId) => navigate(`/projects?projectId=${projectId}`)}
                />
              ))}
            </div>
          ) : (
            <EmptyState
              title="No projects yet"
              description="Import your first PHP project to activate Smart Scan, provisioning, diagnostics, and runtime control."
            />
          )}
        </div>
      </div>
    </PageLayout>
  );
}

function ProjectsRoute() {
  const [searchParams, setSearchParams] = useSearchParams();
  const [search, setSearch] = useState("");
  const [frameworkFilter, setFrameworkFilter] = useState<"all" | "laravel" | "symfony" | "wordpress" | "php" | "unknown">("all");
  const [serverFilter, setServerFilter] = useState<"all" | "apache" | "nginx" | "frankenphp">("all");
  const [statusFilter, setStatusFilter] = useState<"all" | "running" | "stopped" | "error">("all");
  const [sortBy, setSortBy] = useState<"updated-desc" | "name-asc" | "domain-asc">("updated-desc");
  const [viewMode, setViewMode] = useState<"list" | "grid">(() => {
    const fromQuery =
      typeof window !== "undefined"
        ? parseProjectsViewMode(new URLSearchParams(window.location.search).get("view"))
        : null;
    if (fromQuery) {
      return fromQuery;
    }

    if (typeof window !== "undefined") {
      const stored = parseProjectsViewMode(window.localStorage.getItem(PROJECTS_VIEW_STORAGE_KEY));
      if (stored) {
        return stored;
      }
    }

    return "list";
  });
  const diagnosticsByProject = useDiagnosticsStore((state) => state.itemsByProject);
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
    requestedProjectId && projects.some((project) => project.id === requestedProjectId),
  );
  const activeModalProject =
    requestedProjectId && activeProject?.id === requestedProjectId ? activeProject : undefined;
  const modalProjectSummary =
    requestedProjectId && requestedProjectExists
      ? projects.find((project) => project.id === requestedProjectId)
      : undefined;
  const projectModalOpen = requestedProjectExists;
  const projectModalLoading = Boolean(requestedProjectId && requestedProjectExists && (loading || !activeModalProject));
  const showProjectModalScrim = useDelayedBusy(projectModalLoading);
  const requestedViewMode = parseProjectsViewMode(searchParams.get("view"));

  useEffect(() => {
    if (!projects.length) {
      return;
    }

    if (requestedProjectId && projects.some((project) => project.id === requestedProjectId)) {
      if (selectedProjectId !== requestedProjectId) {
        selectProject(requestedProjectId);
      }
      return;
    }

    if (!selectedProjectId) {
      selectProject(projects[0]?.id);
    }
  }, [fetchProject, projects, requestedProjectId, selectProject, selectedProjectId]);

  useEffect(() => {
    if (requestedViewMode) {
      setViewMode((current) => (current === requestedViewMode ? current : requestedViewMode));
      return;
    }

    if (typeof window !== "undefined") {
      const stored = parseProjectsViewMode(window.localStorage.getItem(PROJECTS_VIEW_STORAGE_KEY));
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

      if (document.querySelector(".project-detail-dialog [data-nested-modal='true']")) {
        return;
      }

      event.preventDefault();
      setSearchParams(mergeSearchParams(searchParams, { projectId: undefined }));
    }

    document.addEventListener("keydown", handleKeydown);
    return () => document.removeEventListener("keydown", handleKeydown);
  }, [projectModalOpen, searchParams, setSearchParams]);

  async function handleProjectUpdate(projectId: string, patch: UpdateProjectPatch) {
    const previousDomain = activeProject?.id === projectId ? activeProject.domain : undefined;
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
        message: getAppErrorMessage(invokeError, "Failed to import the selected project profile."),
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

      const frameworkMatches = frameworkFilter === "all" || project.framework === frameworkFilter;
      const serverMatches = serverFilter === "all" || project.serverType === serverFilter;
      const statusMatches = statusFilter === "all" || liveStatus === statusFilter;

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
  }, [frameworkFilter, projects, search, serverFilter, services, sortBy, statusFilter]);

  return (
    <PageLayout
      actions={
        <>
          <Button onClick={() => void handleImportTeamProjectProfile()}>Import Team Profile</Button>
          <Button onClick={() => void handleImportProjectProfile()}>Import Profile</Button>
          <Button onClick={() => { void loadProjects(); void loadServices(); }}>Refresh</Button>
          <Button
            onClick={() => setSearchParams(mergeSearchParams(searchParams, { wizard: "1" }))}
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
          setSearchParams(mergeSearchParams(searchParams, { wizard: undefined }));
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
            <Button onClick={() => handleSetViewMode("list")} variant={viewMode === "list" ? "primary" : "secondary"}>
              List
            </Button>
            <Button onClick={() => handleSetViewMode("grid")} variant={viewMode === "grid" ? "primary" : "secondary"}>
              Grid
            </Button>
          </div>
        </div>

        <div className="stack" style={{ gap: 12 }}>
          <div className="logs-filters" style={{ gridTemplateColumns: "minmax(0, 1.4fr) repeat(4, minmax(0, 180px))" }}>
            <input
              className="input"
              onChange={(event) => setSearch(event.target.value)}
              placeholder="Search by project name, domain, or path"
              value={search}
            />
            <select className="select" onChange={(event) => setFrameworkFilter(event.target.value as typeof frameworkFilter)} value={frameworkFilter}>
              <option value="all">All frameworks</option>
              <option value="laravel">Laravel</option>
              <option value="symfony">Symfony</option>
              <option value="wordpress">WordPress</option>
              <option value="php">PHP</option>
              <option value="unknown">Unknown</option>
            </select>
            <select className="select" onChange={(event) => setServerFilter(event.target.value as typeof serverFilter)} value={serverFilter}>
              <option value="all">All servers</option>
              <option value="apache">Apache</option>
              <option value="nginx">Nginx</option>
              <option value="frankenphp">FrankenPHP</option>
            </select>
            <select className="select" onChange={(event) => setStatusFilter(event.target.value as typeof statusFilter)} value={statusFilter}>
              <option value="all">All statuses</option>
              <option value="running">Running</option>
              <option value="stopped">Stopped</option>
              <option value="error">Error</option>
            </select>
            <select className="select" onChange={(event) => setSortBy(event.target.value as typeof sortBy)} value={sortBy}>
              <option value="updated-desc">Recently updated</option>
              <option value="name-asc">Name A-Z</option>
              <option value="domain-asc">Domain A-Z</option>
            </select>
          </div>
          <span className="helper-text">
            Showing {visibleProjects.length} of {projects.length} tracked projects.
          </span>
        </div>

        <div className="page-header">
          <p>Dense project browsing with a quick path into provisioning, diagnostics, and runtime control.</p>
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
                      <span className="status-chip" data-tone={getStatusTone(liveStatus)}>
                        {liveStatus}
                      </span>
                    </div>
                    <div className="list-row-meta">
                      <span className="status-chip">{project.framework}</span>
                      <span className="status-chip">{project.serverType}</span>
                      <span className="status-chip">PHP {project.phpVersion}</span>
                      <span className="status-chip">{project.documentRoot}</span>
                      <span className="status-chip">
                        {summarizeDiagnostics(diagnosticsByProject[project.id] ?? []).actionable} issues
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
                  issueCount={summarizeDiagnostics(diagnosticsByProject[project.id] ?? []).actionable}
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
        <div className="wizard-overlay" onClick={closeProjectModal} role="dialog" aria-modal="true">
          <div
            className="project-detail-dialog"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="project-detail-header">
              <div>
                <h2>{activeModalProject?.name ?? modalProjectSummary?.name ?? "Project Detail"}</h2>
                <p>Project detail, provisioning, diagnostics, and runtime controls in one modal surface.</p>
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

function ServicesRoute() {
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
  const [optionalToolInventory, setOptionalToolInventory] = useState<OptionalToolInventoryItem[]>([]);
  const [runtimeInventory, setRuntimeInventory] = useState<RuntimeInventoryItem[]>([]);
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
          message: getAppErrorMessage(invokeError, "Failed to inspect the service port."),
        }),
      );
  }, [activeService]);

  const activeRuntime = useMemo(
    () =>
      activeService
        ? runtimeInventory.find(
            (runtime) => runtime.runtimeType === activeService.name && runtime.isActive,
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
      (tool) => tool.toolType === toolType && tool.isActive && tool.status === "available",
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
    await loadServices();
    await refreshRuntimeInventory();
    await refreshOptionalToolInventory();
    if (!selectedServiceName) {
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

  async function runServiceAction(name: ServiceName, action: "start" | "stop" | "restart") {
    try {
      if (action === "start") {
        await startService(name);
        pushToast({
          tone: "success",
          title: "Service started",
          message: `${serviceLabel(name)} started.`,
        });
      } else if (action === "stop") {
        await stopService(name);
        pushToast({
          tone: "success",
          title: "Service stopped",
          message: `${serviceLabel(name)} stopped.`,
        });
      } else {
        await restartService(name);
        pushToast({
          tone: "success",
          title: "Service restarted",
          message: `${serviceLabel(name)} restarted.`,
        });
      }

      await refreshSelectedService();
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Service action failed",
        message: getAppErrorMessage(invokeError, `Failed to ${action} ${name}.`),
      });
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
              message: getAppErrorMessage(invokeError, `Failed to start ${service.name}.`),
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
          messageParts.push(`Skipped ${skipped.join(", ")} because they share the same default port.`);
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
        const running = services.filter((service) => service.status === "running");

        for (const service of running) {
          try {
            await stopService(service.name);
          } catch (invokeError) {
            pushToast({
              tone: "error",
              title: "Stop all failed",
              message: getAppErrorMessage(invokeError, `Failed to stop ${service.name}.`),
            });
            return;
          }
        }

        await refreshSelectedService();
        pushToast({
          tone: running.length > 0 ? "success" : "info",
          title: "Service stop complete",
          message: running.length > 0 ? "Stopped all running services." : "No services were running.",
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
            activeService ? handleOpenServiceDashboard(activeService.name) : Promise.resolve()
          }
          onOpenLogs={() => navigate(`/logs?source=${activeService?.name ?? selectedServiceName ?? "apache"}`)}
          onRefresh={refreshSelectedService}
          onRestart={() =>
            activeService ? runServiceAction(activeService.name, "restart") : Promise.resolve()
          }
          onStart={() =>
            activeService ? runServiceAction(activeService.name, "start") : Promise.resolve()
          }
          onStop={() => (activeService ? runServiceAction(activeService.name, "stop") : Promise.resolve())}
          portCheck={portCheck}
          service={activeService}
        />
      </div>
    </PageLayout>
  );
}

function WorkersRoute() {
  const workers = useProjectWorkerStore((state) => state.workers);
  const runningWorkers = workers.filter((worker) => worker.status === "running").length;
  const workerErrors = workers.filter((worker) => worker.status === "error").length;

  return (
    <PageLayout
      subtitle="Per-project queue, schedule, and custom background commands managed without open terminal windows."
      title="Workers"
    >
      <div className="route-grid" data-columns="4">
        <MetricCard
          label="Managed Workers"
          tone={workers.length > 0 ? "success" : "warning"}
          value={String(workers.length)}
        />
        <MetricCard
          label="Running Now"
          tone={runningWorkers > 0 ? "success" : "warning"}
          value={String(runningWorkers)}
        />
        <MetricCard
          label="Needs Review"
          tone={workerErrors > 0 ? "error" : "success"}
          value={String(workerErrors)}
        />
        <MetricCard
          label="Auto-Start Enabled"
          tone={workers.some((worker) => worker.autoStart) ? "success" : "warning"}
          value={String(workers.filter((worker) => worker.autoStart).length)}
        />
      </div>

      <ProjectWorkerPanel mode="workspace" />
    </PageLayout>
  );
}

function TasksRoute() {
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

function LogsRoute() {
  const [searchParams, setSearchParams] = useSearchParams();
  const pushToast = useToastStore((state) => state.push);
  const workspaceLoaded = useWorkspaceStore((state) => state.loaded);
  const services = useServiceStore((state) => state.services);
  const workers = useProjectWorkerStore((state) => state.workers);
  const loadWorkers = useProjectWorkerStore((state) => state.loadWorkers);
  const scheduledTasks = useProjectScheduledTaskStore((state) => state.tasks);
  const runsByTaskId = useProjectScheduledTaskStore((state) => state.runsByTaskId);
  const loadScheduledTasks = useProjectScheduledTaskStore((state) => state.loadTasks);
  const loadTaskRuns = useProjectScheduledTaskStore((state) => state.loadTaskRuns);
  const clearTaskHistory = useProjectScheduledTaskStore((state) => state.clearTaskHistory);
  const [loading, setLoading] = useState(false);
  const [clearing, setClearing] = useState(false);
  const [error, setError] = useState<string>();
  const [payload, setPayload] = useState<
    ServiceLogPayload | ProjectWorkerLogPayload | ProjectScheduledTaskRunLogPayload | null
  >(null);
  const [search, setSearch] = useState("");
  const [severityFilter, setSeverityFilter] = useState<"all" | "error" | "warning" | "info">("all");
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
  const selectedWorker = workers.find((worker) => worker.id === selectedWorkerId);
  const selectedTask = scheduledTasks.find((task) => task.id === selectedTaskId);
  const selectedTaskRuns = selectedTaskId ? runsByTaskId[selectedTaskId] ?? [] : [];
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
  }, [selectedRunId, selectedService, selectedType, selectedWorkerId, services, setSearchParams]);

  async function loadLogs() {
    setLoading(true);
    setError(undefined);
    try {
      if (selectedType === "worker" && selectedWorkerId) {
        setPayload(await projectWorkerApi.readLogs(selectedWorkerId, 300));
        return;
      }

      if (selectedType === "scheduled-task-run" && selectedRunId) {
        setPayload(await projectScheduledTaskApi.readRunLogs(selectedRunId, 300));
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

        setPayload(await projectScheduledTaskApi.readRunLogs(latestRun.id, 300));
        return;
      } catch (invokeError) {
        setError(
          getAppErrorMessage(invokeError, "Failed to refresh scheduled task run logs."),
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
      } else if (selectedType === "scheduled-task-run" && selectedTaskId && selectedRunId) {
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
        message: getAppErrorMessage(invokeError, "Could not open the selected task logs."),
      });
    }
  }

  return (
    <PageLayout
      actions={
        <>
          <Button
            disabled={
              (!selectedService && !selectedWorkerId && !selectedRunId) || loading || clearing
            }
            onClick={() => void refreshLogs()}
          >
            {loading ? "Refreshing..." : "Refresh"}
          </Button>
          <Button
            disabled={
              (!selectedService && !selectedWorkerId && !selectedRunId) || loading || clearing
            }
            onClick={() => void handleClearLogs()}
          >
            {clearing
              ? "Clearing..."
              : selectedType === "scheduled-task-run"
                ? "Clear History"
                : "Clear Logs"}
          </Button>
          <Button onClick={() => setWrap((value) => !value)}>{wrap ? "Wrap On" : "Wrap Off"}</Button>
          <Button onClick={() => setAutoScroll((value) => !value)}>
            {autoScroll ? "Auto-scroll On" : "Auto-scroll Off"}
          </Button>
        </>
      }
      subtitle="Tail logs by service, worker, or scheduled task run source with search, severity filtering, wrap, and auto-scroll."
      title="Logs"
    >
      {services.length > 0 || workers.length > 0 || scheduledTasks.length > 0 ? (
        <>
          <div className="logs-toolbar">
            <div className="logs-tabs">
              {services.length > 0 ? (
                <div className="stack" style={{ gap: 8 }}>
                  <span className="helper-text">Services</span>
                  <div className="page-toolbar" style={{ justifyContent: "flex-start" }}>
                    {services.map((service) => (
                      <button
                        className="logs-tab"
                        data-active={selectedType === "service" && selectedService === service.name}
                        key={service.name}
                        onClick={() => setSearchParams({ type: "service", source: service.name })}
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
                  <div className="page-toolbar" style={{ justifyContent: "flex-start" }}>
                    {workers.map((worker) => (
                      <button
                        className="logs-tab"
                        data-active={selectedType === "worker" && selectedWorkerId === worker.id}
                        key={worker.id}
                        onClick={() => setSearchParams({ type: "worker", workerId: worker.id })}
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
                  <div className="page-toolbar" style={{ justifyContent: "flex-start" }}>
                    {scheduledTasks.map((task) => (
                      <button
                        className="logs-tab"
                        data-active={selectedType === "scheduled-task-run" && selectedTaskId === task.id}
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
                  setSeverityFilter(event.target.value as "all" | "error" | "warning" | "info")
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
              ? selectedWorker?.name ?? "Selected worker"
              : selectedType === "scheduled-task-run"
                ? selectedTask?.name ?? selectedRun?.id ?? "Selected task run"
              : selectedService ?? "Selected service"
          }
          severityFilter={severityFilter}
          wrap={wrap}
        />
      ) : null}
    </PageLayout>
  );
}

function DiagnosticsRoute() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const projects = useProjectStore((state) => state.projects);
  const itemsByProject = useDiagnosticsStore((state) => state.itemsByProject);
  const lastRunAtByProject = useDiagnosticsStore((state) => state.lastRunAtByProject);
  const loadingProjectId = useDiagnosticsStore((state) => state.loadingProjectId);
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

  const selectedProject = projects.find((project) => project.id === selectedProjectId) ?? projects[0];
  const diagnosticsItems = selectedProject ? itemsByProject[selectedProject.id] ?? [] : [];
  const diagnosticsSummary = summarizeDiagnostics(diagnosticsItems);
  const isLoading = selectedProject ? loadingProjectId === selectedProject.id : false;
  const lastRunAt = selectedProject ? lastRunAtByProject[selectedProject.id] : undefined;
  const initialDiagnosticsLoading = Boolean(selectedProject && diagnosticsItems.length === 0 && isLoading);
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
          message: getAppErrorMessage(error, "DevNest could not apply that quick fix."),
        });
      }
      return;
    }

    switch (code) {
      case "PORT_IN_USE":
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
            <Button onClick={() => navigate(`/projects?projectId=${selectedProject.id}`)}>Open Project</Button>
            <Button disabled={isLoading} onClick={() => void handleRunDiagnostics()} variant="primary">
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
                <p>Choose a persisted project profile and run diagnostics against its current runtime setup.</p>
              </div>
            </div>
            <div className="logs-filters" style={{ gridTemplateColumns: "minmax(0, 1fr) 220px" }}>
              <select
                className="select"
                onChange={(event) =>
                  setSearchParams(mergeSearchParams(searchParams, { projectId: event.target.value }))
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
                <strong>{selectedProject.serverType} / PHP {selectedProject.phpVersion}</strong>
              </div>
            </div>
          </Card>

          <div className="route-grid" data-columns="4" style={{ marginBottom: 16 }}>
            <MetricCard label="Errors" tone={diagnosticsSummary.errors > 0 ? "error" : "success"} value={String(diagnosticsSummary.errors)} />
            <MetricCard label="Warnings" tone={diagnosticsSummary.warnings > 0 ? "warning" : "success"} value={String(diagnosticsSummary.warnings)} />
            <MetricCard label="Suggestions" tone={diagnosticsSummary.suggestions > 0 ? "warning" : "success"} value={String(diagnosticsSummary.suggestions)} />
            <MetricCard label="Last Run" tone={lastRunAt ? "success" : "warning"} value={lastRunAt ? formatUpdatedAt(lastRunAt) : "Not run"} />
          </div>

          <Card>
            <div className="page-header">
              <div>
                <h2>Issue List</h2>
                <p>Review issues, then jump straight to the next fix.</p>
              </div>
            </div>

            {diagnosticsError ? <span className="error-text">{diagnosticsError}</span> : null}

            {isLoading && diagnosticsItems.length === 0 ? (
              <div className="log-viewer-empty">Running diagnostics...</div>
            ) : diagnosticsItems.length > 0 ? (
              <div className="stack" style={{ gap: 12 }}>
                {diagnosticsItems.map((item: DiagnosticItem) => (
                  <div className="detail-item" key={item.id}>
                    <div className="page-toolbar" style={{ alignItems: "flex-start" }}>
                      <div>
                        <strong>{item.title}</strong>
                        <p style={{ marginTop: 6 }}>{item.message}</p>
                      </div>
                      <span className="status-chip" data-tone={item.level === "error" ? "error" : item.level === "warning" ? "warning" : "success"}>
                        {item.level}
                      </span>
                    </div>
                    {item.suggestion ? <span className="helper-text">{item.suggestion}</span> : null}
                    <div className="page-toolbar" style={{ justifyContent: "flex-start" }}>
                      <Button onClick={() => void openDiagnosticAction(item.code)}>
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

function formatDatabaseSnapshotSize(value: number): string {
  if (value < 1024) {
    return `${value} B`;
  }

  if (value < 1024 * 1024) {
    return `${(value / 1024).toFixed(1)} KB`;
  }

  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

function databaseSnapshotTriggerLabel(triggerSource: DatabaseSnapshotSummary["triggerSource"]): string {
  switch (triggerSource) {
    case "pre-action":
      return "Pre-action";
    case "scheduled":
      return "Scheduled";
    default:
      return "Manual";
  }
}

function databaseSnapshotBackendLabel(
  storageBackend: DatabaseSnapshotSummary["storageBackend"] | undefined,
): string {
  return storageBackend === "restic" ? "Restic dedup" : "SQL";
}

function formatDatabaseScheduleLabel(status: DatabaseTimeMachineStatus): string {
  if (!status.enabled || !status.scheduleEnabled) {
    return "Scheduled snapshots are off.";
  }

  const nextRun = status.nextScheduledSnapshotAt
    ? formatUpdatedAt(status.nextScheduledSnapshotAt)
    : "waiting for the next interval";
  return `Every ${status.scheduleIntervalMinutes} minutes, next ${nextRun}.`;
}

function getDatabaseTimeMachinePresentation(
  status: DatabaseTimeMachineStatus | undefined,
  busy: boolean,
  loading = false,
) {
  if (busy) {
    return {
      label: "Busy",
      tone: "warning" as const,
      message: "DevNest is capturing or restoring a managed snapshot.",
    };
  }

  if (loading && !status) {
    return {
      label: "Loading",
      tone: "warning" as const,
      message: "Checking managed snapshot protection.",
    };
  }

  if (!status || status.status === "off") {
    return {
      label: "Off",
      tone: "warning" as const,
      message: "Take the first snapshot to enable rolling protection.",
    };
  }

  if (status.status === "error") {
    return {
      label: "Error",
      tone: "error" as const,
      message: status.lastError ?? "Stored snapshot metadata needs attention.",
    };
  }

  return {
    label: "Protected",
    tone: "success" as const,
    message:
      status.snapshotCount > 0
        ? `${status.snapshotCount} managed snapshot${status.snapshotCount === 1 ? "" : "s"} retained.`
        : "Time Machine is enabled and ready to capture the first snapshot.",
  };
}

function DatabasesRoute() {
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
  const [timeMachineStatusLoading, setTimeMachineStatusLoading] = useState(false);
  const timeMachineStatusRequestRef = useRef(0);
  const [runtimeInventory, setRuntimeInventory] = useState<RuntimeInventoryItem[]>([]);
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
  const [snapshotDialogDatabase, setSnapshotDialogDatabase] = useState<string | null>(null);
  const [snapshotDialogMode, setSnapshotDialogMode] = useState<"history" | "rollback">("history");
  const [snapshotDialogLoading, setSnapshotDialogLoading] = useState(false);
  const [snapshotDialogError, setSnapshotDialogError] = useState<string>();
  const [snapshotDialogSnapshots, setSnapshotDialogSnapshots] = useState<DatabaseSnapshotSummary[]>([]);
  const [selectedRollbackSnapshotId, setSelectedRollbackSnapshotId] = useState("");
  const [rollbackConfirmationInput, setRollbackConfirmationInput] = useState("");

  const mysqlService = services.find((service) => service.name === "mysql");
  const activeMysqlRuntime =
    runtimeInventory.find((runtime) => runtime.runtimeType === "mysql" && runtime.isActive) ?? null;
  const mysqlRunning = mysqlService?.status === "running";
  const linkedProjects = projects.filter((project) => project.databaseName);
  const linkedProjectCount = linkedProjects.length;
  const mysqlPort = mysqlService?.port ?? 3306;
  const databaseTabs = [
    { id: "overview", label: "Overview", meta: mysqlRunning ? `MySQL on ${mysqlPort}` : "MySQL stopped" },
    { id: "databases", label: "Databases", meta: `${databases.length} local` },
    { id: "links", label: "Project Links", meta: `${linkedProjectCount} linked` },
  ] as const;
  const activeTab = (() => {
    const tab = searchParams.get("tab");
    if (tab === "databases" || tab === "links") {
      return tab;
    }
    return "overview";
  })();

  function handleSelectTab(tab: "overview" | "databases" | "links") {
    setSearchParams(mergeSearchParams(searchParams, { tab: tab === "overview" ? undefined : tab }));
  }

  function isTimeMachineBusy(databaseName: string) {
    return Boolean(timeMachineActionKey?.endsWith(`:${databaseName}`));
  }

  function buildTimeMachineStatusFallback(databaseName: string, error: unknown): DatabaseTimeMachineStatus {
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
      lastError: getAppErrorMessage(error, "Time Machine status could not be loaded."),
    };
  }

  useEffect(() => {
    setLinkDrafts(
      Object.fromEntries(projects.map((project) => [project.id, project.databaseName ?? ""])),
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
      setLoadError(getAppErrorMessage(error, "Could not load the database workspace."));
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
            return [databaseName, await databaseApi.getTimeMachineStatus(databaseName)] as const;
          } catch (error) {
            return [databaseName, buildTimeMachineStatusFallback(databaseName, error)] as const;
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

  async function refreshSnapshotDialogData(databaseName: string, resetSelection = false) {
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

      return snapshots.some((snapshot) => snapshot.id === current) ? current : snapshots[0]?.id ?? "";
    });
    setTimeMachineStatusByDatabase((current) => ({
      ...current,
      [databaseName]: status,
    }));
    setSnapshotDialogError(undefined);
  }

  async function loadSnapshotHistory(databaseName: string, mode: "history" | "rollback") {
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
      setSnapshotDialogError(getAppErrorMessage(error, "Could not load managed snapshots."));
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
        setSnapshotDialogError(getAppErrorMessage(error, "Could not refresh managed snapshots."));
      });
    }, 30000);

    return () => window.clearInterval(intervalId);
  }, [mysqlRunning, snapshotDialogDatabase, snapshotDialogLoading, timeMachineActionKey]);

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
        message: getAppErrorMessage(error, "Could not create the requested database."),
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
        message: getAppErrorMessage(error, "Could not update the project database metadata."),
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
        message: getAppErrorMessage(error, "Could not delete the selected database."),
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
        message: getAppErrorMessage(error, "Could not export the selected database."),
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
        message: getAppErrorMessage(error, "Could not restore the selected SQL backup."),
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

        return [result.snapshot, ...current.filter((snapshot) => snapshot.id !== result.snapshot.id)].slice(0, 3);
      });
      setSelectedRollbackSnapshotId((current) =>
        snapshotDialogDatabase === databaseName ? current || result.snapshot.id : current,
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
        message: getAppErrorMessage(error, "Could not create a managed database snapshot."),
      });
    } finally {
      setTimeMachineActionKey(undefined);
    }
  }

  async function handleToggleTimeMachine(databaseName: string, enabled: boolean) {
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
        title: enabled ? "Enable Protection Failed" : "Disable Protection Failed",
        message: getAppErrorMessage(error, "Could not update the Time Machine state."),
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
        message: getAppErrorMessage(error, "Could not roll back the selected database snapshot."),
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
      if (event.key !== "Escape" || snapshotDialogBusy || snapshotDialogLoading) {
        return;
      }

      event.preventDefault();
      setSnapshotDialogDatabase(null);
      setRollbackConfirmationInput("");
    }

    document.addEventListener("keydown", handleKeydown);
    return () => document.removeEventListener("keydown", handleKeydown);
  }, [snapshotDialogBusy, snapshotDialogDatabase, snapshotDialogLoading]);

  const protectedDatabaseCount = Object.values(timeMachineStatusByDatabase).filter(
    (status) => status.enabled,
  ).length;
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
    snapshotDialogDatabase !== null && rollbackConfirmationInput.trim() === snapshotDialogDatabase;

  return (
    <PageLayout
      actions={
        <>
          <Button onClick={() => void loadDatabaseWorkspace()}>{loading ? "Refreshing..." : "Refresh"}</Button>
          {!mysqlRunning ? (
            <Button disabled={actionName === "mysql"} onClick={() => void handleStartMysql()} variant="primary">
              {actionName === "mysql" ? "Starting..." : "Start MySQL"}
            </Button>
          ) : null}
        </>
      }
      subtitle="Create, restore, and recover local MySQL databases without leaving the workspace."
      title="Databases"
    >
      <div className="route-grid" data-columns="4">
        <MetricCard label="Databases" tone={databases.length > 0 ? "success" : "warning"} value={loading ? "..." : String(databases.length)} />
        <MetricCard label="Linked Projects" tone={linkedProjectCount > 0 ? "success" : "warning"} value={String(linkedProjectCount)} />
        <MetricCard
          label="MySQL Service"
          tone={mysqlRunning ? "success" : mysqlService?.status === "error" ? "error" : "warning"}
          value={mysqlRunning ? "Running" : mysqlService?.status === "error" ? "Error" : "Stopped"}
        />
        <MetricCard
          label="Protected DBs"
          tone={
            !protectedDatabaseCountPending && protectedDatabaseCount > 0
              ? "success"
              : "warning"
          }
          value={protectedDatabaseCountPending ? "..." : String(protectedDatabaseCount)}
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
                <p>Provision a new UTF-8 database on the active MySQL runtime, then link it to a project below.</p>
              </div>
              <span
                className="status-chip"
                data-tone={mysqlRunning ? "success" : mysqlService?.status === "error" ? "error" : "warning"}
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
                  <strong>{activeMysqlRuntime ? `MySQL ${activeMysqlRuntime.version}` : "No active MySQL runtime"}</strong>
                  <span>{activeMysqlRuntime?.path ?? "Install or activate a MySQL runtime from Settings first."}</span>
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
                <p>Managed list from the active MySQL runtime. System schemas stay hidden.</p>
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
              <EmptyState description={loadError} title="Database workspace is not ready" />
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
                      const linked = projects.filter((project) => project.databaseName === databaseName);
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
                              <span>{linked.length > 0 ? "Tracked by project metadata." : "Available to link."}</span>
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
                              <span className="runtime-table-note">No tracked projects linked.</span>
                            )}
                          </td>
                          <td>
                            <span className="mono runtime-table-note">{mysqlPort}</span>
                          </td>
                          <td>
                            <div className="database-time-machine-cell">
                              <span className="status-chip" data-tone={timeMachine.tone}>
                                {timeMachine.label}
                              </span>
                              <span className="runtime-table-note">{timeMachine.message}</span>
                            </div>
                          </td>
                          <td>
                            <div className="runtime-table-actions">
                              <ActionMenu disabled={busy || dropBusy || Boolean(transferActionKey)} label="Time Machine">
                                <ActionMenuItem onClick={() => void handleTakeSnapshot(databaseName)}>
                                  Take Snapshot
                                </ActionMenuItem>
                                <ActionMenuItem onClick={() => void loadSnapshotHistory(databaseName, "rollback")}>
                                  Rollback
                                </ActionMenuItem>
                                <ActionMenuItem onClick={() => void loadSnapshotHistory(databaseName, "history")}>
                                  History
                                </ActionMenuItem>
                              </ActionMenu>
                              <Button
                                disabled={dropBusy || Boolean(transferActionKey) || busy}
                                onClick={() => void handleBackupDatabase(databaseName)}
                              >
                                {transferActionKey === `backup:${databaseName}` ? "Backing Up..." : "Backup"}
                              </Button>
                              <Button
                                disabled={dropBusy || Boolean(transferActionKey) || busy}
                                onClick={() => void handleRestoreDatabase(databaseName)}
                              >
                                {transferActionKey === `restore:${databaseName}` ? "Restoring..." : "Restore"}
                              </Button>
                              <Button
                                disabled={linked.length > 0 || dropBusy || Boolean(transferActionKey) || busy}
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
                <p>Keep project metadata aligned with the local database that project expects.</p>
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
                      const selectedDatabase = linkDrafts[project.id] ?? currentDatabase;
                      const hasMissingDatabase =
                        Boolean(currentDatabase) && !databases.includes(currentDatabase);
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
                                <strong className="mono">{currentDatabase}</strong>
                                <span>{hasMissingDatabase ? "Tracked, but not found in MySQL." : `Port ${project.databasePort ?? mysqlPort}`}</span>
                              </div>
                            ) : (
                              <span className="runtime-table-note">No database linked.</span>
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
                                <option value={currentDatabase}>{currentDatabase} (missing)</option>
                              ) : null}
                            </select>
                          </td>
                          <td>
                            <div className="runtime-table-actions">
                              <Button
                                disabled={!isDirty || linkingProjectId === project.id}
                                onClick={() => void handleSaveProjectLink(project.id)}
                                variant="primary"
                              >
                                {linkingProjectId === project.id ? "Saving..." : "Save"}
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
          <div className="confirm-dialog" onClick={(event) => event.stopPropagation()}>
            <div className="confirm-dialog-copy">
              <h3>Delete Database</h3>
              <p>
                DevNest will drop <strong className="mono">{dropTarget}</strong> from the active MySQL runtime.
                This cannot be undone.
              </p>
            </div>
            <div className="confirm-dialog-actions">
              <Button disabled={dropBusy} onClick={() => setDropTarget(undefined)}>
                Cancel
              </Button>
              <Button disabled={dropBusy} onClick={() => void handleDropDatabase()} variant="primary">
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
                    {snapshotDialogMode === "rollback" ? "Rollback Database" : "Time Machine History"}
                  </h3>
                  <p>
                    Managed snapshots for <strong className="mono">{snapshotDialogDatabase}</strong>.
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

              {snapshotDialogError ? <span className="error-text">{snapshotDialogError}</span> : null}

              {snapshotDialogStatus ? (
                <div className="database-snapshot-summary">
                  <div className="database-snapshot-summary-item">
                    <span className="detail-label">Schedule</span>
                    <strong>{formatDatabaseScheduleLabel(snapshotDialogStatus)}</strong>
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
                <span className="helper-text">Loading the last managed snapshots...</span>
              ) : snapshotDialogSnapshots.length === 0 ? (
                <div className="database-snapshot-empty">
                  <strong>No managed snapshots yet.</strong>
                  <span>
                    Use Take Snapshot to start a rolling ring of local recovery points for this database.
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
                        onChange={() => setSelectedRollbackSnapshotId(snapshot.id)}
                        type="radio"
                      />
                      <div className="database-snapshot-item-copy">
                        <strong>{formatUpdatedAt(snapshot.createdAt)}</strong>
                        <span>
                          {databaseSnapshotTriggerLabel(snapshot.triggerSource)} •{" "}
                          {databaseSnapshotBackendLabel(snapshot.storageBackend)} •{" "}
                          {formatDatabaseSnapshotSize(snapshot.sizeBytes)}
                        </span>
                        {snapshot.linkedProjectNames.length > 0 ? (
                          <span>Projects: {snapshot.linkedProjectNames.join(", ")}</span>
                        ) : null}
                        {snapshot.scheduledIntervalMinutes ? (
                          <span>Scheduled every {snapshot.scheduledIntervalMinutes} minutes</span>
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
                    Roll back to {formatUpdatedAt(selectedRollbackSnapshot.createdAt)}?
                  </strong>
                  <span>
                    DevNest will replace the current contents of{" "}
                    <strong className="mono">{snapshotDialogDatabase}</strong> with this managed
                    snapshot and capture one more safety snapshot first when possible.
                  </span>
                  <div className="field">
                    <label htmlFor="database-rollback-confirm">
                      Type <strong className="mono">{snapshotDialogDatabase}</strong> to confirm
                    </label>
                    <input
                      className="input"
                      id="database-rollback-confirm"
                      onChange={(event) => setRollbackConfirmationInput(event.target.value)}
                      placeholder={snapshotDialogDatabase}
                      value={rollbackConfirmationInput}
                    />
                  </div>
                </div>
              ) : null}

              <span className="helper-text">
                DevNest keeps the latest 3 managed snapshots per database. Protection is still scoped
                per database, not the whole MySQL datadir.
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
                {snapshotDialogStatus?.enabled ? "Disable Protection" : "Enable Protection"}
              </Button>
              <Button
                disabled={snapshotDialogBusy}
                onClick={() => void handleTakeSnapshot(snapshotDialogDatabase)}
              >
                {timeMachineActionKey === `snapshot:${snapshotDialogDatabase}` ? "Capturing..." : "Take Snapshot"}
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
                    ? snapshotDialogBusy || snapshotDialogLoading || snapshotDialogSnapshots.length === 0
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
                  : timeMachineActionKey === `rollback:${snapshotDialogDatabase}`
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

function SettingsRoute() {
  const [searchParams, setSearchParams] = useSearchParams();
  const [releaseInfo, setReleaseInfo] = useState<AppReleaseInfo | null>(null);
  const [appUpdateState, setAppUpdateState] = useState<AppUpdateState>("idle");
  const [updateResult, setUpdateResult] = useState<AppUpdateCheckResult | null>(null);
  const [lastUpdateCheckAt, setLastUpdateCheckAt] = useState<string | null>(null);
  const [runtimes, setRuntimes] = useState<RuntimeInventoryItem[]>([]);
  const [runtimePackages, setRuntimePackages] = useState<RuntimePackage[]>([]);
  const [optionalTools, setOptionalTools] = useState<OptionalToolInventoryItem[]>([]);
  const [optionalToolPackages, setOptionalToolPackages] = useState<OptionalToolPackage[]>([]);
  const [persistentTunnelSetup, setPersistentTunnelSetup] =
    useState<PersistentTunnelSetupStatus | null>(null);
  const [namedTunnels, setNamedTunnels] = useState<PersistentTunnelNamedTunnelSummary[]>([]);
  const [namedTunnelsLoading, setNamedTunnelsLoading] = useState(false);
  const [persistentTunnelSetupLoading, setPersistentTunnelSetupLoading] = useState(false);
  const [createTunnelName, setCreateTunnelName] = useState("devnest-main");
  const [defaultHostnameZone, setDefaultHostnameZone] = useState("");
  const [phpExtensions, setPhpExtensions] = useState<PhpExtensionState[]>([]);
  const [phpExtensionPackages, setPhpExtensionPackages] = useState<PhpExtensionPackage[]>([]);
  const [installTask, setInstallTask] = useState<RuntimeInstallTask | null>(null);
  const [optionalToolInstallTask, setOptionalToolInstallTask] = useState<OptionalToolInstallTask | null>(null);
  const [loading, setLoading] = useState(false);
  const [packagesLoading, setPackagesLoading] = useState(false);
  const [phpExtensionsLoading, setPhpExtensionsLoading] = useState(false);
  const [phpExtensionPackagesLoading, setPhpExtensionPackagesLoading] = useState(false);
  const [phpFunctionsLoading, setPhpFunctionsLoading] = useState(false);
  const [error, setError] = useState<string>();
  const [packageError, setPackageError] = useState<string>();
  const [optionalToolError, setOptionalToolError] = useState<string>();
  const [optionalToolPackageError, setOptionalToolPackageError] = useState<string>();
  const [persistentTunnelError, setPersistentTunnelError] = useState<string>();
  const [updateError, setUpdateError] = useState<string>();
  const [phpExtensionsError, setPhpExtensionsError] = useState<string>();
  const [phpExtensionPackagesError, setPhpExtensionPackagesError] = useState<string>();
  const [phpFunctionsError, setPhpFunctionsError] = useState<string>();
  const [actionLoading, setActionLoading] = useState<string | null>(null);
  const [pendingRuntimeRemoval, setPendingRuntimeRemoval] = useState<RuntimeInventoryItem | null>(null);
  const [pendingOptionalToolRemoval, setPendingOptionalToolRemoval] =
    useState<OptionalToolInventoryItem | null>(null);
  const [pendingPhpExtensionRemoval, setPendingPhpExtensionRemoval] =
    useState<PhpExtensionState | null>(null);
  const [pendingPersistentTunnelDeletion, setPendingPersistentTunnelDeletion] =
    useState<PersistentTunnelNamedTunnelSummary | null>(null);
  const [disconnectPersistentTunnelConfirm, setDisconnectPersistentTunnelConfirm] =
    useState(false);
  const [selectedPhpRuntimeId, setSelectedPhpRuntimeId] = useState<string>("");
  const [phpToolsRuntimeId, setPhpToolsRuntimeId] = useState<string | null>(null);
  const [runtimeConfigRuntimeId, setRuntimeConfigRuntimeId] = useState<string | null>(null);
  const [runtimeConfigSchema, setRuntimeConfigSchema] = useState<RuntimeConfigSchema | null>(null);
  const [runtimeConfigValues, setRuntimeConfigValues] = useState<RuntimeConfigValues | null>(null);
  const [runtimeConfigLoading, setRuntimeConfigLoading] = useState(false);
  const [runtimeConfigSaving, setRuntimeConfigSaving] = useState(false);
  const [runtimeConfigOpenFileLoading, setRuntimeConfigOpenFileLoading] = useState(false);
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
    activeTab === "tunnel" && persistentTunnelSetupLoading && !workspaceBootLoading,
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
          typeof invokeError === "object" && invokeError !== null && "details" in invokeError
            ? String((invokeError as AppError).details ?? "")
            : "";
        const baseMessage = getAppErrorMessage(invokeError, "Failed to load app release info.");
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
          typeof invokeError === "object" && invokeError !== null && "details" in invokeError
            ? String((invokeError as AppError).details ?? "")
            : "";
        const baseMessage = getAppErrorMessage(invokeError, "Failed to check for updates.");
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
          typeof invokeError === "object" && invokeError !== null && "details" in invokeError
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

  async function loadRuntimeInventory() {
    setLoading(true);
    setError(undefined);

    try {
      setRuntimes(await runtimeApi.list());
    } catch (invokeError) {
      setError(getAppErrorMessage(invokeError, "Failed to load runtime inventory."));
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
      setPackageError(getAppErrorMessage(invokeError, "Failed to load the runtime package catalog."));
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
        getAppErrorMessage(invokeError, "Failed to load installed optional tools."),
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
        getAppErrorMessage(invokeError, "Failed to load the optional tool catalog."),
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
        getAppErrorMessage(invokeError, "Failed to load the PHP extension catalog."),
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

    const stored = window.localStorage.getItem(SETTINGS_UPDATE_LAST_CHECKED_KEY);
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

    if (!selectedPhpRuntimeId || !phpRuntimes.some((runtime) => runtime.id === selectedPhpRuntimeId)) {
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

      if (document.querySelector(".runtime-tools-dialog [data-nested-modal='true']")) {
        return;
      }

      event.preventDefault();
      setPhpToolsRuntimeId(null);
    }

    document.addEventListener("keydown", handleKeydown);
    return () => document.removeEventListener("keydown", handleKeydown);
  }, [phpToolsRuntimeId]);

  useEffect(() => {
    if (!runtimeConfigRuntimeId) {
      return;
    }

    function handleKeydown(event: KeyboardEvent) {
      if (event.key !== "Escape" || runtimeConfigSaving || runtimeConfigOpenFileLoading) {
        return;
      }

      event.preventDefault();
      setRuntimeConfigRuntimeId(null);
      setRuntimeConfigError(undefined);
    }

    document.addEventListener("keydown", handleKeydown);
    return () => document.removeEventListener("keydown", handleKeydown);
  }, [runtimeConfigOpenFileLoading, runtimeConfigRuntimeId, runtimeConfigSaving]);

  useEffect(() => {
    if (!pendingPhpExtensionRemoval) {
      return;
    }

    const extensionName = pendingPhpExtensionRemoval.extensionName;

    function handleKeydown(event: KeyboardEvent) {
      if (event.key !== "Escape" || actionLoading === `php-extension-remove:${extensionName}`) {
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
          message: "Managed cloudflared auth cert is ready. Create or select a tunnel next.",
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
      const setup = await persistentTunnelApi.deleteNamedTunnel(tunnel.tunnelId);
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
          nextRuntime.runtimeType === "php" && nextRuntime.details ? "warning" : "success",
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
        message: getAppErrorMessage(invokeError, "Failed to set the active runtime."),
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
          (service.name === "apache" || service.name === "nginx") && service.status === "running",
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
        title: runtime.source === "external" ? "Runtime reference removed" : "Runtime uninstalled",
        message: `${runtimeTypeLabel(runtime.runtimeType)} ${runtime.version} was removed from DevNest.`,
      });
    } catch (invokeError) {
      const details =
        typeof invokeError === "object" && invokeError !== null && "details" in invokeError
          ? String((invokeError as AppError).details ?? "")
          : "";
      const baseMessage = getAppErrorMessage(invokeError, "Failed to remove the runtime reference.");
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
        message: getAppErrorMessage(invokeError, "Failed to open the runtime path in Explorer."),
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
        getAppErrorMessage(invokeError, "Failed to load the managed runtime config."),
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
      const nextValues = await runtimeApi.updateConfig(runtimeConfigRuntimeId, patch);
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
        (runtime) => runtime.runtimeType === runtimePackage.runtimeType && runtime.isActive,
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
      const runtime = await runtimeApi.installPackage(runtimePackage.id, shouldSetActive);
      await loadRuntimeInventory();
      await loadInstallTask();
      pushToast({
        tone:
          runtime.runtimeType === "php" && runtime.details ? "warning" : "success",
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
        typeof invokeError === "object" && invokeError !== null && "details" in invokeError
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
        message: getAppErrorMessage(invokeError, "Failed to update the PHP extension state."),
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

  async function handleInstallPhpExtensionPackage(extensionPackage: PhpExtensionPackage) {
    const runtimeId = phpToolsRuntimeId ?? selectedPhpRuntimeId;
    if (!runtimeId) {
      return;
    }

    setActionLoading(`php-extension-package:${extensionPackage.id}`);

    try {
      const result = await runtimeApi.installPhpExtensionPackage(runtimeId, extensionPackage.id);
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
        message: getAppErrorMessage(invokeError, "Failed to update the PHP function state."),
      });
    } finally {
      setActionLoading(null);
    }
  }

  async function handleInstallOptionalToolPackage(optionalToolPackage: OptionalToolPackage) {
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
      const installedTool = await optionalToolApi.installPackage(optionalToolPackage.id);
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
        typeof invokeError === "object" && invokeError !== null && "details" in invokeError
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
        typeof invokeError === "object" && invokeError !== null && "details" in invokeError
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

  const runtimeTypeOrder: RuntimeType[] = ["php", "apache", "nginx", "frankenphp", "mysql"];
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
          runtimeTypeOrder.indexOf(left.runtimeType) - runtimeTypeOrder.indexOf(right.runtimeType);
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
          runtimeTypeOrder.indexOf(left.runtimeType) - runtimeTypeOrder.indexOf(right.runtimeType);
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
          optionalToolTypeOrder.indexOf(left.toolType) - optionalToolTypeOrder.indexOf(right.toolType);
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
          optionalToolTypeOrder.indexOf(left.toolType) - optionalToolTypeOrder.indexOf(right.toolType);
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
              runtimeCatalogKey(runtime.runtimeType, runtime.version, runtime.phpFamily),
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
            const nextPackage = findRuntimeUpdatePackage(runtime, runtimePackages);
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
          .filter((entry): entry is readonly [string, RuntimePackage] => entry[1] !== null),
      ),
    [installedPackageRuntimeIds, runtimePackages, sortedRuntimes],
  );
  const installedOptionalToolIds = useMemo(
    () =>
      new Map(
        optionalTools.map(
          (tool) =>
            [`${tool.toolType}:${normalizeCatalogVersion(tool.version)}`, tool] as const,
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
              [tool.id, findOptionalToolUpdatePackage(tool, optionalToolPackages)] as const,
          )
          .filter((entry): entry is readonly [string, OptionalToolPackage] => entry[1] !== null),
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
    phpRuntimes.find((runtime) => runtime.id === activePhpToolsRuntimeId) ?? null;
  const selectedRuntimeConfigRuntime =
    sortedRuntimes.find((runtime) => runtime.id === runtimeConfigRuntimeId) ?? null;
  const phpToolsSearchQuery = phpToolsSearch.trim().toLowerCase();
  const recommendedPhpExtensions = useMemo(() => {
    const installedExtensions = new Map(
      phpExtensions.map((extension) => [extension.extensionName, extension] as const),
    );
    const packagesByExtension = new Map(
      phpExtensionPackages.map((extensionPackage) => [
        extensionPackage.extensionName,
        extensionPackage,
      ] as const),
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
        phpExtensionPackages.map((extensionPackage) => [
          extensionPackage.extensionName,
          extensionPackage,
        ] as const),
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
        ({ installedState, extensionPackage }) => installedState === null && extensionPackage !== null,
      ),
    [recommendedPhpExtensions],
  );
  const missingBundledPhpExtensionRecommendations = useMemo(
    () =>
      recommendedPhpExtensions.filter(
        ({ installedState, extensionPackage, spec }) =>
          installedState === null && extensionPackage === null && spec.source === "bundled",
      ),
    [recommendedPhpExtensions],
  );
  const enabledPhpFunctions = useMemo(
    () => filteredPhpFunctions.filter((functionState) => functionState.enabled),
    [filteredPhpFunctions],
  );
  const disabledPhpFunctions = useMemo(
    () => filteredPhpFunctions.filter((functionState) => !functionState.enabled),
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
    pendingRuntimeRemoval?.source === "external" ? "Remove Runtime" : "Uninstall Runtime";
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
    () => sortedPackages.filter((runtimePackage) => runtimePackage.runtimeType === "php"),
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
    () => sortedPackages.filter((runtimePackage) => runtimePackage.runtimeType === "mysql"),
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
      (runtime.runtimeType === "apache" || runtime.runtimeType === "nginx") && runtime.isActive
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
      if (runtime.runtimeType === "php" || runtime.runtimeType === "frankenphp") {
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
                <ActionMenuItem onClick={() => void handleOpenRuntimeConfigFile(runtime)}>
                  {runtimeConfigOpenFileLoading && runtimeConfigOpenFileRuntimeId === runtime.id
                    ? "Opening file..."
                    : "Open file"}
                </ActionMenuItem>
              ) : null}
              <ActionMenuItem onClick={() => void handleRevealRuntime(runtime)}>
                {actionLoading === `reveal:${runtime.id}` ? "Opening folder..." : "Open folder"}
              </ActionMenuItem>
              {actionableUpdatePackage ? (
                <ActionMenuItem
                  onClick={() =>
                    void handleInstallPackage(actionableUpdatePackage, runtime.isActive)
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
                {actionLoading === `activate:${runtime.id}` ? "Setting..." : "Set active"}
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
              <ActionMenuItem onClick={() => setPendingRuntimeRemoval(runtime)} tone="danger">
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
              <ActionMenuItem onClick={() => void handleOpenRuntimeConfigFile(runtime)}>
                {runtimeConfigOpenFileLoading && runtimeConfigOpenFileRuntimeId === runtime.id
                  ? "Opening file..."
                  : "Open file"}
              </ActionMenuItem>
            ) : null}
            <ActionMenuItem onClick={() => void handleRevealRuntime(runtime)}>
              {actionLoading === `reveal:${runtime.id}` ? "Opening folder..." : "Open folder"}
            </ActionMenuItem>
            {actionableUpdatePackage ? (
              <ActionMenuItem
                onClick={() =>
                  void handleInstallPackage(actionableUpdatePackage, runtime.isActive)
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
              {actionLoading === `activate:${runtime.id}` ? "Setting..." : "Set active"}
            </ActionMenuItem>
            <ActionMenuItem onClick={() => setPendingRuntimeRemoval(runtime)} tone="danger">
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
                DevNest currently reuses one managed data directory for all installed MariaDB/MySQL runtimes. Version switches keep the same databases and recovery files, so older builds can fail to start until the previous data state is shut down cleanly, backed up, or moved aside.
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
                    const updatePackage = runtimeUpdatePackages.get(runtime.id) ?? null;
                    const higherInstalledRuntime =
                      [...familyRuntimes]
                        .filter((candidate) => runtimeCanOfferUpdateTo(runtime, candidate))
                        .sort((left, right) => compareRuntimeVersions(right.version, left.version))[0] ??
                      null;
                    const actionableUpdatePackage =
                      higherInstalledRuntime === null ? updatePackage : null;
                    const status = runtimeStatusPresentation(runtime);

                    return (
                      <tr key={runtime.id}>
                        <td>
                          <div className="runtime-table-type">
                            <strong>{runtimeTypeLabel(runtime.runtimeType)}</strong>
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
                                  data-tone={runtime.phpFamily ? "success" : "warning"}
                                >
                                  {runtime.phpFamily ? `PHP ${runtime.phpFamily}` : "Unknown"}
                                </span>
                                <span className="runtime-table-note">
                                  Embedded in the selected FrankenPHP binary.
                                </span>
                              </div>
                            ) : (
                              <span className="runtime-table-note">Uses linked PHP runtime</span>
                            )}
                          </td>
                        ) : null}
                        <td>
                          <div className="runtime-status-copy">
                            <span className="status-chip" data-tone={status.tone}>
                              {status.label}
                            </span>
                            {runtime.details ? (
                              <span className="helper-text">{runtime.details}</span>
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
                                {actionableUpdatePackage.displayName} is ready to install.
                              </span>
                            </div>
                          ) : higherInstalledRuntime ? (
                            <div className="runtime-status-copy">
                              <span className="status-chip" data-tone="success">
                                Installed
                              </span>
                              <span className="runtime-table-note">
                                {runtimeTypeLabel(runtime.runtimeType)} {higherInstalledRuntime.version} is already installed.
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
                          {renderInstalledRuntimeActions(runtime, actionableUpdatePackage)}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          ) : (
            <EmptyState
              title={loading ? `Loading ${installedTitle.toLowerCase()}` : emptyInstalledTitle}
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
                      installTask?.packageId === runtimePackage.id ? installTask : null;

                    return (
                      <tr key={runtimePackage.id}>
                        <td>
                          <div className="runtime-table-type">
                            <strong>{runtimePackage.displayName}</strong>
                            <span className="runtime-table-note">
                              {runtimeTypeLabel(runtimePackage.runtimeType)}
                            </span>
                            <span className="mono">{runtimePackage.entryBinary}</span>
                          </div>
                        </td>
                        <td>{runtimePackage.version}</td>
                        {family === "web" ? (
                          <td>
                            {runtimePackage.runtimeType === "frankenphp" ? (
                              <div className="runtime-status-copy">
                                <span
                                  className="status-chip"
                                  data-tone={runtimePackage.phpFamily ? "success" : "warning"}
                                >
                                  {runtimePackage.phpFamily
                                    ? `PHP ${runtimePackage.phpFamily}`
                                    : "Unknown"}
                                </span>
                                <span className="runtime-table-note">
                                  Select a build matching the projects that will use FrankenPHP.
                                </span>
                              </div>
                            ) : (
                              <span className="runtime-table-note">External PHP runtime</span>
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
                                  : currentTask?.stage === "completed" || installedRuntime
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
                              disabled={loading || packagesLoading || actionLoading !== null}
                              onClick={() => void handleInstallPackage(runtimePackage)}
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
                                disabled={loading || packagesLoading || actionLoading !== null}
                                onClick={() => setPendingRuntimeRemoval(installedRuntime)}
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
              title={packagesLoading ? `Loading ${downloadsTitle.toLowerCase()}` : emptyDownloadsTitle}
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
  function handleSelectTab(tab: "general" | "php" | "web" | "database" | "tunnel" | "tools") {
    setSearchParams(mergeSearchParams(searchParams, { tab: tab === "general" ? undefined : tab }));
  }

  const appUpdateTone =
    appUpdateState === "failed"
      ? "error"
      : appUpdateState === "updateAvailable" || appUpdateState === "restartRequired"
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
            void loadRuntimeInventory();
            void loadRuntimePackages();
            void loadOptionalToolInventory();
            void loadOptionalToolPackages();
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
      {optionalToolError ? <span className="error-text">{optionalToolError}</span> : null}
      {optionalToolPackageError ? <span className="error-text">{optionalToolPackageError}</span> : null}
      {persistentTunnelError ? <span className="error-text">{persistentTunnelError}</span> : null}
      {updateError && appUpdateState !== "failed" ? <span className="error-text">{updateError}</span> : null}

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
              <span className="helper-text">{optionalToolInstallTask.message}</span>
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
                <p>Check the signed Windows release feed, then install packaged updates without leaving DevNest.</p>
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
                  {appUpdateState === "checking" ? "Checking..." : "Check Updates"}
                </Button>
                {appUpdateState === "updateAvailable" ||
                appUpdateState === "downloading" ||
                appUpdateState === "installing" ? (
                  <>
                    <Button
                      disabled={appUpdateState === "downloading" || appUpdateState === "installing"}
                      onClick={() => void handleInstallUpdate()}
                      variant="primary"
                    >
                      {appUpdateState === "downloading"
                        ? "Downloading..."
                        : appUpdateState === "installing"
                          ? "Installing..."
                          : "Download and Install"}
                    </Button>
                    <Button onClick={handleDismissAvailableUpdate}>Later</Button>
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
                    data-tone={releaseInfo?.updaterConfigured ? "success" : "warning"}
                  >
                    {releaseInfo?.updaterConfigured ? "embedded" : "missing"}
                  </span>
                </strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">Last Checked</span>
                <strong>{lastUpdateCheckAt ? formatUpdatedAt(lastUpdateCheckAt) : "Not checked yet"}</strong>
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
                  {appUpdateState === "updateAvailable" && updateResult?.latestVersion
                    ? `DevNest ${updateResult.latestVersion} is available`
                    : "Manual update flow"}
                </strong>
                <span>{updateSummary}</span>
              </div>
              {updateResult?.pubDate ? (
                <span className="helper-text">Published {formatUpdatedAt(updateResult.pubDate)}</span>
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
            <p>Connect Cloudflare once, choose the one shared named tunnel DevNest should use, then set the default zone projects will publish under when you leave the hostname blank.</p>
          </div>
          <div className="page-toolbar">
            <Button
              disabled={actionLoading !== null}
              onClick={() => void loadPersistentTunnelSetup()}
            >
              {loading || persistentTunnelSetupLoading ? "Refreshing..." : "Refresh Setup"}
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
              className={persistentTunnelSetup?.tunnelId ? "button-danger" : undefined}
              disabled={actionLoading !== null || !persistentTunnelSetup?.managed}
              onClick={() => setDisconnectPersistentTunnelConfirm(true)}
            >
              {actionLoading === "persistent-disconnect" ? "Disconnecting..." : "Disconnect"}
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
                    data-tone={persistentTunnelProjectReady ? "success" : "warning"}
                  >
                    {persistentTunnelProjectReady ? "ready" : "needs setup"}
                  </span>
                </strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">Connection</span>
                <strong>
                  <span
                    className="status-chip"
                    data-tone={persistentTunnelSetup.authCertPath ? "success" : "warning"}
                  >
                    {persistentTunnelSetup.authCertPath ? "connected" : "not connected"}
                  </span>
                </strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">Shared Tunnel</span>
                <strong>
                  <span
                    className="status-chip"
                    data-tone={persistentTunnelSharedTunnelReady ? "success" : "warning"}
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
                  {persistentTunnelSetup.defaultHostnameZone ?? "Not configured"}
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
                  onChange={(event) => setCreateTunnelName(event.target.value)}
                  placeholder="devnest-main"
                  type="text"
                  value={createTunnelName}
                />
                <span className="helper-text">
                  Create one DevNest-owned shared tunnel, then reuse it across published projects through managed ingress rules.
                </span>
                <div className="page-toolbar" style={{ marginTop: 12 }}>
                  <Button
                    disabled={actionLoading !== null || !persistentTunnelSetup.authCertPath}
                    onClick={() => void handleCreatePersistentNamedTunnel()}
                    variant="primary"
                  >
                    {actionLoading === "persistent-create-tunnel" ? "Creating..." : "Create Tunnel"}
                  </Button>
                </div>
                {persistentTunnelSetup.tunnelName ? (
                  <span className="helper-text" style={{ marginTop: 8 }}>
                    Current shared tunnel: <span className="mono">{persistentTunnelSetup.tunnelName}</span>
                  </span>
                ) : null}
              </div>
              <div className="detail-item">
                <span className="detail-label">Default Public Zone</span>
                <input
                  className="input mono"
                  onChange={(event) => setDefaultHostnameZone(event.target.value)}
                  placeholder="preview.example.com"
                  type="text"
                  value={defaultHostnameZone}
                />
                <span className="helper-text">
                  Projects that leave the hostname blank publish as `project-name.{defaultHostnameZone || "your-zone"}`.
                </span>
                <div className="page-toolbar" style={{ marginTop: 12 }}>
                  <Button
                    disabled={actionLoading !== null || !persistentTunnelSetup.authCertPath}
                    onClick={() => void handleSavePersistentTunnelZone()}
                  >
                    {actionLoading === "persistent-save-zone" ? "Saving..." : "Set Default Zone"}
                  </Button>
                </div>
              </div>
            </div>

            <div className="detail-grid" style={{ marginTop: 16 }}>
              <div className="detail-item">
                <span className="detail-label">Advanced Recovery</span>
                <span className="helper-text">
                  Only use these if you already have a Cloudflare auth cert or a named tunnel credentials JSON that DevNest should adopt.
                </span>
                <div className="page-toolbar" style={{ marginTop: 12 }}>
                  <Button
                    disabled={actionLoading !== null}
                    onClick={() => void handleImportPersistentTunnelAuthCert()}
                  >
                    {actionLoading === "persistent-import-cert" ? "Importing..." : "Import Cert"}
                  </Button>
                  <Button
                    disabled={actionLoading !== null}
                    onClick={() => void handleImportPersistentTunnelCredentials()}
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
                <span className="helper-text" style={{ marginTop: 10 }}>Credentials</span>
                <strong className="mono detail-value">
                  {persistentTunnelSetup.credentialsPath ?? "Not found"}
                </strong>
              </div>
            </div>

            {namedTunnels.length > 0 ? (
              <div className="runtime-table-shell" style={{ marginTop: 16 }}>
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
                            <span className="helper-text mono">{tunnel.tunnelId}</span>
                          </div>
                        </td>
                        <td>{tunnel.credentialsPath ? "Managed" : "Cloudflare account"}</td>
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
                                disabled={actionLoading !== null || tunnel.selected}
                                onClick={() => void handleSelectPersistentNamedTunnel(tunnel)}
                                variant={tunnel.selected ? undefined : "primary"}
                              >
                                {actionLoading === `persistent-select:${tunnel.tunnelId}`
                                  ? "Selecting..."
                                  : tunnel.selected
                                    ? "Selected"
                                    : "Use"}
                              </Button>
                            ) : (
                              <Button
                                disabled={actionLoading !== null}
                                onClick={() => void handleImportPersistentTunnelCredentials()}
                              >
                                {actionLoading === "persistent-import-credentials"
                                  ? "Importing..."
                                  : "Import Credentials"}
                              </Button>
                            )}
                            <Button
                              className="button-danger"
                              disabled={actionLoading !== null}
                              onClick={() => setPendingPersistentTunnelDeletion(tunnel)}
                            >
                              {actionLoading === `persistent-delete:${tunnel.tunnelId}`
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
              Delete removes a tunnel from Cloudflare. Disconnect clears DevNest's managed setup only and does not delete remote tunnels from your Cloudflare account.
            </span>
            <span className="helper-text" style={{ marginTop: 6 }}>
              If a tunnel shows `Needs Credentials`, DevNest can see it in your Cloudflare account but still needs that tunnel's JSON imported into this workspace before it can be selected.
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
      <Card>
        <div className="page-header">
          <div>
            <h2>Installed Optional Tools</h2>
            <p>Manage additional tools that enhance your development environment.</p>
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
                  const updatePackage = optionalToolUpdatePackages.get(tool.id) ?? null;

                  return (
                    <tr key={tool.id}>
                      <td>
                        <div className="runtime-table-type">
                          <strong>{optionalToolLabel(tool.toolType)}</strong>
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
                          {tool.details ? <span className="helper-text">{tool.details}</span> : null}
                        </div>
                      </td>
                      <td>
                        {updatePackage ? (
                          <div className="runtime-status-copy">
                            <span className="status-chip" data-tone="warning">
                              Update
                            </span>
                            <span className="runtime-table-note">
                              {updatePackage.displayName} is available.
                            </span>
                          </div>
                        ) : (
                          <div className="runtime-status-copy">
                            <span className="status-chip" data-tone="success">
                              Current
                            </span>
                            <span className="runtime-table-note">Installed package is current.</span>
                          </div>
                        )}
                      </td>
                      <td>{formatUpdatedAt(tool.updatedAt)}</td>
                      <td>
                        <div className="runtime-table-actions">
                          <Button
                            disabled={actionLoading !== null}
                            onClick={() => void handleRevealOptionalTool(tool)}
                          >
                            {actionLoading === `optional-reveal:${tool.id}` ? "Opening..." : "Open"}
                          </Button>
                          {updatePackage ? (
                            <Button
                              disabled={packagesLoading || actionLoading !== null}
                              onClick={() => void handleInstallOptionalToolPackage(updatePackage)}
                              variant="primary"
                            >
                              {actionLoading === `optional-install:${updatePackage.id}`
                                ? "Updating..."
                                : "Update"}
                            </Button>
                          ) : null}
                          <Button
                            className="button-danger"
                            disabled={actionLoading !== null}
                            onClick={() => setPendingOptionalToolRemoval(tool)}
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
            <p>Download and install additional tools to enhance your development environment.</p>
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
                          <strong>{optionalToolLabel(toolPackage.toolType)}</strong>
                          <span className="runtime-table-note">
                            {optionalToolFamilyLabel(toolPackage.toolType)}
                          </span>
                        </div>
                      </td>
                      <td>
                        <div className="runtime-table-type">
                          <strong>{toolPackage.displayName}</strong>
                          <span className="mono">{toolPackage.entryBinary}</span>
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
                                : currentTask?.stage === "completed" || installedTool
                                  ? "success"
                                  : "warning"
                            }
                          >
                            {currentTask
                              ? optionalToolInstallStageLabel(currentTask.stage)
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
                            disabled={packagesLoading || actionLoading !== null}
                            onClick={() => void handleInstallOptionalToolPackage(toolPackage)}
                            variant="primary"
                          >
                            {actionLoading === `optional-install:${toolPackage.id}`
                              ? "Installing..."
                              : installedTool
                                ? "Reinstall"
                                : "Install"}
                          </Button>
                          {installedTool ? (
                            <Button
                              className="button-danger"
                              disabled={actionLoading !== null}
                              onClick={() => setPendingOptionalToolRemoval(installedTool)}
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

      {phpToolsRuntimeId && selectedPhpRuntime ? (
        <div
          className="wizard-overlay"
          onClick={() => setPhpToolsRuntimeId(null)}
          role="dialog"
          aria-modal="true"
        >
          <div className="runtime-tools-dialog" onClick={(event) => event.stopPropagation()}>
            <div className="runtime-tools-header">
              <div>
                <h2>PHP Tools</h2>
                <p>
                  Manage extensions and guarded functions for one tracked PHP or
                  FrankenPHP runtime without leaving the Installed Runtimes table.
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
                      {actionLoading === `php-extension-install:${activePhpToolsRuntimeId}`
                        ? "Importing..."
                        : "Import Local"}
                    </Button>
                    <Button
                      disabled={phpExtensionsLoading || phpExtensionPackagesLoading || actionLoading !== null}
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
                    onClick={() => void loadPhpFunctions(activePhpToolsRuntimeId)}
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
                {phpExtensionsError ? <span className="error-text">{phpExtensionsError}</span> : null}
                {phpExtensionPackagesError ? (
                  <span className="error-text">{phpExtensionPackagesError}</span>
                ) : null}
                <section className="runtime-tools-overview">
                  <article className="runtime-tools-stat">
                    <span className="runtime-tools-stat-label">Enabled Now</span>
                    <strong>{enabledPhpExtensions.length}</strong>
                    <p>Extensions currently active in DevNest-managed `php.ini` for this runtime.</p>
                  </article>
                  <article className="runtime-tools-stat">
                    <span className="runtime-tools-stat-label">Available to Review</span>
                    <strong>{disabledPhpExtensions.length}</strong>
                    <p>Tracked DLLs already in this runtime but still disabled or intentionally held back.</p>
                  </article>
                  <article className="runtime-tools-stat">
                    <span className="runtime-tools-stat-label">Install More</span>
                    <strong>{installablePhpExtensionRecommendations.length}</strong>
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
                        Keep the currently active capabilities visible first so this screen answers
                        "what is loaded right now?" before anything else.
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
                              RECOMMENDED_PHP_EXTENSION_BY_NAME.get(extension.extensionName) ?? null;
                            const extensionPackage =
                              phpExtensionPackagesByName.get(extension.extensionName) ?? null;

                            return (
                              <tr key={`${extension.runtimeId}:${extension.extensionName}`}>
                                <td>
                                  <div className="runtime-table-type">
                                    <strong>{phpExtensionLabel(extension.extensionName)}</strong>
                                    <span className="runtime-table-note">{extension.extensionName}</span>
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
                                      {recommendedSpec?.summary ?? "Tracked extension already active."}
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
                                      disabled={phpExtensionsLoading || actionLoading !== null}
                                      onClick={() => void handleTogglePhpExtension(extension)}
                                    >
                                      {actionLoading === `php-extension:${extension.extensionName}`
                                        ? "Saving..."
                                        : "Disable"}
                                    </Button>
                                    <Button
                                      className="button-danger"
                                      disabled={phpExtensionsLoading || actionLoading !== null}
                                      onClick={() => setPendingPhpExtensionRemoval(extension)}
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
                        These DLLs are already present in the runtime. Turn them on here or leave
                        them off if the runtime policy should stay tighter.
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
                            RECOMMENDED_PHP_EXTENSION_BY_NAME.get(extension.extensionName) ?? null;
                          const extensionPackage =
                            phpExtensionPackagesByName.get(extension.extensionName) ?? null;
                          const disabledByDefault = isPhpExtensionDisabledByDefault(
                            extension.extensionName,
                          );

                          return (
                          <tr key={`${extension.runtimeId}:${extension.extensionName}`}>
                            <td>
                              <div className="runtime-table-type">
                                <strong>{phpExtensionLabel(extension.extensionName)}</strong>
                                <span className="runtime-table-note">
                                  {recommendedSpec?.summary ?? extension.extensionName}
                                </span>
                              </div>
                            </td>
                            <td>
                              <div className="runtime-table-type">
                                <strong className="mono">{extension.dllFile}</strong>
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
                                <strong>{formatUpdatedAt(extension.updatedAt)}</strong>
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
                                  disabled={phpExtensionsLoading || actionLoading !== null}
                                  onClick={() => void handleTogglePhpExtension(extension)}
                                >
                                  {actionLoading === `php-extension:${extension.extensionName}`
                                    ? "Saving..."
                                    : "Enable"}
                                </Button>
                                <Button
                                  className="button-danger"
                                  disabled={phpExtensionsLoading || actionLoading !== null}
                                  onClick={() => setPendingPhpExtensionRemoval(extension)}
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
                                  <tr key={`php-extension-package:${extensionPackage.id}`}>
                                    <td>
                                      <div className="runtime-table-type">
                                        <strong>{phpExtensionLabel(spec.extensionName)}</strong>
                                        <span className="runtime-table-note">{spec.summary}</span>
                                      </div>
                                    </td>
                                    <td>
                                      <div className="runtime-tools-pill-row">
                                        <span className="runtime-tools-pill runtime-tools-pill-success">
                                          Ready to install
                                        </span>
                                        <span className="runtime-tools-pill">
                                          {extensionPackage.packageKind === "zip"
                                            ? "ZIP package"
                                            : "Binary package"}
                                        </span>
                                      </div>
                                    </td>
                                    <td>
                                      <div className="runtime-table-type">
                                        <strong>{extensionPackage.displayName}</strong>
                                        <span className="runtime-table-note">
                                          {extensionPackage.notes ??
                                            "DevNest can download this package into the runtime ext directory."}
                                        </span>
                                      </div>
                                    </td>
                                    <td>
                                      <div className="runtime-table-actions">
                                        <Button
                                          disabled={phpExtensionPackagesLoading || actionLoading !== null}
                                          onClick={() =>
                                            void handleInstallPhpExtensionPackage(extensionPackage)
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
                            {missingBundledPhpExtensionRecommendations.map(({ spec }) => (
                              <tr key={`recommended-missing:${spec.extensionName}`}>
                                <td>
                                  <div className="runtime-table-type">
                                    <strong>{phpExtensionLabel(spec.extensionName)}</strong>
                                    <span className="runtime-table-note">{spec.summary}</span>
                                  </div>
                                </td>
                                <td>
                                  <div className="runtime-tools-pill-row">
                                    <span className="runtime-tools-pill runtime-tools-pill-warning">
                                      Not in runtime
                                    </span>
                                    <span className="runtime-tools-pill">Bundled DLL</span>
                                  </div>
                                </td>
                                <td>
                                  <div className="runtime-table-type">
                                    <strong>Bring in a compatible local DLL</strong>
                                    <span className="runtime-table-note">
                                      DevNest expects this extension to come from the PHP runtime
                                      bundle. If your build does not ship it, use Import Local.
                                    </span>
                                  </div>
                                </td>
                                <td>
                                  <div className="runtime-table-actions">
                                    <span className="helper-text">Use Import Local</span>
                                  </div>
                                </td>
                              </tr>
                            ))}
                          </>
                        ) : (
                          <tr>
                            <td colSpan={4}>
                              <span className="helper-text">
                                No install candidates matched this search. Try another keyword or
                                use Import Local for a custom DLL.
                              </span>
                            </td>
                          </tr>
                        )}
                      </tbody>
                    </table>
                  </div>
                </section>
                <span className="helper-text runtime-tools-note">
                  Extension changes land in DevNest-managed `php.ini`. Restart the linked Apache,
                  Nginx, FrankenPHP, or long-running PHP worker after enabling, disabling, or
                  installing one.
                </span>
              </div>
            ) : (
              <div
                aria-labelledby="php-tools-tab-policy"
                className="workspace-panel runtime-tools-panel"
                id="php-tools-panel-policy"
                role="tabpanel"
              >
                {phpFunctionsError ? <span className="error-text">{phpFunctionsError}</span> : null}
                <section className="runtime-tools-overview">
                  <article className="runtime-tools-stat">
                    <span className="runtime-tools-stat-label">Allowed</span>
                    <strong>{enabledPhpFunctions.length}</strong>
                    <p>Functions currently allowed to execute in this runtime.</p>
                  </article>
                  <article className="runtime-tools-stat">
                    <span className="runtime-tools-stat-label">Restricted</span>
                    <strong>{disabledPhpFunctions.length}</strong>
                    <p>Functions written into `disable_functions` for safer or cleaner project defaults.</p>
                  </article>
                  <article className="runtime-tools-stat">
                    <span className="runtime-tools-stat-label">Scope</span>
                    <strong>Per Runtime</strong>
                    <p>These guards apply to the selected PHP runtime, not to a single project only.</p>
                  </article>
                </section>
                <section className="runtime-tools-section">
                  <div className="runtime-tools-section-head">
                    <div className="runtime-tools-section-copy">
                      <h3>Restricted Functions</h3>
                      <span className="helper-text">
                        This is runtime policy, not extension inventory. Use it to control what PHP
                        functions stay callable in DevNest-managed environments.
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
                          <tr key={`${functionState.runtimeId}:${functionState.functionName}`}>
                            <td>
                              <div className="runtime-table-type">
                                <strong>{phpExtensionLabel(functionState.functionName)}</strong>
                                <span className="runtime-table-note">{functionState.functionName}</span>
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
                                  {functionState.enabled ? "Allowed" : "Restricted"}
                                </span>
                              </div>
                            </td>
                            <td>
                              <div className="runtime-table-type">
                                <strong>{formatUpdatedAt(functionState.updatedAt)}</strong>
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
                                  disabled={phpFunctionsLoading || actionLoading !== null}
                                  onClick={() => void handleTogglePhpFunction(functionState)}
                                >
                                  {actionLoading === `php-function:${functionState.functionName}`
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
                  Runtime policy changes write into `disable_functions`. Restart the linked Apache,
                  Nginx, FrankenPHP, or long-running PHP worker after changing these guards.
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
                <div className="confirm-dialog" onClick={(event) => event.stopPropagation()}>
                  <div className="confirm-dialog-copy">
                    <h3>Uninstall PHP extension?</h3>
                    <p>
                      This will remove{" "}
                      <strong>{phpExtensionLabel(pendingPhpExtensionRemoval.extensionName)}</strong>{" "}
                      from {pendingPhpExtensionRemoval.runtimeVersion}.
                    </p>
                    <div className="detail-item">
                      <span className="detail-label">DLL</span>
                      <strong className="mono detail-value">
                        {pendingPhpExtensionRemoval.dllFile}
                      </strong>
                    </div>
                    <span className="helper-text">
                      DevNest removes the managed DLL from this runtime's `ext` directory and
                      clears its saved override. Restart the linked web server after uninstalling.
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
                      onClick={() => void handleRemovePhpExtension(pendingPhpExtensionRemoval)}
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
          onOpenFile={() => handleOpenRuntimeConfigFile(selectedRuntimeConfigRuntime)}
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
          <div className="confirm-dialog" onClick={(event) => event.stopPropagation()}>
            <div className="confirm-dialog-copy">
              <h3>{removalDialogTitle}</h3>
              <p>
                This will remove <strong>{runtimeTypeLabel(pendingRuntimeRemoval.runtimeType)} {pendingRuntimeRemoval.version}</strong> from the DevNest runtime registry.
              </p>
              <div className="detail-item">
                <span className="detail-label">Source</span>
                <strong>{runtimeSourceLabel(pendingRuntimeRemoval.source)}</strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">Path</span>
                <strong className="mono detail-value">{pendingRuntimeRemoval.path}</strong>
              </div>
              <span className="helper-text">
                Imported runtimes inside the managed DevNest folder will also have their copied runtime files removed. External runtimes on your machine are not deleted.
              </span>
            </div>
            <div className="confirm-dialog-actions">
              <Button
                disabled={actionLoading === `remove:${pendingRuntimeRemoval.id}`}
                onClick={() => setPendingRuntimeRemoval(null)}
              >
                Cancel
              </Button>
              <Button
                className="button-danger"
                disabled={actionLoading === `remove:${pendingRuntimeRemoval.id}`}
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
            if (actionLoading !== `optional-remove:${pendingOptionalToolRemoval.id}`) {
              setPendingOptionalToolRemoval(null);
            }
          }}
          role="dialog"
          aria-modal="true"
        >
          <div className="confirm-dialog" onClick={(event) => event.stopPropagation()}>
            <div className="confirm-dialog-copy">
              <h3>Uninstall optional tool?</h3>
              <p>
                This will remove <strong>{optionalToolLabel(pendingOptionalToolRemoval.toolType)} {displayCatalogVersion(pendingOptionalToolRemoval.version)}</strong> from the DevNest optional tools inventory.
              </p>
              <div className="detail-item">
                <span className="detail-label">Path</span>
                <strong className="mono detail-value">{pendingOptionalToolRemoval.path}</strong>
              </div>
              <span className="helper-text">
                DevNest only removes the managed install it owns. If the tool is currently in use, uninstall stays blocked until you stop the related service or tunnel first.
              </span>
            </div>
            <div className="confirm-dialog-actions">
              <Button
                disabled={actionLoading === `optional-remove:${pendingOptionalToolRemoval.id}`}
                onClick={() => setPendingOptionalToolRemoval(null)}
              >
                Cancel
              </Button>
              <Button
                className="button-danger"
                disabled={actionLoading === `optional-remove:${pendingOptionalToolRemoval.id}`}
                onClick={() => void handleRemoveOptionalTool(pendingOptionalToolRemoval)}
              >
                {actionLoading === `optional-remove:${pendingOptionalToolRemoval.id}`
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
            if (actionLoading !== `persistent-delete:${pendingPersistentTunnelDeletion.tunnelId}`) {
              setPendingPersistentTunnelDeletion(null);
            }
          }}
          role="dialog"
          aria-modal="true"
        >
          <div className="confirm-dialog" onClick={(event) => event.stopPropagation()}>
            <div className="confirm-dialog-copy">
              <h3>Delete Named Tunnel?</h3>
              <p>
                DevNest will delete{" "}
                <strong>{pendingPersistentTunnelDeletion.tunnelName}</strong> from Cloudflare and
                remove its managed credentials from this app.
              </p>
              <div className="detail-item">
                <span className="detail-label">Tunnel ID</span>
                <strong className="mono detail-value">
                  {pendingPersistentTunnelDeletion.tunnelId}
                </strong>
              </div>
              <span className="helper-text">
                If projects are still using the selected shared tunnel, stop them or delete their hostname first.
              </span>
            </div>
            <div className="confirm-dialog-actions">
              <Button
                disabled={actionLoading === `persistent-delete:${pendingPersistentTunnelDeletion.tunnelId}`}
                onClick={() => setPendingPersistentTunnelDeletion(null)}
              >
                Cancel
              </Button>
              <Button
                className="button-danger"
                disabled={actionLoading === `persistent-delete:${pendingPersistentTunnelDeletion.tunnelId}`}
                onClick={() => void handleDeletePersistentNamedTunnel(pendingPersistentTunnelDeletion)}
              >
                {actionLoading === `persistent-delete:${pendingPersistentTunnelDeletion.tunnelId}`
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
          <div className="confirm-dialog" onClick={(event) => event.stopPropagation()}>
            <div className="confirm-dialog-copy">
              <h3>Disconnect Cloudflare Setup?</h3>
              <p>
                DevNest will remove the managed auth cert, named tunnel credentials, and selected tunnel identity from this app.
              </p>
              <span className="helper-text">
                This does not delete projects or managed tunnel credentials. It only disconnects Cloudflare auth until you connect again.
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
                {actionLoading === "persistent-disconnect" ? "Disconnecting..." : "Disconnect"}
              </Button>
            </div>
          </div>
        </div>
      ) : null}
    </PageLayout>
  );
}

function RecipesRoute() {
  return (
    <PageLayout
      subtitle="Create a new project from a recipe or clone a repository and register it in one pass."
      title="Recipes"
    >
      <RecipeStudio />
    </PageLayout>
  );
}

function ReliabilityRoute() {
  const [searchParams] = useSearchParams();

  return (
    <PageLayout
      subtitle="Recovery tools, safety checks, state inspection, and metadata backup for the current workspace."
      title="Reliability"
    >
      <ReliabilityWorkbench projectId={searchParams.get("projectId")} />
    </PageLayout>
  );
}

function RootLayout() {
  return (
    <AppShell>
      <DashboardRoute />
    </AppShell>
  );
}

export const appRouter = createBrowserRouter([
  {
    path: "/",
    element: <RootLayout />,
  },
  {
    path: "/projects",
    element: (
      <AppShell>
        <ProjectsRoute />
      </AppShell>
    ),
  },
  {
    path: "/services",
    element: (
      <AppShell>
        <ServicesRoute />
      </AppShell>
    ),
  },
  {
    path: "/workers",
    element: (
      <AppShell>
        <WorkersRoute />
      </AppShell>
    ),
  },
  {
    path: "/tasks",
    element: (
      <AppShell>
        <TasksRoute />
      </AppShell>
    ),
  },
  {
    path: "/logs",
    element: (
      <AppShell>
        <LogsRoute />
      </AppShell>
    ),
  },
  {
    path: "/diagnostics",
    element: (
      <AppShell>
        <DiagnosticsRoute />
      </AppShell>
    ),
  },
  {
    path: "/reliability",
    element: (
      <AppShell>
        <ReliabilityRoute />
      </AppShell>
    ),
  },
  {
    path: "/databases",
    element: (
      <AppShell>
        <DatabasesRoute />
      </AppShell>
    ),
  },
  {
    path: "/settings",
    element: (
      <AppShell>
        <SettingsRoute />
      </AppShell>
    ),
  },
  {
    path: "/recipes",
    element: (
      <AppShell>
        <RecipesRoute />
      </AppShell>
    ),
  },
]);
