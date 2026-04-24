import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Icon } from "@/components/ui/icons";
import type { ServiceName, ServiceState } from "@/types/service";

interface ServiceTableProps {
  services: ServiceState[];
  actionName?: ServiceName;
  selectedServiceName?: ServiceName;
  onInspect: (name: ServiceName) => void;
  onStart: (name: ServiceName) => Promise<void> | void;
  onStop: (name: ServiceName) => Promise<void> | void;
  onRestart: (name: ServiceName) => Promise<void> | void;
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

export function ServiceTable({
  services,
  actionName,
  onInspect,
  onRestart,
  onStart,
  onStop,
  selectedServiceName,
}: ServiceTableProps) {
  return (
    <Card>
      <div className="page-header">
        <div>
          <h2>Runtime Matrix</h2>
          <p>Control local web, database, and helper services from one dense table.</p>
        </div>
      </div>

      <div className="service-table-shell">
        <table className="service-table">
          <thead>
            <tr>
              <th>Service</th>
              <th>Status</th>
              <th>Port</th>
              <th>PID</th>
              <th>Mode</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {services.map((service) => {
              const busy = actionName === service.name;
              const running = service.status === "running";

              return (
                <tr
                  className="service-table-row"
                  data-active={selectedServiceName === service.name}
                  key={service.name}
                  onClick={() => onInspect(service.name)}
                >
                  <td>
                    <div className="table-primary">
                      <strong>{serviceLabel(service.name)}</strong>
                      <span className="helper-text">{service.enabled ? "Enabled" : "Disabled"}</span>
                    </div>
                  </td>
                  <td>
                    <span className="status-chip" data-tone={getTone(service.status)}>
                      {service.status}
                    </span>
                  </td>
                  <td>{service.port ?? "-"}</td>
                  <td>{service.pid ?? "-"}</td>
                  <td>{service.autoStart ? "Auto" : "Manual"}</td>
                  <td>
                    <div className="service-table-actions" onClick={(event) => event.stopPropagation()}>
                      <Button
                        busy={busy && !running}
                        busyLabel={`Starting ${serviceLabel(service.name)}...`}
                        disabled={busy || running}
                        onClick={() => void onStart(service.name)}
                        size="icon"
                        title="Start"
                        variant="ghost"
                      >
                        <Icon name="play" />
                      </Button>
                      <Button
                        busy={busy && running}
                        busyLabel={`Stopping ${serviceLabel(service.name)}...`}
                        disabled={busy || !running}
                        onClick={() => void onStop(service.name)}
                        size="icon"
                        title="Stop"
                        variant="ghost"
                      >
                        <Icon name="stop" />
                      </Button>
                      <Button
                        busy={busy}
                        busyLabel={`Restarting ${serviceLabel(service.name)}...`}
                        disabled={busy}
                        onClick={() => void onRestart(service.name)}
                        size="icon"
                        title="Restart"
                        variant="ghost"
                      >
                        <Icon name="refresh" />
                      </Button>
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </Card>
  );
}
