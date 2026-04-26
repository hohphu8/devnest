import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useToastStore } from "@/app/store/toast-store";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
  configApi,
  type LocalSslAuthorityResult,
  type PreviewVhostConfigResult,
} from "@/lib/api/config-api";
import { persistentTunnelApi } from "@/lib/api/persistent-tunnel-api";
import { tunnelApi } from "@/lib/api/tunnel-api";
import { getAppErrorMessage, type AppError } from "@/lib/tauri";
import { reliabilityApi } from "@/lib/api/reliability-api";
import type {
  PersistentTunnelHealthReport,
  ProjectPersistentTunnelState,
  PersistentTunnelSetupStatus,
  ProjectPersistentHostname,
} from "@/types/persistent-tunnel";
import type { Project } from "@/types/project";
import type { ProjectTunnelState } from "@/types/tunnel";

interface ProjectProvisioningPanelProps {
  project: Project;
  title?: string;
  description?: string;
}

function getErrorCode(error: unknown): string | undefined {
  if (typeof error === "object" && error !== null && "code" in error) {
    return String((error as AppError).code);
  }

  return undefined;
}

function suggestedPersistentHostname(
  project: Project,
  defaultZone?: string | null,
): string | null {
  const zone = defaultZone?.trim().replace(/^\.+|\.+$/g, "").toLowerCase();
  if (!zone) {
    return null;
  }

  const base =
    project.domain.replace(/\.test$/i, "").replace(/\./g, "-") ||
    project.name.replace(/[^a-z0-9]+/gi, "-");
  const slug = base
    .toLowerCase()
    .replace(/[^a-z0-9-]+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");

  if (!slug) {
    return null;
  }

  return `${slug}.${zone}`;
}

export function ProjectProvisioningPanel({
  project,
  title = "Provisioning",
  description = "Preview app-managed config, generate the managed file, and sync the local hosts entry.",
}: ProjectProvisioningPanelProps) {
  const navigate = useNavigate();
  const [preview, setPreview] = useState<PreviewVhostConfigResult>();
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string>();
  const [hostsErrorCode, setHostsErrorCode] = useState<string>();
  const [generating, setGenerating] = useState(false);
  const [applyingHosts, setApplyingHosts] = useState(false);
  const [removingHosts, setRemovingHosts] = useState(false);
  const [sslAuthority, setSslAuthority] = useState<LocalSslAuthorityResult>();
  const [sslAuthorityLoading, setSslAuthorityLoading] = useState(false);
  const [trustingSsl, setTrustingSsl] = useState(false);
  const [untrustingSsl, setUntrustingSsl] = useState(false);
  const [regeneratingSsl, setRegeneratingSsl] = useState(false);
  const [openingHttp, setOpeningHttp] = useState(false);
  const [openingHttps, setOpeningHttps] = useState(false);
  const [tunnelState, setTunnelState] = useState<ProjectTunnelState | null>(null);
  const [tunnelLoading, setTunnelLoading] = useState(false);
  const [startingTunnel, setStartingTunnel] = useState(false);
  const [stoppingTunnel, setStoppingTunnel] = useState(false);
  const [openingTunnel, setOpeningTunnel] = useState(false);
  const [persistentSetup, setPersistentSetup] = useState<PersistentTunnelSetupStatus | null>(null);
  const [persistentHostname, setPersistentHostname] =
    useState<ProjectPersistentHostname | null>(null);
  const [persistentHostnameInput, setPersistentHostnameInput] = useState("");
  const [persistentLoading, setPersistentLoading] = useState(false);
  const [applyingPersistentHostname, setApplyingPersistentHostname] = useState(false);
  const [deletingPersistentHostname, setDeletingPersistentHostname] = useState(false);
  const [refreshingPersistentTunnel, setRefreshingPersistentTunnel] = useState(false);
  const [openingPersistentTunnel, setOpeningPersistentTunnel] = useState(false);
  const [persistentTunnelState, setPersistentTunnelState] =
    useState<ProjectPersistentTunnelState | null>(null);
  const [stoppingPersistentTunnel, setStoppingPersistentTunnel] = useState(false);
  const [persistentHealth, setPersistentHealth] = useState<PersistentTunnelHealthReport | null>(null);
  const [showRemovePersistentHostnameConfirm, setShowRemovePersistentHostnameConfirm] = useState(false);
  const [bootLoading, setBootLoading] = useState(true);
  const pushToast = useToastStore((state) => state.push);

  async function guardPreflight(action: "provisionProject" | "publishPersistentDomain") {
    const report = await reliabilityApi.runPreflight(action, project.id);
    if (report.ready) {
      return true;
    }

    pushToast({
      tone: "warning",
      title: "Preflight blocked the action",
      message: report.summary,
    });
    navigate(`/reliability?projectId=${project.id}`);
    return false;
  }

  async function loadPreview() {
    setPreviewLoading(true);
    setPreviewError(undefined);

    try {
      const result = await configApi.preview(project.id);
      setPreview(result);
    } catch (error) {
      setPreview(undefined);
      setPreviewError(getAppErrorMessage(error, "Failed to preview config."));
    } finally {
      setPreviewLoading(false);
    }
  }

  async function loadSslAuthorityStatus() {
    if (!project.sslEnabled) {
      setSslAuthority(undefined);
      return;
    }

    setSslAuthorityLoading(true);
    try {
      const result = await configApi.getSslAuthorityStatus();
      setSslAuthority(result);
    } catch {
      setSslAuthority(undefined);
    } finally {
      setSslAuthorityLoading(false);
    }
  }

  async function loadTunnelState() {
    setTunnelLoading(true);
    try {
      setTunnelState(await tunnelApi.getState(project.id));
    } catch {
      setTunnelState(null);
    } finally {
      setTunnelLoading(false);
    }
  }

  async function loadPersistentTunnelState() {
    setPersistentLoading(true);

    try {
      const [setup, hostname, tunnel] = await Promise.all([
        persistentTunnelApi.getSetupStatus(),
        persistentTunnelApi.getProjectHostname(project.id),
        persistentTunnelApi.getProjectTunnelState(project.id),
      ]);
      setPersistentSetup(setup);
      setPersistentHostname(hostname);
      setPersistentHostnameInput(hostname?.hostname ?? "");
      setPersistentTunnelState(tunnel);
      setPersistentHealth(await persistentTunnelApi.inspectProjectHealth(project.id));
    } catch {
      setPersistentSetup(null);
      setPersistentHostname(null);
      setPersistentHostnameInput("");
      setPersistentTunnelState(null);
      setPersistentHealth(null);
    } finally {
      setPersistentLoading(false);
    }
  }

  async function refreshPersistentTunnelRuntimeState() {
    try {
      const [tunnel, health] = await Promise.all([
        persistentTunnelApi.getProjectTunnelState(project.id),
        persistentTunnelApi.inspectProjectHealth(project.id),
      ]);
      setPersistentTunnelState(tunnel);
      setPersistentHealth(health);
    } catch {
      setPersistentTunnelState(null);
      setPersistentHealth(null);
    }
  }

  useEffect(() => {
    let cancelled = false;
    setBootLoading(true);
    setPreview(undefined);
    setPreviewError(undefined);
    setHostsErrorCode(undefined);

    Promise.allSettled([
      loadPreview(),
      loadSslAuthorityStatus(),
      loadTunnelState(),
      loadPersistentTunnelState(),
    ]).finally(() => {
      if (!cancelled) {
        setBootLoading(false);
      }
    });

    return () => {
      cancelled = true;
    };
  }, [project.id, project.updatedAt]);

  useEffect(() => {
    if (tunnelState?.status !== "starting") {
      return;
    }

    const intervalId = window.setInterval(() => {
      void loadTunnelState();
    }, 1800);

    return () => window.clearInterval(intervalId);
  }, [tunnelState?.status, project.id]);

  useEffect(() => {
    if (persistentTunnelState?.status !== "starting") {
      return;
    }

    const intervalId = window.setInterval(() => {
      void refreshPersistentTunnelRuntimeState();
    }, 1800);

    return () => window.clearInterval(intervalId);
  }, [persistentTunnelState?.status, project.id]);

  useEffect(() => {
    if (!showRemovePersistentHostnameConfirm) {
      return;
    }

    function handleKeydown(event: KeyboardEvent) {
      if (event.key !== "Escape" || deletingPersistentHostname) {
        return;
      }

      event.preventDefault();
      setShowRemovePersistentHostnameConfirm(false);
    }

    document.addEventListener("keydown", handleKeydown);
    return () => document.removeEventListener("keydown", handleKeydown);
  }, [deletingPersistentHostname, showRemovePersistentHostnameConfirm]);

  async function handleGenerate() {
    setGenerating(true);

    try {
      if (!(await guardPreflight("provisionProject"))) {
        return;
      }
      const result = await configApi.generate(project.id);
      pushToast({
        tone: "success",
        title: "Config generated",
        message: `Managed config written to ${result.outputPath}.`,
      });
      await loadPreview();
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Config generation failed",
        message: getAppErrorMessage(error, "Failed to generate config."),
      });
    } finally {
      setGenerating(false);
    }
  }

  async function handleApplyHosts() {
    setApplyingHosts(true);
    setHostsErrorCode(undefined);

    try {
      if (!(await guardPreflight("provisionProject"))) {
        return;
      }
      const result = await configApi.applyHosts(project.domain);
      pushToast({
        tone: "success",
        title: "Hosts updated",
        message: `${result.domain} now points to ${result.targetIp}.`,
      });
    } catch (error) {
      setHostsErrorCode(getErrorCode(error));
      pushToast({
        tone: "error",
        title: "Hosts update failed",
        message: getAppErrorMessage(error, "Failed to apply hosts entry."),
      });
    } finally {
      setApplyingHosts(false);
    }
  }

  async function handleRemoveHosts() {
    setRemovingHosts(true);
    setHostsErrorCode(undefined);

    try {
      await configApi.removeHosts(project.domain);
      pushToast({
        tone: "success",
        title: "Hosts removed",
        message: `${project.domain} was removed from the Windows hosts file.`,
      });
    } catch (error) {
      setHostsErrorCode(getErrorCode(error));
      pushToast({
        tone: "error",
        title: "Hosts removal failed",
        message: getAppErrorMessage(error, "Failed to remove hosts entry."),
      });
    } finally {
      setRemovingHosts(false);
    }
  }

  async function handleTrustSsl() {
    setTrustingSsl(true);

    try {
      const result = await configApi.trustSslAuthority();
      setSslAuthority(result);
      pushToast({
        tone: "success",
        title: "DevNest CA trusted",
        message: `The DevNest local certificate authority was added to the current user trust store from ${result.certPath}.`,
      });
    } catch (error) {
      pushToast({
        tone: "error",
        title: "CA trust failed",
        message: getAppErrorMessage(error, "Failed to trust the DevNest local certificate authority."),
      });
    } finally {
      setTrustingSsl(false);
    }
  }

  async function handleUntrustSsl() {
    setUntrustingSsl(true);

    try {
      const result = await configApi.untrustSslAuthority();
      setSslAuthority(result);
      pushToast({
        tone: "success",
        title: "DevNest CA removed",
        message: "The DevNest local certificate authority was removed from the current user trust store.",
      });
    } catch (error) {
      pushToast({
        tone: "error",
        title: "CA removal failed",
        message: getAppErrorMessage(error, "Failed to remove the DevNest local certificate authority."),
      });
    } finally {
      setUntrustingSsl(false);
    }
  }

  async function handleRegenerateSsl() {
    setRegeneratingSsl(true);

    try {
      const result = await configApi.regenerateSsl(project.id);
      pushToast({
        tone: "success",
        title: "Certificate regenerated",
        message: `Fresh SSL material was written for ${result.domain}.`,
      });
      await loadPreview();
      await loadSslAuthorityStatus();
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Certificate regeneration failed",
        message: getAppErrorMessage(error, "Failed to regenerate the local SSL certificate."),
      });
    } finally {
      setRegeneratingSsl(false);
    }
  }

  async function handleOpenSite(preferHttps: boolean) {
    if (preferHttps) {
      setOpeningHttps(true);
    } else {
      setOpeningHttp(true);
    }

    try {
      await configApi.openSite(project.id, preferHttps);
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Open site failed",
        message: getAppErrorMessage(error, "Failed to open the project site."),
      });
    } finally {
      if (preferHttps) {
        setOpeningHttps(false);
      } else {
        setOpeningHttp(false);
      }
    }
  }

  async function handleStartTunnel() {
    setStartingTunnel(true);

    try {
      const nextTunnel = await tunnelApi.start(project.id);
      setTunnelState(nextTunnel);
      pushToast({
        tone: nextTunnel.publicUrl ? "success" : "info",
        title: "Tunnel started",
        message: nextTunnel.publicUrl
          ? `Public tunnel is live at ${nextTunnel.publicUrl}.`
          : "Tunnel process started. DevNest is waiting for the public URL.",
      });
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Tunnel start failed",
        message: getAppErrorMessage(error, "DevNest could not start the optional project tunnel."),
      });
    } finally {
      setStartingTunnel(false);
      await loadTunnelState();
    }
  }

  async function handleStopTunnel() {
    setStoppingTunnel(true);

    try {
      const nextTunnel = await tunnelApi.stop(project.id);
      setTunnelState(nextTunnel);
      pushToast({
        tone: "success",
        title: "Tunnel stopped",
        message: "The optional public tunnel was stopped for this project.",
      });
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Tunnel stop failed",
        message: getAppErrorMessage(error, "DevNest could not stop the optional project tunnel."),
      });
    } finally {
      setStoppingTunnel(false);
      await loadTunnelState();
    }
  }

  async function handleOpenTunnel() {
    setOpeningTunnel(true);

    try {
      await tunnelApi.open(project.id);
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Open tunnel failed",
        message: getAppErrorMessage(error, "Tunnel URL is not ready yet."),
      });
    } finally {
      setOpeningTunnel(false);
      await loadTunnelState();
    }
  }

  async function handleApplyPersistentHostname() {
    setApplyingPersistentHostname(true);

    try {
      if (!(await guardPreflight("publishPersistentDomain"))) {
        return;
      }
      const result = await persistentTunnelApi.applyProjectHostname({
        projectId: project.id,
        hostname: persistentHostnameInput || null,
      });
      setPersistentHostname(result.hostname);
      setPersistentHostnameInput(result.hostname.hostname);
      setPersistentTunnelState(result.tunnel);
      pushToast({
        tone: "success",
        title: "Persistent domain is live",
        message: `${result.tunnel.publicUrl} is now routed to ${project.name}.`,
      });
      await loadPreview();
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Persistent domain failed",
        message: getAppErrorMessage(
          error,
          "DevNest could not apply the stable public hostname for this project.",
        ),
      });
    } finally {
      setApplyingPersistentHostname(false);
      await loadPersistentTunnelState();
    }
  }

  async function handleDeletePersistentHostname() {
    setDeletingPersistentHostname(true);

    try {
      const result = await persistentTunnelApi.deleteProjectHostname(project.id);
      setPersistentHostname(null);
      setPersistentHostnameInput("");
      setPersistentTunnelState(null);
      pushToast({
        tone: "success",
        title: "Persistent hostname deleted",
        message: `${result.hostname} was removed from Cloudflare DNS and cleared from ${project.name}.`,
      });
      await loadPreview();
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Delete hostname failed",
        message: getAppErrorMessage(
          error,
          "DevNest could not delete the stable public hostname for this project.",
        ),
      });
    } finally {
      setDeletingPersistentHostname(false);
      setShowRemovePersistentHostnameConfirm(false);
      await loadPersistentTunnelState();
    }
  }

  async function handleStopPersistentTunnel() {
    setStoppingPersistentTunnel(true);

    try {
      const tunnel = await persistentTunnelApi.stopProjectTunnel(project.id);
      setPersistentTunnelState(tunnel);
      pushToast({
        tone: "success",
        title: "Persistent tunnel stopped",
        message: "The stable public tunnel was stopped for this project.",
      });
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Persistent tunnel stop failed",
        message: getAppErrorMessage(
          error,
          "DevNest could not stop the persistent public tunnel for this project.",
        ),
      });
    } finally {
      setStoppingPersistentTunnel(false);
      await loadPersistentTunnelState();
    }
  }

  async function handleRefreshPersistentTunnel() {
    setRefreshingPersistentTunnel(true);

    try {
      await refreshPersistentTunnelRuntimeState();
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Refresh failed",
        message: getAppErrorMessage(
          error,
          "DevNest could not refresh the persistent tunnel status for this project.",
        ),
      });
    } finally {
      setRefreshingPersistentTunnel(false);
    }
  }

  async function handleOpenPersistentTunnel() {
    setOpeningPersistentTunnel(true);

    try {
      await persistentTunnelApi.openProjectTunnel(project.id);
    } catch (error) {
      pushToast({
        tone: "error",
        title: "Open tunnel failed",
        message: getAppErrorMessage(error, "Persistent public URL is not ready yet."),
      });
    } finally {
      setOpeningPersistentTunnel(false);
      await refreshPersistentTunnelRuntimeState();
    }
  }

  const tunnelTone =
    tunnelState?.status === "running"
      ? "success"
      : tunnelState?.status === "error"
        ? "error"
        : "warning";
  const persistentTunnelTone =
    persistentTunnelState?.status === "running"
      ? "success"
      : persistentTunnelState?.status === "error"
        ? "error"
        : "warning";
  const generatedHostname = suggestedPersistentHostname(
    project,
    persistentSetup?.defaultHostnameZone,
  );
  const hasPersistentHostname = Boolean(persistentHostname?.hostname);
  const persistentTunnelRunning =
    persistentTunnelState?.status === "running" ||
    persistentTunnelState?.status === "starting";
  const primaryPersistentActionLabel = persistentTunnelRunning
    ? persistentHostnameInput.trim() &&
      persistentHostnameInput.trim().toLowerCase() !==
        (persistentHostname?.hostname ?? "").toLowerCase()
      ? "Update Live"
      : "Apply Live"
    : "Go Live";
  const persistentActionBusy =
    applyingPersistentHostname || stoppingPersistentTunnel || deletingPersistentHostname;
  const persistentBusyCopy = applyingPersistentHostname
    ? {
        title: "Applying Persistent Domain",
        message:
          "DevNest is syncing the hostname, Cloudflare DNS route, shared tunnel ingress, and managed server aliases for this project.",
      }
    : stoppingPersistentTunnel
      ? {
          title: "Stopping Persistent Tunnel",
          message:
            "DevNest is pausing this project's stable public tunnel while keeping the hostname ready for restart.",
        }
      : {
          title: "Deleting Persistent Hostname",
          message:
            "DevNest is removing this hostname from Cloudflare DNS and clearing the app-managed mapping for this project.",
        };

  if (bootLoading) {
    return (
      <Card className="tab-loading-card">
        <div aria-live="polite" className="tab-loading-content" role="status">
          <span aria-hidden="true" className="loading-spinner" />
          <div className="loading-scrim-copy">
            <strong>Loading provisioning state</strong>
            <span>DevNest is preparing config preview, SSL, hosts, and tunnel state for this project.</span>
          </div>
        </div>
      </Card>
    );
  }

  return (
    <Card className="route-loading-shell">
      <div className="page-header" style={{ alignItems: "flex-start" }}>
        <div>
          <p style={{ marginTop: 6 }}>
            Sync <span className="mono">{project.domain}</span> into the Windows hosts file.
          </p>
        </div>
        <div className="page-toolbar">
          <Button
            busy={applyingHosts}
            busyLabel="Applying hosts entry..."
            onClick={() => void handleApplyHosts()}
            variant="primary"
          >
            {hostsErrorCode === "HOSTS_PERMISSION_DENIED" ? "Retry Hosts Apply" : "Apply Hosts"}
          </Button>
          <Button
            busy={removingHosts}
            busyLabel="Removing hosts entry..."
            onClick={() => void handleRemoveHosts()}
          >
            Remove Hosts
          </Button>
        </div>
      </div>

      {hostsErrorCode ? (
        <span className="error-text">
          {hostsErrorCode === "HOSTS_PERMISSION_DENIED"
            ? "Administrator permission was denied or cancelled. Retry when you are ready to allow Windows elevation."
            : "Hosts provisioning needs review. Check the latest toast for the detailed error."}
        </span>
      ) : (
        <span className="helper-text">
          DevNest writes the hosts file directly first. If Windows blocks it, the app will prompt for administrator permission and retry automatically.
        </span>
      )}

      <div className="page-header">
        <div>
          <h3>{title}</h3>
          <p>{description}</p>
        </div>
        <div className="page-toolbar">
          <Button
            busy={openingHttp}
            busyLabel="Opening HTTP site..."
            onClick={() => void handleOpenSite(false)}
          >
            Open HTTP
          </Button>
          {project.sslEnabled ? (
            <Button
              busy={openingHttps}
              busyLabel="Opening HTTPS site..."
              onClick={() => void handleOpenSite(true)}
            >
              Open HTTPS
            </Button>
          ) : null}
          <Button
            busy={previewLoading}
            busyLabel="Refreshing preview..."
            onClick={() => void loadPreview()}
          >
            Refresh Preview
          </Button>
          <Button
            busy={generating}
            busyLabel="Generating config..."
            onClick={() => void handleGenerate()}
            variant="primary"
          >
            Generate Config
          </Button>
        </div>
      </div>

      <div className="detail-grid">
        <div className="detail-item">
          <span className="detail-label">Server</span>
          <strong>{preview?.serverType ?? project.serverType}</strong>
        </div>
        <div className="detail-item">
          <span className="detail-label">Domain</span>
          <strong>{project.domain}</strong>
        </div>
        <div className="detail-item">
          <span className="detail-label">Document Root</span>
          <strong className="mono detail-value">{project.documentRoot}</strong>
        </div>
        <div className="detail-item">
          <span className="detail-label">SSL</span>
          <strong>{project.sslEnabled ? "Enabled" : "Disabled"}</strong>
        </div>
      </div>
      <div className="detail-item">
        <span className="detail-label">Output Path</span>
        <strong className="mono detail-value">{preview?.outputPath ?? "Loading preview..."}</strong>
      </div>

      {previewError ? <span className="error-text">{previewError}</span> : null}

      {project.sslEnabled ? (
        <div className="page-header" style={{ alignItems: "flex-start" }}>
          <div>
            <h4 style={{ margin: 0 }}>Local SSL</h4>
            <p style={{ marginTop: 6 }}>
              Trust the DevNest local CA once, then reuse it across all SSL-enabled projects.
            </p>
            <p className="helper-text" style={{ marginTop: 8 }}>
              {sslAuthorityLoading
                ? "Checking trust status..."
                : sslAuthority?.trusted
                  ? "CA status: Trusted"
                  : "CA status: Not trusted"}
            </p>
          </div>
          <div className="page-toolbar">
            <Button
              busy={trustingSsl}
              busyLabel="Trusting DevNest CA..."
              onClick={() => void handleTrustSsl()}
            >
              Trust DevNest CA
            </Button>
            {sslAuthority?.trusted ? (
              <Button
                busy={untrustingSsl}
                busyLabel="Removing DevNest CA..."
                onClick={() => void handleUntrustSsl()}
              >
                Untrust DevNest CA
              </Button>
            ) : null}
            <Button
              busy={regeneratingSsl}
              busyLabel="Regenerating certificate..."
              onClick={() => void handleRegenerateSsl()}
            >
              Regenerate Certificate
            </Button>
          </div>
        </div>
      ) : null}

      <div className="stack" style={{ gap: 10 }}>
        <span className="detail-label">Config Preview</span>
        <pre className="config-preview mono">{preview?.configText ?? "Config preview will appear here once the renderer responds."}</pre>
      </div>

      <div className="page-header" style={{ alignItems: "flex-start" }}>
        <div>
          <h4 style={{ margin: 0 }}>Persistent Domain</h4>
          <p className="helper-text" style={{ marginTop: 8 }}>
            {persistentLoading
              ? "Checking persistent tunnel setup..."
              : persistentSetup?.details ??
                "Named tunnel setup is not configured yet. Install cloudflared and provide the required Cloudflare credentials first."}
          </p>
          <p className="helper-text" style={{ marginTop: 8 }}>
            Enter a bare subdomain like <span className="mono">hocnow-laravel-new</span> or a full hostname.
            Leave it blank to use {generatedHostname ? <span className="mono">{generatedHostname}</span> : "the project-name default once a zone is configured"}.
          </p>
        </div>

        <div className="stack" style={{ gap: 10, width: "100%" }}>
          <label className="detail-label" htmlFor={`persistent-hostname-${project.id}`}>
            Stable Public Hostname
          </label>
          <input
            className="input mono"
            id={`persistent-hostname-${project.id}`}
            onChange={(event) => setPersistentHostnameInput(event.target.value)}
            placeholder={generatedHostname ?? "subdomain.example.com"}
            type="text"
            value={persistentHostnameInput}
          />
        </div>

        <div className="page-toolbar">
          <Button
            busy={refreshingPersistentTunnel}
            busyLabel="Refreshing tunnel status..."
            disabled={
              persistentLoading ||
              refreshingPersistentTunnel ||
              applyingPersistentHostname ||
              deletingPersistentHostname ||
              stoppingPersistentTunnel
            }
            onClick={() => void handleRefreshPersistentTunnel()}
          >
            Refresh
          </Button>
          <Button
            busy={applyingPersistentHostname}
            busyLabel="Applying persistent domain..."
            disabled={
              persistentLoading ||
              applyingPersistentHostname ||
              deletingPersistentHostname ||
              stoppingPersistentTunnel ||
              !persistentSetup?.ready
            }
            onClick={() => void handleApplyPersistentHostname()}
            variant="primary"
          >
            {primaryPersistentActionLabel}
          </Button>
          <Button
            busy={openingPersistentTunnel}
            busyLabel="Opening persistent tunnel..."
            disabled={
              !persistentTunnelState?.publicUrl ||
              openingPersistentTunnel ||
              persistentTunnelState?.status !== "running"
            }
            onClick={() => void handleOpenPersistentTunnel()}
          >
            Open Tunnel
          </Button>
          {persistentTunnelRunning ? (
            <Button
              busy={stoppingPersistentTunnel}
              busyLabel="Stopping persistent tunnel..."
              disabled={stoppingPersistentTunnel || persistentLoading}
              onClick={() => void handleStopPersistentTunnel()}
            >
              Stop
            </Button>
          ) : null}
          {hasPersistentHostname ? (
            <Button
              busy={deletingPersistentHostname}
              busyLabel="Deleting hostname..."
              className="button-danger"
              disabled={persistentLoading || deletingPersistentHostname || applyingPersistentHostname}
              onClick={() => setShowRemovePersistentHostnameConfirm(true)}
            >
              Delete Hostname
            </Button>
          ) : null}
        </div>
      </div>

      <div className="detail-grid" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(480px, 1fr))" }}>
        <div className="detail-item">
          <span className="detail-label">Setup</span>
          <strong>
            <span
              className="status-chip"
              data-tone={persistentSetup?.ready ? "success" : "warning"}
            >
              {persistentSetup?.ready ? "ready" : "needs setup"}
            </span>
          </strong>
        </div>
        <div className="detail-item">
          <span className="detail-label">Default Zone</span>
          <strong className="mono detail-value">
            {persistentSetup?.defaultHostnameZone ?? "Not configured"}
          </strong>
        </div>
        <div className="detail-item">
          <span className="detail-label">Tunnel Status</span>
          <strong>
            <span className="status-chip" data-tone={persistentTunnelTone}>
              {persistentTunnelState?.status ?? "stopped"}
            </span>
          </strong>
        </div>
        <div className="detail-item">
          <span className="detail-label">Public URL</span>
          <strong className="mono detail-value">
            {persistentTunnelState?.publicUrl ??
              (persistentHostname ? `https://${persistentHostname.hostname}` : "Not ready yet")}
          </strong>
        </div>
      </div>

      {persistentTunnelState?.details ? (
        <span className="helper-text">{persistentTunnelState.details}</span>
      ) : null}

      {persistentHealth?.checks?.length ? (
        <div className="stack" style={{ gap: 8 }}>
          <span className="detail-label">Persistent Tunnel Health</span>
          <div className="detail-grid" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(480px, 1fr))" }}>
            {persistentHealth.checks.map((check) => (
              <div className="detail-item" key={check.code}>
                <span className="detail-label">{check.label}</span>
                <strong>
                  <span className="status-chip" data-tone={
                    check.status === "running"
                      ? "success"
                      : check.status === "error"
                        ? "error"
                        : "warning"
                  }>
                    {check.status}
                  </span>
                </strong>
                <span className="helper-text">{check.message}</span>
              </div>
            ))}
          </div>
        </div>
      ) : null}

      <div className="page-header" style={{ alignItems: "flex-start" }}>
        <div>
          <h4 style={{ margin: 0 }}>Optional Tunnel</h4>
          <p style={{ marginTop: 6 }}>
            Publish this local project through a lightweight cloudflared quick tunnel when you need a temporary public URL.
          </p>
          <p className="helper-text" style={{ marginTop: 8 }}>
            {tunnelLoading
              ? "Checking tunnel state..."
              : tunnelState?.details ??
                "Tunnel is stopped. Install cloudflared first or set DEVNEST_TUNNEL_BIN before starting it."}
          </p>
        </div>
        <div className="page-toolbar">
          <Button
            busy={tunnelLoading}
            busyLabel="Refreshing tunnel..."
            disabled={startingTunnel || stoppingTunnel}
            onClick={() => void loadTunnelState()}
          >
            Refresh Tunnel
          </Button>
          {tunnelState?.status === "running" || tunnelState?.status === "starting" ? (
            <Button
              busy={stoppingTunnel}
              busyLabel="Stopping tunnel..."
              onClick={() => void handleStopTunnel()}
            >
              Stop Tunnel
            </Button>
          ) : (
            <Button
              busy={startingTunnel}
              busyLabel="Starting tunnel..."
              onClick={() => void handleStartTunnel()}
            >
              Start Tunnel
            </Button>
          )}
          <Button
            busy={openingTunnel}
            busyLabel="Opening tunnel..."
            disabled={
              !tunnelState?.publicUrl || openingTunnel || tunnelState?.status !== "running"
            }
            onClick={() => void handleOpenTunnel()}
          >
            Open Tunnel
          </Button>
        </div>
      </div>

      <div className="detail-grid" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(480px, 1fr))" }}>
        <div className="detail-item">
          <span className="detail-label">Status</span>
          <strong>
            <span className="status-chip" data-tone={tunnelTone}>
              {tunnelState?.status ?? "stopped"}
            </span>
          </strong>
        </div>
        <div className="detail-item">
          <span className="detail-label">Local URL</span>
          <strong className="mono detail-value">
            {tunnelState?.localUrl ?? `http://${project.domain}`}
          </strong>
        </div>
        <div className="detail-item">
          <span className="detail-label">Public URL</span>
          <strong className="mono detail-value">{tunnelState?.publicUrl ?? "Not ready yet"}</strong>
        </div>
        <div className="detail-item">
          <span className="detail-label">Log Path</span>
          <strong className="mono detail-value">{tunnelState?.logPath ?? "Will be created on first start"}</strong>
        </div>
      </div>

      {showRemovePersistentHostnameConfirm ? (
        <div
          data-nested-modal="true"
          className="wizard-overlay"
          onClick={() => {
            if (!deletingPersistentHostname) {
              setShowRemovePersistentHostnameConfirm(false);
            }
          }}
          role="dialog"
          aria-modal="true"
        >
          <div className="confirm-dialog" onClick={(event) => event.stopPropagation()}>
            <div className="confirm-dialog-copy">
              <h3>Delete Hostname</h3>
              <p>
                DevNest will remove{" "}
                <strong className="mono">{persistentHostname?.hostname}</strong> from this
                project's stable public domain setup.
              </p>
              <span className="helper-text">
                If the hostname is live, DevNest will stop its route first, delete the Cloudflare DNS record, and clear the app-managed hostname mapping.
              </span>
            </div>
            <div className="confirm-dialog-actions">
              <Button
                disabled={deletingPersistentHostname}
                onClick={() => setShowRemovePersistentHostnameConfirm(false)}
              >
                Cancel
              </Button>
              <Button
                busy={deletingPersistentHostname}
                busyLabel="Deleting hostname..."
                className="button-danger"
                disabled={deletingPersistentHostname}
                onClick={() => void handleDeletePersistentHostname()}
              >
                Delete Hostname
              </Button>
            </div>
          </div>
        </div>
      ) : null}

      {persistentActionBusy ? (
        <div aria-live="polite" className="loading-scrim" role="status">
          <div className="loading-scrim-card">
            <span aria-hidden="true" className="loading-spinner" />
            <div className="loading-scrim-copy">
              <strong>{persistentBusyCopy.title}</strong>
              <span>{persistentBusyCopy.message}</span>
            </div>
          </div>
        </div>
      ) : null}
    </Card>
  );
}
