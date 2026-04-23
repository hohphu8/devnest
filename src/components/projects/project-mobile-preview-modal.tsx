import { useEffect, useRef, useState } from "react";
import { useToastStore } from "@/app/store/toast-store";
import { Button } from "@/components/ui/button";
import { QrCode } from "@/components/ui/qr-code";
import { mobilePreviewApi } from "@/lib/api/mobile-preview-api";
import { getAppErrorMessage } from "@/lib/tauri";
import { formatUpdatedAt } from "@/lib/utils";
import type { ProjectMobilePreviewState } from "@/types/mobile-preview";
import type { Project } from "@/types/project";

interface ProjectMobilePreviewModalProps {
  onPreviewStateChange?: (state: ProjectMobilePreviewState | null) => void;
  onClose: () => void;
  open: boolean;
  project: Project;
}

function toneForStatus(status: ProjectMobilePreviewState["status"] | undefined) {
  switch (status) {
    case "running":
      return "success";
    case "error":
      return "error";
    case "starting":
      return "warning";
    default:
      return undefined;
  }
}

export function ProjectMobilePreviewModal({
  onPreviewStateChange,
  onClose,
  open,
  project,
}: ProjectMobilePreviewModalProps) {
  const [state, setState] = useState<ProjectMobilePreviewState | null>(null);
  const [error, setError] = useState<string>();
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [starting, setStarting] = useState(false);
  const [stopping, setStopping] = useState(false);
  const intervalRef = useRef<number | undefined>(undefined);
  const pushToast = useToastStore((store) => store.push);

  useEffect(() => {
    if (!open) {
      return;
    }

    setError(undefined);
    setLoading(true);
    let cancelled = false;

    async function syncPreview() {
      try {
        const existing = await mobilePreviewApi.getState(project.id);
        if (cancelled) {
          return;
        }

        if (existing && (existing.status === "running" || existing.status === "starting")) {
          setState(existing);
          onPreviewStateChange?.(existing);
          return;
        }

        const next = await mobilePreviewApi.start(project.id);
        if (cancelled) {
          return;
        }

        setState(next);
        onPreviewStateChange?.(next);
      } catch (invokeError) {
        if (cancelled) {
          return;
        }

        setState(null);
        onPreviewStateChange?.(null);
        setError(getAppErrorMessage(invokeError, "DevNest could not start Mobile Preview."));
      }
    }

    void syncPreview().finally(() => {
      if (!cancelled) {
        setLoading(false);
      }
    });

    intervalRef.current = window.setInterval(() => {
      mobilePreviewApi
        .getState(project.id)
        .then((next) => {
          if (cancelled) {
            return;
          }

          setState(next);
          onPreviewStateChange?.(next);
        })
        .catch(() => undefined);
    }, 2500);

    return () => {
      cancelled = true;
      if (intervalRef.current) {
        window.clearInterval(intervalRef.current);
      }
    };
  }, [open, project.id]);

  useEffect(() => {
    if (!open) {
      return;
    }

    function handleKeydown(event: KeyboardEvent) {
      if (event.key !== "Escape" || stopping) {
        return;
      }

      event.preventDefault();
      handleClose();
    }

    document.addEventListener("keydown", handleKeydown);
    return () => document.removeEventListener("keydown", handleKeydown);
  }, [open, stopping]);

  function handleClose() {
    onClose();
  }

  async function handleStart() {
    setStarting(true);
    setError(undefined);

    try {
      const next = await mobilePreviewApi.start(project.id);
      setState(next);
      onPreviewStateChange?.(next);
    } catch (invokeError) {
      setState(null);
      onPreviewStateChange?.(null);
      setError(getAppErrorMessage(invokeError, "DevNest could not start Mobile Preview."));
    } finally {
      setStarting(false);
    }
  }

  async function handleStop() {
    setStopping(true);
    setError(undefined);

    try {
      const next = await mobilePreviewApi.stop(project.id);
      setState(next);
      onPreviewStateChange?.(next);
    } catch (invokeError) {
      setError(getAppErrorMessage(invokeError, "DevNest could not stop Mobile Preview cleanly."));
    } finally {
      setStopping(false);
    }
  }

  async function handleRefresh() {
    setRefreshing(true);
    setError(undefined);

    try {
      await mobilePreviewApi.stop(project.id).catch(() => undefined);
      const next = await mobilePreviewApi.start(project.id);
      setState(next);
      onPreviewStateChange?.(next);
      pushToast({
        tone: "success",
        title: "Mobile Preview refreshed",
        message: `Fresh LAN preview session is ready for ${project.name}.`,
      });
    } catch (invokeError) {
      setState(null);
      onPreviewStateChange?.(null);
      setError(getAppErrorMessage(invokeError, "DevNest could not refresh Mobile Preview."));
    } finally {
      setRefreshing(false);
    }
  }

  async function handleCopyUrl() {
    if (!state?.proxyUrl) {
      return;
    }

    try {
      await navigator.clipboard.writeText(state.proxyUrl);
      pushToast({
        tone: "success",
        title: "Preview URL copied",
        message: "The Mobile Preview URL is now in your clipboard.",
      });
    } catch (invokeError) {
      pushToast({
        tone: "error",
        title: "Copy failed",
        message: getAppErrorMessage(invokeError, "DevNest could not copy the Mobile Preview URL."),
      });
    }
  }

  if (!open) {
    return null;
  }

  return (
    <div
      className="wizard-overlay"
      onClick={handleClose}
      role="dialog"
      aria-modal="true"
    >
      <div className="mobile-preview-dialog" onClick={(event) => event.stopPropagation()}>
        <div className="page-header">
          <div>
            <h2>Mobile Preview</h2>
            <p>
              Scan once from a phone on the same Wi-Fi network. DevNest proxies the request to{" "}
              <span className="mono">{project.domain}</span> automatically.
            </p>
          </div>
          <div className="page-toolbar">
            <Button disabled={!state?.proxyUrl} onClick={() => void handleCopyUrl()}>
              Copy URL
            </Button>
            {state?.status === "running" || state?.status === "starting" ? (
              <Button
                busy={stopping}
                busyLabel="Stopping mobile preview..."
                disabled={stopping || loading || refreshing}
                onClick={() => void handleStop()}
              >
                Stop Preview
              </Button>
            ) : (
              <Button
                busy={starting}
                busyLabel="Starting mobile preview..."
                disabled={starting || loading || refreshing}
                onClick={() => void handleStart()}
              >
                Start Preview
              </Button>
            )}
            <Button
              busy={refreshing}
              busyLabel="Refreshing mobile preview..."
              disabled={refreshing || loading || stopping}
              onClick={() => void handleRefresh()}
            >
              Refresh
            </Button>
            <Button disabled={loading} onClick={handleClose} variant="primary">
              Close
            </Button>
          </div>
        </div>

        <div className="mobile-preview-body">
          <div className="mobile-preview-qr-shell">
            {loading ? (
              <span className="helper-text">Starting LAN preview session...</span>
            ) : state?.qrUrl ? (
              <QrCode size={228} value={state.qrUrl} />
            ) : (
              <span className="helper-text">QR code is not available yet.</span>
            )}
          </div>

          <div className="stack" style={{ gap: 14 }}>
            <div className="page-toolbar" style={{ justifyContent: "flex-start" }}>
              <span className="status-chip">{project.framework}</span>
              <span className="status-chip">{project.serverType}</span>
              <span className="status-chip">PHP {project.phpVersion}</span>
              <span className="status-chip" data-tone={toneForStatus(state?.status)}>
                {state?.status ?? (loading ? "starting" : "stopped")}
              </span>
            </div>

            <div className="mobile-preview-url-shell">
              <span className="detail-label">Phone URL</span>
              <strong className="mono detail-value">
                {state?.proxyUrl ?? "Preview URL will appear when ready."}
              </strong>
            </div>

            <div className="detail-grid">
              <div className="detail-item">
                <span className="detail-label">LAN IP</span>
                <strong>{state?.lanIp ?? "-"}</strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">Proxy Port</span>
                <strong>{state?.port ?? "-"}</strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">Local Project URL</span>
                <strong className="mono detail-value">{state?.localProjectUrl ?? `${project.sslEnabled ? "https" : "http"}://${project.domain}`}</strong>
              </div>
              <div className="detail-item">
                <span className="detail-label">Updated</span>
                <strong>{state?.updatedAt ? formatUpdatedAt(state.updatedAt) : "Starting..."}</strong>
              </div>
            </div>

            {project.sslEnabled ? (
              <span className="helper-text">
                This project uses local SSL in DevNest, but Mobile Preview falls back to HTTP so phones do not need your desktop certificate authority.
              </span>
            ) : null}

            <span className="helper-text">
              Phone and desktop must be on the same Wi-Fi network. This preview stays LAN-only and keeps running until you explicitly stop it.
            </span>

            {state?.details ? <span className="helper-text">{state.details}</span> : null}
            {error ? <span className="error-text">{error}</span> : null}
          </div>
        </div>
      </div>
    </div>
  );
}
