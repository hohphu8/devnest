import { useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useToastStore } from "@/app/store/toast-store";
import { useProjectStore } from "@/app/store/project-store";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { StickyTabs } from "@/components/ui/sticky-tabs";
import { reliabilityApi } from "@/lib/api/reliability-api";
import { getAppErrorMessage } from "@/lib/tauri";
import { formatUpdatedAt } from "@/lib/utils";
import type {
  ActionPreflightReport,
  ReliabilityAction,
  ReliabilityInspectorSnapshot,
  RepairWorkflow,
  RepairWorkflowInfo,
} from "@/types/reliability";

interface ReliabilityWorkbenchProps {
  projectId?: string | null;
}

type ReliabilityTab = "preflight" | "repair" | "inspector" | "backup";

const RELIABILITY_ACTIONS: Array<{
  action: ReliabilityAction;
  label: string;
  summary: string;
}> = [
  {
    action: "provisionProject",
    label: "Provision Preflight",
    summary: "Check config render, runtime links, and local domain state before config/hosts work.",
  },
  {
    action: "publishPersistentDomain",
    label: "Publish Preflight",
    summary: "Check named tunnel readiness, hostname availability, and local origin before publish.",
  },
  {
    action: "startProjectRuntime",
    label: "Start Preflight",
    summary: "Check runtime binaries and port ownership before starting the selected web service.",
  },
];

function preflightTone(report?: ActionPreflightReport) {
  if (!report) {
    return "warning";
  }

  return report.ready ? "success" : "error";
}

export function ReliabilityWorkbench({ projectId }: ReliabilityWorkbenchProps) {
  const [searchParams, setSearchParams] = useSearchParams();
  const pushToast = useToastStore((state) => state.push);
  const projects = useProjectStore((state) => state.projects);
  const [selectedProjectId, setSelectedProjectId] = useState<string | undefined>(projectId ?? undefined);
  const [workflows, setWorkflows] = useState<RepairWorkflowInfo[]>([]);
  const [workflowsLoading, setWorkflowsLoading] = useState(false);
  const [inspector, setInspector] = useState<ReliabilityInspectorSnapshot | null>(null);
  const [inspectorLoading, setInspectorLoading] = useState(false);
  const [actionLoading, setActionLoading] = useState<string | null>(null);
  const [preflights, setPreflights] = useState<Partial<Record<ReliabilityAction, ActionPreflightReport>>>({});
  const [restorePreflight, setRestorePreflight] = useState<ActionPreflightReport | null>(null);

  useEffect(() => {
    if (projectId && projectId !== selectedProjectId) {
      setSelectedProjectId(projectId);
    }
  }, [projectId, selectedProjectId]);

  useEffect(() => {
    if (!selectedProjectId && projects.length > 0) {
      setSelectedProjectId(projects[0]?.id);
    }
  }, [projects, selectedProjectId]);

  useEffect(() => {
    let cancelled = false;
    setWorkflowsLoading(true);
    reliabilityApi
      .listRepairWorkflows()
      .then((items) => {
        if (!cancelled) {
          setWorkflows(items);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          pushToast({
            tone: "error",
            title: "Reliability load failed",
            message: getAppErrorMessage(error, "Could not load repair workflows."),
          });
        }
      })
      .finally(() => {
        if (!cancelled) {
          setWorkflowsLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [pushToast]);

  useEffect(() => {
    if (!selectedProjectId) {
      setInspector(null);
      setPreflights({});
      return;
    }

    let cancelled = false;
    setInspectorLoading(true);
    Promise.all([
      reliabilityApi.inspectState(selectedProjectId),
      ...RELIABILITY_ACTIONS.map((item) => reliabilityApi.runPreflight(item.action, selectedProjectId)),
      reliabilityApi.runPreflight("restoreAppMetadata"),
    ])
      .then(([nextInspector, ...reports]) => {
        if (cancelled) {
          return;
        }

        const actionReports = reports.slice(0, RELIABILITY_ACTIONS.length) as ActionPreflightReport[];
        const restoreReport = reports[reports.length - 1] as ActionPreflightReport;
        setInspector(nextInspector as ReliabilityInspectorSnapshot);
        setPreflights(
          Object.fromEntries(
            actionReports.map((report) => [report.action, report]),
          ) as Partial<Record<ReliabilityAction, ActionPreflightReport>>,
        );
        setRestorePreflight(restoreReport);
      })
      .catch((error) => {
        if (!cancelled) {
          setInspector(null);
          pushToast({
            tone: "error",
            title: "Reliability inspect failed",
            message: getAppErrorMessage(error, "Could not inspect the selected project state."),
          });
        }
      })
      .finally(() => {
        if (!cancelled) {
          setInspectorLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [pushToast, selectedProjectId]);

  const selectedProject = useMemo(
    () => projects.find((project) => project.id === selectedProjectId),
    [projects, selectedProjectId],
  );
  const reliabilityTabs = [
    { id: "preflight", label: "Preflight", meta: "Gate risky actions" },
    { id: "repair", label: "Repair", meta: workflowsLoading ? "loading" : `${workflows.length} flows` },
    { id: "inspector", label: "Inspector", meta: inspectorLoading ? "loading" : "state + diagnostics" },
    { id: "backup", label: "Backup", meta: "metadata + handoff" },
  ] as const;
  const activeTab = (() => {
    const tab = searchParams.get("tab");
    if (tab === "repair" || tab === "inspector" || tab === "backup") {
      return tab;
    }
    return "preflight";
  })();

  function handleSelectTab(tab: ReliabilityTab) {
    const next = new URLSearchParams(searchParams);
    if (tab === "preflight") {
      next.delete("tab");
    } else {
      next.set("tab", tab);
    }
    setSearchParams(next);
  }

  async function refreshInspector() {
    if (!selectedProjectId) {
      return;
    }

    setActionLoading("refresh");
    try {
      const [nextInspector, ...reports] = await Promise.all([
        reliabilityApi.inspectState(selectedProjectId),
        ...RELIABILITY_ACTIONS.map((item) => reliabilityApi.runPreflight(item.action, selectedProjectId)),
        reliabilityApi.runPreflight("restoreAppMetadata"),
      ]);
      const actionReports = reports.slice(0, RELIABILITY_ACTIONS.length) as ActionPreflightReport[];
      const restoreReport = reports[reports.length - 1] as ActionPreflightReport;
      setInspector(nextInspector);
      setPreflights(
        Object.fromEntries(
          actionReports.map((report) => [report.action, report]),
        ) as Partial<Record<ReliabilityAction, ActionPreflightReport>>,
      );
      setRestorePreflight(restoreReport);
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Refresh failed",
        message: getAppErrorMessage(error, "Could not refresh the reliability state."),
      });
    } finally {
      setActionLoading(null);
    }
  }

  async function handleRunPreflight(action: ReliabilityAction) {
    if (!selectedProjectId) {
      return;
    }

    setActionLoading(`preflight:${action}`);
    try {
      const report = await reliabilityApi.runPreflight(action, selectedProjectId);
      setPreflights((current) => ({ ...current, [action]: report }));
      pushToast({
        tone: report.ready ? "success" : "warning",
        title: "Preflight updated",
        message: report.summary,
      });
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Preflight failed",
        message: getAppErrorMessage(error, "Could not run the requested preflight."),
      });
    } finally {
      setActionLoading(null);
    }
  }

  async function handleRepair(workflow: RepairWorkflow) {
    if (!selectedProjectId || !selectedProject) {
      return;
    }

    setActionLoading(`repair:${workflow}`);
    try {
      const result = await reliabilityApi.runRepairWorkflow(selectedProjectId, workflow);
      pushToast({
        tone: "success",
        title: "Repair complete",
        message: result.message,
      });
      await refreshInspector();
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Repair failed",
        message: getAppErrorMessage(error, "Could not complete the requested repair workflow."),
      });
    } finally {
      setActionLoading(null);
    }
  }

  async function handleExportDiagnosticsBundle() {
    if (!selectedProjectId || !selectedProject) {
      return;
    }

    setActionLoading("export-bundle");
    try {
      const result = await reliabilityApi.exportDiagnosticsBundle(selectedProjectId);
      if (!result) {
        return;
      }

      pushToast({
        tone: "success",
        title: "Bundle exported",
        message: `${selectedProject.name} diagnostics bundle was exported to ${result.path}.`,
      });
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Bundle export failed",
        message: getAppErrorMessage(error, "Could not export the diagnostics bundle."),
      });
    } finally {
      setActionLoading(null);
    }
  }

  async function handleBackupMetadata() {
    setActionLoading("backup-metadata");
    try {
      const result = await reliabilityApi.backupAppMetadata();
      if (!result) {
        return;
      }

      pushToast({
        tone: "success",
        title: "Metadata backed up",
        message: `DevNest app metadata was exported to ${result.path}.`,
      });
      setRestorePreflight(await reliabilityApi.runPreflight("restoreAppMetadata"));
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Metadata backup failed",
        message: getAppErrorMessage(error, "Could not back up DevNest metadata."),
      });
    } finally {
      setActionLoading(null);
    }
  }

  async function handleRestoreMetadata() {
    setActionLoading("restore-metadata");
    try {
      const report = await reliabilityApi.runPreflight("restoreAppMetadata");
      setRestorePreflight(report);
      if (!report.ready) {
        pushToast({
          tone: "warning",
          title: "Restore blocked",
          message: report.summary,
        });
        return;
      }

      const result = await reliabilityApi.restoreAppMetadata();
      if (!result) {
        return;
      }

      pushToast({
        tone: "success",
        title: "Metadata restored",
        message: `DevNest restored metadata from ${result.path}. Refreshing inspector state is recommended now.`,
      });
      await refreshInspector();
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Metadata restore failed",
        message: getAppErrorMessage(error, "Could not restore DevNest metadata."),
      });
    } finally {
      setActionLoading(null);
    }
  }

  return (
    <div className="stack" style={{ gap: 8 }}>
      <Card>
        <div className="page-header">
          <div>
            <select
              className="select"
              disabled={projects.length === 0}
              onChange={(event) => setSelectedProjectId(event.target.value)}
              value={selectedProjectId ?? ""}
            >
              {projects.map((project) => (
                <option key={project.id} value={project.id}>
                  {project.name} ({project.domain})
                </option>
              ))}
            </select>
          </div>
          <div className="page-toolbar">
            <Button disabled={actionLoading === "refresh"} onClick={() => void refreshInspector()}>
              {actionLoading === "refresh" ? "Refreshing..." : "Refresh State"}
            </Button>
          </div>
        </div>

        {selectedProject ? (
          <div className="detail-grid">
            <div className="detail-item">
              <span className="detail-label">Project</span>
              <strong>{selectedProject.name}</strong>
            </div>
            <div className="detail-item">
              <span className="detail-label">Domain</span>
              <strong className="mono detail-value">{selectedProject.domain}</strong>
            </div>
            <div className="detail-item">
              <span className="detail-label">Server</span>
              <strong>{selectedProject.serverType}</strong>
            </div>
            <div className="detail-item">
              <span className="detail-label">PHP</span>
              <strong>{selectedProject.phpVersion}</strong>
            </div>
          </div>
        ) : (
          <EmptyState
            title="No project selected"
            description="Import a project first so DevNest can build repair workflows and state inspector data."
          />
        )}
      </Card>

      <div className="stack workspace-shell">
        <StickyTabs
          activeTab={activeTab}
          ariaLabel="Reliability sections"
          items={reliabilityTabs}
          onSelect={handleSelectTab}
        />

        <div
          aria-labelledby="workspace-tab-preflight"
          className="workspace-panel"
          hidden={activeTab !== "preflight"}
          id="workspace-panel-preflight"
          role="tabpanel"
        >
      <Card>
        <div className="page-header">
          <div>
            <h2>Action Preflight</h2>
            <p>Run bounded safety checks before provisioning, starting runtime, publishing, or restoring workspace metadata.</p>
          </div>
        </div>

        <div className="route-grid reliability-preflight-grid" data-columns="3">
          {RELIABILITY_ACTIONS.map((item) => {
            const report = preflights[item.action];
            return (
              <div className="detail-item reliability-preflight-item" key={item.action}>
                <div className="page-toolbar reliability-preflight-head" style={{ alignItems: "flex-start" }}>
                  <div className="reliability-preflight-copy">
                    <strong>{item.label}</strong>
                    <p style={{ marginTop: 6 }}>{item.summary}</p>
                  </div>
                  <span className="status-chip" data-tone={preflightTone(report)}>
                    {report ? (report.ready ? "ready" : "blocked") : "pending"}
                  </span>
                  <Button
                    disabled={!selectedProjectId || actionLoading === `preflight:${item.action}`}
                    onClick={() => void handleRunPreflight(item.action)}
                    variant="primary"
                  >
                    {actionLoading === `preflight:${item.action}` ? "Running..." : "Run"}
                  </Button>
                </div>
                <span className="helper-text">{report?.summary ?? "Run the preflight to populate checks."}</span>
                {report?.checks.length ? (
                  <div className="stack" style={{ gap: 8, marginTop: 12 }}>
                    {report.checks.map((check) => (
                      <div className="detail-item reliability-preflight-check" key={`${item.action}:${check.code}`}>
                        <div className="page-toolbar reliability-preflight-check-head">
                          <strong>{check.title}</strong>
                          <span className="status-chip" data-tone={check.status === "ok" ? "success" : check.status === "warning" ? "warning" : "error"}>
                            {check.status}
                          </span>
                        </div>
                        <span className="helper-text">{check.message}</span>
                        {check.suggestion ? <span className="helper-text">{check.suggestion}</span> : null}
                      </div>
                    ))}
                  </div>
                ) : null}
              </div>
            );
          })}
        </div>

        <div className="detail-item reliability-preflight-item" style={{ marginTop: 16 }}>
          <div className="page-toolbar reliability-preflight-head">
            <strong>Restore App Metadata</strong>
            <span className="status-chip" data-tone={restorePreflight?.ready ? "success" : restorePreflight ? "error" : "warning"}>
              {restorePreflight ? (restorePreflight.ready ? "ready" : "blocked") : "pending"}
            </span>
          </div>
          <span className="helper-text">
            {restorePreflight?.summary ??
              "DevNest checks that managed services and tunnels are stopped before restoring app metadata."}
          </span>
        </div>
      </Card>
        </div>

        <div
          aria-labelledby="workspace-tab-repair"
          className="workspace-panel"
          hidden={activeTab !== "repair"}
          id="workspace-panel-repair"
          role="tabpanel"
        >
      <Card>
        <div className="page-header">
          <div>
            <h2>Repair Workflows</h2>
            <p>Guided repair stays project-first and explains what each workflow touches before it runs.</p>
          </div>
        </div>

        {workflowsLoading ? (
          <span className="helper-text">Loading repair workflows...</span>
        ) : (
          <div className="stack" style={{ gap: 12 }}>
            {workflows.map((workflow) => (
              <div className="detail-item" key={workflow.workflow}>
                <div className="page-toolbar">
                  <div>
                    <strong>{workflow.title}</strong>
                    <p style={{ marginTop: 6 }}>{workflow.summary}</p>
                  </div>
                  <Button
                    disabled={!selectedProjectId || actionLoading === `repair:${workflow.workflow}`}
                    onClick={() => void handleRepair(workflow.workflow)}
                    variant="primary"
                  >
                    {actionLoading === `repair:${workflow.workflow}` ? "Running..." : "Run Repair"}
                  </Button>
                </div>
                <span className="helper-text">Touches: {workflow.touches.join(", ")}.</span>
              </div>
            ))}
          </div>
        )}
      </Card>
        </div>

        <div
          aria-labelledby="workspace-tab-inspector"
          className="workspace-panel"
          hidden={activeTab !== "inspector"}
          id="workspace-panel-inspector"
          role="tabpanel"
        >
      <Card>
        <div className="page-header">
          <div>
            <h2>State Inspector</h2>
            <p>Dense project/runtime/config/tunnel visibility for debugging drift without digging through multiple pages first.</p>
          </div>
          <div className="page-toolbar">
            <Button
              disabled={!selectedProjectId || actionLoading === "export-bundle"}
              onClick={() => void handleExportDiagnosticsBundle()}
            >
              {actionLoading === "export-bundle" ? "Exporting..." : "Export Diagnostics Bundle"}
            </Button>
          </div>
        </div>

        {inspectorLoading ? (
          <span className="helper-text">Inspecting current reliability state...</span>
        ) : inspector ? (
          <div className="stack" style={{ gap: 16 }}>
            <div className="detail-grid">
              <div className="detail-item">
                <span className="detail-label">Snapshot</span>
                <strong>{formatUpdatedAt(inspector.generatedAt)}</strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">Config Output</span>
                <strong className="mono detail-value">{inspector.config.outputPath || "Not generated yet"}</strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">Local Hosts Alias</span>
                <strong>{inspector.config.localDomainAliasPresent ? "Present" : "Missing"}</strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">Persistent Alias</span>
                <strong>{inspector.config.persistentAliasPresent ? "Synced" : "Not in config"}</strong>
              </div>
            </div>

            <div className="detail-grid reliability-runtime-grid">
              <div className="detail-item reliability-runtime-item">
                <span className="detail-label">Server Runtime</span>
                <strong>
                  {inspector.runtime.server.version
                    ? `${inspector.runtime.server.kind} ${inspector.runtime.server.version}`
                    : "Not linked"}
                </strong>
                <span className="helper-text mono reliability-runtime-path">{inspector.runtime.server.path ?? "No tracked path"}</span>
              </div>
              <div className="detail-item reliability-runtime-item">
                <span className="detail-label">PHP Runtime</span>
                <strong>{inspector.runtime.php.version ? `PHP ${inspector.runtime.php.version}` : "Not linked"}</strong>
                <span className="helper-text mono reliability-runtime-path">{inspector.runtime.php.path ?? "No tracked path"}</span>
              </div>
              <div className="detail-item reliability-runtime-item">
                <span className="detail-label">MySQL Runtime</span>
                <strong>
                  {inspector.runtime.mysql?.version
                    ? `MySQL ${inspector.runtime.mysql.version}`
                    : "Not linked"}
                </strong>
                <span className="helper-text mono reliability-runtime-path">{inspector.runtime.mysql?.path ?? "No tracked path"}</span>
              </div>
              <div className="detail-item reliability-runtime-item">
                <span className="detail-label">Quick Tunnel</span>
                <strong>{inspector.quickTunnel?.status ?? "stopped"}</strong>
                <span className="helper-text mono reliability-runtime-path">{inspector.quickTunnel?.publicUrl ?? "No public URL"}</span>
              </div>
              <div className="detail-item reliability-runtime-item">
                <span className="detail-label">Persistent Hostname</span>
                <strong className="mono detail-value">
                  {inspector.persistentHostname?.hostname ?? "Not reserved"}
                </strong>
              </div>
              <div className="detail-item reliability-runtime-item">
                <span className="detail-label">Persistent Tunnel</span>
                <strong>{inspector.persistentTunnel?.status ?? "stopped"}</strong>
                <span className="helper-text mono reliability-runtime-path">{inspector.persistentTunnel?.publicUrl ?? "No persistent public URL"}</span>
              </div>
            </div>

            {inspector.runtime.issues.length ? (
              <div className="stack" style={{ gap: 8 }}>
                {inspector.runtime.issues.map((issue) => (
                  <span className="error-text" key={issue}>{issue}</span>
                ))}
              </div>
            ) : (
              <span className="helper-text">Runtime bindings look consistent for the selected project.</span>
            )}

            <div className="detail-item">
              <span className="detail-label">Config Preview</span>
              <pre className="config-preview mono" style={{ minHeight: 140, maxHeight: 240 }}>
                {inspector.config.preview ?? "Config preview is unavailable until DevNest can render the managed config."}
              </pre>
            </div>

            <div className="stack" style={{ gap: 8 }}>
              <span className="detail-label">Diagnostics Snapshot</span>
              {inspector.diagnostics.map((diagnostic) => (
                <div className="detail-item" key={diagnostic.id}>
                  <div className="page-toolbar">
                    <strong>{diagnostic.title}</strong>
                    <span className="status-chip" data-tone={diagnostic.level === "error" ? "error" : diagnostic.level === "warning" ? "warning" : "success"}>
                      {diagnostic.level}
                    </span>
                  </div>
                  <span className="helper-text">{diagnostic.message}</span>
                  {diagnostic.suggestion ? <span className="helper-text">{diagnostic.suggestion}</span> : null}
                </div>
              ))}
            </div>
          </div>
        ) : (
          <EmptyState
            title="Inspector unavailable"
            description="Select a project to inspect reliability state across config, runtime, and tunnel layers."
          />
        )}
      </Card>
        </div>

        <div
          aria-labelledby="workspace-tab-backup"
          className="workspace-panel"
          hidden={activeTab !== "backup"}
          id="workspace-panel-backup"
          role="tabpanel"
        >
      <Card>
        <div className="page-header">
          <div>
            <h2>Backup and Collaboration</h2>
            <p>Portable app metadata backup complements project profile sharing so a second machine can recover faster with less re-entry.</p>
          </div>
          <div className="page-toolbar">
            <Button
              disabled={actionLoading === "backup-metadata"}
              onClick={() => void handleBackupMetadata()}
              variant="primary"
            >
              {actionLoading === "backup-metadata" ? "Backing Up..." : "Backup App Metadata"}
            </Button>
            <Button
              disabled={actionLoading === "restore-metadata"}
              onClick={() => void handleRestoreMetadata()}
            >
              {actionLoading === "restore-metadata" ? "Restoring..." : "Restore App Metadata"}
            </Button>
          </div>
        </div>

        <div className="detail-grid">
          <div className="detail-item">
            <span className="detail-label">Team Handoff</span>
            <strong>Use team-share project profiles plus metadata backup when moving the full DevNest setup.</strong>
          </div>
          <div className="detail-item">
            <span className="detail-label">Restore Guard</span>
            <strong>{restorePreflight?.ready ? "Ready" : "Requires idle workspace"}</strong>
          </div>
        </div>

        <span className="helper-text">
          Team-share project profiles move project intent. App metadata backup adds DevNest-managed config, SSL material, and tunnel setup context on top.
        </span>
        <span className="helper-text">
          The app metadata backup includes the DevNest SQLite metadata database plus managed config, SSL, and persistent-tunnel files. It does not copy your project source code or MySQL data directories.
        </span>
      </Card>
        </div>
      </div>
    </div>
  );
}
