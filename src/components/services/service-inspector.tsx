import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import type { RuntimeInventoryItem } from "@/types/runtime";
import type { PortCheckResult, ServiceName, ServiceState } from "@/types/service";

interface ServiceInspectorProps {
  actionName?: ServiceName;
  activeRuntime?: RuntimeInventoryItem;
  portCheck?: PortCheckResult;
  service?: ServiceState;
  onRefresh: () => Promise<void> | void;
  onRestart: () => Promise<void> | void;
  onStart: () => Promise<void> | void;
  onStop: () => Promise<void> | void;
  onOpenLogs: () => void;
  onOpenDashboard?: () => Promise<void> | void;
}

function serviceLabel(name: ServiceName): string {
  switch (name) {
    case "apache":
      return "Apache";
    case "nginx":
      return "Nginx";
    case "mysql":
      return "MySQL";
    case "mailpit":
      return "Mailpit";
    case "redis":
      return "Redis";
  }
}

function getTone(status: ServiceState["status"]): "success" | "warning" | "error" {
  switch (status) {
    case "running":
      return "success";
    case "error":
      return "error";
    default:
      return "warning";
  }
}

export function ServiceInspector({
  actionName,
  activeRuntime,
  onOpenLogs,
  onRefresh,
  onRestart,
  onStart,
  onStop,
  portCheck,
  service,
  onOpenDashboard,
}: ServiceInspectorProps) {
  if (!service) {
    return (
      <Card>
        <div className="page-header">
          <div>
            <h2>Service Inspector</h2>
            <p>Select a service to inspect its live runtime state, ports, and log access.</p>
          </div>
        </div>
      </Card>
    );
  }

  const busy = actionName === service.name;
  const running = service.status === "running";

  return (
    <Card>
      <div className="page-header">
        <div>
          <h2>{serviceLabel(service.name)}</h2>
          <p>Live runtime state for the selected service.</p>
        </div>
        <span className="status-chip" data-tone={getTone(service.status)}>
          {service.status}
        </span>
      </div>

      <div className="stack">
        <div className="detail-grid">
          <div className="detail-item">
            <span className="detail-label">Port</span>
            <strong>{service.port ?? "-"}</strong>
          </div>
          <div className="detail-item">
            <span className="detail-label">PID</span>
            <strong>{service.pid ?? "-"}</strong>
          </div>
          <div className="detail-item">
            <span className="detail-label">Enabled</span>
            <strong>{service.enabled ? "Yes" : "No"}</strong>
          </div>
          <div className="detail-item">
            <span className="detail-label">Startup</span>
            <strong>{service.autoStart ? "Auto" : "Manual"}</strong>
          </div>
          <div className="detail-item">
            <span className="detail-label">Active Runtime</span>
            <strong>
              {activeRuntime
                ? `${activeRuntime.version} (${activeRuntime.status})`
                : "No active runtime linked"}
            </strong>
          </div>
          <div className="detail-item">
            <span className="detail-label">Port Guard</span>
            <strong>
              {portCheck
                ? portCheck.available
                  ? `Port ${portCheck.port} is available`
                  : `Port ${portCheck.port} is in use by ${portCheck.processName ?? "another process"}`
                : "Refresh status to inspect current port ownership."}
            </strong>
          </div>
        </div>

        <div className="detail-item">
          <span className="detail-label">Runtime Path</span>
          <strong className="mono detail-value">
            {activeRuntime?.path ?? "Open Settings to link an active runtime."}
          </strong>
        </div>
        <div className="detail-item">
          <span className="detail-label">Last Error</span>
          <span className="helper-text">{service.lastError ?? "No runtime error recorded."}</span>
        </div>

        {service.name === "mysql" ? (
          <div className="inline-note-card" data-tone="warning">
            <strong>Shared data directory</strong>
            <span>
              DevNest currently mounts all managed MariaDB/MySQL versions onto one shared data directory. If you switch versions, especially to an older MariaDB build, startup can fail until the previous data state is shut down cleanly, backed up, or isolated.
            </span>
          </div>
        ) : null}

        <div className="service-inspector-actions">
          <Button
            busy={busy && !running}
            busyLabel={`Starting ${serviceLabel(service.name)}...`}
            disabled={busy || running}
            onClick={() => void onStart()}
            variant="primary"
          >
            Start
          </Button>
          <Button
            busy={busy && running}
            busyLabel={`Stopping ${serviceLabel(service.name)}...`}
            disabled={busy || !running}
            onClick={() => void onStop()}
          >
            Stop
          </Button>
          <Button
            busy={busy}
            busyLabel={`Restarting ${serviceLabel(service.name)}...`}
            disabled={busy}
            onClick={() => void onRestart()}
          >
            Restart
          </Button>
          <Button onClick={() => void onRefresh()} variant="ghost">
            Refresh
          </Button>
          <Button onClick={onOpenLogs} variant="ghost">
            View Logs
          </Button>
          {service.name === "mailpit" ? (
            <Button disabled={!running} onClick={() => void onOpenDashboard?.()} variant="ghost">
              Open Inbox
            </Button>
          ) : null}
        </div>

        <span className="helper-text">Updated at {service.updatedAt}</span>
      </div>
    </Card>
  );
}
