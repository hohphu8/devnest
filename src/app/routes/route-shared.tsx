import { lazy, Suspense, useEffect, useMemo, useRef, useState } from "react";
import {
  createBrowserRouter,
  Outlet,
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

export const SERVICE_START_ORDER: ServiceName[] = [
  "mysql",
  "redis",
  "mailpit",
  "apache",
  "nginx",
  "frankenphp",
];
export const PROJECTS_VIEW_STORAGE_KEY = "devnest.projects.view-mode";
export const SETTINGS_UPDATE_LAST_CHECKED_KEY =
  "devnest.settings.updates.last-checked-at";
export function PageLayout({
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

export function LoadingScrim({
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

export function useDelayedBusy(active: boolean, delayMs = 160) {
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

export function getStartAllPlan(services: ServiceState[]) {
  const enabled = [...services]
    .filter((service) => service.enabled)
    .sort(
      (left, right) =>
        SERVICE_START_ORDER.indexOf(left.name) -
        SERVICE_START_ORDER.indexOf(right.name),
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

export function mergeSearchParams(
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

export function parseProjectsViewMode(
  value: string | null,
): "list" | "grid" | null {
  return value === "grid" || value === "list" ? value : null;
}

export function diagnosticActionLabel(code: string): string {
  switch (code) {
    case "LARAVEL_DOCUMENT_ROOT_MISMATCH":
    case "SSL_AUTHORITY_MISSING":
    case "SSL_TRUST_MISSING":
    case "SSL_CERTIFICATE_MISSING":
      return "Fix Now";
  }

  switch (code) {
    case "PORT_IN_USE":
    case "WSL_PORT_CONFLICT":
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

export function diagnosticCanAutoFix(code: string): boolean {
  return (
    code === "LARAVEL_DOCUMENT_ROOT_MISMATCH" ||
    code === "SSL_AUTHORITY_MISSING" ||
    code === "SSL_TRUST_MISSING" ||
    code === "SSL_CERTIFICATE_MISSING"
  );
}

export function runtimeTypeLabel(runtimeType: RuntimeType): string {
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

export function phpCliActivationMessage(version: string): string {
  return `PHP ${version} is now active.`;
}

export function withRuntimeDetails(
  message: string,
  runtime: Pick<RuntimeInventoryItem, "details">,
): string {
  return runtime.details ? `${message} ${runtime.details}` : message;
}

export function runtimeSourceLabel(
  source: RuntimeInventoryItem["source"],
): string {
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

export function runtimeFamilyLabel(runtimeType: RuntimeType): string {
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

export function runtimeInstallStageLabel(stage: RuntimeInstallStage): string {
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

export function optionalToolLabel(toolType: OptionalToolType): string {
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

export function optionalToolFamilyLabel(toolType: OptionalToolType): string {
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

export function optionalToolInstallStageLabel(
  stage: OptionalToolInstallStage,
): string {
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

export function findOptionalToolUpdatePackage(
  tool: OptionalToolInventoryItem,
  packages: OptionalToolPackage[],
): OptionalToolPackage | null {
  const installedVersion = normalizeCatalogVersion(tool.version);
  const candidates = packages.filter((toolPackage) => {
    if (toolPackage.toolType !== tool.toolType) {
      return false;
    }

    return (
      compareRuntimeVersions(
        normalizeCatalogVersion(toolPackage.version),
        installedVersion,
      ) > 0
    );
  });

  if (candidates.length === 0) {
    return null;
  }

  return [...candidates].sort((left, right) =>
    compareRuntimeVersions(
      normalizeCatalogVersion(right.version),
      normalizeCatalogVersion(left.version),
    ),
  )[0];
}

export function compareRuntimeVersions(left: string, right: string): number {
  return left.localeCompare(right, undefined, {
    numeric: true,
    sensitivity: "base",
  });
}

export function normalizeCatalogVersion(value: string): string {
  return value
    .trim()
    .replace(/^[^0-9a-z]+/i, "")
    .replace(/[^0-9a-z._-]+$/i, "")
    .replace(/^v/i, "")
    .toLowerCase();
}

export function displayCatalogVersion(value: string): string {
  return value
    .trim()
    .replace(/^[^0-9a-z]+/i, "")
    .replace(/[^0-9a-z._-]+$/i, "")
    .replace(/^v/i, "");
}

export function optionalToolHealthLabel(
  tool: OptionalToolInventoryItem,
): string {
  if (tool.status === "missing") {
    return "Missing";
  }

  return tool.isActive ? "Active install" : "Installed";
}

export function findRuntimeUpdatePackage(
  runtime: RuntimeInventoryItem,
  packages: RuntimePackage[],
): RuntimePackage | null {
  const candidates = packages.filter((runtimePackage) =>
    runtimeCanOfferUpdateTo(runtime, runtimePackage),
  );

  if (candidates.length === 0) {
    return null;
  }

  return [...candidates].sort((left, right) =>
    compareRuntimeVersions(right.version, left.version),
  )[0];
}

export function runtimeCanOfferUpdateTo(
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
    return (
      runtimeVersionFamily(candidate.version) ===
      runtimeVersionFamily(runtime.version)
    );
  }

  if (
    runtime.runtimeType === "frankenphp" &&
    runtime.phpFamily &&
    candidate.phpFamily
  ) {
    return (
      runtime.phpFamily.toLowerCase() === candidate.phpFamily.toLowerCase()
    );
  }

  return true;
}

export function runtimeCatalogKey(
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

export function serviceLabel(name: ServiceName): string {
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

export function optionalToolTypeForService(
  name?: ServiceName | null,
): OptionalToolType | null {
  if (name === "mailpit" || name === "redis") {
    return name;
  }

  return null;
}

export function phpExtensionLabel(extensionName: string): string {
  return extensionName
    .split("_")
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

export type RecommendedPhpExtensionSource = "bundled" | "download";

export interface RecommendedPhpExtensionSpec {
  extensionName: string;
  source: RecommendedPhpExtensionSource;
  summary: string;
  keywords: string[];
}

export type PhpToolsTab = "extensions" | "policy";

export const RECOMMENDED_PHP_EXTENSIONS: RecommendedPhpExtensionSpec[] = [
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
    summary:
      "PDO MySQL driver used by Laravel, Symfony, and WordPress plugins.",
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
    summary:
      "ImageMagick bindings for media pipelines and advanced transforms.",
    keywords: ["images", "media", "imagemagick"],
  },
  {
    extensionName: "xdebug",
    source: "download",
    summary: "Step debugger and profiling hooks for local PHP debugging.",
    keywords: ["debug", "profiling", "zend"],
  },
];

export const RECOMMENDED_PHP_EXTENSION_BY_NAME = new Map(
  RECOMMENDED_PHP_EXTENSIONS.map((spec) => [spec.extensionName, spec] as const),
);

export function isPhpExtensionDisabledByDefault(
  extensionName: string,
): boolean {
  return (
    extensionName === "snmp" ||
    extensionName === "pdo_firebird" ||
    extensionName === "pdo_oci" ||
    extensionName.startsWith("oci8")
  );
}

export function phpExtensionAvailabilityLabel(
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

export function phpExtensionAvailabilityNote(
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

export function matchesPhpToolsSearch(
  query: string,
  values: Array<string | null | undefined>,
): boolean {
  const normalizedQuery = query.trim().toLowerCase();
  if (normalizedQuery.length === 0) {
    return true;
  }

  return values
    .filter((value): value is string => Boolean(value))
    .some((value) => value.toLowerCase().includes(normalizedQuery));
}

export async function waitForNextPaint() {
  await new Promise<void>((resolve) => {
    if (typeof window === "undefined") {
      resolve();
      return;
    }

    window.requestAnimationFrame(() => resolve());
  });
}
