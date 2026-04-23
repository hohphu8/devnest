import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useAsyncActionPending } from "@/app/store/async-action-store";
import { useDiagnosticsStore } from "@/app/store/diagnostics-store";
import { useServiceStore } from "@/app/store/service-store";
import { useToastStore } from "@/app/store/toast-store";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { ProjectMobilePreviewModal } from "@/components/projects/project-mobile-preview-modal";
import { ProjectProvisioningPanel } from "@/components/projects/project-provisioning-panel";
import { diagnosticsApi } from "@/lib/api/diagnostics-api";
import { projectProfileApi } from "@/lib/api/project-profile-api";
import { reliabilityApi } from "@/lib/api/reliability-api";
import { projectEnvApi } from "@/lib/api/project-env-api";
import { projectApi } from "@/lib/api/project-api";
import { runtimeApi } from "@/lib/api/runtime-api";
import { summarizeDiagnostics, getLiveProjectStatus, getStatusTone } from "@/lib/project-health";
import { formatServiceLogPreview } from "@/lib/service-logs";
import { installedPhpVersionFamilies, runtimeVersionMatches } from "@/lib/runtime-version";
import { serviceApi } from "@/lib/api/service-api";
import { getAppErrorMessage } from "@/lib/tauri";
import { documentRootSchema, domainSchema, envVarKeySchema, envVarValueSchema, projectNameSchema } from "@/lib/validators";
import { formatUpdatedAt } from "@/lib/utils";
import type {
  ProjectEnvComparisonStatus,
  ProjectEnvInspection,
  ProjectEnvVar,
} from "@/types/project-env-var";
import type { ProjectMobilePreviewState } from "@/types/mobile-preview";
import type { Project, UpdateProjectPatch } from "@/types/project";
import type { RuntimeInventoryItem } from "@/types/runtime";
import { mobilePreviewApi } from "@/lib/api/mobile-preview-api";

interface ProjectInspectorProps {
  loading: boolean;
  onDelete: (projectId: string) => Promise<void>;
  onUpdate: (projectId: string, patch: UpdateProjectPatch) => Promise<unknown>;
  project?: Project;
}

type ProjectInspectorTab =
  | "overview"
  | "provisioning"
  | "runtime"
  | "workers"
  | "tasks"
  | "diagnostics"
  | "envVars"
  | "settings";

type EnvDiffFilter = "all" | ProjectEnvComparisonStatus;

function quickFixActionLabel(code: string): string {
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
    default:
      return "Open Diagnostics";
  }
}

function envComparisonLabel(status: ProjectEnvComparisonStatus): string {
  switch (status) {
    case "match":
      return "Match";
    case "onlyTracked":
      return "DevNest";
    case "onlyDisk":
      return ".env";
    case "valueMismatch":
      return "Value Mismatch";
  }
}

function envComparisonTone(status: ProjectEnvComparisonStatus): "success" | "warning" | "error" {
  switch (status) {
    case "match":
      return "success";
    case "valueMismatch":
      return "error";
    case "onlyTracked":
    case "onlyDisk":
      return "warning";
  }
}

function envDiffFilterLabel(filter: EnvDiffFilter): string {
  switch (filter) {
    case "all":
      return "All";
    case "match":
      return "Match";
    case "onlyTracked":
      return "Only in DevNest";
    case "onlyDisk":
      return "Only in .env";
    case "valueMismatch":
      return "Mismatch";
  }
}

export function ProjectInspector({
  loading,
  onDelete,
  onUpdate,
  project,
}: ProjectInspectorProps) {
  const navigate = useNavigate();
  const inspectorRootRef = useRef<HTMLDivElement | null>(null);
  const [draft, setDraft] = useState<UpdateProjectPatch>({});
  const [activeTab, setActiveTab] = useState<ProjectInspectorTab>("overview");
  const [mobilePreviewOpen, setMobilePreviewOpen] = useState(false);
  const [mobilePreviewState, setMobilePreviewState] = useState<ProjectMobilePreviewState | null>(null);
  const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false);
  const [deleteLoading, setDeleteLoading] = useState(false);
  const [message, setMessage] = useState<string>();
  const [logPreview, setLogPreview] = useState("");
  const [logPreviewError, setLogPreviewError] = useState<string>();
  const [logPreviewLoading, setLogPreviewLoading] = useState(false);
  const [envVars, setEnvVars] = useState<ProjectEnvVar[]>([]);
  const [envVarsError, setEnvVarsError] = useState<string>();
  const [envVarsLoading, setEnvVarsLoading] = useState(false);
  const [envInspection, setEnvInspection] = useState<ProjectEnvInspection | null>(null);
  const [envInspectionError, setEnvInspectionError] = useState<string>();
  const [envInspectionLoading, setEnvInspectionLoading] = useState(false);
  const [envDiffFilter, setEnvDiffFilter] = useState<EnvDiffFilter>("all");
  const [envVarDrafts, setEnvVarDrafts] = useState<Record<string, { envKey: string; envValue: string }>>({});
  const [newEnvKey, setNewEnvKey] = useState("");
  const [newEnvValue, setNewEnvValue] = useState("");
  const [creatingEnvVar, setCreatingEnvVar] = useState(false);
  const [savingEnvVarId, setSavingEnvVarId] = useState<string>();
  const [deletingEnvVarId, setDeletingEnvVarId] = useState<string>();
  const [refreshingEnvInspection, setRefreshingEnvInspection] = useState(false);
  const [copyingEnvReviewKeys, setCopyingEnvReviewKeys] = useState<false | "all" | "missingInEnv">(false);
  const [runtimeInventory, setRuntimeInventory] = useState<RuntimeInventoryItem[]>([]);
  const diagnosticsByProject = useDiagnosticsStore((state) => state.itemsByProject);
  const diagnosticsError = useDiagnosticsStore((state) => state.error);
  const lastRunAtByProject = useDiagnosticsStore((state) => state.lastRunAtByProject);
  const loadingProjectId = useDiagnosticsStore((state) => state.loadingProjectId);
  const runDiagnostics = useDiagnosticsStore((state) => state.runDiagnostics);
  const {
    actionName,
    services,
    startService,
    stopService,
  } = useServiceStore();
  const pushToast = useToastStore((state) => state.push);

  useEffect(() => {
    if (!project) {
      setDraft({});
      setActiveTab("overview");
      setMobilePreviewOpen(false);
      setMobilePreviewState(null);
      setConfirmDeleteOpen(false);
      setDeleteLoading(false);
      setMessage(undefined);
      setEnvVars([]);
      setEnvVarsError(undefined);
      setEnvVarsLoading(false);
      setEnvInspection(null);
      setEnvInspectionError(undefined);
      setEnvInspectionLoading(false);
      setEnvDiffFilter("all");
      setEnvVarDrafts({});
      setNewEnvKey("");
      setNewEnvValue("");
      setCreatingEnvVar(false);
      setSavingEnvVarId(undefined);
      setDeletingEnvVarId(undefined);
      setRefreshingEnvInspection(false);
      setCopyingEnvReviewKeys(false);
      return;
    }

    setDraft({
      name: project.name,
      domain: project.domain,
      serverType: project.serverType,
      phpVersion: project.phpVersion,
      framework: project.framework,
      documentRoot: project.documentRoot,
      sslEnabled: project.sslEnabled,
      databaseName: project.databaseName ?? null,
      databasePort: project.databasePort ?? null,
      status: project.status,
    });
    setMessage(undefined);
  }, [project]);

  useEffect(() => {
    if (project) {
      setActiveTab("overview");
    }
  }, [project?.id]);

  useEffect(() => {
    if (!project) {
      setMobilePreviewState(null);
      return;
    }

    let cancelled = false;
    mobilePreviewApi
      .getState(project.id)
      .then((next) => {
        if (!cancelled) {
          setMobilePreviewState(next);
        }
      })
      .catch(() => undefined);

    return () => {
      cancelled = true;
    };
  }, [project]);

  useEffect(() => {
    setEnvVarDrafts(
      Object.fromEntries(
        envVars.map((envVar) => [envVar.id, { envKey: envVar.envKey, envValue: envVar.envValue }]),
      ),
    );
  }, [envVars]);

  useEffect(() => {
    if (!confirmDeleteOpen) {
      return;
    }

    function handleKeydown(event: KeyboardEvent) {
      if (event.key !== "Escape" || deleteLoading) {
        return;
      }

      event.preventDefault();
      setConfirmDeleteOpen(false);
    }

    document.addEventListener("keydown", handleKeydown);
    return () => document.removeEventListener("keydown", handleKeydown);
  }, [confirmDeleteOpen, deleteLoading]);

  useEffect(() => {
    if (!project || (activeTab !== "runtime" && activeTab !== "settings")) {
      setRuntimeInventory([]);
      return;
    }

    runtimeApi.list().then(setRuntimeInventory).catch(() => setRuntimeInventory([]));
  }, [actionName, activeTab, project]);

  const projectDiagnostics = project ? diagnosticsByProject[project.id] : undefined;

  useEffect(() => {
    if (!project || activeTab !== "diagnostics" || projectDiagnostics) {
      return;
    }

    void runDiagnostics(project.id).catch(() => undefined);
  }, [activeTab, project, projectDiagnostics, runDiagnostics]);

  useEffect(() => {
    if (!project || activeTab !== "runtime") {
      setLogPreview("");
      setLogPreviewError(undefined);
      setLogPreviewLoading(false);
      return;
    }

    let cancelled = false;
    setLogPreviewLoading(true);
    setLogPreviewError(undefined);

    serviceApi
      .readLogs(project.serverType, 40)
      .then((payload) => {
        if (cancelled) {
          return;
        }

        setLogPreview(formatServiceLogPreview(payload, 40));
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }

        setLogPreview("");
        setLogPreviewError(getAppErrorMessage(error, "Could not load the latest service logs."));
      })
      .finally(() => {
        if (!cancelled) {
          setLogPreviewLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [actionName, activeTab, project]);

  useEffect(() => {
    if (!project || activeTab !== "envVars") {
      setEnvVars([]);
      setEnvVarsError(undefined);
      setEnvVarsLoading(false);
      setEnvInspection(null);
      setEnvInspectionError(undefined);
      setEnvInspectionLoading(false);
      setRefreshingEnvInspection(false);
      return;
    }

    let cancelled = false;
    setEnvVarsLoading(true);
    setEnvVarsError(undefined);
    setEnvInspectionLoading(true);
    setEnvInspectionError(undefined);

    Promise.allSettled([projectEnvApi.list(project.id), projectEnvApi.inspect(project.id)])
      .then(([listResult, inspectResult]) => {
        if (cancelled) {
          return;
        }

        if (listResult.status === "fulfilled") {
          setEnvVars(listResult.value);
          setEnvVarsError(undefined);
        } else {
          setEnvVars([]);
          setEnvVarsError(
            getAppErrorMessage(
              listResult.reason,
              "Could not load the project environment metadata.",
            ),
          );
        }

        if (inspectResult.status === "fulfilled") {
          setEnvInspection(inspectResult.value);
          setEnvInspectionError(undefined);
        } else {
          setEnvInspection(null);
          setEnvInspectionError(
            getAppErrorMessage(
              inspectResult.reason,
              "Could not inspect the project .env file on disk.",
            ),
          );
        }
      })
      .finally(() => {
        if (!cancelled) {
          setEnvVarsLoading(false);
          setEnvInspectionLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [activeTab, project]);

  const projectActionScope = project?.id ?? "__no-project__";
  const projectSavePending = useAsyncActionPending(`project:${projectActionScope}:save`);
  const projectDeletePending = useAsyncActionPending(`project:${projectActionScope}:delete`);

  if (!project) {
    return (
      <Card>
        <div className="page-header">
          <div>
            <h2>Inspector</h2>
            <p>Select a project from the list to inspect its config, health, and runtime behavior.</p>
          </div>
        </div>
      </Card>
    );
  }

  const currentProject = project;
  const diagnostics = diagnosticsByProject[currentProject.id] ?? [];
  const diagnosticsSummary = summarizeDiagnostics(diagnostics);
  const runtimeService = services.find((service) => service.name === currentProject.serverType);
  const mysqlService = services.find((service) => service.name === "mysql");
  const activeServerRuntime =
    runtimeInventory.find(
      (runtime) => runtime.runtimeType === currentProject.serverType && runtime.isActive,
    ) ?? null;
  const activeMysqlRuntime =
    runtimeInventory.find((runtime) => runtime.runtimeType === "mysql" && runtime.isActive) ?? null;
  const activePhpRuntime =
    runtimeInventory.find(
      (runtime) =>
        runtime.runtimeType === "php" &&
        runtimeVersionMatches(currentProject.phpVersion, runtime.version) &&
        runtime.isActive,
    ) ??
    runtimeInventory.find(
      (runtime) =>
        runtime.runtimeType === "php" &&
        runtimeVersionMatches(currentProject.phpVersion, runtime.version),
    ) ??
    runtimeInventory.find((runtime) => runtime.runtimeType === "php" && runtime.isActive) ??
    null;
  const liveStatus = getLiveProjectStatus(currentProject, services);
  const runtimeRunning = runtimeService?.status === "running";
  const diagnosticsLoading = loadingProjectId === currentProject.id;
  const lastRunAt = lastRunAtByProject[currentProject.id];
  const runtimeCompatibilityIssues: string[] = [];
  const phpVersionOptions = installedPhpVersionFamilies(runtimeInventory);
  const tabItems: Array<{ id: ProjectInspectorTab; label: string; meta: string }> = [
    { id: "overview", label: "Overview", meta: currentProject.domain },
    { id: "provisioning", label: "Provisioning", meta: "Config, hosts, tunnel" },
    { id: "runtime", label: "Runtime", meta: liveStatus },
    { id: "diagnostics", label: "Diagnostics", meta: `${diagnosticsSummary.actionable} actionable` },
    { id: "envVars", label: "Env Vars", meta: `${envVars.length} tracked` },
    { id: "settings", label: "Settings", meta: "Edit profile" },
  ];
  const envComparisonSummary = envInspection?.comparison.reduce(
    (summary, item) => {
      summary.total += 1;
      summary[item.status] += 1;
      return summary;
    },
    {
      total: 0,
      match: 0,
      onlyTracked: 0,
      onlyDisk: 0,
      valueMismatch: 0,
    } satisfies Record<ProjectEnvComparisonStatus | "total", number>,
  ) ?? {
    total: 0,
    match: 0,
    onlyTracked: 0,
    onlyDisk: 0,
    valueMismatch: 0,
  };
  const filteredEnvComparison =
    envInspection?.comparison.filter((item) => envDiffFilter === "all" || item.status === envDiffFilter) ?? [];
  const envComparisonByKey = new Map(
    (envInspection?.comparison ?? []).map((item) => [item.key, item]),
  );

  function selectTab(nextTab: ProjectInspectorTab) {
    setActiveTab(nextTab);
    const scrollContainer = inspectorRootRef.current?.closest(".project-detail-content");
    if (scrollContainer instanceof HTMLElement) {
      scrollContainer.scrollTo({ top: 0 });
    }
  }

  async function handleQuickAction(
    action: "folder" | "terminal" | "vscode",
  ) {
    try {
      if (action === "folder") {
        await projectApi.openFolder(currentProject.id);
      } else if (action === "terminal") {
        await projectApi.openTerminal(currentProject.id);
      } else {
        await projectApi.openVsCode(currentProject.id);
      }
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Quick action failed",
        message: getAppErrorMessage(error, "DevNest could not open that project action."),
      });
    }
  }

  async function handleExportProjectProfile() {
    try {
      const result = await projectProfileApi.exportProject(currentProject.id);
      if (!result) {
        return;
      }

      pushToast({
        tone: "success",
        title: "Project profile exported",
        message: `${currentProject.name} was exported to ${result.path}.`,
      });
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Project profile export failed",
        message: getAppErrorMessage(error, "DevNest could not export this project profile."),
      });
    }
  }

  async function handleExportTeamProjectProfile() {
    try {
      const result = await projectProfileApi.exportTeamProject(currentProject.id);
      if (!result) {
        return;
      }

      pushToast({
        tone: "success",
        title: "Team profile exported",
        message: `${currentProject.name} was exported to ${result.path}.`,
      });
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Team profile export failed",
        message: getAppErrorMessage(
          error,
          "DevNest could not export this shared project profile.",
        ),
      });
    }
  }

  if (!activeServerRuntime) {
    runtimeCompatibilityIssues.push(
      `No active ${currentProject.serverType} runtime is linked. Open Settings and link or import one before starting this project.`,
    );
  } else if (activeServerRuntime.status === "missing") {
    runtimeCompatibilityIssues.push(
      `The active ${currentProject.serverType} runtime path is missing. Re-link or re-import the runtime before starting this project.`,
    );
  }

  if (!activePhpRuntime) {
    runtimeCompatibilityIssues.push(
      `No PHP ${currentProject.phpVersion} runtime is linked. Import or link that PHP version in Settings.`,
    );
  } else {
    if (activePhpRuntime.status === "missing") {
      runtimeCompatibilityIssues.push(
        `The selected PHP runtime path is missing. Re-link or re-import PHP ${currentProject.phpVersion}.`,
      );
    }

    if (!runtimeVersionMatches(currentProject.phpVersion, activePhpRuntime.version)) {
      runtimeCompatibilityIssues.push(
        `This project expects PHP ${currentProject.phpVersion}, but DevNest would currently fall back to PHP ${activePhpRuntime.version}. Link PHP ${currentProject.phpVersion} or update the project profile.`,
      );
    }
  }

  if (currentProject.databaseName && !activeMysqlRuntime) {
    runtimeCompatibilityIssues.push(
      "This project is linked to a database, but no active MySQL runtime is currently tracked in Settings.",
    );
  }

  async function handleUpdate(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();

    const nameResult = projectNameSchema.safeParse(draft.name ?? currentProject.name);
    if (!nameResult.success) {
      setMessage(nameResult.error.issues[0]?.message ?? "Project name is invalid.");
      return;
    }

    const domainResult = domainSchema.safeParse(draft.domain ?? currentProject.domain);
    if (!domainResult.success) {
      setMessage(domainResult.error.issues[0]?.message ?? "Domain is invalid.");
      return;
    }

    const rootResult = documentRootSchema.safeParse(draft.documentRoot ?? currentProject.documentRoot);
    if (!rootResult.success) {
      setMessage(rootResult.error.issues[0]?.message ?? "Document root is invalid.");
      return;
    }

    try {
      await onUpdate(currentProject.id, draft);
      setMessage(undefined);
      pushToast({
        tone: "success",
        title: "Project updated",
        message: `${currentProject.name} was updated successfully.`,
      });
    } catch (error) {
      const nextMessage = getAppErrorMessage(error, "Failed to update project.");
      setMessage(nextMessage);
      pushToast({
        tone: "error",
        title: "Project update failed",
        message: nextMessage,
      });
    }
  }

  async function handleDelete() {
    setDeleteLoading(true);
    try {
      await onDelete(currentProject.id);
      setConfirmDeleteOpen(false);
      setMessage(undefined);
    } catch (error) {
      const nextMessage = getAppErrorMessage(error, "Failed to delete project.");
      setMessage(nextMessage);
      pushToast({
        tone: "error",
        title: "Project delete failed",
        message: nextMessage,
      });
    } finally {
      setDeleteLoading(false);
    }
  }

  async function handleRuntimeToggle() {
    try {
      if (runtimeRunning) {
        await stopService(currentProject.serverType);
        setMessage(undefined);
        pushToast({
          tone: "success",
          title: "Runtime stopped",
          message: `${currentProject.serverType} stopped for ${currentProject.name}.`,
        });
      } else {
        const report = await reliabilityApi.runPreflight(
          "startProjectRuntime",
          currentProject.id,
        );
        if (!report.ready) {
          setMessage(report.summary);
          pushToast({
            tone: "warning",
            title: "Runtime start blocked",
            message: report.summary,
          });
          navigate(`/reliability?projectId=${currentProject.id}`);
          return;
        }

        await startService(currentProject.serverType);
        setMessage(undefined);
        pushToast({
          tone: "success",
          title: "Runtime started",
          message: `${currentProject.serverType} started for ${currentProject.name}.`,
        });
      }
    } catch (error) {
      const nextMessage = getAppErrorMessage(error, "Runtime action failed.");
      setMessage(nextMessage);
      pushToast({
        tone: "error",
        title: "Runtime action failed",
        message: nextMessage,
      });
    }
  }

  async function handleDiagnosticsRefresh() {
    try {
      await runDiagnostics(currentProject.id);
      setMessage(undefined);
      pushToast({
        tone: "success",
        title: "Diagnostics updated",
        message: `${currentProject.name} diagnostics were refreshed.`,
      });
    } catch (error) {
      const nextMessage = getAppErrorMessage(error, "Failed to run diagnostics.");
      setMessage(nextMessage);
      pushToast({
        tone: "error",
        title: "Diagnostics failed",
        message: nextMessage,
      });
    }
  }

  async function refreshEnvState() {
    const [items, inspection] = await Promise.all([
      projectEnvApi.list(currentProject.id),
      projectEnvApi.inspect(currentProject.id),
    ]);

    setEnvVars(items);
    setEnvVarsError(undefined);
    setEnvInspection(inspection);
    setEnvInspectionError(undefined);
  }

  async function handleRefreshEnvInspection() {
    setRefreshingEnvInspection(true);
    try {
      await refreshEnvState();
      pushToast({
        tone: "success",
        title: ".env state refreshed",
        message: `${currentProject.name} disk env visibility was refreshed.`,
      });
    } catch (error) {
      const nextMessage = getAppErrorMessage(error, "Could not refresh the project .env state.");
      setEnvInspectionError(nextMessage);
      pushToast({
        tone: "error",
        title: ".env refresh failed",
        message: nextMessage,
      });
    } finally {
      setRefreshingEnvInspection(false);
    }
  }

  async function handleCopyEnvReviewKeys(mode: "all" | "missingInEnv") {
    const comparisonItems = envInspection?.comparison ?? [];
    const keys =
      mode === "missingInEnv"
        ? comparisonItems
            .filter((item) => item.status === "onlyTracked")
            .map((item) => item.key)
        : comparisonItems
            .filter((item) => item.status !== "match")
            .map((item) => `${item.key} (${envComparisonLabel(item.status)})`);

    if (keys.length === 0) {
      pushToast({
        tone: "warning",
        title: "Nothing to copy",
        message:
          mode === "missingInEnv"
            ? "No tracked keys are currently missing from `.env`."
            : "There are no env keys that currently need review.",
      });
      return;
    }

    if (!navigator.clipboard?.writeText) {
      pushToast({
        tone: "error",
        title: "Clipboard unavailable",
        message: "This environment does not allow DevNest to copy text to the clipboard.",
      });
      return;
    }

    setCopyingEnvReviewKeys(mode);
    try {
      await navigator.clipboard.writeText(keys.join("\n"));
      pushToast({
        tone: "success",
        title: mode === "missingInEnv" ? "Missing keys copied" : "Review list copied",
        message:
          mode === "missingInEnv"
            ? `Copied ${keys.length} tracked key(s) missing from \`.env\`.`
            : `Copied ${keys.length} env key(s) that still need review.`,
      });
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Copy failed",
        message: getAppErrorMessage(error, "DevNest could not copy the env review list."),
      });
    } finally {
      setCopyingEnvReviewKeys(false);
    }
  }

  async function handleCreateEnvVar() {
    const keyResult = envVarKeySchema.safeParse(newEnvKey);
    if (!keyResult.success) {
      pushToast({
        tone: "error",
        title: "Env Var Create Failed",
        message: keyResult.error.issues[0]?.message ?? "Environment key is invalid.",
      });
      return;
    }

    const valueResult = envVarValueSchema.safeParse(newEnvValue);
    if (!valueResult.success) {
      pushToast({
        tone: "error",
        title: "Env Var Create Failed",
        message: valueResult.error.issues[0]?.message ?? "Environment value is invalid.",
      });
      return;
    }

    setCreatingEnvVar(true);
    try {
      await projectEnvApi.create({
        projectId: currentProject.id,
        envKey: keyResult.data,
        envValue: valueResult.data,
      });
      setNewEnvKey("");
      setNewEnvValue("");
      await refreshEnvState();
      pushToast({
        tone: "success",
        title: "Env Var Added",
        message: `${keyResult.data.toUpperCase()} is now tracked for ${currentProject.name}.`,
      });
    } catch (error) {
      const nextMessage = getAppErrorMessage(error, "Could not create the environment metadata.");
      setEnvVarsError(nextMessage);
      pushToast({
        tone: "error",
        title: "Env Var Create Failed",
        message: nextMessage,
      });
    } finally {
      setCreatingEnvVar(false);
    }
  }

  async function handleSaveEnvVar(envVarId: string) {
    const draftValue = envVarDrafts[envVarId];
    if (!draftValue) {
      return;
    }

    const keyResult = envVarKeySchema.safeParse(draftValue.envKey);
    if (!keyResult.success) {
      pushToast({
        tone: "error",
        title: "Env Var Update Failed",
        message: keyResult.error.issues[0]?.message ?? "Environment key is invalid.",
      });
      return;
    }

    const valueResult = envVarValueSchema.safeParse(draftValue.envValue);
    if (!valueResult.success) {
      pushToast({
        tone: "error",
        title: "Env Var Update Failed",
        message: valueResult.error.issues[0]?.message ?? "Environment value is invalid.",
      });
      return;
    }

    setSavingEnvVarId(envVarId);
    try {
      await projectEnvApi.update({
        projectId: currentProject.id,
        envVarId,
        envKey: keyResult.data,
        envValue: valueResult.data,
      });
      await refreshEnvState();
      pushToast({
        tone: "success",
        title: "Env Var Updated",
        message: `${keyResult.data.toUpperCase()} was updated for ${currentProject.name}.`,
      });
    } catch (error) {
      const nextMessage = getAppErrorMessage(error, "Could not update the environment metadata.");
      setEnvVarsError(nextMessage);
      pushToast({
        tone: "error",
        title: "Env Var Update Failed",
        message: nextMessage,
      });
    } finally {
      setSavingEnvVarId(undefined);
    }
  }

  async function handleDeleteEnvVar(envVarId: string) {
    setDeletingEnvVarId(envVarId);
    try {
      await projectEnvApi.remove(currentProject.id, envVarId);
      await refreshEnvState();
      pushToast({
        tone: "success",
        title: "Env Var Removed",
        message: `Project environment metadata was removed from ${currentProject.name}.`,
      });
    } catch (error) {
      const nextMessage = getAppErrorMessage(error, "Could not delete the environment metadata.");
      setEnvVarsError(nextMessage);
      pushToast({
        tone: "error",
        title: "Env Var Delete Failed",
        message: nextMessage,
      });
    } finally {
      setDeletingEnvVarId(undefined);
    }
  }

  async function handleQuickFix(code: string) {
    if (
      code !== "LARAVEL_DOCUMENT_ROOT_MISMATCH" &&
      code !== "SSL_AUTHORITY_MISSING" &&
      code !== "SSL_TRUST_MISSING" &&
      code !== "SSL_CERTIFICATE_MISSING"
    ) {
      openQuickFix(code);
      return;
    }

    try {
      const result = await diagnosticsApi.fix(currentProject.id, code);
      if (code === "LARAVEL_DOCUMENT_ROOT_MISMATCH") {
        await onUpdate(currentProject.id, { documentRoot: "public" });
      }
      await runDiagnostics(currentProject.id);
      setMessage(undefined);
      pushToast({
        tone: "success",
        title: "Quick Fix Applied",
        message: result.message,
      });
    } catch (error) {
      const nextMessage = getAppErrorMessage(error, "DevNest could not apply that quick fix.");
      setMessage(nextMessage);
      pushToast({
        tone: "error",
        title: "Quick Fix Failed",
        message: nextMessage,
      });
    }
  }

  function openQuickFix(code: string) {
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
        navigate(`/logs?source=${currentProject.serverType}`);
        return;
      default:
        navigate(`/diagnostics?projectId=${currentProject.id}`);
    }
  }

  return (
    <div className="stack project-detail-shell" ref={inspectorRootRef}>
      <div
        aria-label={`${currentProject.name} detail sections`}
        className="project-detail-tabs sticky"
        role="tablist"
      >
        {tabItems.map((tab) => (
          <button
            aria-controls={`project-detail-panel-${tab.id}`}
            aria-selected={activeTab === tab.id}
            className="project-detail-tab"
            data-active={activeTab === tab.id}
            id={`project-detail-tab-${tab.id}`}
            key={tab.id}
            onClick={() => selectTab(tab.id)}
            role="tab"
            type="button"
          >
            <strong>{tab.label}</strong>
            <span>{tab.meta}</span>
          </button>
        ))}
      </div>

      <div
        aria-labelledby="project-detail-tab-overview"
        className="project-detail-panel"
        hidden={activeTab !== "overview"}
        id="project-detail-panel-overview"
        role="tabpanel"
      >
        <Card>
          <div className="page-header">
            <div>
              <div className="page-title-actions">
                <h2>{project.name}</h2>
                <Button onClick={() => setMobilePreviewOpen(true)} size="sm">
                  {mobilePreviewState?.status === "running" ? "Preview Running" : "Mobile Preview"}
                </Button>
              </div>
              <p>{project.domain}</p>
            </div>
            <div className="page-toolbar">
              <span className="status-chip">{project.framework}</span>
              <span className="status-chip">{project.serverType}</span>
              <span className="status-chip">PHP {project.phpVersion}</span>
              {mobilePreviewState?.status === "running" ? <span className="status-chip">Preview On</span> : null}
              <span className="status-chip" data-tone={getStatusTone(liveStatus)}>
                {liveStatus}
              </span>
            </div>
          </div>

          <div className="detail-grid">
            <div className="detail-item">
              <span className="detail-label">Project Path</span>
              <strong className="mono detail-value">{project.path}</strong>
            </div>
            <div className="detail-item">
              <span className="detail-label">Document Root</span>
              <strong className="mono detail-value">{project.documentRoot}</strong>
            </div>
            <div className="detail-item">
              <span className="detail-label">Created</span>
              <strong>{formatUpdatedAt(project.createdAt)}</strong>
            </div>
            <div className="detail-item">
              <span className="detail-label">Updated</span>
              <strong>{formatUpdatedAt(project.updatedAt)}</strong>
            </div>
          </div>

          <div className="page-toolbar" style={{ justifyContent: "flex-start" }}>
            <Button onClick={() => void handleQuickAction("folder")}>Open Folder</Button>
            <Button onClick={() => void handleQuickAction("terminal")}>Open Terminal</Button>
            <Button onClick={() => void handleQuickAction("vscode")}>Open VS Code</Button>
            <Button onClick={() => setMobilePreviewOpen(true)}>Mobile Preview</Button>
            <Button onClick={() => void handleExportTeamProjectProfile()}>Export Team Profile</Button>
            <Button onClick={() => void handleExportProjectProfile()}>Export Profile</Button>
          </div>
        </Card>
      </div>

      <div
        aria-labelledby="project-detail-tab-provisioning"
        className="project-detail-panel"
        hidden={activeTab !== "provisioning"}
        id="project-detail-panel-provisioning"
        role="tabpanel"
      >
        {activeTab === "provisioning" ? <ProjectProvisioningPanel project={project} /> : null}
      </div>

      <div
        aria-labelledby="project-detail-tab-runtime"
        className="project-detail-panel"
        hidden={activeTab !== "runtime"}
        id="project-detail-panel-runtime"
        role="tabpanel"
      >
        {activeTab === "runtime" ? (
        <div className="stack">
          <Card>
            <div className="page-header">
              <div>
                <h2>Runtime Control</h2>
                <p>Live runtime data is derived from the linked service instead of the stored project row.</p>
              </div>
              <span className="status-chip" data-tone={getStatusTone(liveStatus)}>
                {liveStatus}
              </span>
            </div>

            <div className="detail-grid">
              <div className="detail-item">
                <span className="detail-label">Service</span>
                <strong>{currentProject.serverType}</strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">Port</span>
                <strong>{runtimeService?.port ?? "-"}</strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">PID</span>
                <strong>{runtimeService?.pid ?? "-"}</strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">MySQL</span>
                <strong>{mysqlService?.status ?? "unknown"}</strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">Last Runtime Error</span>
                <strong>{runtimeService?.lastError ?? "None"}</strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">Database Link</span>
                <strong>{currentProject.databaseName ?? "Not linked"}</strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">Server Runtime</span>
                <strong>
                  {activeServerRuntime
                    ? `${activeServerRuntime.version} (${activeServerRuntime.status})`
                    : "No active server runtime linked"}
                </strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">PHP Runtime</span>
                <strong>
                  {activePhpRuntime
                    ? `${activePhpRuntime.version} (${activePhpRuntime.status})`
                    : `No PHP ${currentProject.phpVersion} runtime linked`}
                </strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">Server Runtime Path</span>
                <strong className="mono detail-value">
                  {activeServerRuntime?.path ?? "Open Settings to link the selected server runtime."}
                </strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">PHP Runtime Path</span>
                <strong className="mono detail-value">
                  {activePhpRuntime?.path ?? "Open Settings to link the selected PHP runtime."}
                </strong>
              </div>
            </div>

            {runtimeCompatibilityIssues.length > 0 ? (
              <div className="stack" style={{ gap: 8 }}>
                {runtimeCompatibilityIssues.map((issue) => (
                  <span className="error-text" key={issue}>{issue}</span>
                ))}
              </div>
            ) : (
              <span className="helper-text">
                Runtime compatibility looks clean for the current project profile.
              </span>
            )}

            <div className="page-toolbar" style={{ justifyContent: "flex-start" }}>
              <Button
                disabled={actionName === currentProject.serverType}
                onClick={() => void handleRuntimeToggle()}
                variant="primary"
              >
                {actionName === currentProject.serverType
                  ? runtimeRunning
                    ? "Stopping..."
                    : "Starting..."
                  : runtimeRunning
                    ? "Stop Server"
                    : "Start Server"}
              </Button>
              <Button onClick={() => navigate(`/logs?source=${currentProject.serverType}`)}>
                View Logs
              </Button>
            </div>
          </Card>

          <Card>
            <div className="page-header">
              <div>
                <h2>Logs Preview</h2>
                <p>Latest runtime output for the selected project server, without leaving the detail view.</p>
              </div>
              <Button onClick={() => navigate(`/logs?source=${currentProject.serverType}`)}>Open Full Logs</Button>
            </div>

            {logPreviewError ? <span className="error-text">{logPreviewError}</span> : null}

            <pre className="config-preview mono" style={{ minHeight: 180, maxHeight: 260 }}>
              {logPreviewLoading
                ? "Loading latest runtime output..."
                : logPreview || "No log output is available yet for the selected runtime."}
            </pre>
          </Card>
        </div>
        ) : null}
      </div>

      <div
        aria-labelledby="project-detail-tab-diagnostics"
        className="project-detail-panel"
        hidden={activeTab !== "diagnostics"}
        id="project-detail-panel-diagnostics"
        role="tabpanel"
      >
        {activeTab === "diagnostics" ? (
        <Card>
          <div className="page-header">
            <div>
              <h2>Health Summary</h2>
              <p>See the main issues first, then jump directly to the next fix.</p>
            </div>
            <div className="page-toolbar">
              <Button
                busy={diagnosticsLoading}
                busyLabel="Running diagnostics..."
                onClick={() => void handleDiagnosticsRefresh()}
                variant="primary"
              >
                Run Diagnostics
              </Button>
              <Button onClick={() => navigate(`/diagnostics?projectId=${currentProject.id}`)}>Open Full View</Button>
            </div>
          </div>

          <div className="route-grid" data-columns="2">
            <div className="detail-item">
              <span className="detail-label">Errors</span>
              <strong>{diagnosticsSummary.errors}</strong>
            </div>
            <div className="detail-item">
              <span className="detail-label">Warnings</span>
              <strong>{diagnosticsSummary.warnings}</strong>
            </div>
            <div className="detail-item">
              <span className="detail-label">Suggestions</span>
              <strong>{diagnosticsSummary.suggestions}</strong>
            </div>
            <div className="detail-item">
              <span className="detail-label">Last Run</span>
              <strong>{lastRunAt ? formatUpdatedAt(lastRunAt) : "Not run yet"}</strong>
            </div>
          </div>

          {diagnosticsError ? <span className="error-text">{diagnosticsError}</span> : null}

          <div className="stack" style={{ gap: 12 }}>
            {diagnostics.map((diagnostic) => (
              <div className="detail-item" key={diagnostic.id}>
                <div className="page-toolbar" style={{ alignItems: "flex-start" }}>
                  <div>
                    <strong>{diagnostic.title}</strong>
                    <p style={{ marginTop: 6 }}>{diagnostic.message}</p>
                  </div>
                  <span className="status-chip" data-tone={diagnostic.level === "error" ? "error" : diagnostic.level === "warning" ? "warning" : "success"}>
                    {diagnostic.level}
                  </span>
                </div>
                {diagnostic.suggestion ? <span className="helper-text">{diagnostic.suggestion}</span> : null}
                {diagnostic.code !== "WORKSPACE_READY" ? (
                  <div className="page-toolbar" style={{ justifyContent: "flex-start" }}>
                    <Button onClick={() => void handleQuickFix(diagnostic.code)}>
                      {quickFixActionLabel(diagnostic.code)}
                    </Button>
                  </div>
                ) : null}
              </div>
            ))}
          </div>
        </Card>
        ) : null}
      </div>

      <div
        aria-labelledby="project-detail-tab-envVars"
        className="project-detail-panel"
        hidden={activeTab !== "envVars"}
        id="project-detail-panel-envVars"
        role="tabpanel"
      >
        {activeTab === "envVars" ? (
        <Card>
          <div className="page-header">
            <div>
              <h2>Project Env Vars</h2>
              <p>DevNest tracks lightweight project metadata here. It does not overwrite the real `.env` file on disk.</p>
            </div>
            <div className="page-toolbar" style={{ gap: 10 }}>
              <span className="status-chip">{envVars.length} tracked</span>
              <Button
                busy={refreshingEnvInspection}
                busyLabel="Refreshing..."
                onClick={() => void handleRefreshEnvInspection()}
              >
                Refresh .env
              </Button>
            </div>
          </div>

          <div className="detail-grid">
            <div className="detail-item">
              <span className="runtime-table-note">Tracked in DevNest</span>
              <strong>{envVars.length}</strong>
              <span className="helper-text">App-managed metadata for project-aware hints and tooling context.</span>
            </div>
            <div className="detail-item">
              <span className="runtime-table-note">Disk `.env`</span>
              <strong>{envInspection?.envFileExists ? `${envInspection.diskCount} entries` : "Missing"}</strong>
              <span className="helper-text">
                {envInspection?.envFileExists
                  ? "Read-only visibility from the real `.env` file on disk."
                  : "DevNest did not find a `.env` file yet for this project path."}
              </span>
            </div>
            <div className="detail-item">
              <span className="runtime-table-note">Matches</span>
              <strong>{envComparisonSummary.match}</strong>
              <span className="helper-text">Keys where tracked metadata and `.env` currently agree.</span>
            </div>
            <div className="detail-item">
              <span className="runtime-table-note">Needs Review</span>
              <strong>
                {envComparisonSummary.onlyTracked + envComparisonSummary.onlyDisk + envComparisonSummary.valueMismatch}
              </strong>
              <span className="helper-text">Missing keys or mismatched values that may confuse project context.</span>
            </div>
          </div>

          <div className="stack" style={{ gap: 16, marginTop: 20 }}>
            <div className="page-header">
              <div>
                <h3>Tracked in DevNest</h3>
                <p>Use this for project-aware hints and quick workflow metadata, not as a full secret manager.</p>
              </div>
            </div>

            <div className="form-grid">
              <div className="field">
                <label htmlFor="project-env-key">Key</label>
                <input
                  className="input"
                  id="project-env-key"
                  onChange={(event) => setNewEnvKey(event.target.value.toUpperCase())}
                  placeholder="APP_ENV"
                  value={newEnvKey}
                />
              </div>
              <div className="field">
                <label htmlFor="project-env-value">Value</label>
                <input
                  className="input"
                  id="project-env-value"
                  onChange={(event) => setNewEnvValue(event.target.value)}
                  placeholder="local"
                  value={newEnvValue}
                />
              </div>
            </div>

            <div className="page-toolbar" style={{ justifyContent: "space-between" }}>
              <span className="helper-text">
                Tracked vars stay in DevNest metadata. They do not automatically become the app's runtime env.
              </span>
              <div className="runtime-table-actions">
                <Button
                  busy={copyingEnvReviewKeys === "missingInEnv"}
                  busyLabel="Copying..."
                  onClick={() => void handleCopyEnvReviewKeys("missingInEnv")}
                >
                  Copy Missing in .env
                </Button>
                <Button
                  busy={creatingEnvVar}
                  busyLabel="Adding env var..."
                  onClick={() => void handleCreateEnvVar()}
                  variant="primary"
                >
                  Add Env Var
                </Button>
              </div>
            </div>

            {envVarsError ? <span className="error-text">{envVarsError}</span> : null}

            {envVarsLoading ? (
              <span className="helper-text">Loading project environment metadata...</span>
            ) : envVars.length === 0 ? (
              <span className="helper-text">
                No project env vars tracked yet. Add only the keys DevNest should understand for this project.
              </span>
            ) : (
              <div className="runtime-table-shell">
                <table className="runtime-table">
                  <thead>
                    <tr>
                      <th>Key</th>
                      <th>Value</th>
                      <th>Disk Status</th>
                      <th>Updated</th>
                      <th>Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {envVars.map((envVar) => {
                      const draftValue = envVarDrafts[envVar.id] ?? {
                        envKey: envVar.envKey,
                        envValue: envVar.envValue,
                      };
                      const trackedState = envComparisonByKey.get(envVar.envKey);

                      return (
                        <tr key={envVar.id}>
                          <td>
                            <input
                              className="input"
                              onChange={(event) =>
                                setEnvVarDrafts((current) => ({
                                  ...current,
                                  [envVar.id]: {
                                    envKey: event.target.value.toUpperCase(),
                                    envValue: draftValue.envValue,
                                  },
                                }))
                              }
                              value={draftValue.envKey}
                            />
                          </td>
                          <td>
                            <input
                              className="input"
                              onChange={(event) =>
                                setEnvVarDrafts((current) => ({
                                  ...current,
                                  [envVar.id]: {
                                    envKey: draftValue.envKey,
                                    envValue: event.target.value,
                                  },
                                }))
                              }
                              value={draftValue.envValue}
                            />
                          </td>
                          <td>
                            {trackedState ? (
                              <span className="status-chip" data-tone={envComparisonTone(trackedState.status)}>
                                {envComparisonLabel(trackedState.status)}
                              </span>
                            ) : (
                              <span className="runtime-table-note">Not inspected yet</span>
                            )}
                          </td>
                          <td>
                            <span className="runtime-table-note">{formatUpdatedAt(envVar.updatedAt)}</span>
                          </td>
                          <td>
                            <div className="runtime-table-actions">
                              <Button
                                busy={savingEnvVarId === envVar.id}
                                busyLabel="Saving env var..."
                                onClick={() => void handleSaveEnvVar(envVar.id)}
                                variant="primary"
                              >
                                Save
                              </Button>
                              <Button
                                busy={deletingEnvVarId === envVar.id}
                                busyLabel="Deleting env var..."
                                className="button-danger"
                                disabled={deletingEnvVarId === envVar.id}
                                onClick={() => void handleDeleteEnvVar(envVar.id)}
                              >
                                Delete
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
          </div>

          <div className="stack" style={{ gap: 16, marginTop: 24 }}>
            <div className="page-header">
              <div>
                <h3>Read-Only `.env` Diff</h3>
                <p>Compare the metadata DevNest tracks with the real `.env` file without rewriting disk state.</p>
              </div>
              <div className="runtime-table-actions">
                {envInspection ? (
                  <span className="status-chip">{envComparisonSummary.total} keys compared</span>
                ) : null}
                <Button
                  busy={copyingEnvReviewKeys === "all"}
                  busyLabel="Copying..."
                  onClick={() => void handleCopyEnvReviewKeys("all")}
                >
                  Copy Needs Review
                </Button>
              </div>
            </div>

            {envInspectionError ? <span className="error-text">{envInspectionError}</span> : null}
            {envInspection?.diskReadError ? <span className="error-text">{envInspection.diskReadError}</span> : null}

            {envInspectionLoading ? (
              <span className="helper-text">Inspecting the project `.env` file...</span>
            ) : envInspection == null ? (
              <span className="helper-text">
                DevNest could not inspect the disk `.env` state for this project yet.
              </span>
            ) : envInspection.comparison.length === 0 ? (
              <span className="helper-text">
                No comparable env keys found yet. Track vars in DevNest or add a `.env` file to see comparison data.
              </span>
            ) : (
              <div className="stack" style={{ gap: 12 }}>
                <div className="env-diff-filters">
                  {(["all", "valueMismatch", "onlyTracked", "onlyDisk", "match"] as const).map((filter) => (
                    <button
                      key={filter}
                      className="env-diff-filter"
                      data-active={envDiffFilter === filter}
                      onClick={() => setEnvDiffFilter(filter)}
                      type="button"
                    >
                      <span>{envDiffFilterLabel(filter)}</span>
                      <span className="runtime-table-note">
                        {filter === "all" ? envComparisonSummary.total : envComparisonSummary[filter]}
                      </span>
                    </button>
                  ))}
                </div>

                {envDiffFilter !== "all" ? (
                  <span className="helper-text">
                    Showing only <strong>{envDiffFilterLabel(envDiffFilter)}</strong> items in the comparison table.
                  </span>
                ) : (
                  <span className="helper-text">
                    Review `Only in DevNest` before assuming the app really sees those values, and review `Mismatch` before trusting diagnostics tied to env state.
                  </span>
                )}

                {filteredEnvComparison.length === 0 ? (
                  <span className="helper-text">
                    No items match the current filter.
                  </span>
                ) : (
                  <div className="runtime-table-shell">
                    <table className="runtime-table">
                      <thead>
                        <tr>
                          <th>Key</th>
                          <th>Tracked in DevNest</th>
                          <th>Disk `.env`</th>
                          <th>Status</th>
                        </tr>
                      </thead>
                      <tbody>
                        {filteredEnvComparison.map((item) => {
                          const diskEntry = envInspection.diskVars.find((diskVar) => diskVar.key === item.key);

                          return (
                            <tr key={item.key}>
                              <td>
                                <strong>{item.key}</strong>
                              </td>
                              <td>
                                {item.trackedValue != null ? (
                                  <code>{item.trackedValue}</code>
                                ) : (
                                  <span className="runtime-table-note">Not tracked</span>
                                )}
                              </td>
                              <td>
                                {item.diskValue != null ? (
                                  <div style={{ display: "grid", gap: 4 }}>
                                    <code>{item.diskValue}</code>
                                    {diskEntry ? (
                                      <span className="runtime-table-note">Line {diskEntry.sourceLine}</span>
                                    ) : null}
                                  </div>
                                ) : (
                                  <span className="runtime-table-note">Missing in `.env`</span>
                                )}
                              </td>
                              <td>
                                <span className="status-chip" data-tone={envComparisonTone(item.status)}>
                                  {envComparisonLabel(item.status)}
                                </span>
                              </td>
                            </tr>
                          );
                        })}
                      </tbody>
                    </table>
                  </div>
                )}
              </div>
            )}
          </div>
        </Card>
        ) : null}
      </div>

      <div
        aria-labelledby="project-detail-tab-settings"
        className="project-detail-panel"
        hidden={activeTab !== "settings"}
        id="project-detail-panel-settings"
        role="tabpanel"
      >
        <Card>
          <div className="page-header">
            <div>
              <h2>Project Settings</h2>
              <p>Edit the persisted project profile without leaving the detail workspace.</p>
            </div>
            <span className="status-chip">{project.framework}</span>
          </div>

          <form className="stack" onSubmit={handleUpdate}>
            <div className="form-grid">
              <div className="field">
                <label htmlFor="inspector-name">Project Name</label>
                <input
                  className="input"
                  id="inspector-name"
                  onChange={(event) => setDraft((current) => ({ ...current, name: event.target.value }))}
                  value={draft.name ?? ""}
                />
              </div>
              <div className="field">
                <label htmlFor="inspector-domain">Domain</label>
                <input
                  className="input"
                  id="inspector-domain"
                  onChange={(event) => setDraft((current) => ({ ...current, domain: event.target.value }))}
                  value={draft.domain ?? ""}
                />
              </div>
              <div className="field">
                <label htmlFor="inspector-server">Server</label>
                <select
                  className="select"
                  id="inspector-server"
                  onChange={(event) =>
                    setDraft((current) => ({
                      ...current,
                      serverType: event.target.value as UpdateProjectPatch["serverType"],
                    }))
                  }
                  value={draft.serverType ?? project.serverType}
                >
                  <option value="apache">Apache</option>
                  <option value="nginx">Nginx</option>
                </select>
              </div>
              <div className="field">
                <label htmlFor="inspector-php">PHP Version</label>
                <select
                  className="select"
                  id="inspector-php"
                  onChange={(event) => setDraft((current) => ({ ...current, phpVersion: event.target.value }))}
                  value={draft.phpVersion ?? ""}
                >
                  {(
                    phpVersionOptions.includes(draft.phpVersion ?? "")
                      ? phpVersionOptions
                      : [...phpVersionOptions, draft.phpVersion ?? project.phpVersion].filter(Boolean)
                  )
                    .sort((left, right) => left.localeCompare(right, undefined, { numeric: true }))
                    .map((version) => (
                      <option key={version} value={version}>
                        PHP {version}
                      </option>
                    ))}
                </select>
              </div>
              <div className="field">
                <label htmlFor="inspector-framework">Framework</label>
                <select
                  className="select"
                  id="inspector-framework"
                  onChange={(event) =>
                    setDraft((current) => ({
                      ...current,
                      framework: event.target.value as UpdateProjectPatch["framework"],
                    }))
                  }
                  value={draft.framework ?? project.framework}
                >
                  <option value="laravel">Laravel</option>
                  <option value="wordpress">WordPress</option>
                  <option value="php">PHP</option>
                  <option value="unknown">Unknown</option>
                </select>
              </div>
              <div className="field">
                <label htmlFor="inspector-status">Fallback Status Tag</label>
                <select
                  className="select"
                  id="inspector-status"
                  onChange={(event) =>
                    setDraft((current) => ({
                      ...current,
                      status: event.target.value as UpdateProjectPatch["status"],
                    }))
                  }
                  value={draft.status ?? project.status}
                >
                  <option value="running">Running</option>
                  <option value="stopped">Stopped</option>
                  <option value="error">Error</option>
                </select>
              </div>
              <div className="field" data-span="2">
                <label htmlFor="inspector-root">Document Root</label>
                <input
                  className="input"
                  id="inspector-root"
                  onChange={(event) => setDraft((current) => ({ ...current, documentRoot: event.target.value }))}
                  value={draft.documentRoot ?? ""}
                />
              </div>
              <div className="field">
                <label htmlFor="inspector-db-name">Database Name</label>
                <input
                  className="input"
                  id="inspector-db-name"
                  onChange={(event) =>
                    setDraft((current) => ({
                      ...current,
                      databaseName: event.target.value.trim().length > 0 ? event.target.value : null,
                    }))
                  }
                  placeholder="Optional"
                  value={draft.databaseName ?? ""}
                />
              </div>
              <div className="field">
                <label htmlFor="inspector-db-port">Database Port</label>
                <input
                  className="input"
                  id="inspector-db-port"
                  max={65535}
                  min={1}
                  onChange={(event) => {
                    const value = event.target.value.trim();
                    setDraft((current) => ({
                      ...current,
                      databasePort: value.length > 0 ? Number(value) : null,
                    }));
                  }}
                  placeholder="3306"
                  type="number"
                  value={draft.databasePort ?? ""}
                />
              </div>
            </div>

            <label className="checkbox-row">
              <input
                checked={draft.sslEnabled ?? false}
                onChange={(event) => setDraft((current) => ({ ...current, sslEnabled: event.target.checked }))}
                type="checkbox"
              />
              <span>Enable local SSL certificate provisioning and HTTPS config</span>
            </label>

            {message ? (
              <span
                className={
                  message.toLowerCase().includes("failed") || message.toLowerCase().includes("invalid")
                    ? "error-text"
                    : "helper-text"
                }
              >
                {message}
              </span>
            ) : null}

            <div className="page-toolbar" style={{ justifyContent: "space-between" }}>
              <Button
                className="button-danger"
                busy={projectDeletePending || deleteLoading}
                busyLabel="Deleting project..."
                disabled={projectSavePending || (loading && !projectSavePending)}
                onClick={() => setConfirmDeleteOpen(true)}
                type="button"
              >
                Delete Project
              </Button>
              <Button
                busy={projectSavePending}
                busyLabel="Saving project..."
                disabled={projectDeletePending || (loading && !projectSavePending)}
                type="submit"
                variant="primary"
              >
                Save Changes
              </Button>
            </div>
          </form>
        </Card>
      </div>

      {confirmDeleteOpen ? (
        <div
          data-nested-modal="true"
          className="wizard-overlay"
          onClick={() => {
            if (!deleteLoading) {
              setConfirmDeleteOpen(false);
            }
          }}
          role="dialog"
          aria-modal="true"
        >
          <div className="confirm-dialog" onClick={(event) => event.stopPropagation()}>
            <div className="confirm-dialog-copy">
              <h3>Delete project?</h3>
              <p>
                This will remove <strong>{currentProject.name}</strong> from the DevNest registry and attempt
                to clean up its hosts entry.
              </p>
              <div className="detail-item">
                <span className="detail-label">Domain</span>
                <strong>{currentProject.domain}</strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">Path</span>
                <strong className="mono detail-value">{currentProject.path}</strong>
              </div>
              <span className="error-text">
                Project files on disk are not deleted, but this registry entry will be removed immediately.
              </span>
            </div>
            <div className="confirm-dialog-actions">
              <Button disabled={deleteLoading} onClick={() => setConfirmDeleteOpen(false)}>
                Cancel
              </Button>
              <Button
                busy={projectDeletePending || deleteLoading}
                busyLabel="Deleting project..."
                className="button-danger"
                disabled={deleteLoading}
                onClick={() => void handleDelete()}
              >
                Delete Project
              </Button>
            </div>
          </div>
        </div>
      ) : null}

      <ProjectMobilePreviewModal
        onClose={() => setMobilePreviewOpen(false)}
        onPreviewStateChange={setMobilePreviewState}
        open={mobilePreviewOpen}
        project={currentProject}
      />
    </div>
  );
}
