import { useEffect, useMemo, useRef, useState } from "react";
import { useToastStore } from "@/app/store/toast-store";
import { ProjectProvisioningPanel } from "@/components/projects/project-provisioning-panel";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { configApi } from "@/lib/api/config-api";
import { projectApi } from "@/lib/api/project-api";
import { runtimeApi } from "@/lib/api/runtime-api";
import { serviceApi } from "@/lib/api/service-api";
import {
  activeFrankenphpVersionFamily,
  frankenphpVersionFamilies,
  installedPhpVersionFamilies,
  runtimeVersionMatches,
} from "@/lib/runtime-version";
import { documentRootSchema, domainSchema, projectNameSchema, projectPathSchema } from "@/lib/validators";
import type { CreateProjectInput, Project, ScanResult } from "@/types/project";
import type { RuntimeInventoryItem } from "@/types/runtime";

const steps = ["Select Folder", "Scan Results", "Configure", "Review", "Complete"];

function serverTypeLabel(serverType: CreateProjectInput["serverType"]): string {
  switch (serverType) {
    case "apache":
      return "Apache";
    case "nginx":
      return "Nginx";
    case "frankenphp":
      return "FrankenPHP";
  }
}

interface AddProjectWizardProps {
  open: boolean;
  recentPaths: string[];
  onClose: () => void;
  onCreated: (project: Project) => void;
}

const defaultForm: CreateProjectInput = {
  name: "",
  path: "",
  domain: "",
  serverType: "apache",
  phpVersion: "8.2",
  framework: "unknown",
  documentRoot: ".",
  sslEnabled: false,
  databaseName: null,
  databasePort: 3306,
};

function buildValidationError(form: CreateProjectInput): string | undefined {
  const pathCheck = projectPathSchema.safeParse(form.path);
  if (!pathCheck.success) {
    return pathCheck.error.issues[0]?.message ?? "Project path is required.";
  }

  const nameCheck = projectNameSchema.safeParse(form.name);
  if (!nameCheck.success) {
    return nameCheck.error.issues[0]?.message ?? "Project name is invalid.";
  }

  const domainCheck = domainSchema.safeParse(form.domain);
  if (!domainCheck.success) {
    return domainCheck.error.issues[0]?.message ?? "Domain is invalid.";
  }

  const rootCheck = documentRootSchema.safeParse(form.documentRoot);
  if (!rootCheck.success) {
    return rootCheck.error.issues[0]?.message ?? "Document root is invalid.";
  }

  return undefined;
}

function nameFromPath(path: string): string {
  const segments = path.split(/[\\/]/).filter(Boolean);
  const tail = segments.length > 0 ? segments[segments.length - 1] : "Project";
  return tail
    .split(/[-_\s]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

function provisioningMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    return String(error.message);
  }

  return "DevNest could not finish the automatic provisioning handoff.";
}

export function AddProjectWizard({ open, recentPaths, onClose, onCreated }: AddProjectWizardProps) {
  const [stepIndex, setStepIndex] = useState(0);
  const [form, setForm] = useState<CreateProjectInput>(defaultForm);
  const [scanResult, setScanResult] = useState<ScanResult>();
  const [createdProject, setCreatedProject] = useState<Project>();
  const [submitting, setSubmitting] = useState(false);
  const [message, setMessage] = useState<string>();
  const [completionNotes, setCompletionNotes] = useState<string[]>([]);
  const [runtimeInventory, setRuntimeInventory] = useState<RuntimeInventoryItem[]>([]);
  const contentRef = useRef<HTMLDivElement>(null);
  const pushToast = useToastStore((state) => state.push);

  useEffect(() => {
    if (!open) {
      return;
    }

    setStepIndex(0);
    setForm(defaultForm);
    setScanResult(undefined);
    setCreatedProject(undefined);
    setSubmitting(false);
    setMessage(undefined);
    setCompletionNotes([]);
  }, [open]);

  useEffect(() => {
    if (!open) {
      return;
    }

    runtimeApi.list().then(setRuntimeInventory).catch(() => setRuntimeInventory([]));
  }, [open]);

  useEffect(() => {
    if (!open) {
      return;
    }

    function handleKeydown(event: KeyboardEvent) {
      if (event.key !== "Escape") {
        return;
      }

      event.preventDefault();
      onClose();
    }

    document.addEventListener("keydown", handleKeydown);
    return () => document.removeEventListener("keydown", handleKeydown);
  }, [onClose, open]);

  useEffect(() => {
    if (!open) {
      return;
    }

    contentRef.current?.scrollTo({ top: 0, left: 0, behavior: "auto" });
  }, [open, stepIndex]);

  const uniqueRecentPaths = useMemo(
    () => Array.from(new Set(recentPaths.filter(Boolean))).slice(0, 5),
    [recentPaths],
  );
  const activeFrankenphpPhpFamily = useMemo(
    () => activeFrankenphpVersionFamily(runtimeInventory),
    [runtimeInventory],
  );
  const trackedFrankenphpPhpFamilies = useMemo(
    () => frankenphpVersionFamilies(runtimeInventory),
    [runtimeInventory],
  );
  const phpVersionOptions = useMemo(
    () => {
      if (form.serverType === "frankenphp") {
        const embeddedFamilies =
          activeFrankenphpPhpFamily != null
            ? [activeFrankenphpPhpFamily]
            : trackedFrankenphpPhpFamilies;
        const options = [...embeddedFamilies];
        if (form.phpVersion && !options.includes(form.phpVersion)) {
          options.push(form.phpVersion);
        }

        return Array.from(new Set(options)).sort((left, right) =>
          left.localeCompare(right, undefined, { numeric: true }),
        );
      }

      const installed = installedPhpVersionFamilies(runtimeInventory);
      if (form.phpVersion && !installed.includes(form.phpVersion)) {
        return [...installed, form.phpVersion].sort((left, right) =>
          left.localeCompare(right, undefined, { numeric: true }),
        );
      }
      return installed;
    },
    [
      activeFrankenphpPhpFamily,
      form.phpVersion,
      form.serverType,
      runtimeInventory,
      trackedFrankenphpPhpFamilies,
    ],
  );
  const frankenphpPhpLocked =
    form.serverType === "frankenphp" && activeFrankenphpPhpFamily != null;

  useEffect(() => {
    if (!open || form.serverType !== "frankenphp" || !activeFrankenphpPhpFamily) {
      return;
    }

    if (runtimeVersionMatches(form.phpVersion, activeFrankenphpPhpFamily)) {
      return;
    }

    setForm((current) => {
      if (
        current.serverType !== "frankenphp" ||
        runtimeVersionMatches(current.phpVersion, activeFrankenphpPhpFamily)
      ) {
        return current;
      }

      return { ...current, phpVersion: activeFrankenphpPhpFamily };
    });
  }, [activeFrankenphpPhpFamily, form.phpVersion, form.serverType, open]);

  function resetWizard() {
    setStepIndex(0);
    setForm(defaultForm);
    setScanResult(undefined);
    setCreatedProject(undefined);
    setSubmitting(false);
    setMessage(undefined);
    setCompletionNotes([]);
  }

  async function handleChooseFolder() {
    try {
      const selectedPath = await projectApi.pickFolder();
      if (!selectedPath) {
        return;
      }

      setForm((current) => ({
        ...current,
        path: selectedPath,
        name: current.name || nameFromPath(selectedPath),
      }));
      setMessage(undefined);
    } catch (error) {
      if (typeof error === "object" && error !== null && "message" in error) {
        const nextMessage = String(error.message);
        setMessage(nextMessage);
        pushToast({
          tone: "error",
          title: "Folder picker failed",
          message: nextMessage,
        });
      } else {
        const nextMessage = "Failed to open the native folder picker. Paste the path manually.";
        setMessage(nextMessage);
        pushToast({
          tone: "error",
          title: "Folder picker failed",
          message: nextMessage,
        });
      }
    }
  }

  if (!open) {
    return null;
  }

  async function runScan() {
    const pathCheck = projectPathSchema.safeParse(form.path);
    if (!pathCheck.success) {
      setMessage(pathCheck.error.issues[0]?.message ?? "Project path is required.");
      return false;
    }

    setSubmitting(true);
    setMessage(undefined);

    try {
      const result = await projectApi.scan(form.path.trim());
      setScanResult(result);
      setForm((current) => ({
        ...current,
        name: current.name || nameFromPath(current.path),
        domain: result.suggestedDomain,
        serverType: result.recommendedServer,
        phpVersion: result.recommendedPhpVersion ?? current.phpVersion,
        framework: result.framework,
        documentRoot: result.documentRoot,
      }));
      setStepIndex(1);
      pushToast({
        tone: "success",
        title: "Scan complete",
        message: `${result.framework} project detected for ${result.suggestedDomain}.`,
        durationMs: 2600,
      });
      return true;
    } catch (error) {
      if (typeof error === "object" && error !== null && "message" in error) {
        const nextMessage = String(error.message);
        setMessage(nextMessage);
        pushToast({
          tone: "error",
          title: "Project scan failed",
          message: nextMessage,
        });
      } else {
        const nextMessage = "Failed to scan project.";
        setMessage(nextMessage);
        pushToast({
          tone: "error",
          title: "Project scan failed",
          message: nextMessage,
        });
      }
      return false;
    } finally {
      setSubmitting(false);
    }
  }

  async function handleCreate() {
    const validationError = buildValidationError(form);
    if (validationError) {
      setMessage(validationError);
      return;
    }

    setSubmitting(true);
    setMessage(undefined);

    try {
      const created = await projectApi.create(form);
      const nextNotes = ["Project saved to the local workspace."];

      try {
        await configApi.generate(created.id);
        nextNotes.push(`${serverTypeLabel(created.serverType)} config generated.`);

        await configApi.applyHosts(created.domain);
        nextNotes.push(`${created.domain} added to local hosts.`);

        const service = await serviceApi.get(created.serverType);
        if (service.status === "running") {
          await serviceApi.restart(created.serverType);
          nextNotes.push(`${serverTypeLabel(created.serverType)} restarted.`);
        } else {
          nextNotes.push(`${serverTypeLabel(created.serverType)} is stopped, so restart was skipped.`);
        }
      } catch (provisionError) {
        const nextMessage = provisioningMessage(provisionError);
        nextNotes.push(`Setup paused: ${nextMessage}`);
        setMessage(
          `Project was created, but setup did not fully complete: ${nextMessage}`,
        );
        pushToast({
          tone: "error",
          title: "Auto-provisioning incomplete",
          message: nextMessage,
        });
      }

      setCompletionNotes(nextNotes);
      setCreatedProject(created);
      setStepIndex(4);
      pushToast({
        tone: "success",
        title: "Project created",
        message: `${created.name} is now tracked in DevNest and auto-provisioned where possible.`,
      });
      onCreated(created);
    } catch (error) {
      if (typeof error === "object" && error !== null && "message" in error) {
        const nextMessage = String(error.message);
        setMessage(nextMessage);
        pushToast({
          tone: "error",
          title: "Create project failed",
          message: nextMessage,
        });
      } else {
        const nextMessage = "Failed to create project.";
        setMessage(nextMessage);
        pushToast({
          tone: "error",
          title: "Create project failed",
          message: nextMessage,
        });
      }
    } finally {
      setSubmitting(false);
    }
  }

  async function handleNext() {
    if (stepIndex === 0) {
      await runScan();
      return;
    }

    if (stepIndex === 1) {
      setStepIndex(2);
      return;
    }

    if (stepIndex === 2) {
      const validationError = buildValidationError(form);
      if (validationError) {
        setMessage(validationError);
        return;
      }

      setMessage(undefined);
      setStepIndex(3);
      return;
    }

    if (stepIndex === 3) {
      await handleCreate();
    }
  }

  function handleBack() {
    if (stepIndex === 0) {
      onClose();
      return;
    }

    if (stepIndex === 4) {
      onClose();
      return;
    }

    setMessage(undefined);
    setStepIndex((current) => current - 1);
  }

  return (
    <div className="wizard-overlay" onClick={onClose} role="dialog" aria-modal="true">
      <div
        className="wizard-dialog"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="wizard-header">
          <div>
            <h2>Add Project</h2>
            <p>Import a local PHP app, review the scan, and save it as a project.</p>
          </div>
          <Button onClick={onClose}>Close</Button>
        </div>

        <div className="wizard-stepper">
          {steps.map((step, index) => (
            <div className="wizard-step" data-active={index === stepIndex} data-complete={index < stepIndex} key={step}>
              <span className="wizard-step-index">{index + 1}</span>
              <span>{step}</span>
            </div>
          ))}
        </div>

        <div className="wizard-content" ref={contentRef}>
          {stepIndex === 0 ? (
            <Card>
              <div className="page-header">
                <div>
                  <h3>Select Folder</h3>
                  <p>Choose the project folder you want DevNest to manage.</p>
                </div>
              </div>
              <div className="stack">
                <div className="detail-item">
                  <span className="detail-label">Folder Picker</span>
                  <div className="page-toolbar" style={{ justifyContent: "flex-start" }}>
                    <Button
                      busy={submitting && stepIndex === 0}
                      busyLabel="Choosing folder..."
                      onClick={() => void handleChooseFolder()}
                      variant="primary"
                    >
                      Choose Folder
                    </Button>
                    <span className="helper-text">Use the folder picker or paste a path below.</span>
                  </div>
                </div>

                <div className="field">
                  <label htmlFor="wizard-project-path">Project Path</label>
                  <input
                    className="input"
                    id="wizard-project-path"
                    onChange={(event) =>
                      setForm((current) => ({
                        ...current,
                        path: event.target.value,
                        name: current.name || nameFromPath(event.target.value),
                      }))
                    }
                    placeholder="D:/Sites/shop-api"
                    value={form.path}
                  />
                </div>

                {uniqueRecentPaths.length > 0 ? (
                  <div className="stack" style={{ gap: 10 }}>
                    <span className="detail-label">Recent Paths</span>
                    <div className="list-row-meta">
                      {uniqueRecentPaths.map((path) => (
                        <button
                          className="status-chip"
                          key={path}
                          onClick={() =>
                            setForm((current) => ({
                              ...current,
                              path,
                              name: current.name || nameFromPath(path),
                            }))
                          }
                          type="button"
                        >
                          {path}
                        </button>
                      ))}
                    </div>
                  </div>
                ) : null}

                <div className="detail-item">
                  <span className="detail-label">Provisioning Scope</span>
                  <div className="stack" style={{ gap: 8 }}>
                    <span className="helper-text">DevNest scans the folder, saves the project, and prepares managed config.</span>
                    <span className="helper-text">Hosts and runtime actions stay available after import from the project inspector and Services page.</span>
                  </div>
                </div>
              </div>
            </Card>
          ) : null}

          {stepIndex === 1 && scanResult ? (
            <Card>
              <div className="page-header">
                <div>
                  <h3>Scan Results</h3>
                  <p>Review what Smart Scan found before continuing.</p>
                </div>
              </div>
              <div className="detail-grid" style={{ gridTemplateColumns: "repeat(3, minmax(0, 1fr))", }}>
                <div className="detail-item">
                  <span className="detail-label">Framework</span>
                  <strong>{scanResult.framework}</strong>
                </div>
                <div className="detail-item">
                  <span className="detail-label">Recommended Server</span>
                  <strong>{scanResult.recommendedServer}</strong>
                  {scanResult.serverReason ? (
                    <span className="helper-text">{scanResult.serverReason}</span>
                  ) : null}
                </div>
                <div className="detail-item">
                  <span className="detail-label">PHP Hint</span>
                  <strong>{scanResult.recommendedPhpVersion ?? "No explicit hint"}</strong>
                </div>
                <div className="detail-item">
                  <span className="detail-label">Suggested Domain</span>
                  <strong>{scanResult.suggestedDomain}</strong>
                </div>
                <div className="detail-item" style={{ gridColumn: "span 2" }}>
                  <span className="detail-label">Document Root</span>
                  <strong>{scanResult.documentRoot}</strong>
                  {scanResult.documentRootReason ? (
                    <span className="helper-text">{scanResult.documentRootReason}</span>
                  ) : null}
                </div>
              </div>

              <div className="stack">
                <div className="stack" style={{ gap: 8 }}>
                  <span className="detail-label">Detected Files</span>
                  <div className="list-row-meta">
                    {scanResult.detectedFiles.map((file) => (
                      <span className="status-chip" key={file}>
                        {file}
                      </span>
                    ))}
                  </div>
                </div>

                {scanResult.missingPhpExtensions.length > 0 ? (
                  <div className="stack" style={{ gap: 8 }}>
                    <span className="detail-label">Extension Hints</span>
                    <div className="list-row-meta">
                      {scanResult.missingPhpExtensions.map((extension) => (
                        <span className="status-chip" key={extension}>
                          ext-{extension}
                        </span>
                      ))}
                    </div>
                  </div>
                ) : null}

                {scanResult.warnings.length > 0 ? (
                  <div className="stack" style={{ gap: 8 }}>
                    <span className="detail-label">Warnings</span>
                    <div className="stack" style={{ gap: 6 }}>
                      {scanResult.warnings.map((warning) => (
                        <span className="error-text" key={warning}>
                          {warning}
                        </span>
                      ))}
                    </div>
                  </div>
                ) : (
                  <div className="detail-item">
                    <span className="detail-label">Scan Summary</span>
                    <span className="helper-text">No blocking warnings. The inferred project profile is ready for review.</span>
                  </div>
                )}
              </div>
            </Card>
          ) : null}

          {stepIndex === 2 ? (
            <Card>
              <div className="page-header">
                <div>
                  <h3>Configure</h3>
                  <p>Adjust the saved project settings before import.</p>
                </div>
              </div>
              <div className="form-grid">
                <div className="field">
                  <label htmlFor="wizard-name">Project Name</label>
                  <input
                    className="input"
                    id="wizard-name"
                    onChange={(event) => setForm((current) => ({ ...current, name: event.target.value }))}
                    value={form.name}
                  />
                </div>
                <div className="field">
                  <label htmlFor="wizard-domain">Domain</label>
                  <input
                    className="input"
                    id="wizard-domain"
                    onChange={(event) => setForm((current) => ({ ...current, domain: event.target.value }))}
                    value={form.domain}
                  />
                </div>
                <div className="field">
                  <label htmlFor="wizard-server">Web Server</label>
                  <select
                    className="select"
                    id="wizard-server"
                    onChange={(event) =>
                      setForm((current) => {
                        const serverType = event.target.value as CreateProjectInput["serverType"];
                        return {
                          ...current,
                          serverType,
                          phpVersion:
                            serverType === "frankenphp" && activeFrankenphpPhpFamily
                              ? activeFrankenphpPhpFamily
                              : current.phpVersion,
                        };
                      })
                    }
                    value={form.serverType}
                  >
                    <option value="apache">Apache</option>
                    <option value="nginx">Nginx</option>
                    <option value="frankenphp">FrankenPHP (Experimental)</option>
                  </select>
                </div>
                <div className="field">
                  <label htmlFor="wizard-php">PHP Version</label>
                  <select
                    className="select"
                    disabled={frankenphpPhpLocked}
                    id="wizard-php"
                    onChange={(event) => setForm((current) => ({ ...current, phpVersion: event.target.value }))}
                    value={form.phpVersion}
                  >
                    {phpVersionOptions.map((version) => (
                      <option key={version} value={version}>
                        PHP {version}
                      </option>
                    ))}
                  </select>
                </div>
                <div className="field">
                  <label htmlFor="wizard-framework">Framework</label>
                  <select
                    className="select"
                    id="wizard-framework"
                    onChange={(event) =>
                      setForm((current) => ({
                        ...current,
                        framework: event.target.value as CreateProjectInput["framework"],
                      }))
                    }
                    value={form.framework}
                  >
                    <option value="laravel">Laravel</option>
                    <option value="symfony">Symfony</option>
                    <option value="wordpress">WordPress</option>
                    <option value="php">PHP</option>
                    <option value="unknown">Unknown</option>
                  </select>
                </div>
                <div className="field">
                  <label htmlFor="wizard-root">Document Root</label>
                  <input
                    className="input"
                    id="wizard-root"
                    onChange={(event) => setForm((current) => ({ ...current, documentRoot: event.target.value }))}
                    value={form.documentRoot}
                  />
                </div>
                <div className="field">
                  <label htmlFor="wizard-db-name">Database Name</label>
                  <input
                    className="input"
                    id="wizard-db-name"
                    onChange={(event) =>
                      setForm((current) => ({
                        ...current,
                        databaseName: event.target.value || null,
                      }))
                    }
                    value={form.databaseName ?? ""}
                  />
                </div>
                <div className="field">
                  <label htmlFor="wizard-db-port">Database Port</label>
                  <input
                    className="input"
                    id="wizard-db-port"
                    onChange={(event) =>
                      setForm((current) => ({
                        ...current,
                        databasePort: event.target.value ? Number(event.target.value) : null,
                      }))
                    }
                    value={form.databasePort ?? ""}
                  />
                </div>
              </div>

              {form.serverType === "frankenphp" ? (
                <>
                  <span className="helper-text">
                    {activeFrankenphpPhpFamily
                      ? `Active FrankenPHP embeds PHP ${activeFrankenphpPhpFamily}. DevNest keeps this project synced to that embedded PHP family; switch the active FrankenPHP runtime in Settings to change it.`
                      : trackedFrankenphpPhpFamilies.length > 0
                        ? "FrankenPHP does not need a standalone PHP runtime. Pick the embedded PHP family you want, then activate a matching FrankenPHP runtime in Settings."
                        : "FrankenPHP does not need a standalone PHP runtime. Install or link FrankenPHP first, then DevNest will sync this project to its embedded PHP family."}
                  </span>
                  <div className="inline-note-card" data-tone="warning">
                    <strong>Experimental web server</strong>
                    <span>
                      {activeFrankenphpPhpFamily
                        ? `FrankenPHP runs with its own embedded PHP runtime on Windows. This project is pinned to the active FrankenPHP PHP ${activeFrankenphpPhpFamily} family.`
                        : "FrankenPHP runs with its own embedded PHP runtime on Windows. No separate PHP install is required, but DevNest still validates the embedded PHP family before start."}
                    </span>
                  </div>
                </>
              ) : null}

              <label className="checkbox-row">
                <input
                  checked={form.sslEnabled}
                  onChange={(event) => setForm((current) => ({ ...current, sslEnabled: event.target.checked }))}
                  type="checkbox"
                />
                <span>Provision local SSL certs and HTTPS config for this project</span>
              </label>
            </Card>
          ) : null}

          {stepIndex === 3 ? (
            <Card>
              <div className="page-header">
                <div>
                  <h3>Review</h3>
                  <p>Review the saved settings and setup steps before import.</p>
                </div>
              </div>

              <div className="detail-grid">
                <div className="detail-item">
                  <span className="detail-label">Project</span>
                  <strong>{form.name}</strong>
                </div>
                <div className="detail-item">
                  <span className="detail-label">Domain</span>
                  <strong>{form.domain}</strong>
                </div>
                <div className="detail-item">
                  <span className="detail-label">Server</span>
                  <strong>{serverTypeLabel(form.serverType)}</strong>
                </div>
                <div className="detail-item">
                  <span className="detail-label">PHP</span>
                  <strong>{form.phpVersion}</strong>
                </div>
                <div className="detail-item">
                  <span className="detail-label">Framework</span>
                  <strong>{form.framework}</strong>
                </div>
                <div className="detail-item">
                  <span className="detail-label">Document Root</span>
                  <strong>{form.documentRoot}</strong>
                </div>
              </div>

              <div className="stack">
                <div className="detail-item">
                  <span className="detail-label">Provisioning Summary</span>
                  <div className="stack" style={{ gap: 8 }}>
                    <span className="helper-text">Save the project to the local registry.</span>
                    <span className="helper-text">Generate managed {serverTypeLabel(form.serverType)} config for <span className="mono">{form.domain}</span>.</span>
                    <span className="helper-text">Use <span className="mono">{form.documentRoot}</span> as the document root.</span>
                    <span className="helper-text">Apply hosts changes when permission is available.</span>
                    {form.serverType === "frankenphp" ? (
                      <span className="helper-text">
                        {activeFrankenphpPhpFamily
                          ? `Use embedded PHP ${activeFrankenphpPhpFamily} from the active FrankenPHP runtime. No standalone PHP install is required.`
                          : "Use the embedded PHP family from a matching FrankenPHP runtime. No standalone PHP install is required."}
                      </span>
                    ) : null}
                  </div>
                </div>
              </div>
            </Card>
          ) : null}

          {stepIndex === 4 && createdProject ? (
            <div className="stack">
              <Card>
                <div className="page-header">
                  <div>
                  <h3>Complete</h3>
                    <p>Project imported. You can review setup details and continue from the project workspace.</p>
                  </div>
                </div>

                <div className="detail-grid">
                  <div className="detail-item">
                    <span className="detail-label">Project</span>
                    <strong>{createdProject.name}</strong>
                  </div>
                  <div className="detail-item">
                    <span className="detail-label">Domain</span>
                    <strong>{createdProject.domain}</strong>
                  </div>
                  <div className="detail-item">
                    <span className="detail-label">Server</span>
                    <strong>{serverTypeLabel(createdProject.serverType)}</strong>
                  </div>
                  <div className="detail-item">
                    <span className="detail-label">PHP</span>
                    <strong>{createdProject.phpVersion}</strong>
                  </div>
                </div>

                <div className="detail-item">
                  <span className="detail-label">Completed Steps</span>
                  <div className="stack" style={{ gap: 8 }}>
                    {completionNotes.length > 0 ? (
                      completionNotes.map((note) => (
                        <span className="helper-text" key={note}>
                          {note}
                        </span>
                      ))
                    ) : (
                      <>
                        <span className="helper-text">Review the managed config before writing it to disk.</span>
                        <span className="helper-text">Apply the hosts entry when local permissions are ready.</span>
                        <span className="helper-text">Use the project inspector or Services page to start the linked runtime and view logs.</span>
                      </>
                    )}
                  </div>
                </div>

                <div className="page-toolbar" style={{ justifyContent: "flex-start" }}>
                  <Button onClick={resetWizard}>Import Another</Button>
                  <Button onClick={onClose} variant="primary">
                    Inspect Project
                  </Button>
                </div>
              </Card>

              <ProjectProvisioningPanel
                description="Use this panel to review, regenerate, or retry any setup step."
                project={createdProject}
                title="Project Setup"
              />
            </div>
          ) : null}
        </div>

        <div className="wizard-footer">
          <div className="stack" style={{ gap: 6 }}>
            {message ? <span className="error-text">{message}</span> : <span className="helper-text">Ready to scan or save the project.</span>}
          </div>
          <div className="page-toolbar">
            <Button onClick={handleBack}>{stepIndex === 0 || stepIndex === 4 ? "Close" : "Back"}</Button>
            {stepIndex < 4 ? (
              <Button
                busy={submitting}
                busyLabel={
                  stepIndex === 3
                    ? "Creating project..."
                    : stepIndex === 0
                      ? "Scanning project..."
                      : undefined
                }
                disabled={submitting}
                onClick={() => void handleNext()}
                variant="primary"
              >
                {stepIndex === 3 ? "Create Project" : stepIndex === 0 ? "Scan Project" : "Next"}
              </Button>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}
